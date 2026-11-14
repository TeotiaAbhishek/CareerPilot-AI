//! Evals: how often each model agrees with your labels, compared with v1.

use std::collections::BTreeMap;

use axum::{Json, extract::State, http::StatusCode};
use serde::{Deserialize, Serialize};

use crate::{
    AppState,
    error::{AppError, AppResult},
    scoring::{
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
    /// "ollama" for local models, "v1" for the old OpenAI scores.
    pub source: &'static str,
    pub model: String,
    pub prompt_version: String,
    /// True for the model and prompt the app is using now.
    pub active: bool,
    pub metrics: Metrics,
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
    pub v1_score: Option<i16>,
}

#[derive(Serialize)]
pub struct EvalReport {
    pub threshold: i32,
    pub active_model: String,
    pub prompt_version: &'static str,
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
}

pub async fn report(State(state): State<AppState>) -> AppResult<Json<EvalReport>> {
    let settings = settings::load(&state.db).await?;
    let threshold = settings.relevance_threshold;

    let labels: LabelCounts = {
        let (apply, maybe, no): (i64, i64, i64) = sqlx::query_as(
            "SELECT count(*) FILTER (WHERE label = 'apply'),
                    count(*) FILTER (WHERE label = 'maybe'),
                    count(*) FILTER (WHERE label = 'no')
             FROM job_labels",
        )
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
                jsonb_array_length(s.hard_blockers) AS blockers
         FROM job_scores s JOIN job_labels l ON l.job_id = s.job_id
         ORDER BY s.job_id, s.model, s.prompt_version, s.created_at DESC",
    )
    .fetch_all(&state.db)
    .await?;

    let mut groups: BTreeMap<(String, String), Vec<Scored>> = BTreeMap::new();
    for row in scores {
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

    let mut rows: Vec<EvalRow> = groups
        .into_iter()
        .map(|((model, prompt_version), scored)| EvalRow {
            source: "ollama",
            active: model == settings.scoring_model && prompt_version == PROMPT_VERSION,
            metrics: metrics::compute(&scored, threshold),
            model,
            prompt_version,
        })
        .collect();

    // v1's OpenAI scores for the same jobs (imported from Supabase in F2).
    let v1: Vec<(String, i16, i32, String)> = sqlx::query_as(
        "SELECT l.label, a.relevance_score,
                coalesce(jsonb_array_length(a.analysis -> 'hard_blockers'), 0),
                a.model
         FROM legacy_job_analysis a JOIN job_labels l ON l.job_id = a.job_id",
    )
    .fetch_all(&state.db)
    .await?;
    if !v1.is_empty() {
        let model = v1[0].3.clone();
        let scored: Vec<Scored> = v1
            .into_iter()
            .filter_map(|(label, score, blockers, _)| {
                Label::parse(&label).map(|label| Scored {
                    label,
                    score: score.into(),
                    blockers,
                })
            })
            .collect();
        rows.push(EvalRow {
            source: "v1",
            model: format!("{model} (v1)"),
            prompt_version: "v1".into(),
            active: false,
            metrics: metrics::compute(&scored, threshold),
        });
    }

    let disagreements: Vec<Disagreement> = sqlx::query_as(
        "SELECT * FROM (
            SELECT DISTINCT ON (s.job_id)
                   j.id AS job_id, j.title, j.company, l.label, s.score,
                   jsonb_array_length(s.hard_blockers) AS blockers, s.summary,
                   a.relevance_score AS v1_score
            FROM job_scores s
            JOIN job_labels l ON l.job_id = s.job_id
            JOIN jobs j ON j.id = s.job_id
            LEFT JOIN legacy_job_analysis a ON a.job_id = s.job_id
            WHERE s.model = $1 AND s.prompt_version = $2 AND l.label <> 'maybe'
            ORDER BY s.job_id, s.created_at DESC
         ) latest
         WHERE (label = 'apply') <> (score >= $3 AND blockers = 0)
         ORDER BY abs(score - $3) DESC",
    )
    .bind(&settings.scoring_model)
    .bind(PROMPT_VERSION)
    .bind(threshold)
    .fetch_all(&state.db)
    .await?;

    Ok(Json(EvalReport {
        threshold,
        active_model: settings.scoring_model,
        prompt_version: PROMPT_VERSION,
        labels,
        rows,
        disagreements,
    }))
}

#[derive(Deserialize)]
pub struct RunBody {
    pub model: String,
}

#[derive(Serialize)]
pub struct RunResult {
    pub queued: u64,
}

/// Queue every labelled job that `model` hasn't scored with the current prompt.
pub async fn run(
    State(state): State<AppState>,
    Json(body): Json<RunBody>,
) -> AppResult<(StatusCode, Json<RunResult>)> {
    let model = body.model.trim();
    if model.is_empty() {
        return Err(AppError::BadRequest("Pick a model.".into()));
    }
    let result = sqlx::query(
        "INSERT INTO score_requests (job_id, model)
         SELECT l.job_id, $1 FROM job_labels l
         WHERE NOT EXISTS (SELECT 1 FROM job_scores s
                           WHERE s.job_id = l.job_id AND s.model = $1 AND s.prompt_version = $2)
         ON CONFLICT DO NOTHING",
    )
    .bind(model)
    .bind(PROMPT_VERSION)
    .execute(&state.db)
    .await?;

    state.scorer.wake();
    Ok((
        StatusCode::ACCEPTED,
        Json(RunResult {
            queued: result.rows_affected(),
        }),
    ))
}
