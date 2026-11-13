mod common;

use serde_json::{Value, json};
use sqlx::PgPool;

async fn get_json(url: String) -> Value {
    reqwest::get(url).await.unwrap().json().await.unwrap()
}

#[sqlx::test]
async fn lists_filters_and_pages_jobs(db: PgPool) {
    common::insert_job(&db, "1", "Data Analyst", "Full time", 1).await;
    common::insert_job(&db, "2", "Data Scientist", "Contract/Temp", 5).await;
    common::insert_job(&db, "3", "Barista", "Casual/Vacation", 20).await;
    let base = common::spawn_app(db, "http://127.0.0.1:9").await;

    // Newest first by default.
    let page = get_json(format!("{base}/api/jobs")).await;
    assert_eq!(page["total"], 3);
    assert_eq!(page["items"][0]["id"], "1");

    // Full-text search.
    let page = get_json(format!("{base}/api/jobs?q=scientist")).await;
    assert_eq!(page["total"], 1);
    assert_eq!(page["items"][0]["id"], "2");

    // Filters combine.
    let page = get_json(format!(
        "{base}/api/jobs?posted_within_days=7&work_type=Full%20time"
    ))
    .await;
    assert_eq!(page["total"], 1);

    // Paging.
    let page = get_json(format!("{base}/api/jobs?page=2&page_size=2")).await;
    assert_eq!(page["items"].as_array().unwrap().len(), 1);
    assert_eq!(page["total"], 3);

    // Bad input is a 400, not a 500.
    let response = reqwest::get(format!("{base}/api/jobs?sort=sideways"))
        .await
        .unwrap();
    assert_eq!(response.status(), 400);
}

#[sqlx::test]
async fn save_hide_and_view_a_job(db: PgPool) {
    common::insert_job(&db, "1", "Data Analyst", "Full time", 1).await;
    let base = common::spawn_app(db, "http://127.0.0.1:9").await;
    let client = reqwest::Client::new();

    let job: Value = client
        .patch(format!("{base}/api/jobs/1"))
        .json(&json!({ "saved": true, "viewed": true }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(job["saved"], true);
    assert!(job["viewed_at"].is_string());

    let saved = get_json(format!("{base}/api/jobs?status=saved")).await;
    assert_eq!(saved["total"], 1);
    let unread = get_json(format!("{base}/api/jobs?status=new")).await;
    assert_eq!(unread["total"], 0);

    // Hidden jobs drop out of the default list but show under status=hidden.
    client
        .patch(format!("{base}/api/jobs/1"))
        .json(&json!({ "hidden": true }))
        .send()
        .await
        .unwrap();
    assert_eq!(get_json(format!("{base}/api/jobs")).await["total"], 0);
    assert_eq!(
        get_json(format!("{base}/api/jobs?status=hidden")).await["total"],
        1
    );

    let missing = client
        .patch(format!("{base}/api/jobs/nope"))
        .json(&json!({ "saved": true }))
        .send()
        .await
        .unwrap();
    assert_eq!(missing.status(), 404);
}

#[sqlx::test]
async fn searches_crud_and_validation(db: PgPool) {
    let base = common::spawn_app(db, "http://127.0.0.1:9").await;
    let client = reqwest::Client::new();

    // The migration seeds v1's two searches.
    let list = get_json(format!("{base}/api/searches")).await;
    assert_eq!(list.as_array().unwrap().len(), 2);

    let created = client
        .post(format!("{base}/api/searches"))
        .json(&json!({ "keywords": "  BI Developer ", "location": "Sydney NSW", "max_pages": 2 }))
        .send()
        .await
        .unwrap();
    assert_eq!(created.status(), 201);
    let created: Value = created.json().await.unwrap();
    assert_eq!(created["keywords"], "BI Developer", "input is trimmed");

    let duplicate = client
        .post(format!("{base}/api/searches"))
        .json(&json!({ "keywords": "BI Developer", "location": "Sydney NSW" }))
        .send()
        .await
        .unwrap();
    assert_eq!(duplicate.status(), 409);

    let invalid = client
        .post(format!("{base}/api/searches"))
        .json(&json!({ "keywords": "", "location": "Sydney NSW" }))
        .send()
        .await
        .unwrap();
    assert_eq!(invalid.status(), 400);

    let id = created["id"].as_str().unwrap();
    let deleted = client
        .delete(format!("{base}/api/searches/{id}"))
        .send()
        .await
        .unwrap();
    assert_eq!(deleted.status(), 204);
}
