//! Scoring endpoints: worker status, Re-score, and your Apply/Maybe/No label.

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};

use crate::{
    AppState,
    error::{AppError, AppResult},
    scoring::{self, WorkerState, metrics::Label, prompt::PROMPT_VERSION},
    settings,
};

#[derive(Serialize)]
pub struct OllamaStatus {
    pub url: String,
    pub reachable: bool,
    /// Installed models, so the Settings page can offer them.
    pub models: Vec<String>,
    /// False when the selected model isn't installed yet.
    pub model_installed: bool,
    pub error: Option<String>,
}

#[derive(Serialize)]
pub struct Queue {
    /// Re-score and eval requests waiting.
    pub requested: i64,
    /// Recent jobs with no score yet for the active model (0 when automatic scoring is off).
    pub unscored: i64,
    /// Jobs whose last attempt failed in the past hour.
    pub failed_recently: i64,
}

#[derive(Serialize)]
pub struct Status {
    pub enabled: bool,
    pub model: String,
    pub prompt_version: &'static str,
    pub threshold: i32,
    pub ollama: OllamaStatus,
    pub queue: Queue,
    pub worker: WorkerState,
}

pub async fn status(State(state): State<AppState>) -> AppResult<Json<Status>> {
    let settings = settings::load(&state.db).await?;
    let ollama = state.scorer.ollama();

    let (reachable, models, error) = match ollama.models().await {
        Ok(models) => (true, models, None),
        Err(error) => (false, Vec::new(), Some(error.to_string())),
    };
    // "qwen3:14b" is also listed as "qwen3:14b"; "llama3" is listed as "llama3:latest".
    let model_installed = models.iter().any(|m| {
        m == &settings.scoring_model || m == &format!("{}:latest", settings.scoring_model)
    });

    let requested: i64 = sqlx::query_scalar("SELECT count(*) FROM score_requests")
        .fetch_one(&state.db)
        .await?;
    let unscored = if settings.scoring_enabled {
        scoring::unscored_count(&state.db, &settings.scoring_model).await?
    } else {
        0
    };
    let failed_recently: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM jobs WHERE score_failed_at > now() - interval '1 hour'",
    )
    .fetch_one(&state.db)
    .await?;

    Ok(Json(Status {
        enabled: settings.scoring_enabled,
        model: settings.scoring_model,
        prompt_version: PROMPT_VERSION,
        threshold: settings.relevance_threshold,
        ollama: OllamaStatus {
            url: ollama.base_url().to_string(),
            reachable,
            models,
            model_installed,
            error,
        },
        queue: Queue {
            requested,
            unscored,
            failed_recently,
        },
        worker: state.scorer.state(),
    }))
}

/// Queue the job for scoring again with the active model.
pub async fn rescore(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<StatusCode> {
    let settings = settings::load(&state.db).await?;
    let result = sqlx::query(
        "INSERT INTO score_requests (job_id, model)
         SELECT id, $2 FROM jobs WHERE id = $1
         ON CONFLICT DO NOTHING",
    )
    .bind(&id)
    .bind(&settings.scoring_model)
    .execute(&state.db)
    .await?;

    if result.rows_affected() == 0 {
        let exists: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM jobs WHERE id = $1)")
            .bind(&id)
            .fetch_one(&state.db)
            .await?;
        if !exists {
            return Err(AppError::NotFound("Job not found.".into()));
        }
    }
    state.scorer.wake();
    Ok(StatusCode::ACCEPTED)
}

#[derive(Debug, Deserialize, Serialize)]
pub struct LabelBody {
    /// "apply" | "maybe" | "no", or null to clear it.
    pub label: Option<String>,
}

pub async fn set_label(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<LabelBody>,
) -> AppResult<Json<LabelBody>> {
    let exists: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM jobs WHERE id = $1)")
        .bind(&id)
        .fetch_one(&state.db)
        .await?;
    if !exists {
        return Err(AppError::NotFound("Job not found.".into()));
    }

    match body.label.as_deref() {
        None => {
            sqlx::query("DELETE FROM job_labels WHERE job_id = $1")
                .bind(&id)
                .execute(&state.db)
                .await?;
        }
        Some(label) => {
            if Label::parse(label).is_none() {
                return Err(AppError::BadRequest(
                    "Label must be apply, maybe or no.".into(),
                ));
            }
            sqlx::query(
                "INSERT INTO job_labels (job_id, label) VALUES ($1, $2)
                 ON CONFLICT (job_id) DO UPDATE SET label = EXCLUDED.label, labelled_at = now()",
            )
            .bind(&id)
            .bind(label)
            .execute(&state.db)
            .await?;
        }
    }
    Ok(Json(body))
}
