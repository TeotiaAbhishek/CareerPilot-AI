use careerpilot_api::{AppState, build_app, db};

/// Starts the real app on a random port against the database in DATABASE_URL
/// and checks /api/health end to end.
#[tokio::test]
async fn health_reports_ok_with_database() {
    dotenvy::dotenv().ok();
    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set for tests");

    let pool = db::connect_and_migrate(&database_url).await.unwrap();
    let app = build_app(AppState { db: pool }, "http://localhost:3000");

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

    let body: serde_json::Value = reqwest::get(format!("http://{addr}/api/health"))
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    assert_eq!(body["status"], "ok");
    assert_eq!(body["database"], "ok");
}
