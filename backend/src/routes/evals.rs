//! Evals: how often each model agrees with your labels, on jobs from the
//! last 7 days (the same window automatic scoring uses). Older jobs and the
//! jobs imported from v1 are left out.

use std::collections::BTreeMap;

use axum::{Json, extract::State, http::StatusCode};
use serde::{Deserialize, Serialize};

use crate::{
    AppState,
    error::{AppError, AppResult},
    llm::provider_of,
    scoring::{
        AUTO_SCORE_WINDOW,
        metrics::{self, Label, Metrics, Scored},
        prompt::PROMPT_VERSION,
    },
    settings,
};

#[derive(Serialize)]
pub struct LabelCounts {
    pub apply: i64,
    pub maybe: i64,
    pub no: i64,
    pub total: i64,
}

#[derive(Serialize)]
pub struct EvalRow {
    /// "ollama", "claude" or "openai".
    pub source: &'static str,
    pub model: String,
    pub prompt_version: String,
    /// True for the model and prompt the app is using now.
    pub active: bool,
    pub metrics: Metrics,
    /// Average cost of one score in US dollars (None for local models).
    pub avg_cost_usd: Option<f64>,
    /// Average seconds per score.
    pub avg_seconds: Option<f64>,
    /// Scores the seniority cap brought down.
    pub capped: usize,
}

/// A labelled job where the active model disagrees with you.
#[derive(Serialize, sqlx::FromRow)]
pub struct Disagreement {
    pub job_id: String,
    pub title: String,
    pub company: Option<String>,
    pub label: String,
    pub score: i16,
    pub blockers: i32,
    pub summary: String,
}

#[derive(Serialize)]
pub struct EvalReport {
    pub threshold: i32,
    pub active_model: String,
    pub prompt_version: &'static str,
    /// Only labelled jobs listed this recently count, e.g. "7 days".
    pub window: &'static str,
    pub labels: LabelCounts,
    pub rows: Vec<EvalRow>,
    pub disagreements: Vec<Disagreement>,
}

#[derive(sqlx::FromRow)]
struct ScoreRow {
    model: String,
    prompt_version: String,
    label: String,
    score: i16,
    blockers: i32,
    cost_usd: Option<f64>,
    duration_ms: Option<i32>,
    capped: bool,
}

/// Cost, time and caps for one model's scores.
#[derive(Default)]
struct Extras {
    costs: Vec<f64>,
    seconds: Vec<f64>,
    capped: usize,
}

fn average(values: &[f64]) -> Option<f64> {
    (!values.is_empty()).then(|| values.iter().sum::<f64>() / values.len() as f64)
}

pub async fn report(State(state): State<AppState>) -> AppResult<Json<EvalReport>> {
    let settings = settings::load(&state.db).await?;
    let threshold = settings.relevance_threshold;

    let labels: LabelCounts = {
        let (apply, maybe, no): (i64, i64, i64) = sqlx::query_as(
            "SELECT count(*) FILTER (WHERE label = 'apply'),
                    count(*) FILTER (WHERE label = 'maybe'),
                    count(*) FILTER (WHERE label = 'no')
             FROM job_labels l JOIN jobs j ON j.id = l.job_id
             WHERE coalesce(j.listed_at, j.first_seen_at) > now() - $1::interval",
        )
        .bind(AUTO_SCORE_WINDOW)
        .fetch_one(&state.db)
        .await?;
        LabelCounts {
            apply,
            maybe,
            no,
            total: apply + maybe + no,
        }
    };

    // The latest score of every labelled job, per model and prompt version.
    let scores: Vec<ScoreRow> = sqlx::query_as(
        "SELECT DISTINCT ON (s.job_id, s.model, s.prompt_version)
                s.model, s.prompt_version, l.label, s.score,
                jsonb_array_length(s.hard_blockers) AS blockers,
                s.cost_usd::float8 AS cost_usd, s.duration_ms,
                s.seniority_note IS NOT NULL AS capped
         FROM job_scores s
         JOIN job_labels l ON l.job_id = s.job_id
         JOIN jobs j ON j.id = s.job_id
         WHERE coalesce(j.listed_at, j.first_seen_at) > now() - $1::interval
         ORDER BY s.job_id, s.model, s.prompt_version, s.created_at DESC",
    )
    .bind(AUTO_SCORE_WINDOW)
    .fetch_all(&state.db)
    .await?;

    let mut groups: BTreeMap<(String, String), Vec<Scored>> = BTreeMap::new();
    let mut extras: BTreeMap<(String, String), Extras> = BTreeMap::new();
    for row in scores {
        let extra = extras
            .entry((row.model.clone(), row.prompt_version.clone()))
            .or_default();
        extra.costs.extend(row.cost_usd);
        extra
            .seconds
            .extend(row.duration_ms.map(|ms| ms as f64 / 1000.0));
        extra.capped += usize::from(row.capped);
        if let Some(label) = Label::parse(&row.label) {
            groups
                .entry((row.model, row.prompt_version))
                .or_default()
                .push(Scored {
                    label,
                    score: row.score.into(),
                    blockers: row.blockers,
                });
        }
    }

    let rows: Vec<EvalRow> = groups
        .into_iter()
        .map(|((model, prompt_version), scored)| {
            let extra = extras
                .remove(&(model.clone(), prompt_version.clone()))
                .unwrap_or_default();
            EvalRow {
                source: provider_of(&model).name(),
                active: model == settings.scoring_model && prompt_version == PROMPT_VERSION,
                metrics: metrics::compute(&scored, threshold),
                avg_cost_usd: average(&extra.costs),
                avg_seconds: average(&extra.seconds),
                capped: extra.capped,
                model,
                prompt_version,
            }
        })
        .collect();

    let disagreements: Vec<Disagreement> = sqlx::query_as(
        "SELECT * FROM (
            SELECT DISTINCT ON (s.job_id)
                   j.id AS job_id, j.title, j.company, l.label, s.score,
                   jsonb_array_length(s.hard_blockers) AS blockers, s.summary
            FROM job_scores s
            JOIN job_labels l ON l.job_id = s.job_id
            JOIN jobs j ON j.id = s.job_id
            WHERE s.model = $1 AND s.prompt_version = $2 AND l.label <> 'maybe'
              AND coalesce(j.listed_at, j.first_seen_at) > now() - $4::interval
            ORDER BY s.job_id, s.created_at DESC
         ) latest
         WHERE (label = 'apply') <> (score >= $3 AND blockers = 0)
         ORDER BY abs(score - $3) DESC",
    )
    .bind(&settings.scoring_model)
    .bind(PROMPT_VERSION)
    .bind(threshold)
    .bind(AUTO_SCORE_WINDOW)
    .fetch_all(&state.db)
    .await?;

    Ok(Json(EvalReport {
        threshold,
        active_model: settings.scoring_model,
        prompt_version: PROMPT_VERSION,
        window: AUTO_SCORE_WINDOW,
        labels,
        rows,
        disagreements,
    }))
}

/// One model, or several at once ("models"), e.g. local, Claude and OpenAI.
#[derive(Deserialize)]
pub struct RunBody {
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub models: Vec<String>,
}

#[derive(Serialize)]
pub struct RunResult {
    pub queued: u64,
}

/// Queue every recent labelled job that each model hasn't scored with the
/// current prompt.
pub async fn run(
    State(state): State<AppState>,
    Json(body): Json<RunBody>,
) -> AppResult<(StatusCode, Json<RunResult>)> {
    let mut models: Vec<String> = body
        .models
        .iter()
        .chain(std::iter::once(&body.model))
        .map(|m| m.trim().to_string())
        .filter(|m| !m.is_empty())
        .collect();
    models.sort();
    models.dedup();
    if models.is_empty() {
        return Err(AppError::BadRequest("Pick a model.".into()));
    }
    if models.len() > 4 {
        return Err(AppError::BadRequest("At most 4 models at once.".into()));
    }
    let mut queued = 0;
    for model in &models {
        let result = sqlx::query(
            "INSERT INTO score_requests (job_id, model)
             SELECT l.job_id, $1 FROM job_labels l JOIN jobs j ON j.id = l.job_id
             WHERE coalesce(j.listed_at, j.first_seen_at) > now() - $3::interval
               AND NOT EXISTS (SELECT 1 FROM job_scores s
                               WHERE s.job_id = l.job_id AND s.model = $1 AND s.prompt_version = $2)
             ON CONFLICT DO NOTHING",
        )
        .bind(model)
        .bind(PROMPT_VERSION)
        .bind(AUTO_SCORE_WINDOW)
        .execute(&state.db)
        .await?;
        queued += result.rows_affected();
    }

    state.scorer.wake();
    Ok((StatusCode::ACCEPTED, Json(RunResult { queued })))
}
