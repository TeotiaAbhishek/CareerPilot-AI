//! F8b: the suggestions assistant, against a fake Ollama that calls tools.

mod common;

use axum::{Json, Router, routing::post};
use careerpilot_api::{assistant::tools, scoring::prompt::PROMPT_VERSION, settings};
use serde_json::{Value, json};
use sqlx::PgPool;

/// Asks for relevant jobs first, then answers with a link to what it found.
async fn fake_ollama() -> String {
    async fn chat(Json(body): Json<Value>) -> String {
        assert_eq!(body["stream"], true);
        let messages = body["messages"].as_array().unwrap();
        let last = messages.last().unwrap();
        let line = |m: Value| format!("{m}\n");
        if last["role"] == "tool" {
            let found = last["content"].as_str().unwrap();
            assert!(found.contains("\"link\":\"/jobs/j1\""), "{found}");
            assert!(!found.contains("j2"), "j2 scores below the threshold");
            return [
                line(json!({ "message": { "role": "assistant", "content": "Start with " }, "done": false })),
                line(json!({ "message": { "role": "assistant", "content": "[Data Analyst at Globex](/jobs/j1)." }, "done": false })),
                line(json!({ "message": { "role": "assistant", "content": "" }, "done": true })),
            ]
            .concat();
        }
        assert_eq!(body["tools"].as_array().unwrap().len(), 6);
        let question = last["content"].as_str().unwrap();
        assert!(question.contains("apply to first"), "{question}");
        [
            line(json!({ "message": { "role": "assistant", "content": "", "tool_calls": [
                { "function": { "name": "search_jobs", "arguments": { "relevant_only": true, "stage": "not_applied" } } }
            ] }, "done": false })),
            line(json!({ "message": { "role": "assistant", "content": "" }, "done": true })),
        ]
        .concat()
    }
    let app = Router::new().route("/api/chat", post(chat));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    format!("http://{addr}")
}

async fn setup(db: &PgPool) {
    sqlx::query(
        "INSERT INTO jobs (id, title, company, description_text, job_url) VALUES
         ('j1', 'Data Analyst', 'Globex', 'SQL and Power BI.', 'https://example.com/1'),
         ('j2', 'Data Engineer', 'Initech', 'Spark and Kafka.', 'https://example.com/2'),
         ('j3', 'BI Analyst', 'Hooli', 'Power BI.', 'https://example.com/3')",
    )
    .execute(db)
    .await
    .unwrap();
    // j1 and j3 are relevant (threshold 60 by default), j2 isn't. j3 is already sent.
    sqlx::query(
        "INSERT INTO job_scores (job_id, model, prompt_version, score, summary, matched, missing, duration_ms) VALUES
         ('j1', 'qwen3:14b', $1, 85, 'Good fit.', '[\"SQL\"]', '[\"Databricks\"]', 10),
         ('j2', 'qwen3:14b', $1, 40, 'Weak fit.', '[]', '[\"Kafka\", \"databricks\"]', 10),
         ('j3', 'qwen3:14b', $1, 80, 'Good fit.', '[\"Power BI\"]', '[]', 10)",
    )
    .bind(PROMPT_VERSION)
    .execute(db)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO tracker (job_id, stage, applied_at) VALUES ('j3', 'submitted', now())",
    )
    .execute(db)
    .await
    .unwrap();
}

/// Ask a question and read every streamed line.
async fn ask(base: &str, body: Value) -> Vec<Value> {
    let response = reqwest::Client::new()
        .post(format!("{base}/api/assistant/ask"))
        .json(&body)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(response.headers()["content-type"], "application/x-ndjson");
    response
        .text()
        .await
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

#[sqlx::test]
async fn answers_with_tools_and_saves_the_chat(db: PgPool) {
    let (base, _) = common::spawn_drafting_app(db.clone(), &fake_ollama().await).await;
    setup(&db).await;

    let events = ask(
        &base,
        json!({ "question": "Which jobs should I apply to first?" }),
    )
    .await;
    let types: Vec<&str> = events.iter().map(|e| e["type"].as_str().unwrap()).collect();
    assert_eq!(types, ["start", "tool", "text", "text", "done"]);
    assert_eq!(
        events[0]["chat"]["title"],
        "Which jobs should I apply to first?"
    );
    assert_eq!(events[1]["chip"]["label"], "Looked at 1 job");
    let answer = &events[4]["message"];
    assert_eq!(
        answer["content"],
        "Start with [Data Analyst at Globex](/jobs/j1)."
    );
    assert_eq!(answer["tools"][0]["tool"], "search_jobs");

    let chat_id = events[0]["chat"]["id"].as_str().unwrap();
    let chat: Value = reqwest::get(format!("{base}/api/assistant/chats/{chat_id}"))
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let roles: Vec<&str> = chat["messages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["role"].as_str().unwrap())
        .collect();
    assert_eq!(roles, ["user", "assistant"]);
}

#[sqlx::test]
async fn a_model_that_is_down_is_saved_as_a_failed_answer(db: PgPool) {
    let (base, _) = common::spawn_drafting_app(db.clone(), common::NOWHERE).await;

    let events = ask(&base, json!({ "question": "How am I going?" })).await;
    let last = events.last().unwrap();
    assert_eq!(last["type"], "error");
    assert_eq!(last["message"]["failed"], true);
    assert!(
        last["message"]["content"]
            .as_str()
            .unwrap()
            .contains("ollama serve")
    );

    // Asking again in the same chat works the same way and keeps both tries.
    let chat_id = events[0]["chat"]["id"].clone();
    ask(
        &base,
        json!({ "chat_id": chat_id, "question": "How am I going?" }),
    )
    .await;
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM chat_messages")
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!(count, 4);
}

#[sqlx::test]
async fn empty_questions_and_unknown_chats_are_refused(db: PgPool) {
    let base = common::spawn_app(db.clone(), common::NOWHERE).await;
    let client = reqwest::Client::new();
    let empty = client
        .post(format!("{base}/api/assistant/ask"))
        .json(&json!({ "question": "   " }))
        .send()
        .await
        .unwrap();
    assert_eq!(empty.status(), 400);
    let unknown = client
        .post(format!("{base}/api/assistant/ask"))
        .json(&json!({ "question": "Hi", "chat_id": "00000000-0000-0000-0000-000000000000" }))
        .send()
        .await
        .unwrap();
    assert_eq!(unknown.status(), 404);
}

#[sqlx::test]
async fn chats_can_be_renamed_and_deleted(db: PgPool) {
    let base = common::spawn_app(db.clone(), common::NOWHERE).await;
    let id: uuid::Uuid =
        sqlx::query_scalar("INSERT INTO chats (title) VALUES ('Old') RETURNING id")
            .fetch_one(&db)
            .await
            .unwrap();
    let client = reqwest::Client::new();
    let renamed: Value = client
        .put(format!("{base}/api/assistant/chats/{id}"))
        .json(&json!({ "title": " Weekly plan " }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(renamed["title"], "Weekly plan");
    let list: Value = reqwest::get(format!("{base}/api/assistant/chats"))
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(list.as_array().unwrap().len(), 1);
    let deleted = client
        .delete(format!("{base}/api/assistant/chats/{id}"))
        .send()
        .await
        .unwrap();
    assert_eq!(deleted.status(), 204);
    let gone = reqwest::get(format!("{base}/api/assistant/chats/{id}"))
        .await
        .unwrap();
    assert_eq!(gone.status(), 404);
}

#[sqlx::test]
async fn tools_count_gaps_and_never_send_contact_details(db: PgPool) {
    setup(&db).await;
    sqlx::query("INSERT INTO resume_versions (content) VALUES ($1)")
        .bind("# Test Person\n\ntest@example.com | 0400 000 000\n\n## Skills\n\n* **Programming:** Python, SQL\n")
        .execute(&db)
        .await
        .unwrap();
    sqlx::query("UPDATE profile_facts SET email = 'test@example.com', phone = '0400 000 000', city = 'Sydney'")
        .execute(&db)
        .await
        .unwrap();
    let s = settings::load(&db).await.unwrap();

    let profile = tools::run(&db, &s, "my_profile", &json!({})).await.unwrap();
    assert!(profile.content.contains("Python, SQL"));
    assert!(!profile.content.contains("test@example.com"));
    assert!(!profile.content.contains("0400"));

    let gaps = tools::run(&db, &s, "missing_skills", &json!({}))
        .await
        .unwrap();
    let gaps: Value = serde_json::from_str(&gaps.content).unwrap();
    assert_eq!(gaps["jobs_considered"], 3);
    // "Databricks" and "databricks" are one requirement, missing in two jobs.
    assert_eq!(
        gaps["most_often_missing"][0],
        json!({ "requirement": "Databricks", "jobs": 2 })
    );

    let bad = tools::run(&db, &s, "search_jobs", &json!({ "limit": 500 }))
        .await
        .unwrap();
    assert!(bad.content.contains("limit must be 1-20"));
    assert_eq!(bad.chip.label, "Couldn't run search jobs");

    let pipeline = tools::run(&db, &s, "pipeline", &json!({})).await.unwrap();
    let pipeline: Value = serde_json::from_str(&pipeline.content).unwrap();
    assert_eq!(pipeline["numbers"]["sent_total"], 1);
    assert_eq!(pipeline["numbers"]["relevant_jobs_not_on_board"], 1);

    let apps = tools::run(&db, &s, "my_applications", &json!({ "stage": "submitted" }))
        .await
        .unwrap();
    assert_eq!(apps.chip.label, "Looked at 1 application");
}
