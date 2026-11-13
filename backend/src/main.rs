use anyhow::Result;
use careerpilot_api::{
    AppState, ScheduleInfo, build_app,
    config::Config,
    db, import,
    scraper::{
        self,
        seek::{Politeness, SeekClient},
    },
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
    let config = Config::from_env()?;
    let db = db::connect_and_migrate(&config.database_url).await?;

    match cli.command.unwrap_or(Command::Serve) {
        Command::ImportSupabase => import::run(&db).await,
        Command::Serve => serve(config, db).await,
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

    let state = AppState {
        db,
        seek,
        schedule: ScheduleInfo {
            daily_at: config.scrape_daily_at,
            timezone: config.scrape_timezone,
        },
    };
    let app = build_app(state, &config.web_origin);

    let listener = tokio::net::TcpListener::bind(config.bind_addr).await?;
    tracing::info!("API listening on http://{}", config.bind_addr);

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}

/// Finish in-flight requests on Ctrl+C instead of dropping them.
async fn shutdown_signal() {
    tokio::signal::ctrl_c()
        .await
        .expect("failed to listen for Ctrl+C");
    tracing::info!("shutting down");
}
