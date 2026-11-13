//! Shared helpers for integration tests.
#![allow(dead_code)]

use careerpilot_api::{
    AppState, ScheduleInfo, build_app,
    scraper::seek::{Politeness, SeekClient},
};
use sqlx::PgPool;

/// Start the real app on a random port and return its base URL.
/// `seek_base_url` points the scraper at a fake SEEK in tests.
pub async fn spawn_app(db: PgPool, seek_base_url: &str) -> String {
    let state = AppState {
        db,
        seek: SeekClient::new(seek_base_url, Politeness::none()).unwrap(),
        schedule: ScheduleInfo {
            daily_at: None,
            timezone: chrono_tz::Australia::Sydney,
        },
    };
    let app = build_app(state, "http://localhost:3000");

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    format!("http://{addr}")
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
