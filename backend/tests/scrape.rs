//! End-to-end scrape against a fake SEEK served from the test fixtures.

mod common;

use std::time::Duration;

use axum::{Router, routing::get};
use serde_json::Value;
use sqlx::PgPool;

const SEARCH: &str = include_str!("fixtures/seek_search.html");
const JOB: &str = include_str!("fixtures/seek_job.html");

/// A tiny server that answers like SEEK: any search URL gives the search page,
/// any /job/{id} gives the job page.
async fn fake_seek() -> String {
    let app = Router::new()
        .route(
            "/{search}/{location}",
            get(|| async { axum::response::Html(SEARCH) }),
        )
        .route("/job/{id}", get(|| async { axum::response::Html(JOB) }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    format!("http://{addr}")
}

async fn wait_for_run_to_finish(base: &str) -> Value {
    for _ in 0..100 {
        let status: Value = reqwest::get(format!("{base}/api/scrape/status"))
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        if status["current"].is_null() {
            return status["recent"][0].clone();
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("scrape did not finish");
}

#[sqlx::test]
async fn scrape_saves_new_jobs_and_skips_known_ones(db: PgPool) {
    // Keep it to one search so the counts are easy to reason about.
    sqlx::query("DELETE FROM searches WHERE keywords = 'Data Scientist'")
        .execute(&db)
        .await
        .unwrap();
    let seek = fake_seek().await;
    let base = common::spawn_app(db.clone(), &seek).await;
    let client = reqwest::Client::new();

    let started = client
        .post(format!("{base}/api/scrape"))
        .send()
        .await
        .unwrap();
    assert_eq!(started.status(), 202);

    let run = wait_for_run_to_finish(&base).await;
    assert_eq!(run["status"], "succeeded", "{run}");
    assert_eq!(run["jobs_found"], 2);
    assert_eq!(run["jobs_new"], 2);

    let job: Value = reqwest::get(format!("{base}/api/jobs/80000001"))
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(job["work_type"], "Full time");
    assert_eq!(job["apply_type"], "Quick apply");
    assert!(job["listed_at"].is_string());

    // A second run finds the same jobs but has nothing new to fetch.
    client
        .post(format!("{base}/api/scrape"))
        .send()
        .await
        .unwrap();
    let run = wait_for_run_to_finish(&base).await;
    assert_eq!(run["jobs_found"], 2);
    assert_eq!(run["jobs_new"], 0);
}

#[sqlx::test]
async fn only_one_scrape_runs_at_a_time(db: PgPool) {
    sqlx::query("INSERT INTO scrape_runs (trigger) VALUES ('manual')")
        .execute(&db)
        .await
        .unwrap();
    let base = common::spawn_app(db, "http://127.0.0.1:9").await;

    let second = reqwest::Client::new()
        .post(format!("{base}/api/scrape"))
        .send()
        .await
        .unwrap();
    assert_eq!(second.status(), 409);
}
