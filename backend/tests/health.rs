mod common;

use sqlx::PgPool;

/// Starts the real app against a fresh test database and checks /api/health.
/// #[sqlx::test] creates a new database for each test and runs the migrations.
#[sqlx::test]
async fn health_reports_ok_with_database(db: PgPool) {
    let base = common::spawn_app(db, "http://127.0.0.1:9").await;

    let body: serde_json::Value = reqwest::get(format!("{base}/api/health"))
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    assert_eq!(body["status"], "ok");
    assert_eq!(body["database"], "ok");
}
