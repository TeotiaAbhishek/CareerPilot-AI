use anyhow::Result;
use careerpilot_api::{AppState, build_app, config::Config, db};
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

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

    let config = Config::from_env()?;
    let db = db::connect_and_migrate(&config.database_url).await?;
    let app = build_app(AppState { db }, &config.web_origin);

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
