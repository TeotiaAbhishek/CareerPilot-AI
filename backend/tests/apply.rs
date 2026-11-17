//! Browser apply (F6a) against a fake browser worker and a fake Ollama.

mod common;

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
};
use serde_json::{Value, json};
use sqlx::PgPool;

/// A two-page application form, like a careers site.
#[derive(Default)]
struct Form {
    page: u8,
    captcha: bool,
    values: HashMap<String, String>,
    uploads: Vec<(String, String)>,
    clicks: Vec<String>,
    submit_attempts: u32,
    focused: u32,
    tabs_opened: u32,
}

type Shared = Arc<Mutex<Form>>;

fn field(
    form: &Form,
    id: &str,
    kind: &str,
    label: &str,
    required: bool,
    options: &[&str],
) -> Value {
    json!({
        "id": id, "kind": kind, "label": label, "required": required,
        "options": options, "value": form.values.get(id).cloned().unwrap_or_default()
    })
}

fn snapshot(form: &Form) -> Value {
    let (fields, buttons, heading) = if form.page == 1 {
        let mut fields = vec![
            field(form, "f1", "text", "First name *", true, &[]),
            field(form, "f2", "email", "Email address *", true, &[]),
            field(
                form,
                "f3",
                "radio",
                "Resumé",
                true,
                &["Upload a resumé", "Don't include a resumé"],
            ),
        ];
        // The upload box only shows once "Upload a resumé" is picked.
        if form.values.get("f3").map(String::as_str) == Some("Upload a resumé") {
            fields.push(field(form, "f4", "file", "Resumé file", true, &[]));
        }
        (
            fields,
            vec![json!({ "id": "b1", "text": "Continue", "kind": "next" })],
            "Your details",
        )
    } else {
        (
            vec![
                field(
                    form,
                    "f5",
                    "select",
                    "How many years' experience do you have with SQL?",
                    true,
                    &["1 year", "2 years", "3 years"],
                ),
                field(
                    form,
                    "f6",
                    "radio",
                    "Do you have a current driver's licence?",
                    true,
                    &["Yes", "No"],
                ),
                field(
                    form,
                    "f7",
                    "textarea",
                    "Why do you want this role? *",
                    true,
                    &[],
                ),
                field(form, "f8", "url", "Portfolio link", false, &[]),
                // A tick box answered "No" still looks empty afterwards, one
                // has no label, and an optional one stays as it is. None may loop.
                field(
                    form,
                    "f10",
                    "checkbox",
                    "I hold a current police check *",
                    true,
                    &["Yes", "No"],
                ),
                field(
                    form,
                    "f12",
                    "checkbox",
                    "Send me job alerts",
                    false,
                    &["Yes", "No"],
                ),
                field(form, "f11", "checkbox", "", false, &["Yes", "No"]),
                field(
                    form,
                    "f9",
                    "textarea",
                    "Any conflicts of interest to declare? *",
                    true,
                    &[],
                ),
            ],
            vec![
                json!({ "id": "b2", "text": "Back", "kind": "back" }),
                json!({ "id": "b3", "text": "Submit application", "kind": "submit" }),
            ],
            "Employer questions",
        )
    };
    json!({
        "url": "https://careers.example.com/apply/1", "title": "Apply", "headings": [heading],
        "fields": fields, "buttons": buttons, "errors": [],
        "captcha": form.captcha, "password": false
    })
}

async fn fake_worker(form: Shared) -> String {
    async fn health() -> Json<Value> {
        Json(json!({ "ok": true }))
    }
    async fn open(State(form): State<Shared>) -> (StatusCode, Json<Value>) {
        let mut f = form.lock().unwrap();
        f.tabs_opened += 1;
        f.page = 1;
        (StatusCode::CREATED, Json(json!({ "tab": "t1" })))
    }
    async fn tab(Path(tab): Path<String>) -> (StatusCode, Json<Value>) {
        if tab == "t1" {
            (StatusCode::OK, Json(json!({ "tab": "t1" })))
        } else {
            (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "gone", "code": "tab_not_found" })),
            )
        }
    }
    async fn snap(State(form): State<Shared>) -> Json<Value> {
        Json(snapshot(&form.lock().unwrap()))
    }
    async fn fill(State(form): State<Shared>, Json(body): Json<Value>) -> Json<Value> {
        let mut f = form.lock().unwrap();
        let mut results = Vec::new();
        for item in body["fields"].as_array().unwrap() {
            let id = item["id"].as_str().unwrap().to_string();
            f.values
                .insert(id.clone(), item["value"].as_str().unwrap().to_string());
            results.push(json!({ "id": id, "ok": true }));
        }
        Json(json!({ "results": results }))
    }
    async fn upload(State(form): State<Shared>, Json(body): Json<Value>) -> Json<Value> {
        let mut f = form.lock().unwrap();
        let id = body["id"].as_str().unwrap().to_string();
        let name = body["filename"].as_str().unwrap().to_string();
        assert!(!body["content_base64"].as_str().unwrap().is_empty());
        f.values.insert(id.clone(), name.clone());
        f.uploads.push((id, name));
        Json(json!({ "ok": true }))
    }
    async fn click(
        State(form): State<Shared>,
        Json(body): Json<Value>,
    ) -> (StatusCode, Json<Value>) {
        let mut f = form.lock().unwrap();
        let id = body["id"].as_str().unwrap().to_string();
        if id == "b3" {
            f.submit_attempts += 1;
            return (
                StatusCode::CONFLICT,
                Json(json!({ "error": "no", "code": "submit_blocked" })),
            );
        }
        if id == "b1"
            && ["f1", "f2", "f3", "f4"]
                .iter()
                .all(|k| f.values.contains_key(*k))
        {
            f.page = 2;
        }
        f.clicks.push(id);
        (StatusCode::OK, Json(json!({ "ok": true })))
    }
    async fn focus(State(form): State<Shared>) -> Json<Value> {
        form.lock().unwrap().focused += 1;
        Json(json!({ "ok": true }))
    }
    async fn close() -> Json<Value> {
        Json(json!({ "ok": true }))
    }
    /// The worker prints the documents to PDF: one page each here.
    async fn pdf(Json(body): Json<Value>) -> Json<Value> {
        assert!(body["html"].as_str().unwrap().contains("<html"));
        Json(json!({ "pdf_base64": "JVBERi0xLjQ=", "pages": 1 }))
    }

    let app = Router::new()
        .route("/health", get(health))
        .route("/tabs", post(open))
        .route("/tabs/{tab}", get(tab).delete(close))
        .route("/tabs/{tab}/snapshot", get(snap))
        .route("/tabs/{tab}/fill", post(fill))
        .route("/tabs/{tab}/upload", post(upload))
        .route("/tabs/{tab}/click", post(click))
        .route("/tabs/{tab}/focus", post(focus))
        .route("/pdf", post(pdf))
        .with_state(form);
    serve(app).await
}

/// Answers "years with SQL" from the resume, writes "why this role", and
/// leaves the declaration to you. As the reviewer, it flags "3 years" of SQL
/// once, and the answering call revises it to "2 years".
const WHY: &str = "I have built SQL reporting for three years, and this role is mostly SQL.";

async fn fake_ollama(prompts: Arc<Mutex<Vec<String>>>) -> String {
    async fn chat(
        State(prompts): State<Arc<Mutex<Vec<String>>>>,
        Json(body): Json<Value>,
    ) -> Json<Value> {
        assert_eq!(
            body["think"], false,
            "form filling doesn't think by default"
        );
        let prompt = body["messages"][1]["content"].as_str().unwrap().to_string();
        let system = body["messages"][0]["content"].as_str().unwrap_or("");
        if system.starts_with("You review") {
            let flag_f5 = prompt.contains("\"answer\": \"3 years\"");
            let mut reviews = Vec::new();
            for id in ["f5", "f7", "f8", "f9", "f10"] {
                if prompt.contains(&format!("\"id\": \"{id}\"")) {
                    let bad = id == "f5" && flag_f5;
                    reviews.push(json!({
                        "id": id,
                        "ok": !bad,
                        "issue": if bad { json!("The resume shows SQL for 2 years, not 3.") } else { Value::Null },
                    }));
                }
            }
            prompts.lock().unwrap().push(prompt);
            let content = json!({ "reviews": reviews }).to_string();
            return Json(json!({ "message": { "role": "assistant", "content": content } }));
        }
        let revising = prompt.contains("Reviewer:");
        let mut answers = Vec::new();
        for id in ["f5", "f7", "f8", "f9", "f10"] {
            if prompt.contains(&format!("\"id\": \"{id}\"")) {
                let value = match id {
                    "f5" if revising => json!("2 years"),
                    "f5" => json!("3 years"),
                    "f7" => json!(WHY),
                    "f10" => json!("No"),
                    _ => Value::Null,
                };
                answers.push(json!({ "id": id, "value": value }));
            }
        }
        prompts.lock().unwrap().push(prompt);
        let content = json!({ "answers": answers }).to_string();
        Json(json!({ "message": { "role": "assistant", "content": content } }))
    }
    serve(
        Router::new()
            .route("/api/chat", post(chat))
            .with_state(prompts),
    )
    .await
}

async fn serve(app: Router) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    format!("http://{addr}")
}

async fn setup(db: &PgPool, worker: &str) {
    sqlx::query(
        "UPDATE profile_facts SET full_name = 'Test Person', email = 'test@example.com',
                phone = '0400 000 000', australian_work_rights = TRUE",
    )
    .execute(db)
    .await
    .unwrap();
    for (id, title) in [("j1", "Data Analyst"), ("j2", "BI Analyst")] {
        sqlx::query(
            "INSERT INTO jobs (id, title, company, description_text, job_url, apply_url)
             VALUES ($1, $2, 'Acme', 'SQL role', 'https://example.com/job', $3)",
        )
        .bind(id)
        .bind(title)
        .bind(format!("{worker}/form"))
        .execute(db)
        .await
        .unwrap();
    }
    sqlx::query(
        "INSERT INTO answers (question, answer) VALUES ('Do you have a driver''s licence?', 'Yes')",
    )
    .execute(db)
    .await
    .unwrap();
}

async fn approve_resume(db: &PgPool, job_id: &str) {
    let content = json!({
        "name": "Test Person", "contact": ["Sydney, NSW"],
        "summary": { "text": "Data analyst.", "evidence": ["S"] },
        "skills": [{ "group": "Core", "items": ["SQL"] }],
        "experience": [], "projects": [], "education": [], "other": []
    });
    sqlx::query(
        "INSERT INTO documents (job_id, kind, status, content, approved_at)
         VALUES ($1, 'resume', 'approved', $2, now())",
    )
    .bind(job_id)
    .bind(content)
    .execute(db)
    .await
    .unwrap();
}

async fn status(db: &PgPool, id: &str) -> String {
    sqlx::query_scalar("SELECT status FROM applications WHERE id = $1::uuid")
        .bind(id)
        .fetch_one(db)
        .await
        .unwrap()
}

#[sqlx::test]
async fn fills_the_form_asks_you_once_and_stops_at_submit(db: PgPool) {
    let form: Shared = Arc::default();
    let prompts: Arc<Mutex<Vec<String>>> = Arc::default();
    let worker = fake_worker(form.clone()).await;
    let ollama = fake_ollama(prompts.clone()).await;
    let (url, applier, _) = common::spawn_apply_app(db.clone(), &ollama, &worker).await;
    setup(&db, &worker).await;
    let http = reqwest::Client::new();

    // No approved resume yet.
    let res = http
        .post(format!("{url}/api/jobs/j1/apply"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 409);

    approve_resume(&db, "j1").await;
    let res = http
        .post(format!("{url}/api/jobs/j1/apply"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 201);
    let app: Value = res.json().await.unwrap();
    let id = app["id"].as_str().unwrap().to_string();
    assert_eq!(app["status"], "queued");
    assert_eq!(app["start_url"], format!("{worker}/form"));

    // One active run per job.
    let res = http
        .post(format!("{url}/api/jobs/j1/apply"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 409);

    // First run: page 1 from the profile and the documents, page 2 from the
    // answer bank and the model (which also writes "why this role"), then
    // one question for you: the declaration.
    applier.run_next().await.unwrap();
    assert_eq!(status(&db, &id).await, "waiting_for_you");
    {
        let f = form.lock().unwrap();
        assert_eq!(f.values["f1"], "Test");
        assert_eq!(f.values["f2"], "test@example.com");
        assert_eq!(f.values["f3"], "Upload a resumé");
        assert_eq!(
            f.uploads,
            vec![("f4".to_string(), "Test Person - Resume.pdf".to_string())]
        );
        assert_eq!(
            f.values["f5"], "2 years",
            "revised after the reviewer flagged it"
        );
        assert_eq!(f.values["f6"], "Yes");
        assert_eq!(f.values["f7"], WHY);
        assert!(!f.values.contains_key("f9"));
        assert_eq!(f.submit_attempts, 0);
    }

    let view: Value = http
        .get(format!("{url}/api/applications/{id}"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(view["open_questions"], 1);
    let questions = view["questions"].as_array().unwrap();
    assert_eq!(
        questions.len(),
        1,
        "the optional portfolio link isn't asked"
    );
    assert_eq!(
        questions[0]["label"],
        "Any conflicts of interest to declare? *"
    );
    let log: Vec<&str> = view["events"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["message"].as_str().unwrap())
        .collect();
    assert!(
        log.iter()
            .any(|m| m.contains("Uploaded Test Person - Resume.pdf"))
    );
    assert!(log.iter().any(|m| m.contains("(answer bank)")));
    assert!(log.iter().any(|m| m.contains("the model's answer")));
    assert!(
        log.iter()
            .any(|m| m.contains("Left a checkbox with no label alone."))
    );
    assert!(
        log.iter()
            .any(|m| m.contains("written by the model") && m.contains(WHY))
    );
    assert!(
        log.iter()
            .any(|m| m.contains("Left \"Portfolio link\" empty"))
    );

    // A choice question needs one of its options; this one is free text.
    let res = http
        .post(format!("{url}/api/applications/{id}/answers"))
        .json(&json!({ "answers": [{ "question_id": questions[0]["id"], "answer": "None." }] }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    assert_eq!(status(&db, &id).await, "queued");
    let saved: i64 = sqlx::query_scalar("SELECT count(*) FROM answers WHERE source = 'agent'")
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!(saved, 1, "your answer went to the answer bank");

    // Second run: same tab, fills your answer, stops at Submit.
    applier.run_next().await.unwrap();
    assert_eq!(status(&db, &id).await, "ready_to_submit");
    {
        let f = form.lock().unwrap();
        assert_eq!(f.tabs_opened, 1, "carried on in the same tab");
        assert_eq!(f.values["f9"], "None.");
        assert_eq!(f.submit_attempts, 0, "the agent never clicks Submit");
        assert_eq!(f.focused, 0, "the window isn't pulled forward by itself");
    }

    let ready: String = sqlx::query_scalar(
        "SELECT message FROM application_events WHERE application_id = $1::uuid ORDER BY id DESC LIMIT 1",
    )
    .bind(&id)
    .fetch_one(&db)
    .await
    .unwrap();
    assert!(ready.contains("the 1 the model wrote"), "{ready}");
    let model_answers: i64 = sqlx::query_scalar("SELECT count(*) FROM answers WHERE answer = $1")
        .bind(WHY)
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!(model_answers, 0, "the model's writing isn't saved as yours");

    let prompts = prompts.lock().unwrap().clone();
    assert!(prompts[0].contains("SQL role"), "the model sees the job ad");
    assert!(
        prompts.iter().any(|p| p.contains("ANSWERS TO CHECK")),
        "the reviewer checked the answers"
    );
    let reviewed: Vec<String> = sqlx::query_scalar(
        "SELECT message FROM application_events WHERE application_id = $1::uuid AND kind = 'review' ORDER BY id",
    )
    .bind(&id)
    .fetch_all(&db)
    .await
    .unwrap();
    assert!(
        reviewed
            .iter()
            .any(|m| m.contains("Reviewer flagged") && m.contains("2 years, not 3")),
        "{reviewed:?}"
    );
    assert!(
        reviewed
            .iter()
            .any(|m| m.starts_with("Revised") && m.contains("2 years")),
        "{reviewed:?}"
    );
    assert_eq!(
        prompts
            .iter()
            .filter(|p| p.contains("FIELDS TO ANSWER") && p.contains("police check"))
            .count(),
        2,
        "once per pass (before and after your answer), even though it still looks empty"
    );
    assert!(
        prompts.iter().all(|p| !p.contains("Send me job alerts")),
        "an optional tick box never goes to the model"
    );
    assert!(
        prompts.iter().all(|p| !p.contains("\"label\": \"\"")),
        "a field with no label never goes to the model"
    );
    for prompt in &prompts {
        for secret in ["Test Person", "test@example.com", "0400"] {
            assert!(!prompt.contains(secret), "{secret} reached the model");
        }
    }

    let res = http
        .post(format!("{url}/api/applications/{id}/submitted"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    assert_eq!(status(&db, &id).await, "submitted");
}

#[sqlx::test]
async fn captcha_daily_limit_and_a_stopped_worker(db: PgPool) {
    let form: Shared = Arc::default();
    form.lock().unwrap().captcha = true;
    let worker = fake_worker(form.clone()).await;
    let ollama = fake_ollama(Arc::default()).await;
    let (url, applier, _) = common::spawn_apply_app(db.clone(), &ollama, &worker).await;
    setup(&db, &worker).await;
    approve_resume(&db, "j1").await;
    approve_resume(&db, "j2").await;
    sqlx::query("UPDATE settings SET apply_daily_cap = 1")
        .execute(&db)
        .await
        .unwrap();
    let http = reqwest::Client::new();

    let first: Value = http
        .post(format!("{url}/api/jobs/j1/apply"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let first = first["id"].as_str().unwrap().to_string();
    let second: Value = http
        .post(format!("{url}/api/jobs/j2/apply"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let second = second["id"].as_str().unwrap().to_string();

    // A CAPTCHA stops the run: it's yours to finish.
    applier.run_next().await.unwrap();
    assert_eq!(status(&db, &first).await, "needs_manual");
    let reason: String =
        sqlx::query_scalar("SELECT manual_reason FROM applications WHERE id = $1::uuid")
            .bind(&first)
            .fetch_one(&db)
            .await
            .unwrap();
    assert_eq!(reason, "captcha");
    assert!(
        form.lock().unwrap().values.is_empty(),
        "nothing is filled on a CAPTCHA page"
    );

    // The daily limit (1) is used up, so the second one waits.
    assert_eq!(applier.run_next().await.unwrap(), None);
    assert_eq!(status(&db, &second).await, "queued");
    let s: Value = http
        .get(format!("{url}/api/apply/status"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(s["started_today"], 1);
    assert_eq!(s["worker_ok"], true);
    assert!(s["paused_reason"].as_str().unwrap().contains("Daily limit"));

    // Retry puts it back in the queue. Runs that already started don't count
    // against the limit again.
    let res = http
        .post(format!("{url}/api/applications/{first}/retry"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    form.lock().unwrap().captcha = false;
    assert!(applier.run_next().await.unwrap().is_some());
    assert_ne!(status(&db, &first).await, "queued");

    // Settings validate the new numbers.
    let res = http
        .put(format!("{url}/api/settings"))
        .json(&json!({ "apply_parallel": 9 }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 400);
    // The apply agent never runs on a cloud model.
    let res = http
        .put(format!("{url}/api/settings"))
        .json(&json!({ "apply_model": "claude-sonnet-5-5" }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 400);
    let res = http
        .put(format!("{url}/api/settings"))
        .json(&json!({ "apply_daily_cap": 5 }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);

    // With the worker stopped, a run goes back in the queue instead of failing.
    let (url2, applier2, _) = common::spawn_apply_app(db.clone(), &ollama, common::NOWHERE).await;
    assert_eq!(
        applier2.run_next().await.unwrap().map(|u| u.to_string()),
        Some(second.clone())
    );
    assert_eq!(status(&db, &second).await, "queued");
    let s: Value = http
        .get(format!("{url2}/api/apply/status"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(s["worker_ok"], false);

    // Cancel closes it for good.
    let res = http
        .post(format!("{url}/api/applications/{second}/cancel"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    assert_eq!(status(&db, &second).await, "cancelled");
    let res = http
        .post(format!("{url}/api/applications/{second}/retry"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 409);
}
