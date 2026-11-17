//! Tailored documents (F5) against a fake Ollama.

mod common;

use axum::{Json, Router, routing::post};
use careerpilot_api::documents::worker::Step;
use serde_json::{Value, json};
use sqlx::PgPool;

const RESUME: &str = "# Test Person

Sydney, NSW
test@example.com

## Skills

* **Programming:** Python, SQL

## Experience

### Acme Bank

**Data Analyst**
Sep 2022 – Sep 2023

* Built a fraud model with 80% recall.
* Automated daily reports for 400+ users.

## Projects

### Lakehouse

* Built a medallion lakehouse.
";

/// A fake Ollama. A job titled "Broken" gets text that isn't JSON. Otherwise
/// the resume answer has one faithful bullet and two the checker should flag.
async fn fake_ollama() -> String {
    async fn chat(Json(body): Json<Value>) -> Json<Value> {
        assert_eq!(body["think"], true, "drafting thinks by default");
        assert_eq!(body["options"]["num_ctx"], 12288);
        let system = body["messages"][0]["content"].as_str().unwrap();
        let prompt = body["messages"][1]["content"].as_str().unwrap();
        assert!(
            !prompt.contains("test@example.com"),
            "contact details never go to the model"
        );
        assert!(prompt.contains("[W1.1] Built a fraud model with 80% recall."));

        let answer = if prompt.contains("Title: Broken") {
            "not json".to_string()
        } else if system.contains("cover letter") {
            json!({ "paragraphs": [
                { "text": "I built a fraud model with 80% recall at Acme Bank.", "evidence": ["W1.1"] },
                { "text": "I'd like to discuss the role.", "evidence": ["W1"] }
            ]})
            .to_string()
        } else {
            json!({
                "summary": { "text": "Data analyst — Python and SQL.", "evidence": ["K1", "W1"] },
                "skills": [{ "group": "Core", "items": ["SQL", "Python", "Snowflake"] }],
                "experience": [{ "id": "W1", "bullets": [
                    { "text": "Built a fraud model with 80% recall.", "evidence": ["W1.1"] },
                    { "text": "Automated reports for 900 users.", "evidence": ["W1.2"] },
                    { "text": "Managed Kubernetes clusters.", "evidence": ["W1.2"] }
                ]}],
                "projects": [{ "id": "P1", "bullets": [] }]
            })
            .to_string()
        };
        Json(json!({ "message": { "role": "assistant", "content": answer } }))
    }

    let app = Router::new().route("/api/chat", post(chat));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    format!("http://{addr}")
}

async fn send(method: reqwest::Method, url: String, body: Value) -> reqwest::Response {
    reqwest::Client::new()
        .request(method, url)
        .json(&body)
        .send()
        .await
        .unwrap()
}

async fn get_json(url: String) -> Value {
    reqwest::get(url).await.unwrap().json().await.unwrap()
}

#[sqlx::test]
async fn drafts_checks_edits_approves_and_exports(db: PgPool) {
    common::insert_job(&db, "1", "Data Analyst", "Full time", 1).await;
    let ollama = fake_ollama().await;
    let (api, drafter) = common::spawn_drafting_app(db.clone(), &ollama).await;

    // The test resume has no Experience section yet: drafting pauses.
    let res = send(
        reqwest::Method::POST,
        format!("{api}/api/jobs/1/documents"),
        json!({ "cover_letter": true }),
    )
    .await;
    assert_eq!(res.status(), 200);
    let queued: Value = res.json().await.unwrap();
    assert_eq!(queued["resume"]["status"], "queued");
    assert_eq!(queued["cover_letter"]["status"], "queued");
    assert!(matches!(
        drafter.process_next().await.unwrap(),
        Step::Paused(_)
    ));
    assert_eq!(
        get_json(format!("{api}/api/jobs/1/documents")).await["resume"]["status"],
        "queued"
    );

    // Asking again while queued doesn't add duplicates.
    send(
        reqwest::Method::POST,
        format!("{api}/api/jobs/1/documents"),
        json!({}),
    )
    .await;
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM documents")
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!(count, 2);

    sqlx::query("INSERT INTO resume_versions (content) VALUES ($1)")
        .bind(RESUME)
        .execute(&db)
        .await
        .unwrap();
    assert!(matches!(
        drafter.process_next().await.unwrap(),
        Step::Drafted(_)
    ));
    assert!(matches!(
        drafter.process_next().await.unwrap(),
        Step::Drafted(_)
    ));
    assert_eq!(drafter.process_next().await.unwrap(), Step::Idle);

    let docs = get_json(format!("{api}/api/jobs/1/documents")).await;
    let resume = &docs["resume"];
    assert_eq!(resume["status"], "ready");
    assert_eq!(resume["model"], "qwen3:14b");
    assert_eq!(resume["content"]["name"], "Test Person");
    assert_eq!(
        resume["content"]["summary"]["text"],
        "Data analyst, Python and SQL."
    );
    assert_eq!(
        resume["content"]["experience"][0]["details"][0],
        "**Data Analyst**"
    );
    assert_eq!(
        resume["content"]["skills"][0]["items"],
        json!(["SQL", "Python"])
    );
    assert!(
        resume["evidence"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["id"] == "W1.2")
    );

    // The made-up number and technology are flagged; the removed skill is a note.
    let flags = resume["flags"].as_array().unwrap();
    let messages: Vec<&str> = flags
        .iter()
        .map(|f| f["message"].as_str().unwrap())
        .collect();
    assert!(
        messages.iter().any(|m| m.contains("\"900\"")),
        "{messages:?}"
    );
    assert!(
        messages.iter().any(|m| m.contains("Kubernetes")),
        "{messages:?}"
    );
    assert!(
        messages.iter().any(|m| m.contains("Snowflake")),
        "{messages:?}"
    );
    assert_eq!(resume["open_errors"], 2, "{messages:?}");

    let id = resume["id"].as_str().unwrap().to_string();
    let res = send(
        reqwest::Method::POST,
        format!("{api}/api/documents/{id}/approve"),
        json!({}),
    )
    .await;
    assert_eq!(res.status(), 409, "flagged drafts can't be approved");

    // Fix one line and dismiss the other flag, then approve.
    let mut content = resume["content"].clone();
    content["experience"][0]["bullets"][1]["text"] =
        json!("Automated daily reports for 400+ users.");
    let kubernetes = flags
        .iter()
        .find(|f| f["message"].as_str().unwrap().contains("Kubernetes"))
        .unwrap();
    let saved: Value = send(
        reqwest::Method::PUT,
        format!("{api}/api/documents/{id}"),
        json!({ "content": content, "dismissed_flags": [kubernetes["key"]] }),
    )
    .await
    .json()
    .await
    .unwrap();
    assert_eq!(saved["open_errors"], 0, "{saved}");
    assert_eq!(saved["edited"], true);

    let approved: Value = send(
        reqwest::Method::POST,
        format!("{api}/api/documents/{id}/approve"),
        json!({}),
    )
    .await
    .json()
    .await
    .unwrap();
    assert_eq!(approved["status"], "approved");

    // Editing an approved draft sends it back for approval.
    let res = send(
        reqwest::Method::PUT,
        format!("{api}/api/documents/{id}"),
        json!({ "content": approved["content"] }),
    )
    .await;
    let edited: Value = res.json().await.unwrap();
    assert_eq!(edited["status"], "ready");
    assert_eq!(
        edited["open_errors"], 1,
        "the dismissal wasn't sent this time"
    );

    // Word export.
    let res = reqwest::get(format!("{api}/api/documents/{id}/docx"))
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    assert!(
        res.headers()["content-disposition"]
            .to_str()
            .unwrap()
            .contains("Test Person - Resume.docx")
    );
    assert_eq!(&res.bytes().await.unwrap()[..2], b"PK");

    let letter = &docs["cover_letter"];
    assert_eq!(letter["status"], "ready");
    assert_eq!(letter["content"]["greeting"], "Dear Hiring Manager,");
    assert_eq!(letter["open_errors"], 0, "{}", letter["flags"]);

    let list = get_json(format!("{api}/api/documents")).await;
    assert_eq!(list.as_array().unwrap().len(), 2);
}

#[sqlx::test]
async fn a_bad_answer_fails_only_that_draft(db: PgPool) {
    common::insert_job(&db, "b", "Broken", "Full time", 1).await;
    let ollama = fake_ollama().await;
    let (api, drafter) = common::spawn_drafting_app(db.clone(), &ollama).await;
    sqlx::query("INSERT INTO resume_versions (content) VALUES ($1)")
        .bind(RESUME)
        .execute(&db)
        .await
        .unwrap();

    send(
        reqwest::Method::POST,
        format!("{api}/api/jobs/b/documents"),
        json!({ "note": "  " }),
    )
    .await;
    assert!(matches!(
        drafter.process_next().await.unwrap(),
        Step::Failed(_)
    ));
    let docs = get_json(format!("{api}/api/jobs/b/documents")).await;
    assert_eq!(docs["resume"]["status"], "failed");
    assert!(
        docs["resume"]["error"]
            .as_str()
            .unwrap()
            .contains("not JSON")
    );
    assert!(
        docs["cover_letter"].is_null(),
        "no cover letter unless asked"
    );
}

#[sqlx::test]
async fn rejects_bad_requests(db: PgPool) {
    let (api, _) = common::spawn_drafting_app(db.clone(), common::NOWHERE).await;
    let res = send(
        reqwest::Method::POST,
        format!("{api}/api/jobs/nope/documents"),
        json!({}),
    )
    .await;
    assert_eq!(res.status(), 404);

    common::insert_job(&db, "1", "Data Analyst", "Full time", 1).await;
    let res = send(
        reqwest::Method::POST,
        format!("{api}/api/jobs/1/documents"),
        json!({ "resume": false, "cover_letter": false }),
    )
    .await;
    assert_eq!(res.status(), 400);

    let res = reqwest::get(format!(
        "{api}/api/documents/00000000-0000-0000-0000-000000000000"
    ))
    .await
    .unwrap();
    assert_eq!(res.status(), 404);
}
