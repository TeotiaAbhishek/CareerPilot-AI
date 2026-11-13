pub mod config;
pub mod db;
pub mod error;
pub mod routes;

use std::time::Duration;

use axum::{
    Router,
    http::{HeaderValue, StatusCode},
    routing::get,
};
use sqlx::PgPool;
use tower_http::{
    cors::{Any, CorsLayer},
    timeout::TimeoutLayer,
    trace::TraceLayer,
};

/// Shared state handed to every handler. Cloning is cheap (the pool is an Arc inside).
#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
}

/// Build the whole HTTP app. Kept separate from `main` so tests can build it too.
pub fn build_app(state: AppState, web_origin: &str) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(
            web_origin
                .parse::<HeaderValue>()
                .expect("WEB_ORIGIN must be a valid origin"),
        )
        .allow_methods(Any)
        .allow_headers(Any);

    let api = Router::new().route("/health", get(routes::health::health));

    Router::new()
        .nest("/api", api)
        .layer(TraceLayer::new_for_http())
        .layer(TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            Duration::from_secs(30),
        ))
        .layer(cors)
        .with_state(state)
}
