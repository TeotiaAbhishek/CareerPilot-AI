//! Shared helpers for integration tests.
#![allow(dead_code)]

use careerpilot_api::{
    AppState, ScheduleInfo,
    apply::{browser::BrowserClient, runner::Applier},
    build_app,
    documents::worker::Drafter,
    llm::{LlmClient, OllamaClient},
    scoring::Scorer,
    scraper::seek::{Politeness, SeekClient},
    vault::crypto::VaultKey,
};
use sqlx::PgPool;

/// A fixed key for tests (32 zero bytes). Never use it for real.
pub const TEST_VAULT_KEY: &str = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=";

/// Nothing listens on port 9, so calls to it fail fast.
pub const NOWHERE: &str = "http://127.0.0.1:9";

/// Start the real app on a random port and return its base URL.
/// `seek_base_url` points the scraper at a fake SEEK in tests.
pub async fn spawn_app(db: PgPool, seek_base_url: &str) -> String {
    spawn_full(db, seek_base_url, NOWHERE, NOWHERE, true)
        .await
        .0
}

/// Like `spawn_scoring_app`, but returns the drafting worker (also not started).
pub async fn spawn_drafting_app(db: PgPool, ollama_url: &str) -> (String, Drafter) {
    let (url, _, drafter, _) = spawn_full(db, NOWHERE, ollama_url, NOWHERE, true).await;
    (url, drafter)
}

/// The app with the apply runner pointed at a fake browser worker and a fake
/// Ollama. The runner is not started: tests call `applier.run_next()`.
pub async fn spawn_apply_app(
    db: PgPool,
    ollama_url: &str,
    worker_url: &str,
) -> (String, Applier, Drafter) {
    let (url, _, drafter, applier) = spawn_full(db, NOWHERE, ollama_url, worker_url, true).await;
    (url, applier, drafter)
}

/// The app with no VAULT_KEY, so the vault is locked.
pub async fn spawn_locked_vault_app(db: PgPool) -> String {
    spawn_full(db, NOWHERE, NOWHERE, NOWHERE, false).await.0
}

/// Like `spawn_app`, but scoring talks to `ollama_url` (a fake Ollama in tests).
/// The worker is not started: tests call `scorer.process_next()` themselves.
pub async fn spawn_scoring_app(db: PgPool, ollama_url: &str) -> (String, Scorer) {
    let (url, scorer, _, _) = spawn_full(db, NOWHERE, ollama_url, NOWHERE, true).await;
    (url, scorer)
}

async fn spawn_full(
    db: PgPool,
    seek_base_url: &str,
    ollama_url: &str,
    worker_url: &str,
    unlocked: bool,
) -> (String, Scorer, Drafter, Applier) {
    // Scoring needs a resume and a location.
    sqlx::query("UPDATE profile_facts SET city = 'Sydney', state = 'NSW'")
        .execute(&db)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO resume_versions (content) VALUES ('# Test Person, SQL, Python, Power BI')",
    )
    .execute(&db)
    .await
    .unwrap();

    let llm = LlmClient::from(OllamaClient::new(ollama_url, 8192).unwrap());
    let scorer = Scorer::new(db.clone(), llm.clone());
    let drafter = Drafter::new(db.clone(), llm.clone());
    let applier = Applier::new(
        db.clone(),
        llm,
        BrowserClient::new(worker_url).unwrap(),
        false,
    );
    let state = AppState {
        db: db.clone(),
        seek: SeekClient::new(seek_base_url, Politeness::none()).unwrap(),
        schedule: ScheduleInfo {
            daily_at: None,
            timezone: chrono_tz::Australia::Sydney,
        },
        scorer: scorer.clone(),
        drafter: drafter.clone(),
        applier: applier.clone(),
        vault: unlocked.then(|| VaultKey::from_base64(TEST_VAULT_KEY).unwrap()),
    };
    let app = build_app(state, "http://localhost:3000");

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (format!("http://{addr}"), scorer, drafter, applier)
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
