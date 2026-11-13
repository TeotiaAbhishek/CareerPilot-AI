//! Saved SEEK searches: list, create, update, delete.

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    AppState,
    error::{AppError, AppResult, is_unique_violation},
};

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Search {
    pub id: Uuid,
    pub keywords: String,
    pub location: String,
    pub max_pages: i32,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct SearchInput {
    pub keywords: String,
    pub location: String,
    #[serde(default = "default_pages")]
    pub max_pages: i32,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

fn default_pages() -> i32 {
    3
}
fn default_enabled() -> bool {
    true
}

impl SearchInput {
    /// Trim and check the input. Returns a clean copy or a 400 with a readable reason.
    fn validated(self) -> AppResult<Self> {
        let keywords = self.keywords.trim().to_string();
        let location = self.location.trim().to_string();
        if keywords.is_empty() {
            return Err(AppError::BadRequest("Keywords can't be empty.".into()));
        }
        if location.is_empty() {
            return Err(AppError::BadRequest("Location can't be empty.".into()));
        }
        if !(1..=10).contains(&self.max_pages) {
            return Err(AppError::BadRequest(
                "Pages must be between 1 and 10.".into(),
            ));
        }
        Ok(Self {
            keywords,
            location,
            ..self
        })
    }
}

fn duplicate(error: sqlx::Error) -> AppError {
    if is_unique_violation(&error) {
        AppError::Conflict("You already have a search with these keywords and location.".into())
    } else {
        error.into()
    }
}

pub async fn list(State(state): State<AppState>) -> AppResult<Json<Vec<Search>>> {
    let searches = sqlx::query_as("SELECT * FROM searches ORDER BY created_at")
        .fetch_all(&state.db)
        .await?;
    Ok(Json(searches))
}

pub async fn create(
    State(state): State<AppState>,
    Json(input): Json<SearchInput>,
) -> AppResult<(StatusCode, Json<Search>)> {
    let input = input.validated()?;
    let search = sqlx::query_as(
        "INSERT INTO searches (keywords, location, max_pages, enabled)
         VALUES ($1, $2, $3, $4) RETURNING *",
    )
    .bind(&input.keywords)
    .bind(&input.location)
    .bind(input.max_pages)
    .bind(input.enabled)
    .fetch_one(&state.db)
    .await
    .map_err(duplicate)?;
    Ok((StatusCode::CREATED, Json(search)))
}

pub async fn update(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(input): Json<SearchInput>,
) -> AppResult<Json<Search>> {
    let input = input.validated()?;
    sqlx::query_as(
        "UPDATE searches
         SET keywords = $2, location = $3, max_pages = $4, enabled = $5, updated_at = now()
         WHERE id = $1 RETURNING *",
    )
    .bind(id)
    .bind(&input.keywords)
    .bind(&input.location)
    .bind(input.max_pages)
    .bind(input.enabled)
    .fetch_optional(&state.db)
    .await
    .map_err(duplicate)?
    .map(Json)
    .ok_or_else(|| AppError::NotFound("Search not found.".into()))
}

pub async fn delete(State(state): State<AppState>, Path(id): Path<Uuid>) -> AppResult<StatusCode> {
    let result = sqlx::query("DELETE FROM searches WHERE id = $1")
        .bind(id)
        .execute(&state.db)
        .await?;
    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("Search not found.".into()));
    }
    Ok(StatusCode::NO_CONTENT)
}
