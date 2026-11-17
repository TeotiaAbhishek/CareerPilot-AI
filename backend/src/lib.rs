pub mod answers;
pub mod apply;
pub mod assistant;
pub mod config;
pub mod db;
pub mod documents;
pub mod emails;
pub mod error;
pub mod import;
pub mod jobs;
pub mod llm;
pub mod mail;
pub mod profile;
pub mod routes;
pub mod scoring;
pub mod scraper;
pub mod searches;
pub mod settings;
pub mod tracker;
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

use apply::runner::Applier;
use documents::worker::Drafter;
use mail::Mail;
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
    pub drafter: Drafter,
    pub applier: Applier,
    /// None when VAULT_KEY isn't set: the vault endpoints then say it's locked.
    pub vault: Option<VaultKey>,
    /// Email codes for sign-ups (Gmail, read-only).
    pub mail: Mail,
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
            "/compare/drafts",
            get(documents::compare::latest).post(documents::compare::start),
        )
        .route("/compare/drafts/{id}/html", get(documents::compare::html))
        .route("/llm/usage", get(llm::usage::report))
        .route("/llm/caps", put(llm::usage::set_cap))
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
        .route(
            "/vault/default",
            get(vault::get_default)
                .put(vault::set_default)
                .delete(vault::delete_default),
        )
        .route("/vault/{id}", put(vault::update).delete(vault::delete))
        .route("/vault/{id}/reveal", post(vault::reveal))
        .route(
            "/jobs/{id}/documents",
            get(documents::job_documents).post(documents::prepare),
        )
        .route("/documents", get(documents::list))
        .route("/documents/status", get(documents::status))
        .route("/documents/{id}", get(documents::get).put(documents::save))
        .route("/documents/{id}/approve", post(documents::approve))
        .route("/documents/{id}/docx", get(documents::download_docx))
        .route("/documents/{id}/pdf", get(documents::pdf::download))
        .route("/documents/{id}/pdf-check", get(documents::pdf::check))
        .route("/jobs/{id}/apply", post(apply::start))
        .route("/jobs/{id}/one-click", post(tracker::one_click))
        .route("/apply/bulk", post(tracker::bulk))
        .route("/tracker", get(tracker::board))
        .route(
            "/tracker/{id}",
            get(tracker::get)
                .put(tracker::update)
                .delete(tracker::remove),
        )
        .route("/insights", get(tracker::insights))
        .route(
            "/jobs/{id}/emails",
            get(emails::for_job).post(emails::draft_now),
        )
        .route("/emails/due", get(emails::due))
        .route("/emails/{id}", put(emails::save))
        .route("/emails/{id}/sent", post(emails::mark_sent))
        .route("/emails/{id}/skip", post(emails::skip))
        .route("/assistant/chats", get(assistant::list))
        .route(
            "/assistant/chats/{id}",
            get(assistant::get)
                .put(assistant::rename)
                .delete(assistant::delete),
        )
        .route("/assistant/ask", post(assistant::ask))
        .route("/applications", get(apply::list))
        .route("/applications/practice", post(apply::practice))
        .route("/applications/{id}", get(apply::get))
        .route("/applications/{id}/events", get(apply::events))
        .route("/applications/{id}/answers", post(apply::answer))
        .route("/applications/{id}/retry", post(apply::retry))
        .route("/applications/{id}/submitted", post(apply::submitted))
        .route("/applications/{id}/cancel", post(apply::cancel))
        .route("/applications/{id}/focus", post(apply::focus))
        .route("/apply/status", get(apply::status))
        .route("/browser/open", post(apply::open_browser))
        .route("/seek/resumes", get(apply::cleanup::scan))
        .route("/seek/resumes/delete", post(apply::cleanup::delete))
        .route("/mail", get(mail::status).delete(mail::disconnect))
        .route("/mail/connect", get(mail::connect))
        .route("/mail/callback", get(mail::callback));

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
