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
    /// The Ollama model that writes tailored documents.
    pub drafting_model: String,
    /// Let the drafting model think before writing (slower, better).
    pub drafting_think: bool,
    /// How many applications the agent fills at once (one browser tab each).
    pub apply_parallel: i32,
    /// At most this many new applications in any 24 hours.
    pub apply_daily_cap: i32,
    /// The Ollama model for form questions code can't answer.
    pub apply_model: String,
    pub apply_think: bool,
    pub updated_at: DateTime<Utc>,
}

pub async fn load(db: &PgPool) -> sqlx::Result<Settings> {
    sqlx::query_as(
        "SELECT scoring_enabled, scoring_model, relevance_threshold, drafting_model, drafting_think,
                apply_parallel, apply_daily_cap, apply_model, apply_think, updated_at
         FROM settings",
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
    pub drafting_model: Option<String>,
    pub drafting_think: Option<bool>,
    pub apply_parallel: Option<i32>,
    pub apply_daily_cap: Option<i32>,
    pub apply_model: Option<String>,
    pub apply_think: Option<bool>,
}

pub async fn update(
    State(state): State<AppState>,
    Json(patch): Json<SettingsPatch>,
) -> AppResult<Json<Settings>> {
    let model = patch.scoring_model.map(|m| m.trim().to_string());
    let drafting_model = patch.drafting_model.map(|m| m.trim().to_string());
    let apply_model = patch.apply_model.map(|m| m.trim().to_string());
    if model.as_deref().is_some_and(str::is_empty)
        || drafting_model.as_deref().is_some_and(str::is_empty)
        || apply_model.as_deref().is_some_and(str::is_empty)
    {
        return Err(AppError::BadRequest("Model can't be empty.".into()));
    }
    if patch
        .relevance_threshold
        .is_some_and(|t| !(0..=100).contains(&t))
    {
        return Err(AppError::BadRequest("Threshold must be 0-100.".into()));
    }
    if patch.apply_parallel.is_some_and(|n| !(1..=5).contains(&n)) {
        return Err(AppError::BadRequest(
            "Applications at once must be 1-5.".into(),
        ));
    }
    if patch
        .apply_daily_cap
        .is_some_and(|n| !(1..=50).contains(&n))
    {
        return Err(AppError::BadRequest("The daily limit must be 1-50.".into()));
    }

    let settings = sqlx::query_as(
        "UPDATE settings SET
            scoring_enabled = coalesce($1, scoring_enabled),
            scoring_model = coalesce($2, scoring_model),
            relevance_threshold = coalesce($3, relevance_threshold),
            drafting_model = coalesce($4, drafting_model),
            drafting_think = coalesce($5, drafting_think),
            apply_parallel = coalesce($6, apply_parallel),
            apply_daily_cap = coalesce($7, apply_daily_cap),
            apply_model = coalesce($8, apply_model),
            apply_think = coalesce($9, apply_think),
            updated_at = now()
         RETURNING scoring_enabled, scoring_model, relevance_threshold, drafting_model,
                   drafting_think, apply_parallel, apply_daily_cap, apply_model, apply_think,
                   updated_at",
    )
    .bind(patch.scoring_enabled)
    .bind(model)
    .bind(patch.relevance_threshold)
    .bind(drafting_model)
    .bind(patch.drafting_think)
    .bind(patch.apply_parallel)
    .bind(patch.apply_daily_cap)
    .bind(apply_model)
    .bind(patch.apply_think)
    .fetch_one(&state.db)
    .await?;

    // A new model or switching scoring on means there may be work to do now.
    state.scorer.wake();
    state.drafter.wake();
    state.applier.wake();
    Ok(Json(settings))
}
