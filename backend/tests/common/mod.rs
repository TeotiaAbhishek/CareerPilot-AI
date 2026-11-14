//! Shared helpers for integration tests.
#![allow(dead_code)]

use careerpilot_api::{
    AppState, ScheduleInfo, build_app,
    llm::ollama::OllamaClient,
    scoring::Scorer,
    scraper::seek::{Politeness, SeekClient},
};
use sqlx::PgPool;

/// Nothing listens on port 9, so calls to it fail fast.
pub const NOWHERE: &str = "http://127.0.0.1:9";

/// Start the real app on a random port and return its base URL.
/// `seek_base_url` points the scraper at a fake SEEK in tests.
pub async fn spawn_app(db: PgPool, seek_base_url: &str) -> String {
    spawn_full(db, seek_base_url, NOWHERE).await.0
}

/// Like `spawn_app`, but scoring talks to `ollama_url` (a fake Ollama in tests).
/// The worker is not started: tests call `scorer.process_next()` themselves.
pub async fn spawn_scoring_app(db: PgPool, ollama_url: &str) -> (String, Scorer) {
    spawn_full(db, NOWHERE, ollama_url).await
}

async fn spawn_full(db: PgPool, seek_base_url: &str, ollama_url: &str) -> (String, Scorer) {
    // A resume and profile in a fresh temp folder.
    let dir = std::env::temp_dir().join(format!("careerpilot-test-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("resume.md"),
        "# Test Person\nSQL, Python, Power BI",
    )
    .unwrap();
    std::fs::write(dir.join("profile.yaml"), "location:\n  city: Sydney").unwrap();

    let scorer = Scorer::new(
        db.clone(),
        OllamaClient::new(ollama_url, 8192).unwrap(),
        dir.join("resume.md"),
        dir.join("profile.yaml"),
    );
    let state = AppState {
        db,
        seek: SeekClient::new(seek_base_url, Politeness::none()).unwrap(),
        schedule: ScheduleInfo {
            daily_at: None,
            timezone: chrono_tz::Australia::Sydney,
        },
        scorer: scorer.clone(),
    };
    let app = build_app(state, "http://localhost:3000");

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (format!("http://{addr}"), scorer)
}

/// Insert a minimal job directly, for tests that don't need the scraper.
pub async fn insert_job(db: &PgPool, id: &str, title: &str, work_type: &str, days_ago: i32) {
    sqlx::query(
        "INSERT INTO jobs (id, title, company, work_type, description_text, job_url, listed_at)
         VALUES ($1, $2, 'Acme', $3, $4, 'https://example.com', now() - make_interval(days => $5))",
    )
    .bind(id)
    .bind(title)
    .bind(work_type)
    .bind(format!("{title} role working with SQL"))
    .bind(days_ago)
    .execute(db)
    .await
    .unwrap();
}
