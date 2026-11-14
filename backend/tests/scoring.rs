//! Scoring, labels, settings and evals, against a fake Ollama.

mod common;

use axum::{Json, Router, routing::get, routing::post};
use careerpilot_api::scoring::Step;
use serde_json::{Value, json};
use sqlx::PgPool;

/// A fake Ollama. It answers based on the job title in the prompt:
/// "Analyst" -> 85, "Citizen" -> 80 with a hard blocker,
/// "Broken" -> text that isn't JSON, anything else -> 20.
async fn fake_ollama() -> String {
    async fn chat(Json(body): Json<Value>) -> Json<Value> {
        assert_eq!(body["stream"], false);
        assert_eq!(body["think"], false);
        assert!(body["format"].is_object(), "the schema must be sent");

        let prompt = body["messages"][1]["content"].as_str().unwrap();
        let title = prompt
            .lines()
            .find_map(|l| l.strip_prefix("Title: "))
            .unwrap();
        let answer = if title.contains("Broken") {
            "sorry, I can't do that".to_string()
        } else {
            let (score, blockers) = if title.contains("Analyst") {
                (85, json!([]))
            } else if title.contains("Citizen") {
                (80, json!(["Australian citizenship required"]))
            } else {
                (20, json!([]))
            };
            json!({
                "score": score,
                "summary": format!("Scored {title}."),
                "matched_requirements": ["SQL", " SQL ", "Python"],
                "missing_requirements": [],
                "unknown_requirements": [],
                "hard_blockers": blockers
            })
            .to_string()
        };
        Json(json!({ "message": { "role": "assistant", "content": answer } }))
    }

    let app = Router::new()
        .route(
            "/api/tags",
            get(|| async {
                Json(json!({ "models": [{ "name": "qwen3:14b" }, { "name": "qwen3:8b" }] }))
            }),
        )
        .route("/api/chat", post(chat));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    format!("http://{addr}")
}

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
async fn scores_new_jobs_and_shows_them(db: PgPool) {
    common::insert_job(&db, "1", "Data Analyst", "Full time", 1).await;
    common::insert_job(&db, "2", "Citizen Data Scientist", "Full time", 2).await;
    common::insert_job(&db, "3", "Barista", "Casual/Vacation", 3).await;
    common::insert_job(&db, "old", "Old Data Analyst", "Full time", 60).await;
    let ollama = fake_ollama().await;
    let (base, scorer) = common::spawn_scoring_app(db, &ollama).await;

    // Newest first, then nothing left (the 60-day-old job is skipped).
    assert_eq!(
        scorer.process_next().await.unwrap(),
        Step::Scored("1".into())
    );
    assert_eq!(
        scorer.process_next().await.unwrap(),
        Step::Scored("2".into())
    );
    assert_eq!(
        scorer.process_next().await.unwrap(),
        Step::Scored("3".into())
    );
    assert_eq!(scorer.process_next().await.unwrap(), Step::Idle);

    // Best fit: relevant first, the blocked job after, unscored last.
    let page = get_json(format!("{base}/api/jobs?sort=best_fit")).await;
    let ids: Vec<&str> = page["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|j| j["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["1", "3", "2", "old"]);
    assert_eq!(page["items"][0]["score"], 85);
    assert_eq!(page["items"][0]["relevant"], true);
    assert_eq!(page["items"][2]["relevant"], false); // hard blocker
    assert_eq!(page["items"][3]["score"], Value::Null);

    let page = get_json(format!("{base}/api/jobs?relevant_only=true")).await;
    assert_eq!(page["total"], 1);
    assert_eq!(page["items"][0]["id"], "1");

    // The detail has the explanation, cleaned up (duplicate "SQL" removed).
    let job = get_json(format!("{base}/api/jobs/1")).await;
    assert_eq!(job["score"]["score"], 85);
    assert_eq!(job["score"]["model"], "qwen3:14b");
    assert_eq!(job["score"]["matched"], json!(["SQL", "Python"]));
    assert_eq!(job["relevant"], true);
    assert_eq!(job["score_pending"], false);

    let stats = get_json(format!("{base}/api/stats")).await;
    assert_eq!(stats["jobs_scored"], 3);
    assert_eq!(stats["jobs_relevant"], 1);

    // Lowering the threshold makes the barista job relevant too.
    let response = send(
        reqwest::Method::PUT,
        format!("{base}/api/settings"),
        json!({ "relevance_threshold": 10 }),
    )
    .await;
    assert_eq!(response.status(), 200);
    let stats = get_json(format!("{base}/api/stats")).await;
    assert_eq!(stats["jobs_relevant"], 2);
}

#[sqlx::test]
async fn a_bad_answer_fails_only_that_job(db: PgPool) {
    common::insert_job(&db, "1", "Broken Job", "Full time", 1).await;
    common::insert_job(&db, "2", "Data Analyst", "Full time", 2).await;
    let ollama = fake_ollama().await;
    let (base, scorer) = common::spawn_scoring_app(db, &ollama).await;

    assert_eq!(
        scorer.process_next().await.unwrap(),
        Step::Failed("1".into())
    );
    // The failed job is not retried straight away; the next one is scored.
    assert_eq!(
        scorer.process_next().await.unwrap(),
        Step::Scored("2".into())
    );
    assert_eq!(scorer.process_next().await.unwrap(), Step::Idle);

    let job = get_json(format!("{base}/api/jobs/1")).await;
    assert!(job["score_error"].as_str().unwrap().contains("not JSON"));
    let status = get_json(format!("{base}/api/scoring/status")).await;
    assert_eq!(status["queue"]["failed_recently"], 1);
}

#[sqlx::test]
async fn pauses_without_failing_jobs_when_ollama_is_down(db: PgPool) {
    common::insert_job(&db, "1", "Data Analyst", "Full time", 1).await;
    let (base, scorer) = common::spawn_scoring_app(db, common::NOWHERE).await;

    assert!(matches!(
        scorer.process_next().await.unwrap(),
        Step::Paused(_)
    ));

    let job = get_json(format!("{base}/api/jobs/1")).await;
    assert_eq!(job["score_error"], Value::Null);

    let status = get_json(format!("{base}/api/scoring/status")).await;
    assert_eq!(status["ollama"]["reachable"], false);
    assert_eq!(status["queue"]["unscored"], 1);
    assert!(
        status["worker"]["paused_reason"]
            .as_str()
            .unwrap()
            .contains("not reachable")
    );
}

#[sqlx::test]
async fn rescore_and_disabled_scoring(db: PgPool) {
    common::insert_job(&db, "1", "Data Analyst", "Full time", 1).await;
    let ollama = fake_ollama().await;
    let (base, scorer) = common::spawn_scoring_app(db, &ollama).await;

    // Automatic scoring off: nothing happens by itself...
    send(
        reqwest::Method::PUT,
        format!("{base}/api/settings"),
        json!({ "scoring_enabled": false }),
    )
    .await;
    assert_eq!(scorer.process_next().await.unwrap(), Step::Idle);

    // ...but Re-score still works.
    let response = send(
        reqwest::Method::POST,
        format!("{base}/api/jobs/1/rescore"),
        json!({}),
    )
    .await;
    assert_eq!(response.status(), 202);
    let job = get_json(format!("{base}/api/jobs/1")).await;
    assert_eq!(job["score_pending"], true);

    assert_eq!(
        scorer.process_next().await.unwrap(),
        Step::Scored("1".into())
    );
    let job = get_json(format!("{base}/api/jobs/1")).await;
    assert_eq!(job["score_pending"], false);
    assert_eq!(job["score"]["score"], 85);

    let response = send(
        reqwest::Method::POST,
        format!("{base}/api/jobs/nope/rescore"),
        json!({}),
    )
    .await;
    assert_eq!(response.status(), 404);

    let status = get_json(format!("{base}/api/scoring/status")).await;
    assert_eq!(status["enabled"], false);
    assert_eq!(status["ollama"]["reachable"], true);
    assert_eq!(status["ollama"]["model_installed"], true);
}

#[sqlx::test]
async fn settings_are_validated(db: PgPool) {
    let (base, _) = common::spawn_scoring_app(db, common::NOWHERE).await;

    for bad in [
        json!({ "relevance_threshold": 101 }),
        json!({ "scoring_model": "  " }),
    ] {
        let response = send(reqwest::Method::PUT, format!("{base}/api/settings"), bad).await;
        assert_eq!(response.status(), 400);
    }

    let settings = get_json(format!("{base}/api/settings")).await;
    assert_eq!(settings["scoring_model"], "qwen3:14b");
    assert_eq!(settings["relevance_threshold"], 60);
}

#[sqlx::test]
async fn labels_and_evals(db: PgPool) {
    common::insert_job(&db, "1", "Data Analyst", "Full time", 1).await;
    common::insert_job(&db, "2", "Barista", "Casual/Vacation", 2).await;
    common::insert_job(&db, "3", "Citizen Role", "Full time", 3).await;
    // v1 thought the barista job was great.
    sqlx::query(
        "INSERT INTO legacy_job_analysis (job_id, relevance_score, kept, analysis, model, analysed_at)
         VALUES ('2', 90, true, '{\"hard_blockers\": []}', 'gpt-4.1-mini', now())",
    )
    .execute(&db)
    .await
    .unwrap();
    let ollama = fake_ollama().await;
    let (base, scorer) = common::spawn_scoring_app(db, &ollama).await;

    // Label: apply to the analyst job, not the barista one; maybe for the citizen one.
    for (id, label) in [("1", "apply"), ("2", "no"), ("3", "maybe")] {
        let response = send(
            reqwest::Method::PUT,
            format!("{base}/api/jobs/{id}/label"),
            json!({ "label": label }),
        )
        .await;
        assert_eq!(response.status(), 200);
    }
    let response = send(
        reqwest::Method::PUT,
        format!("{base}/api/jobs/1/label"),
        json!({ "label": "love it" }),
    )
    .await;
    assert_eq!(response.status(), 400);

    let job = get_json(format!("{base}/api/jobs/2")).await;
    assert_eq!(job["label"], "no");

    // Run an eval with a second model: all 3 labelled jobs are queued for it.
    let response = send(
        reqwest::Method::POST,
        format!("{base}/api/evals/run"),
        json!({ "model": "qwen3:8b" }),
    )
    .await;
    assert_eq!(response.status(), 202);
    let body: Value = response.json().await.unwrap();
    assert_eq!(body["queued"], 3);

    // 3 requested (qwen3:8b) + 3 automatic (qwen3:14b), then idle.
    for _ in 0..6 {
        assert!(matches!(
            scorer.process_next().await.unwrap(),
            Step::Scored(_)
        ));
    }
    assert_eq!(scorer.process_next().await.unwrap(), Step::Idle);

    let report = get_json(format!("{base}/api/evals")).await;
    assert_eq!(report["labels"]["total"], 3);
    let rows = report["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 3); // qwen3:14b, qwen3:8b, v1

    let active = rows.iter().find(|r| r["active"] == true).unwrap();
    assert_eq!(active["model"], "qwen3:14b");
    assert_eq!(active["metrics"]["agreement"], 1.0);
    assert_eq!(active["metrics"]["mean_score_maybe"], 80.0);

    let v1 = rows.iter().find(|r| r["source"] == "v1").unwrap();
    assert_eq!(v1["metrics"]["false_positive"], 1);

    // The fake model agrees with every apply/no label.
    assert_eq!(report["disagreements"].as_array().unwrap().len(), 0);

    // Clearing a label removes it.
    send(
        reqwest::Method::PUT,
        format!("{base}/api/jobs/3/label"),
        json!({ "label": null }),
    )
    .await;
    let report = get_json(format!("{base}/api/evals")).await;
    assert_eq!(report["labels"]["total"], 2);
}
