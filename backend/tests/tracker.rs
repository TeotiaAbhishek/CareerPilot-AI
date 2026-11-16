//! F7: one-click Apply, bulk apply, the board and insights.

mod common;

use serde_json::{Value, json};
use sqlx::PgPool;

async fn send(method: reqwest::Method, url: String, body: Value) -> reqwest::Response {
    reqwest::Client::new()
        .request(method, url)
        .json(&body)
        .send()
        .await
        .unwrap()
}

async fn job(db: &PgPool, id: &str) {
    sqlx::query(
        "INSERT INTO jobs (id, title, company, description_text, job_url, apply_type)
         VALUES ($1, 'Data Analyst', 'Acme', 'SQL role', 'https://example.com/job', 'Quick apply')",
    )
    .bind(id)
    .execute(db)
    .await
    .unwrap();
}

/// A resume draft with `flags` (an empty list means the checker found nothing).
async fn resume(db: &PgPool, job_id: &str, status: &str, flags: Value) {
    let content = json!({
        "name": "Test Person", "contact": [],
        "summary": { "text": "Data analyst.", "evidence": ["S"] },
        "skills": [], "experience": [], "projects": [], "education": [], "other": []
    });
    sqlx::query(
        "INSERT INTO documents (job_id, kind, status, content, flags) VALUES ($1, 'resume', $2, $3, $4)",
    )
    .bind(job_id)
    .bind(status)
    .bind(content)
    .bind(flags)
    .execute(db)
    .await
    .unwrap();
}

async fn card(base: &str, job_id: &str) -> Value {
    reqwest::get(format!("{base}/api/tracker/{job_id}"))
        .await
        .unwrap()
        .json()
        .await
        .unwrap()
}

#[sqlx::test]
async fn one_click_drafts_approves_clean_drafts_and_applies(db: PgPool) {
    let base = common::spawn_app(db.clone(), common::NOWHERE).await;
    for id in ["j1", "j2", "j3"] {
        job(&db, id).await;
    }
    // j1: no resume yet. j2: a clean draft. j3: a draft with a flagged line.
    resume(&db, "j2", "ready", json!([])).await;
    resume(
        &db,
        "j3",
        "ready",
        json!([{ "key": "k1", "path": "summary", "severity": "error", "message": "Not in your resume" }]),
    )
    .await;

    let one = |id: &str| {
        send(
            reqwest::Method::POST,
            format!("{base}/api/jobs/{id}/one-click"),
            json!({}),
        )
    };

    let r: Value = one("j1").await.json().await.unwrap();
    assert_eq!(r["started"], "drafting");
    let queued: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM documents WHERE job_id = 'j1' AND status = 'queued'",
    )
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(queued, 1, "a resume draft was queued");
    let c = card(&base, "j1").await;
    assert_eq!(c["column"], "applying");
    assert_eq!(c["detail"], "Drafting your resume");

    let r: Value = one("j2").await.json().await.unwrap();
    assert_eq!(
        r["started"], "applying",
        "a clean draft is approved and the agent starts"
    );
    let (doc, run): (String, String) = sqlx::query_as(
        "SELECT d.status, a.status FROM documents d JOIN applications a ON a.job_id = d.job_id
         WHERE d.job_id = 'j2'",
    )
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!((doc.as_str(), run.as_str()), ("approved", "queued"));
    assert_eq!(card(&base, "j2").await["column"], "applying");

    let r: Value = one("j3").await.json().await.unwrap();
    assert_eq!(
        r["started"], "waiting_for_you",
        "flagged drafts wait for you"
    );
    let c = card(&base, "j3").await;
    assert_eq!(c["column"], "your_turn");
    assert_eq!(c["detail"], "Review the resume: 1 line to check");

    // Applying twice is refused while the run is going.
    assert_eq!(one("j2").await.status(), 409);

    // With auto-approve off, even a clean draft waits for you.
    job(&db, "j4").await;
    resume(&db, "j4", "ready", json!([])).await;
    sqlx::query("UPDATE settings SET auto_approve = FALSE")
        .execute(&db)
        .await
        .unwrap();
    let r: Value = one("j4").await.json().await.unwrap();
    assert_eq!(r["started"], "waiting_for_you");
}

#[sqlx::test]
async fn bulk_apply_skips_what_it_cant_start(db: PgPool) {
    let base = common::spawn_app(db.clone(), common::NOWHERE).await;
    job(&db, "j1").await;
    // j2 was applied for and rejected: no second application.
    job(&db, "j2").await;
    send(
        reqwest::Method::PUT,
        format!("{base}/api/tracker/j2"),
        json!({ "stage": "rejected" }),
    )
    .await;
    let r: Value = send(
        reqwest::Method::POST,
        format!("{base}/api/apply/bulk"),
        json!({ "job_ids": ["j1", "j1", "missing", "j2"] }),
    )
    .await
    .json()
    .await
    .unwrap();
    assert_eq!(r["started"], 1, "duplicates count once");
    assert_eq!(r["skipped"][0]["job_id"], "missing");
    assert_eq!(r["skipped"][1]["job_id"], "j2");
    assert_eq!(
        r["skipped"][1]["reason"],
        "You've already applied for this job."
    );

    let empty = send(
        reqwest::Method::POST,
        format!("{base}/api/apply/bulk"),
        json!({ "job_ids": [] }),
    )
    .await;
    assert_eq!(empty.status(), 400);
}

#[sqlx::test]
async fn moving_cards_feeds_the_insights(db: PgPool) {
    let base = common::spawn_app(db.clone(), common::NOWHERE).await;
    for id in ["j1", "j2"] {
        job(&db, id).await;
    }
    let mv = |id: &str, stage: &str| {
        send(
            reqwest::Method::PUT,
            format!("{base}/api/tracker/{id}"),
            json!({ "stage": stage }),
        )
    };
    // Only after-Submit stages can be set by hand.
    assert_eq!(mv("j1", "active").await.status(), 400);

    let c: Value = mv("j1", "submitted").await.json().await.unwrap();
    assert_eq!(c["column"], "submitted");
    assert!(c["applied_at"].is_string());
    assert!(c["replied_at"].is_null());
    let c: Value = mv("j1", "interview").await.json().await.unwrap();
    assert!(c["replied_at"].is_string(), "the first reply is recorded");
    mv("j2", "submitted").await;

    let i: Value = reqwest::get(format!("{base}/api/insights"))
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(i["funnel"]["applied"], 2);
    assert_eq!(i["funnel"]["replied"], 1);
    assert_eq!(i["funnel"]["interview"], 1);
    assert_eq!(i["response_rate"], 0.5);
    assert_eq!(i["per_day"].as_array().unwrap().len(), 60);
    assert_eq!(i["per_day"][59]["count"], 2, "both applied today");
    assert_eq!(i["per_week"].as_array().unwrap().len(), 12);
    assert_eq!(i["by_apply_type"][0]["label"], "Quick Apply");
    assert_eq!(i["by_apply_type"][0]["interviews"], 1);

    // Off the board; the job itself stays.
    let gone = send(
        reqwest::Method::DELETE,
        format!("{base}/api/tracker/j2"),
        json!({}),
    )
    .await;
    assert_eq!(gone.status(), 204);
    let board: Value = reqwest::get(format!("{base}/api/tracker"))
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(board.as_array().unwrap().len(), 1);
}
