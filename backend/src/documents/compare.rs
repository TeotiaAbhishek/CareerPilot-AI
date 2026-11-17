//! Compare models on tailored resumes: the same jobs, drafted once by each
//! model you pick, with the checker's flags, length, time and cost side by
//! side. The drafts live in `draft_comparisons`, apart from your real
//! documents, so they're never approved or used to apply.

use std::sync::atomic::{AtomicBool, Ordering};

use axum::{
    Json,
    extract::{Path, State},
    http::{StatusCode, header},
    response::IntoResponse,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, types::Json as SqlJson};
use uuid::Uuid;

use super::{
    Kind,
    draft::ResumeDraft,
    html::resume_html,
    worker::{Failure, write_document},
};
use crate::{
    AppState,
    error::{AppError, AppResult},
    llm::LlmClient,
    settings,
};

const MAX_JOBS: usize = 5;
const MAX_MODELS: usize = 4;

/// One comparison runs at a time; new rows join the running loop.
static RUNNING: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Deserialize)]
pub struct CompareRequest {
    pub job_ids: Vec<String>,
    pub models: Vec<String>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct CompareJob {
    pub id: String,
    pub title: String,
    pub company: Option<String>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct Cell {
    pub id: Uuid,
    pub job_id: String,
    pub model: String,
    pub status: String,
    /// How many problems the checker found (claims your resume doesn't back up).
    pub flags: i32,
    pub words: Option<i32>,
    pub duration_ms: Option<i32>,
    pub cost_usd: Option<f64>,
    pub error: Option<String>,
}

/// One model's results over the whole batch.
#[derive(Debug, Serialize)]
pub struct ModelSummary {
    pub model: String,
    pub done: usize,
    pub failed: usize,
    pub avg_flags: Option<f64>,
    pub avg_words: Option<f64>,
    pub avg_seconds: Option<f64>,
    pub total_cost_usd: f64,
}

#[derive(Debug, Serialize)]
pub struct Batch {
    pub batch_id: Uuid,
    pub created_at: DateTime<Utc>,
    /// True while drafts are still queued or being written.
    pub running: bool,
    pub jobs: Vec<CompareJob>,
    pub models: Vec<String>,
    pub cells: Vec<Cell>,
    pub summary: Vec<ModelSummary>,
}

/// POST /api/compare/drafts: queue one resume per job and model, then draft
/// them in the background.
pub async fn start(
    State(state): State<AppState>,
    Json(body): Json<CompareRequest>,
) -> AppResult<(StatusCode, Json<Batch>)> {
    let mut job_ids = body.job_ids;
    job_ids.dedup();
    let mut models: Vec<String> = body
        .models
        .iter()
        .map(|m| m.trim().to_string())
        .filter(|m| !m.is_empty())
        .collect();
    models.dedup();
    if job_ids.is_empty() || job_ids.len() > MAX_JOBS {
        return Err(AppError::BadRequest(format!("Pick 1 to {MAX_JOBS} jobs.")));
    }
    if models.is_empty() || models.len() > MAX_MODELS {
        return Err(AppError::BadRequest(format!(
            "Pick 1 to {MAX_MODELS} models."
        )));
    }
    let found: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM jobs WHERE id = ANY($1) AND description_text IS NOT NULL",
    )
    .bind(&job_ids)
    .fetch_one(&state.db)
    .await?;
    if found != job_ids.len() as i64 {
        return Err(AppError::BadRequest(
            "Some of those jobs don't exist or have no description.".into(),
        ));
    }

    let batch_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO draft_comparisons (batch_id, job_id, model, position)
         SELECT $1, j, m, (jn * 100 + mn)::int
         FROM unnest($2::text[]) WITH ORDINALITY AS a(j, jn)
         CROSS JOIN unnest($3::text[]) WITH ORDINALITY AS b(m, mn)",
    )
    .bind(batch_id)
    .bind(&job_ids)
    .bind(&models)
    .execute(&state.db)
    .await?;

    spawn_runner(state.db.clone(), state.drafter.llm().clone());
    let batch = load_batch(&state.db, batch_id).await?;
    Ok((StatusCode::ACCEPTED, Json(batch)))
}

/// GET /api/compare/drafts: the latest comparison, or null.
pub async fn latest(State(state): State<AppState>) -> AppResult<Json<Option<Batch>>> {
    // After an API restart nothing is drafting what's left: pick it up again.
    if !RUNNING.load(Ordering::SeqCst) {
        sqlx::query("UPDATE draft_comparisons SET status = 'queued' WHERE status = 'drafting'")
            .execute(&state.db)
            .await?;
        let left: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM draft_comparisons WHERE status = 'queued')",
        )
        .fetch_one(&state.db)
        .await?;
        if left {
            spawn_runner(state.db.clone(), state.drafter.llm().clone());
        }
    }
    let batch_id: Option<Uuid> = sqlx::query_scalar(
        "SELECT batch_id FROM draft_comparisons ORDER BY created_at DESC LIMIT 1",
    )
    .fetch_optional(&state.db)
    .await?;
    Ok(Json(match batch_id {
        Some(id) => Some(load_batch(&state.db, id).await?),
        None => None,
    }))
}

/// GET /api/compare/drafts/{id}/html: one draft as a page, to read it.
pub async fn html(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<impl IntoResponse> {
    let content: Option<Option<SqlJson<ResumeDraft>>> =
        sqlx::query_scalar("SELECT content FROM draft_comparisons WHERE id = $1")
            .bind(id)
            .fetch_optional(&state.db)
            .await?;
    let Some(Some(SqlJson(draft))) = content else {
        return Err(AppError::NotFound("No draft with that id.".into()));
    };
    Ok((
        [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
        resume_html(&draft),
    ))
}

fn spawn_runner(db: PgPool, llm: LlmClient) {
    if RUNNING.swap(true, Ordering::SeqCst) {
        return;
    }
    tokio::spawn(async move {
        loop {
            match run_next(&db, &llm).await {
                Ok(true) => continue,
                Ok(false) => {}
                Err(error) => tracing::error!(?error, "model comparison failed"),
            }
            RUNNING.store(false, Ordering::SeqCst);
            // A row queued between the last check and the store above would
            // wait for the next comparison: check once more.
            let more: bool = sqlx::query_scalar(
                "SELECT EXISTS (SELECT 1 FROM draft_comparisons WHERE status = 'queued')",
            )
            .fetch_one(&db)
            .await
            .unwrap_or(false);
            if !more || RUNNING.swap(true, Ordering::SeqCst) {
                break;
            }
        }
    });
}

/// Draft the oldest queued comparison. False when there's nothing to do.
/// Public so tests can drive it.
pub async fn run_next(db: &PgPool, llm: &LlmClient) -> anyhow::Result<bool> {
    let task: Option<(Uuid, String, String)> = sqlx::query_as(
        "UPDATE draft_comparisons SET status = 'drafting'
         WHERE id = (SELECT id FROM draft_comparisons WHERE status = 'queued'
                     ORDER BY created_at, position LIMIT 1 FOR UPDATE SKIP LOCKED)
         RETURNING id, job_id, model",
    )
    .fetch_optional(db)
    .await?;
    let Some((id, job_id, model)) = task else {
        return Ok(false);
    };

    let think = settings::load(db).await?.drafting_think;
    match write_document(db, llm, &job_id, Kind::Resume, None, &model, think).await {
        Ok(written) => {
            sqlx::query(
                "UPDATE draft_comparisons
                 SET status = 'ready', content = $2, flags = $3, words = $4,
                     duration_ms = $5, cost_usd = $6::float8::numeric, model = $7
                 WHERE id = $1",
            )
            .bind(id)
            .bind(&written.content)
            .bind(SqlJson(&written.flags))
            .bind(written.words as i32)
            .bind(written.duration_ms)
            .bind(written.cost_usd)
            .bind(&written.model)
            .execute(db)
            .await?;
        }
        Err(Failure::Paused(message) | Failure::Document(message)) => {
            sqlx::query("UPDATE draft_comparisons SET status = 'failed', error = $2 WHERE id = $1")
                .bind(id)
                .bind(&message)
                .execute(db)
                .await?;
        }
    }
    Ok(true)
}

pub async fn load_batch(db: &PgPool, batch_id: Uuid) -> sqlx::Result<Batch> {
    let created_at: DateTime<Utc> =
        sqlx::query_scalar("SELECT min(created_at) FROM draft_comparisons WHERE batch_id = $1")
            .bind(batch_id)
            .fetch_one(db)
            .await?;
    let jobs: Vec<CompareJob> = sqlx::query_as(
        "SELECT j.id, j.title, j.company FROM jobs j
         JOIN (SELECT job_id, min(position) AS first
               FROM draft_comparisons WHERE batch_id = $1 GROUP BY job_id) c ON c.job_id = j.id
         ORDER BY c.first",
    )
    .bind(batch_id)
    .fetch_all(db)
    .await?;
    let cells: Vec<Cell> = sqlx::query_as(
        "SELECT id, job_id, model, status, jsonb_array_length(flags) AS flags, words,
                duration_ms, cost_usd::float8 AS cost_usd, error
         FROM draft_comparisons WHERE batch_id = $1
         ORDER BY position",
    )
    .bind(batch_id)
    .fetch_all(db)
    .await?;

    let mut models: Vec<String> = Vec::new();
    for cell in &cells {
        if !models.contains(&cell.model) {
            models.push(cell.model.clone());
        }
    }
    let summary = models
        .iter()
        .map(|model| summarise(model, cells.iter().filter(|c| &c.model == model)))
        .collect();
    let running = cells
        .iter()
        .any(|c| c.status == "queued" || c.status == "drafting");
    Ok(Batch {
        batch_id,
        created_at,
        running,
        jobs,
        models,
        cells,
        summary,
    })
}

fn summarise<'a>(model: &str, cells: impl Iterator<Item = &'a Cell>) -> ModelSummary {
    let cells: Vec<&Cell> = cells.collect();
    let ready: Vec<&&Cell> = cells.iter().filter(|c| c.status == "ready").collect();
    let avg = |values: Vec<f64>| {
        (!values.is_empty()).then(|| values.iter().sum::<f64>() / values.len() as f64)
    };
    ModelSummary {
        model: model.to_string(),
        done: ready.len(),
        failed: cells.iter().filter(|c| c.status == "failed").count(),
        avg_flags: avg(ready.iter().map(|c| c.flags as f64).collect()),
        avg_words: avg(ready
            .iter()
            .filter_map(|c| c.words)
            .map(f64::from)
            .collect()),
        avg_seconds: avg(ready
            .iter()
            .filter_map(|c| c.duration_ms)
            .map(|ms| ms as f64 / 1000.0)
            .collect()),
        total_cost_usd: ready.iter().filter_map(|c| c.cost_usd).sum(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cell(model: &str, status: &str, flags: i32, ms: i32, cost: Option<f64>) -> Cell {
        Cell {
            id: Uuid::new_v4(),
            job_id: "1".into(),
            model: model.into(),
            status: status.into(),
            flags,
            words: Some(300),
            duration_ms: Some(ms),
            cost_usd: cost,
            error: None,
        }
    }

    #[test]
    fn summaries_average_only_finished_drafts() {
        let cells = [
            cell("a", "ready", 2, 10_000, Some(0.05)),
            cell("a", "ready", 0, 20_000, Some(0.03)),
            cell("a", "failed", 9, 1, None),
            cell("b", "queued", 0, 0, None),
        ];
        let a = summarise("a", cells.iter().filter(|c| c.model == "a"));
        assert_eq!((a.done, a.failed), (2, 1));
        assert_eq!(a.avg_flags, Some(1.0));
        assert_eq!(a.avg_seconds, Some(15.0));
        assert!((a.total_cost_usd - 0.08).abs() < 1e-12);
        let b = summarise("b", cells.iter().filter(|c| c.model == "b"));
        assert_eq!((b.done, b.avg_flags), (0, None));
    }
}
