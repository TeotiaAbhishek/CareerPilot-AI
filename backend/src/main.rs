use anyhow::Result;
use careerpilot_api::{
    AppState, ScheduleInfo,
    apply::{browser::BrowserClient, runner::Applier},
    build_app,
    config::Config,
    db,
    documents::worker::Drafter,
    import,
    llm::{LlmClient, OllamaClient},
    mail::Mail,
    profile,
    scoring::Scorer,
    scraper::{
        self,
        seek::{Politeness, SeekClient},
    },
    vault::crypto::VaultKey,
};
use clap::{Parser, Subcommand};
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

#[derive(Parser)]
#[command(name = "careerpilot-api", about = "CareerPilot v2 backend")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Run the API server (the default).
    Serve,
    /// Copy jobs, analyses and applications from v1's Supabase database.
    ImportSupabase,
    /// Create a key for the password vault and save it in backend/.env.
    VaultInit,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Load backend/.env if it exists (real env vars win).
    dotenvy::dotenv().ok();

    tracing_subscriber::registry()
        .with(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "careerpilot_api=debug,tower_http=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let cli = Cli::parse();
    if matches!(cli.command, Some(Command::VaultInit)) {
        return vault_init();
    }
    let config = Config::from_env()?;
    let db = db::connect_and_migrate(&config.database_url).await?;

    match cli.command.unwrap_or(Command::Serve) {
        Command::ImportSupabase => import::run(&db).await,
        Command::Serve => serve(config, db).await,
        Command::VaultInit => unreachable!("handled above"),
    }
}

async fn serve(config: Config, db: sqlx::PgPool) -> Result<()> {
    let closed = scraper::close_interrupted_runs(&db).await?;
    if closed > 0 {
        tracing::warn!(closed, "marked interrupted scrape runs as failed");
    }

    let seek = SeekClient::new(&config.seek_base_url, Politeness::default())?;

    if let Some(at) = config.scrape_daily_at {
        scraper::schedule::spawn_daily(db.clone(), seek.clone(), at, config.scrape_timezone);
    } else {
        tracing::info!("daily scrape is off (SCRAPE_DAILY_AT=off)");
    }

    if profile::import_resume_file(&db, &config.resume_path).await? {
        tracing::info!(file = %config.resume_path.display(), "imported your resume");
    }

    let vault = match config.vault_key.as_deref().map(VaultKey::from_base64) {
        Some(Ok(key)) => {
            tracing::info!("vault unlocked");
            Some(key)
        }
        Some(Err(error)) => return Err(error.context("VAULT_KEY in backend/.env is invalid")),
        None => {
            tracing::warn!("vault locked: run `cargo run -- vault-init` to create a key");
            None
        }
    };

    // The one place that picks the model provider. Switching to OpenAI or
    // Claude later means building a different LlmClient here.
    let llm = LlmClient::from(OllamaClient::new(
        &config.ollama_url,
        config.ollama_num_ctx,
    )?);
    let scorer = Scorer::new(db.clone(), llm.clone());
    scorer.spawn();
    tracing::info!(ollama = %config.ollama_url, "scoring worker started");

    // Shares the model client with scoring, so they take turns on the GPU.
    let drafter = Drafter::new(db.clone(), llm.clone());
    let requeued = drafter.requeue_interrupted().await?;
    if requeued > 0 {
        tracing::warn!(requeued, "re-queued drafts interrupted by a restart");
    }
    drafter.spawn();
    tracing::info!("drafting worker started");

    // The apply runner uses the same model client (and GPU turns) too.
    let browser = BrowserClient::new(&config.browser_worker_url)?;
    let mail = Mail::new(
        db.clone(),
        vault.clone(),
        config.gmail.clone(),
        &config.browser_worker_url,
    )?;
    let applier =
        Applier::new(db.clone(), llm, browser, true).with_auth(vault.clone(), mail.clone());
    let requeued = applier.requeue_interrupted().await?;
    if requeued > 0 {
        tracing::warn!(requeued, "re-queued applications interrupted by a restart");
    }
    applier.spawn();
    tracing::info!(worker = %config.browser_worker_url, "apply runner started");

    let state = AppState {
        db,
        seek,
        schedule: ScheduleInfo {
            daily_at: config.scrape_daily_at,
            timezone: config.scrape_timezone,
        },
        scorer,
        drafter,
        applier,
        vault,
        mail,
    };
    // One-click Apply: moves jobs from "resume drafted" to "agent applying".
    careerpilot_api::tracker::spawn_supervisor(state.clone());
    careerpilot_api::emails::spawn_planner(state.clone());
    let app = build_app(state, &config.web_origin);

    let listener = tokio::net::TcpListener::bind(config.bind_addr).await?;
    tracing::info!("API listening on http://{}", config.bind_addr);

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}

/// Create a vault key and append it to backend/.env. Refuses to replace an
/// existing key: passwords encrypted with it would become unreadable.
fn vault_init() -> Result<()> {
    use std::io::Write;

    if std::env::var("VAULT_KEY").is_ok_and(|k| !k.trim().is_empty()) {
        anyhow::bail!(
            "VAULT_KEY is already set. Keep it: replacing it makes saved passwords unreadable."
        );
    }
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(".env")?;
    writeln!(
        file,
        "\n# Password vault key. Back it up somewhere safe. Never commit it."
    )?;
    writeln!(file, "VAULT_KEY={}", VaultKey::generate_base64())?;
    println!("Saved a new VAULT_KEY to backend/.env. Back it up (e.g. in your password manager).");
    Ok(())
}

/// Finish in-flight requests on Ctrl+C instead of dropping them.
async fn shutdown_signal() {
    tokio::signal::ctrl_c()
        .await
        .expect("failed to listen for Ctrl+C");
    tracing::info!("shutting down");
    // A run page's live log (SSE) never ends by itself, so don't wait for it
    // forever: everything else gets 3 seconds. Nothing is lost, because runs
    // interrupted here go back in the queue on the next start.
    tokio::spawn(async {
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
        tracing::info!("stopped (live logs were still open)");
        std::process::exit(0);
    });
}
