//! App-wide settings (one row), edited on the Settings page.

use axum::{Json, extract::State};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

use crate::{
    AppState,
    error::{AppError, AppResult},
};

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Settings {
    /// Score new jobs automatically. Re-score and eval runs work either way.
    pub scoring_enabled: bool,
    /// The Ollama model that scores jobs, e.g. "qwen3:14b".
    pub scoring_model: String,
    /// A job is relevant when its score is at least this and it has no hard blockers.
    pub relevance_threshold: i32,
    pub updated_at: DateTime<Utc>,
}

pub async fn load(db: &PgPool) -> sqlx::Result<Settings> {
    sqlx::query_as(
        "SELECT scoring_enabled, scoring_model, relevance_threshold, updated_at FROM settings",
    )
    .fetch_one(db)
    .await
}

pub async fn get(State(state): State<AppState>) -> AppResult<Json<Settings>> {
    Ok(Json(load(&state.db).await?))
}

/// Only the fields you send are changed.
#[derive(Debug, Deserialize)]
pub struct SettingsPatch {
    pub scoring_enabled: Option<bool>,
    pub scoring_model: Option<String>,
    pub relevance_threshold: Option<i32>,
}

pub async fn update(
    State(state): State<AppState>,
    Json(patch): Json<SettingsPatch>,
) -> AppResult<Json<Settings>> {
    let model = patch.scoring_model.map(|m| m.trim().to_string());
    if model.as_deref().is_some_and(str::is_empty) {
        return Err(AppError::BadRequest("Model can't be empty.".into()));
    }
    if patch
        .relevance_threshold
        .is_some_and(|t| !(0..=100).contains(&t))
    {
        return Err(AppError::BadRequest("Threshold must be 0-100.".into()));
    }

    let settings = sqlx::query_as(
        "UPDATE settings SET
            scoring_enabled = coalesce($1, scoring_enabled),
            scoring_model = coalesce($2, scoring_model),
            relevance_threshold = coalesce($3, relevance_threshold),
            updated_at = now()
         RETURNING scoring_enabled, scoring_model, relevance_threshold, updated_at",
    )
    .bind(patch.scoring_enabled)
    .bind(model)
    .bind(patch.relevance_threshold)
    .fetch_one(&state.db)
    .await?;

    // A new model or switching scoring on means there may be work to do now.
    state.scorer.wake();
    Ok(Json(settings))
}
