//! F8: email drafts and follow-ups, against a fake Ollama.

mod common;

use axum::{Json, Router, routing::post};
use careerpilot_api::{documents::worker::Step, emails};
use serde_json::{Value, json};
use sqlx::PgPool;

const RESUME: &str = "# Test Person

test@example.com

## Skills

* **Programming:** Python, SQL

## Experience

### Acme Bank

**Data Analyst**

* Built a fraud model with 80% recall.
";

/// Writes every email the same way: one faithful paragraph, one the checker flags.
async fn fake_ollama() -> String {
    async fn chat(Json(body): Json<Value>) -> Json<Value> {
        let system = body["messages"][0]["content"].as_str().unwrap();
        let prompt = body["messages"][1]["content"].as_str().unwrap();
        assert!(system.contains("short, professional email"));
        assert!(
            !prompt.contains("test@example.com"),
            "your contact details never go to the model"
        );
        let answer = json!({
            "subject": "Data Analyst application — following up",
            "paragraphs": [
                { "text": "I built a fraud model with 80% recall at Acme Bank.", "evidence": ["W1.1"] },
                { "text": "I also led a Kubernetes migration.", "evidence": ["W1"] }
            ]
        });
        Json(json!({ "message": { "role": "assistant", "content": answer.to_string() } }))
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

async fn get(url: String) -> Value {
    reqwest::get(url).await.unwrap().json().await.unwrap()
}

async fn setup(db: &PgPool) {
    sqlx::query("INSERT INTO resume_versions (content) VALUES ($1)")
        .bind(RESUME)
        .execute(db)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO jobs (id, title, company, description_text, job_url) VALUES
         ('j1', 'Data Analyst', 'Globex', 'SQL role. Send your CV to Careers@Globex.com.au.', 'https://example.com/1'),
         ('j2', 'BI Analyst', 'Initech', 'Apply on SEEK.', 'https://example.com/2')",
    )
    .execute(db)
    .await
    .unwrap();
}

async fn statuses(db: &PgPool, job: &str) -> Vec<(String, String)> {
    sqlx::query_as("SELECT kind, status FROM emails WHERE job_id = $1 ORDER BY kind")
        .bind(job)
        .fetch_all(db)
        .await
        .unwrap()
}

#[sqlx::test]
async fn follow_ups_are_planned_drafted_checked_and_sent(db: PgPool) {
    let (base, drafter) = common::spawn_drafting_app(db.clone(), &fake_ollama().await).await;
    setup(&db).await;
    // j1 applied 8 days ago; j2 two months ago (too old for follow-ups).
    sqlx::query(
        "INSERT INTO tracker (job_id, stage, applied_at) VALUES
         ('j1', 'submitted', now() - interval '8 days'),
         ('j2', 'submitted', now() - interval '60 days')",
    )
    .execute(&db)
    .await
    .unwrap();

    assert_eq!(
        emails::plan(&db).await.unwrap(),
        1,
        "only the first follow-up is due"
    );
    assert_eq!(
        statuses(&db, "j1").await,
        vec![
            ("follow_up_1".into(), "queued".into()),
            ("follow_up_2".into(), "scheduled".into())
        ]
    );
    assert!(
        statuses(&db, "j2").await.is_empty(),
        "old applications get none"
    );

    // The drafting worker writes it (no documents are waiting).
    assert!(matches!(
        drafter.process_next().await.unwrap(),
        Step::Drafted(_)
    ));
    assert_eq!(drafter.process_next().await.unwrap(), Step::Idle);

    let page = get(format!("{base}/api/jobs/j1/emails")).await;
    assert_eq!(
        page["address"], "careers@globex.com.au",
        "found in the ad by code"
    );
    assert!(
        page["sign_off"]
            .as_str()
            .unwrap()
            .starts_with("Kind regards,\nTest Person")
    );
    let email = &page["emails"][0];
    assert_eq!(email["kind"], "follow_up_1");
    assert_eq!(email["status"], "ready");
    assert_eq!(email["to_address"], "careers@globex.com.au");
    assert_eq!(
        email["subject"], "Data Analyst application - following up",
        "no dashes"
    );
    let flags = email["flags"].as_array().unwrap();
    assert_eq!(flags.len(), 1, "the Kubernetes claim is flagged: {flags:?}");
    assert_eq!(flags[0]["path"], "paragraphs.1");

    // Due list and the card badge.
    let due = get(format!("{base}/api/emails/due")).await;
    assert_eq!(due.as_array().unwrap().len(), 1);
    assert_eq!(due[0]["title"], "Data Analyst");
    assert_eq!(get(format!("{base}/api/tracker/j1")).await["emails_due"], 1);

    // Edit: drop the flagged paragraph, change the address.
    let id = email["id"].as_str().unwrap();
    let bad = send(
        reqwest::Method::PUT,
        format!("{base}/api/emails/{id}"),
        json!({ "to_address": "not an address", "subject": "x", "paragraphs": [] }),
    )
    .await;
    assert_eq!(bad.status(), 400);
    let saved: Value = send(
        reqwest::Method::PUT,
        format!("{base}/api/emails/{id}"),
        json!({
            "to_address": " Hiring@Globex.com.au ",
            "subject": "Following up: Data Analyst",
            "paragraphs": [email["paragraphs"][0]]
        }),
    )
    .await
    .json()
    .await
    .unwrap();
    assert_eq!(saved["to_address"], "hiring@globex.com.au");
    assert_eq!(saved["flags"], json!([]));

    let sent: Value = send(
        reqwest::Method::POST,
        format!("{base}/api/emails/{id}/sent"),
        json!({}),
    )
    .await
    .json()
    .await
    .unwrap();
    assert_eq!(sent["status"], "sent");
    assert!(
        get(format!("{base}/api/emails/due"))
            .await
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[sqlx::test]
async fn moving_on_cancels_follow_ups_and_plans_a_thank_you(db: PgPool) {
    let (base, drafter) = common::spawn_drafting_app(db.clone(), &fake_ollama().await).await;
    setup(&db).await;
    sqlx::query("INSERT INTO tracker (job_id, stage, applied_at) VALUES ('j1', 'submitted', now() - interval '2 days')")
        .execute(&db)
        .await
        .unwrap();
    emails::plan(&db).await.unwrap();
    assert_eq!(
        statuses(&db, "j1").await.len(),
        2,
        "both follow-ups scheduled"
    );

    // Switching the second off cancels it.
    send(
        reqwest::Method::PUT,
        format!("{base}/api/settings"),
        json!({ "follow_up_second": false }),
    )
    .await;
    emails::plan(&db).await.unwrap();
    assert_eq!(
        statuses(&db, "j1").await[1],
        ("follow_up_2".into(), "cancelled".into())
    );

    // Interview: follow-ups stop, a thank-you is drafted.
    send(
        reqwest::Method::PUT,
        format!("{base}/api/tracker/j1"),
        json!({ "stage": "interview" }),
    )
    .await;
    assert_eq!(
        statuses(&db, "j1").await,
        vec![
            ("follow_up_1".into(), "cancelled".into()),
            ("follow_up_2".into(), "cancelled".into()),
            ("thank_you".into(), "queued".into())
        ]
    );
    assert!(matches!(
        drafter.process_next().await.unwrap(),
        Step::Drafted(_)
    ));
}

#[sqlx::test]
async fn an_application_email_on_request(db: PgPool) {
    let (base, drafter) = common::spawn_drafting_app(db.clone(), &fake_ollama().await).await;
    setup(&db).await;
    let r = send(
        reqwest::Method::POST,
        format!("{base}/api/jobs/j1/emails"),
        json!({ "kind": "application" }),
    )
    .await;
    assert_eq!(r.status(), 202);
    assert!(matches!(
        drafter.process_next().await.unwrap(),
        Step::Drafted(_)
    ));
    let page = get(format!("{base}/api/jobs/j1/emails")).await;
    assert_eq!(page["emails"][0]["kind"], "application");
    let id = page["emails"][0]["id"].as_str().unwrap().to_string();

    // Skipped, then drafted again; refused once sent.
    let skipped: Value = send(
        reqwest::Method::POST,
        format!("{base}/api/emails/{id}/skip"),
        json!({}),
    )
    .await
    .json()
    .await
    .unwrap();
    assert_eq!(skipped["status"], "cancelled");
    send(
        reqwest::Method::POST,
        format!("{base}/api/jobs/j1/emails"),
        json!({ "kind": "application" }),
    )
    .await;
    drafter.process_next().await.unwrap();
    send(
        reqwest::Method::POST,
        format!("{base}/api/emails/{id}/sent"),
        json!({}),
    )
    .await;
    let again = send(
        reqwest::Method::POST,
        format!("{base}/api/jobs/j1/emails"),
        json!({ "kind": "application" }),
    )
    .await;
    assert_eq!(again.status(), 409);

    // No address in j2's ad.
    assert!(get(format!("{base}/api/jobs/j2/emails")).await["address"].is_null());
    let missing = send(
        reqwest::Method::POST,
        format!("{base}/api/jobs/nope/emails"),
        json!({ "kind": "application" }),
    )
    .await;
    assert_eq!(missing.status(), 404);
}
