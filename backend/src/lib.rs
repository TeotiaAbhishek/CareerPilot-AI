pub mod answers;
pub mod config;
pub mod db;
pub mod error;
pub mod import;
pub mod jobs;
pub mod llm;
pub mod profile;
pub mod routes;
pub mod scoring;
pub mod scraper;
pub mod searches;
pub mod settings;
pub mod vault;

use std::time::Duration;

use axum::{
    Router,
    http::{HeaderValue, Method, StatusCode},
    routing::{get, post, put},
};
use chrono::NaiveTime;
use chrono_tz::Tz;
use sqlx::PgPool;
use tower_http::{
    cors::{Any, CorsLayer},
    timeout::TimeoutLayer,
    trace::TraceLayer,
};

use scoring::Scorer;
use scraper::seek::SeekClient;
use vault::crypto::VaultKey;

/// The daily scrape settings, shown in the UI.
#[derive(Clone, Debug)]
pub struct ScheduleInfo {
    pub daily_at: Option<NaiveTime>,
    pub timezone: Tz,
}

/// Shared state handed to every handler. Cloning is cheap (everything inside is shared).
#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub seek: SeekClient,
    pub schedule: ScheduleInfo,
    pub scorer: Scorer,
    /// None when VAULT_KEY isn't set: the vault endpoints then say it's locked.
    pub vault: Option<VaultKey>,
}

/// Build the whole HTTP app. Kept separate from `main` so tests can build it too.
pub fn build_app(state: AppState, web_origin: &str) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(
            web_origin
                .parse::<HeaderValue>()
                .expect("WEB_ORIGIN must be a valid origin"),
        )
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::PATCH,
            Method::DELETE,
        ])
        .allow_headers(Any);

    let api = Router::new()
        .route("/health", get(routes::health::health))
        .route("/stats", get(jobs::stats))
        .route("/jobs", get(jobs::list))
        .route("/jobs/work-types", get(jobs::work_types))
        .route("/jobs/{id}", get(jobs::get).patch(jobs::patch))
        .route("/searches", get(searches::list).post(searches::create))
        .route(
            "/searches/{id}",
            put(searches::update).delete(searches::delete),
        )
        .route("/scrape", post(routes::scrape::start))
        .route("/scrape/status", get(routes::scrape::status))
        .route("/jobs/{id}/label", put(routes::scoring::set_label))
        .route("/jobs/{id}/rescore", post(routes::scoring::rescore))
        .route("/scoring/status", get(routes::scoring::status))
        .route("/settings", get(settings::get).put(settings::update))
        .route("/evals", get(routes::evals::report))
        .route("/evals/run", post(routes::evals::run))
        .route(
            "/profile/resume",
            get(profile::get_resume).put(profile::save_resume),
        )
        .route(
            "/profile/resume/versions/{id}",
            get(profile::get_resume_version),
        )
        .route(
            "/profile/facts",
            get(profile::get_facts).put(profile::save_facts),
        )
        .route("/answers", get(answers::list).post(answers::create))
        .route(
            "/answers/{id}",
            put(answers::update).delete(answers::delete),
        )
        .route("/vault", get(vault::list).post(vault::create))
        .route("/vault/status", get(vault::status))
        .route("/vault/generate", get(vault::generate))
        .route("/vault/{id}", put(vault::update).delete(vault::delete))
        .route("/vault/{id}/reveal", post(vault::reveal));

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
