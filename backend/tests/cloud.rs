//! Claude and OpenAI, against fake APIs: scoring with the seniority cap,
//! the usage log, the monthly cap, and Ask's tool loop over a stream.

mod common;

use std::sync::{Arc, Mutex};

use axum::{
    Json, Router,
    http::HeaderMap,
    response::IntoResponse,
    routing::{get, post},
};
use careerpilot_api::{
    assistant::run_agent,
    llm::{CloudKeys, Message},
    scoring::Step,
    settings,
};
use serde_json::{Value, json};
use sqlx::PgPool;

type Seen = Arc<Mutex<Vec<(HeaderMap, Value)>>>;

/// Your resume: about 2.8 years as an analyst (Feb 2024 to now), plus contact
/// details that must never reach a model.
const RESUME: &str = "# Alex Morgan\n\
Sydney | 0400 000 000 | alex@example.com\n\n\
## Experience\n\
### Data Analyst | Acme\n\
Feb 2024 – Present\n\
- Built Power BI dashboards with SQL\n";

/// The scoring answer every fake gives: a lead role the model rates 90.
fn lead_answer() -> String {
    json!({
        "role_level": "lead",
        "required_years": 0,
        "score": 90,
        "summary": "Strong skills match.",
        "matched_requirements": ["SQL", "Power BI"],
        "missing_requirements": [],
        "unknown_requirements": [],
        "hard_blockers": []
    })
    .to_string()
}

/// A tailored resume. One bullet claims a skill the resume doesn't show,
/// so the checker flags it.
fn resume_answer() -> String {
    json!({
        "summary": { "text": "Data analyst who builds Power BI dashboards.", "evidence": ["W1"] },
        "skills": [{ "group": "Tools", "items": ["SQL", "Power BI"] }],
        "experience": [{ "id": "W1", "bullets": [
            { "text": "Built Power BI dashboards with SQL.", "evidence": ["W1.1"] },
            { "text": "Managed Kubernetes clusters.", "evidence": ["W1.1"] }
        ] }],
        "projects": []
    })
    .to_string()
}

async fn serve(app: Router) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    format!("http://{addr}")
}

/// A fake Claude. JSON requests get `lead_answer`; streamed requests play
/// Ask: the first call asks for the `pipeline` tool, the second answers.
async fn fake_claude(seen: Seen) -> String {
    let app = Router::new().route(
        "/v1/messages",
        post(move |headers: HeaderMap, Json(body): Json<Value>| {
            let seen = seen.clone();
            async move {
                seen.lock().unwrap().push((headers, body.clone()));
                if body["stream"] != true {
                    let schema = &body["output_config"]["format"]["schema"];
                    let text = if schema["properties"].get("experience").is_some() {
                        resume_answer()
                    } else {
                        lead_answer()
                    };
                    return Json(json!({
                        "content": [{ "type": "text", "text": text }],
                        "stop_reason": "end_turn",
                        "usage": { "input_tokens": 4000, "output_tokens": 500 }
                    }))
                    .into_response();
                }
                let first = body["messages"].as_array().unwrap().len() == 1;
                let events: Vec<Value> = if first {
                    vec![
                        json!({"type":"message_start","message":{"usage":{"input_tokens":1000,"output_tokens":1}}}),
                        json!({"type":"content_block_start","index":0,"content_block":{"type":"thinking","thinking":""}}),
                        json!({"type":"content_block_delta","index":0,"delta":{"type":"signature_delta","signature":"sig-1"}}),
                        json!({"type":"content_block_stop","index":0}),
                        json!({"type":"content_block_start","index":1,"content_block":{"type":"tool_use","id":"toolu_9","name":"pipeline","input":{}}}),
                        json!({"type":"content_block_delta","index":1,"delta":{"type":"input_json_delta","partial_json":"{}"}}),
                        json!({"type":"content_block_stop","index":1}),
                        json!({"type":"message_delta","delta":{"stop_reason":"tool_use"},"usage":{"output_tokens":20}}),
                        json!({"type":"message_stop"}),
                    ]
                } else {
                    vec![
                        json!({"type":"message_start","message":{"usage":{"input_tokens":1500,"output_tokens":1}}}),
                        json!({"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}),
                        json!({"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Your board "}}),
                        json!({"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"is empty."}}),
                        json!({"type":"content_block_stop","index":0}),
                        json!({"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":8}}),
                        json!({"type":"message_stop"}),
                    ]
                };
                let text: String = events
                    .iter()
                    .map(|e| format!("event: {}\ndata: {e}\n\n", e["type"].as_str().unwrap()))
                    .collect();
                ([("content-type", "text/event-stream")], text).into_response()
            }
        }),
    );
    serve(app).await
}

/// A fake OpenAI Responses API with the same answer.
async fn fake_openai(seen: Seen) -> String {
    let app = Router::new().route(
        "/v1/responses",
        post(move |headers: HeaderMap, Json(body): Json<Value>| {
            let seen = seen.clone();
            async move {
                seen.lock().unwrap().push((headers, body));
                Json(json!({
                    "status": "completed",
                    "output": [
                        { "type": "reasoning", "id": "rs_1", "summary": [] },
                        { "type": "message", "role": "assistant",
                          "content": [{ "type": "output_text", "text": lead_answer() }] }
                    ],
                    "usage": { "input_tokens": 4000, "output_tokens": 500 }
                }))
            }
        }),
    );
    serve(app).await
}

/// A fake Ollama for the local fallback: it scores everything 40.
async fn fake_ollama() -> String {
    let app = Router::new()
        .route(
            "/api/tags",
            get(|| async { Json(json!({ "models": [{ "name": "qwen3:14b" }] })) }),
        )
        .route(
            "/api/chat",
            post(|| async {
                let answer = json!({
                    "role_level": "mid", "required_years": 0, "score": 40, "summary": "Local.",
                    "matched_requirements": [], "missing_requirements": [],
                    "unknown_requirements": [], "hard_blockers": []
                });
                Json(json!({ "message": { "role": "assistant", "content": answer.to_string() } }))
            }),
        );
    serve(app).await
}

async fn setup(db: &PgPool, model: &str) {
    sqlx::query("UPDATE settings SET scoring_model = $1, apply_model = 'qwen3:14b'")
        .bind(model)
        .execute(db)
        .await
        .unwrap();
    common::insert_job(db, "1", "Lead Data Analyst", "Full time", 1).await;
}

/// Saves `RESUME` as your current resume. Call it after spawning the app,
/// which saves a placeholder resume of its own.
async fn save_resume(db: &PgPool) {
    sqlx::query("INSERT INTO resume_versions (content) VALUES ($1)")
        .bind(RESUME)
        .execute(db)
        .await
        .unwrap();
}

fn keys(claude: &str, openai: &str) -> CloudKeys {
    CloudKeys {
        anthropic_key: Some("test-anthropic-key".into()),
        anthropic_url: claude.into(),
        openai_key: Some("test-openai-key".into()),
        openai_url: openai.into(),
    }
}

#[derive(sqlx::FromRow, Debug)]
struct ScoreRow {
    model: String,
    score: i16,
    model_score: Option<i16>,
    cost: Option<f64>,
    note: Option<String>,
    missing: sqlx::types::Json<Vec<String>>,
}

async fn score_row(db: &PgPool) -> ScoreRow {
    sqlx::query_as(
        "SELECT model, score, model_score, cost_usd::float8 AS cost,
                seniority_note AS note, missing
         FROM job_scores WHERE job_id = '1'",
    )
    .fetch_one(db)
    .await
    .unwrap()
}

#[sqlx::test]
async fn claude_scores_a_lead_role_and_the_cap_brings_it_down(db: PgPool) {
    setup(&db, "claude-haiku-5-5").await;
    let seen: Seen = Default::default();
    let claude = fake_claude(seen.clone()).await;
    let (base, scorer, _) =
        common::spawn_cloud_app(db.clone(), common::NOWHERE, keys(&claude, common::NOWHERE)).await;
    save_resume(&db).await;

    assert_eq!(
        scorer.process_next().await.unwrap(),
        Step::Scored("1".into())
    );

    let row = score_row(&db).await;
    assert_eq!(row.model, "claude-haiku-5-5");
    assert_eq!(row.model_score, Some(90));
    assert_eq!(row.score, 50, "a lead role with ~2.8 years is capped at 50");
    assert!(
        row.note
            .as_deref()
            .unwrap()
            .starts_with("Lead role: you have 2.")
    );
    assert_eq!(row.missing.0[0], row.note.clone().unwrap());
    // 4000 * $0.10 + 500 * $0.50 per million tokens.
    assert!((row.cost.unwrap() - 0.00065).abs() < 1e-9);

    let (headers, body) = seen.lock().unwrap()[0].clone();
    assert_eq!(headers["x-api-key"], "test-anthropic-key");
    assert_eq!(headers["anthropic-version"], "2026-10-21");
    assert_eq!(body["model"], "claude-haiku-5-5");
    assert_eq!(body["thinking"], json!({ "type": "disabled" }));
    let schema = &body["output_config"]["format"]["schema"];
    assert_eq!(schema["additionalProperties"], false);
    assert!(schema["properties"]["score"].get("maximum").is_none());
    let prompt = body["messages"][0]["content"].as_str().unwrap();
    assert!(prompt.contains("Total work experience: 2."));
    assert!(
        !prompt.contains("0400 000 000"),
        "contact details must not reach the model"
    );
    assert!(!prompt.contains("alex@example.com"));

    // The usage report shows the spend; the keys themselves never leave the server.
    let usage: Value = reqwest::get(format!("{base}/api/llm/usage"))
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(usage["providers"][0]["provider"], "claude");
    assert_eq!(usage["providers"][0]["key_set"], true);
    assert_eq!(usage["providers"][0]["calls_this_month"], 1);
    assert!(!usage.to_string().contains("test-anthropic-key"));

    // The model list for Settings includes the paid models with a key.
    let status: Value = reqwest::get(format!("{base}/api/scoring/status"))
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let models = status["ollama"]["models"].as_array().unwrap();
    assert!(models.contains(&json!("claude-haiku-5-5")));
    assert!(models.contains(&json!("gpt-6-luna")));
}

#[sqlx::test]
async fn openai_scores_with_strict_structured_output(db: PgPool) {
    setup(&db, "gpt-6-luna").await;
    let seen: Seen = Default::default();
    let openai = fake_openai(seen.clone()).await;
    let (_, scorer, _) =
        common::spawn_cloud_app(db.clone(), common::NOWHERE, keys(common::NOWHERE, &openai)).await;
    save_resume(&db).await;

    assert_eq!(
        scorer.process_next().await.unwrap(),
        Step::Scored("1".into())
    );
    let row = score_row(&db).await;
    assert_eq!((row.model.as_str(), row.score), ("gpt-6-luna", 50));

    let (headers, body) = seen.lock().unwrap()[0].clone();
    assert_eq!(headers["authorization"], "Bearer test-openai-key");
    assert_eq!(body["store"], false);
    assert_eq!(body["reasoning"]["effort"], "none");
    assert_eq!(body["text"]["format"]["strict"], true);
    assert_eq!(
        body["text"]["format"]["schema"]["additionalProperties"],
        false
    );
}

#[sqlx::test]
async fn past_the_monthly_cap_scoring_uses_the_local_model(db: PgPool) {
    setup(&db, "claude-haiku-5-5").await;
    sqlx::query("UPDATE llm_caps SET monthly_cap_usd = 0 WHERE provider = 'claude'")
        .execute(&db)
        .await
        .unwrap();
    let seen: Seen = Default::default();
    let claude = fake_claude(seen.clone()).await;
    let ollama = fake_ollama().await;
    let (_, scorer, _) =
        common::spawn_cloud_app(db.clone(), &ollama, keys(&claude, common::NOWHERE)).await;
    save_resume(&db).await;

    assert_eq!(
        scorer.process_next().await.unwrap(),
        Step::Scored("1".into())
    );
    let row = score_row(&db).await;
    assert_eq!((row.model.as_str(), row.score), ("qwen3:14b", 40));
    assert!(row.cost.is_none());
    assert!(seen.lock().unwrap().is_empty(), "Claude must not be called");
    // Nothing left to do: the job has a score for the model in use.
    assert_eq!(scorer.process_next().await.unwrap(), Step::Idle);
}

#[sqlx::test]
async fn ask_runs_tools_through_a_claude_stream(db: PgPool) {
    setup(&db, "claude-haiku-5-5").await;
    sqlx::query(
        "UPDATE settings SET assistant_model = 'claude-sonnet-5-5', assistant_think = false",
    )
    .execute(&db)
    .await
    .unwrap();
    let seen: Seen = Default::default();
    let claude = fake_claude(seen.clone()).await;
    let (_, scorer, _) =
        common::spawn_cloud_app(db.clone(), common::NOWHERE, keys(&claude, common::NOWHERE)).await;
    save_resume(&db).await;

    let settings = settings::load(&db).await.unwrap();
    let (events, mut receiver) = tokio::sync::mpsc::unbounded_channel();
    let messages = vec![
        Message::System("You help with a job search.".into()),
        Message::User("How is my pipeline?".into()),
    ];
    let (answer, chips) = run_agent(&db, scorer.llm(), &settings, messages, &events)
        .await
        .unwrap();
    assert_eq!(answer, "Your board is empty.");
    assert_eq!(chips.len(), 1);
    drop(events);
    let mut streamed = String::new();
    while let Some(event) = receiver.recv().await {
        if let careerpilot_api::assistant::Event::Text { text } = event {
            streamed.push_str(&text);
        }
    }
    assert_eq!(streamed, "Your board is empty.");

    let calls = seen.lock().unwrap().clone();
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0].1["thinking"], json!({ "type": "between_tools" }));
    // The second call sends back the thinking block and pairs the result with the tool id.
    let second = &calls[1].1["messages"];
    assert_eq!(second[1]["content"][0]["signature"], "sig-1");
    assert_eq!(second[2]["content"][0]["type"], "tool_result");
    assert_eq!(second[2]["content"][0]["tool_use_id"], "toolu_9");
}

#[sqlx::test]
async fn compare_drafts_the_same_job_with_each_model(db: PgPool) {
    setup(&db, "qwen3:14b").await;
    let seen: Seen = Default::default();
    let claude = fake_claude(seen.clone()).await;
    let (base, scorer, _) =
        common::spawn_cloud_app(db.clone(), common::NOWHERE, keys(&claude, common::NOWHERE)).await;
    save_resume(&db).await;

    let client = reqwest::Client::new();
    let bad = client
        .post(format!("{base}/api/compare/drafts"))
        .json(&json!({ "job_ids": [], "models": ["claude-sonnet-5-5"] }))
        .send()
        .await
        .unwrap();
    assert_eq!(bad.status(), 400);

    let started: Value = client
        .post(format!("{base}/api/compare/drafts"))
        .json(&json!({ "job_ids": ["1"], "models": ["claude-sonnet-5-5", "gpt-6.1-sol"] }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(started["cells"].as_array().unwrap().len(), 2);

    // The runner works in the background; drive it here too until it's done.
    while careerpilot_api::documents::compare::run_next(&db, scorer.llm())
        .await
        .unwrap()
    {}
    let mut batch = Value::Null;
    for _ in 0..50 {
        batch = reqwest::get(format!("{base}/api/compare/drafts"))
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        if batch["running"] == false {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    assert_eq!(batch["jobs"][0]["title"], "Lead Data Analyst");
    let claude_cell = &batch["cells"][0];
    assert_eq!(claude_cell["model"], "claude-sonnet-5-5");
    assert_eq!(claude_cell["status"], "ready");
    assert!(
        claude_cell["flags"].as_i64().unwrap() >= 1,
        "Kubernetes is flagged"
    );
    assert!(claude_cell["cost_usd"].as_f64().unwrap() > 0.0);
    // OpenAI points nowhere in this test, so its draft fails and says why.
    assert_eq!(batch["cells"][1]["status"], "failed");
    assert_eq!(batch["summary"][0]["done"], 1);

    let id = claude_cell["id"].as_str().unwrap();
    let page = reqwest::get(format!("{base}/api/compare/drafts/{id}/html"))
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(page.contains("Built Power BI dashboards with SQL."));
    // Comparison drafts never become documents you could approve.
    let documents: i64 = sqlx::query_scalar("SELECT count(*) FROM documents")
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!(documents, 0);

    // A draft cut off by an API restart is picked up again when you look.
    sqlx::query("UPDATE draft_comparisons SET status = 'drafting' WHERE id = $1::uuid")
        .bind(id)
        .execute(&db)
        .await
        .unwrap();
    let mut status = Value::Null;
    for _ in 0..50 {
        let batch: Value = reqwest::get(format!("{base}/api/compare/drafts"))
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        status = batch["cells"][0]["status"].clone();
        if status == "ready" {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    assert_eq!(status, "ready");
}
