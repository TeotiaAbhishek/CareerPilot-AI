//! The answer bank: your answers to screening questions ("Do you have a
//! driver's licence?"). In F6 the form-fill agent searches it, and saves the
//! answers you give it, so you're never asked the same thing twice.

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{Postgres, QueryBuilder};
use uuid::Uuid;

use crate::{
    AppState,
    error::{AppError, AppResult},
};

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct Answer {
    pub id: Uuid,
    pub question: String,
    pub answer: String,
    pub tags: Vec<String>,
    pub source: String,
    pub times_used: i32,
    pub last_used_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct AnswerInput {
    pub question: String,
    pub answer: String,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct ListParams {
    /// Words to look for in questions and answers.
    pub q: Option<String>,
}

pub async fn list(
    State(state): State<AppState>,
    Query(params): Query<ListParams>,
) -> AppResult<Json<Vec<Answer>>> {
    let mut qb: QueryBuilder<Postgres> = QueryBuilder::new(
        "SELECT id, question, answer, tags, source, times_used, last_used_at, created_at,
                updated_at FROM answers",
    );
    match params.q.as_deref().map(str::trim).filter(|q| !q.is_empty()) {
        Some(q) => {
            qb.push(" WHERE search_vector @@ websearch_to_tsquery('english', ")
                .push_bind(q.to_string())
                .push(") ORDER BY ts_rank(search_vector, websearch_to_tsquery('english', ")
                .push_bind(q.to_string())
                .push(")) DESC");
        }
        None => {
            qb.push(" ORDER BY updated_at DESC");
        }
    }
    Ok(Json(qb.build_query_as().fetch_all(&state.db).await?))
}

fn clean(input: AnswerInput) -> AppResult<AnswerInput> {
    let question = input.question.trim().to_string();
    let answer = input.answer.trim().to_string();
    if question.is_empty() || answer.is_empty() {
        return Err(AppError::BadRequest(
            "Both the question and the answer are needed.".into(),
        ));
    }
    if question.chars().count() > 500 || answer.chars().count() > 5000 {
        return Err(AppError::BadRequest("That's too long.".into()));
    }
    let mut tags: Vec<String> = Vec::new();
    for tag in input.tags {
        let tag = tag.trim().to_lowercase();
        if !tag.is_empty() && !tags.contains(&tag) {
            tags.push(tag);
        }
    }
    Ok(AnswerInput {
        question,
        answer,
        tags,
    })
}

const RETURNING: &str = " RETURNING id, question, answer, tags, source, times_used, last_used_at,
    created_at, updated_at";

pub async fn create(
    State(state): State<AppState>,
    Json(input): Json<AnswerInput>,
) -> AppResult<(StatusCode, Json<Answer>)> {
    let a = clean(input)?;
    let mut qb: QueryBuilder<Postgres> =
        QueryBuilder::new("INSERT INTO answers (question, answer, tags) VALUES (");
    qb.push_bind(a.question)
        .push(", ")
        .push_bind(a.answer)
        .push(", ")
        .push_bind(a.tags)
        .push(")")
        .push(RETURNING);
    let row = qb.build_query_as().fetch_one(&state.db).await?;
    Ok((StatusCode::CREATED, Json(row)))
}

pub async fn update(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(input): Json<AnswerInput>,
) -> AppResult<Json<Answer>> {
    let a = clean(input)?;
    let mut qb: QueryBuilder<Postgres> = QueryBuilder::new("UPDATE answers SET question = ");
    qb.push_bind(a.question)
        .push(", answer = ")
        .push_bind(a.answer)
        .push(", tags = ")
        .push_bind(a.tags)
        .push(", updated_at = now() WHERE id = ")
        .push_bind(id)
        .push(RETURNING);
    qb.build_query_as()
        .fetch_optional(&state.db)
        .await?
        .map(Json)
        .ok_or_else(|| AppError::NotFound("Answer not found.".into()))
}

pub async fn delete(State(state): State<AppState>, Path(id): Path<Uuid>) -> AppResult<StatusCode> {
    let result = sqlx::query("DELETE FROM answers WHERE id = $1")
        .bind(id)
        .execute(&state.db)
        .await?;
    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("Answer not found.".into()));
    }
    Ok(StatusCode::NO_CONTENT)
}
