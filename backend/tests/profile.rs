//! Resume versions, facts, the answer bank, and scoring from the database.

mod common;

use axum::{Json, Router, routing::get, routing::post};
use careerpilot_api::scoring::Step;
use serde_json::{Value, json};
use sqlx::PgPool;
use std::sync::{Arc, Mutex};

async fn get_json(url: String) -> Value {
    reqwest::get(url).await.unwrap().json().await.unwrap()
}

async fn send(method: reqwest::Method, url: String, body: Value) -> reqwest::Response {
    reqwest::Client::new()
        .request(method, url)
        .json(&body)
        .send()
        .await
        .unwrap()
}

#[sqlx::test]
async fn resume_versions(db: PgPool) {
    let (base, _) = common::spawn_scoring_app(db, common::NOWHERE).await;

    let resume = get_json(format!("{base}/api/profile/resume")).await;
    assert_eq!(resume["versions"].as_array().unwrap().len(), 1);

    // A change adds a version; saving the same text again doesn't.
    for _ in 0..2 {
        let response = send(
            reqwest::Method::PUT,
            format!("{base}/api/profile/resume"),
            json!({ "content": "  # New resume\nRust, SQL  " }),
        )
        .await;
        assert_eq!(response.status(), 200);
    }
    let resume = get_json(format!("{base}/api/profile/resume")).await;
    assert_eq!(resume["current"]["content"], "# New resume\nRust, SQL");
    let versions = resume["versions"].as_array().unwrap();
    assert_eq!(versions.len(), 2);

    // Old versions can still be read.
    let old_id = versions[1]["id"].as_str().unwrap();
    let old = get_json(format!("{base}/api/profile/resume/versions/{old_id}")).await;
    assert!(
        old["content"]
            .as_str()
            .unwrap()
            .starts_with("# Test Person")
    );

    let response = send(
        reqwest::Method::PUT,
        format!("{base}/api/profile/resume"),
        json!({ "content": "   " }),
    )
    .await;
    assert_eq!(response.status(), 400);
}

#[sqlx::test]
async fn facts_are_saved_and_validated(db: PgPool) {
    let (base, _) = common::spawn_scoring_app(db, common::NOWHERE).await;

    let mut facts = get_json(format!("{base}/api/profile/facts")).await;
    assert_eq!(facts["country"], "Australia");

    facts["full_name"] = json!("  Test Person ");
    facts["email"] = json!("test@example.com");
    facts["australian_work_rights"] = json!(true);
    facts["work_rights_expiry"] = json!("2029-01-16");
    let response = send(
        reqwest::Method::PUT,
        format!("{base}/api/profile/facts"),
        facts.clone(),
    )
    .await;
    assert_eq!(response.status(), 200);
    let saved: Value = response.json().await.unwrap();
    assert_eq!(saved["full_name"], "Test Person");
    assert_eq!(saved["work_rights_expiry"], "2029-01-16");

    for (field, bad) in [
        ("email", "not-an-email"),
        ("linkedin_url", "linkedin.com/in/me"),
    ] {
        let mut body = facts.clone();
        body[field] = json!(bad);
        let response = send(
            reqwest::Method::PUT,
            format!("{base}/api/profile/facts"),
            body,
        )
        .await;
        assert_eq!(response.status(), 400, "{field}");
    }
}

#[sqlx::test]
async fn answer_bank_crud_and_search(db: PgPool) {
    let (base, _) = common::spawn_scoring_app(db, common::NOWHERE).await;

    let response = send(
        reqwest::Method::POST,
        format!("{base}/api/answers"),
        json!({ "question": "Do you have a driver's licence?", "answer": "Yes, a full NSW licence.", "tags": ["Licence", "licence", " "] }),
    )
    .await;
    assert_eq!(response.status(), 201);
    let created: Value = response.json().await.unwrap();
    assert_eq!(created["tags"], json!(["licence"]));
    assert_eq!(created["source"], "you");
    let id = created["id"].as_str().unwrap();

    send(
        reqwest::Method::POST,
        format!("{base}/api/answers"),
        json!({ "question": "Expected salary?", "answer": "$110k plus super" }),
    )
    .await;

    let found = get_json(format!("{base}/api/answers?q=licence")).await;
    assert_eq!(found.as_array().unwrap().len(), 1);
    assert_eq!(
        get_json(format!("{base}/api/answers"))
            .await
            .as_array()
            .unwrap()
            .len(),
        2
    );

    let response = send(
        reqwest::Method::PUT,
        format!("{base}/api/answers/{id}"),
        json!({ "question": "Do you have a driver's licence?", "answer": "Yes, full NSW (car).", "tags": [] }),
    )
    .await;
    assert_eq!(response.status(), 200);

    let response = send(
        reqwest::Method::POST,
        format!("{base}/api/answers"),
        json!({ "question": "", "answer": "x" }),
    )
    .await;
    assert_eq!(response.status(), 400);

    let response = reqwest::Client::new()
        .delete(format!("{base}/api/answers/{id}"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 204);
    assert_eq!(
        get_json(format!("{base}/api/answers"))
            .await
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

/// A fake Ollama that remembers the last prompt it was sent.
async fn recording_ollama(last: Arc<Mutex<String>>) -> String {
    let app = Router::new()
        .route(
            "/api/tags",
            get(|| async { Json(json!({ "models": [{ "name": "qwen3:14b" }] })) }),
        )
        .route(
            "/api/chat",
            post(move |Json(body): Json<Value>| {
                let last = last.clone();
                async move {
                    *last.lock().unwrap() =
                        body["messages"][1]["content"].as_str().unwrap().to_string();
                    let answer = json!({
                        "score": 70, "summary": "ok", "matched_requirements": [],
                        "missing_requirements": [], "unknown_requirements": [], "hard_blockers": []
                    });
                    Json(json!({ "message": { "content": answer.to_string() } }))
                }
            }),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    format!("http://{addr}")
}

#[sqlx::test]
async fn scoring_uses_the_saved_resume_and_facts(db: PgPool) {
    common::insert_job(&db, "1", "Data Analyst", "Full time", 1).await;
    let last = Arc::new(Mutex::new(String::new()));
    let ollama = recording_ollama(last.clone()).await;
    let (base, scorer) = common::spawn_scoring_app(db.clone(), &ollama).await;

    // No resume: scoring pauses and says why.
    sqlx::query("DELETE FROM resume_versions")
        .execute(&db)
        .await
        .unwrap();
    let Step::Paused(reason) = scorer.process_next().await.unwrap() else {
        panic!("expected a pause");
    };
    assert!(reason.contains("resume"));

    send(
        reqwest::Method::PUT,
        format!("{base}/api/profile/resume"),
        json!({ "content": "# Me\nPower BI wizard" }),
    )
    .await;
    // Facts without a location: paused too.
    let mut facts = get_json(format!("{base}/api/profile/facts")).await;
    facts["city"] = json!("");
    send(
        reqwest::Method::PUT,
        format!("{base}/api/profile/facts"),
        facts.clone(),
    )
    .await;
    let Step::Paused(reason) = scorer.process_next().await.unwrap() else {
        panic!("expected a pause");
    };
    assert!(reason.contains("Facts"));

    facts["city"] = json!("Sydney");
    facts["phone"] = json!("0400 123 456");
    facts["visa_type"] = json!("Subclass 485");
    send(
        reqwest::Method::PUT,
        format!("{base}/api/profile/facts"),
        facts,
    )
    .await;

    assert_eq!(
        scorer.process_next().await.unwrap(),
        Step::Scored("1".into())
    );
    let prompt = last.lock().unwrap().clone();
    assert!(prompt.contains("Power BI wizard"));
    assert!(prompt.contains("visa_type: \"Subclass 485\""));
    assert!(
        !prompt.contains("0400 123 456"),
        "contact details must not reach the model"
    );

    // The score remembers which resume version it used.
    let used: Option<uuid::Uuid> =
        sqlx::query_scalar("SELECT resume_version_id FROM job_scores WHERE job_id = '1'")
            .fetch_one(&db)
            .await
            .unwrap();
    let current = get_json(format!("{base}/api/profile/resume")).await;
    assert_eq!(
        used.unwrap().to_string(),
        current["current"]["id"].as_str().unwrap()
    );
}
