//! Jobs: list with search/filter/sort/paging, one job's detail, and
//! the actions you can take on a job (save, hide, mark viewed).
//! Scores shown are the latest ones from the active model and prompt version.

use axum::{
    Json,
    extract::{Path, Query, State},
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{Postgres, QueryBuilder, types::Json as DbJson};

use crate::{
    AppState,
    error::{AppError, AppResult},
    scoring::prompt::PROMPT_VERSION,
    settings::{self, Settings},
};

/// What the list shows for each job (no description, to keep it light).
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct JobSummary {
    pub id: String,
    pub title: String,
    pub company: Option<String>,
    pub location: Option<String>,
    pub work_type: Option<String>,
    pub salary: Option<String>,
    pub short_description: Option<String>,
    pub posted_label: Option<String>,
    pub listed_at: Option<DateTime<Utc>>,
    pub first_seen_at: DateTime<Utc>,
    pub apply_type: Option<String>,
    pub saved: bool,
    pub hidden: bool,
    pub viewed_at: Option<DateTime<Utc>>,
    pub scrape_status: String,
    /// Latest score from the active model, or null if not scored yet.
    pub score: Option<i16>,
    pub blocker_count: Option<i32>,
    /// score >= threshold and no hard blockers. Null if not scored yet.
    pub relevant: Option<bool>,
    /// Your label: apply | maybe | no.
    pub label: Option<String>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct JobDetail {
    pub id: String,
    pub title: String,
    pub company: Option<String>,
    pub location: Option<String>,
    pub work_type: Option<String>,
    pub classification: Option<String>,
    pub subclassification: Option<String>,
    pub salary: Option<String>,
    pub short_description: Option<String>,
    pub description_html: Option<String>,
    pub description_text: Option<String>,
    pub posted_label: Option<String>,
    pub listed_at: Option<DateTime<Utc>>,
    pub job_url: String,
    pub apply_url: Option<String>,
    pub apply_type: Option<String>,
    pub first_seen_at: DateTime<Utc>,
    pub last_seen_at: DateTime<Utc>,
    pub viewed_at: Option<DateTime<Utc>>,
    pub saved: bool,
    pub hidden: bool,
    pub scrape_status: String,
    pub scrape_error: Option<String>,
    /// The v1 OpenAI score, if this job was imported from Supabase.
    pub legacy_score: Option<i16>,
    pub label: Option<String>,
    /// Why the last scoring attempt failed, if it did.
    pub score_error: Option<String>,
}

/// One score with its explanation.
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct JobScore {
    pub model: String,
    pub prompt_version: String,
    pub score: i16,
    pub summary: String,
    pub matched: DbJson<Vec<String>>,
    pub missing: DbJson<Vec<String>>,
    pub unknown: DbJson<Vec<String>>,
    pub hard_blockers: DbJson<Vec<String>>,
    pub duration_ms: i32,
    pub created_at: DateTime<Utc>,
}

/// What GET /api/jobs/{id} returns: the job, plus its score.
#[derive(Debug, Serialize)]
pub struct JobView {
    #[serde(flatten)]
    pub job: JobDetail,
    pub score: Option<JobScore>,
    pub relevant: Option<bool>,
    /// True while a Re-score for the active model is waiting or running.
    pub score_pending: bool,
}

/// One job with everything, plus its v1 score if it was imported.
const DETAIL_BY_ID: &str = "SELECT j.id, j.title, j.company, j.location, j.work_type,
    j.classification, j.subclassification, j.salary, j.short_description,
    j.description_html, j.description_text, j.posted_label, j.listed_at, j.job_url,
    j.apply_url, j.apply_type, j.first_seen_at, j.last_seen_at, j.viewed_at, j.saved,
    j.hidden, j.scrape_status, j.scrape_error, l.relevance_score AS legacy_score,
    lb.label, j.score_error
    FROM jobs j
    LEFT JOIN legacy_job_analysis l ON l.job_id = j.id
    LEFT JOIN job_labels lb ON lb.job_id = j.id
    WHERE j.id = $1";

/// The latest score of one job for one model and prompt version.
const LATEST_SCORE: &str = "SELECT model, prompt_version, score, summary, matched, missing,
    unknown, hard_blockers, duration_ms, created_at
    FROM job_scores WHERE job_id = $1 AND model = $2 AND prompt_version = $3
    ORDER BY created_at DESC LIMIT 1";

#[derive(Debug, Default, Deserialize)]
pub struct ListParams {
    /// Free-text search over title, company and description.
    pub q: Option<String>,
    pub work_type: Option<String>,
    /// Only jobs listed in the last N days.
    pub posted_within_days: Option<i32>,
    /// all (default, hides hidden jobs) | new | saved | hidden
    pub status: Option<String>,
    /// Only jobs the model scored as relevant.
    pub relevant_only: Option<bool>,
    /// newest (default) | oldest | relevance (only with q) | best_fit
    pub sort: Option<String>,
    pub page: Option<i64>,
    pub page_size: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct JobPage {
    pub items: Vec<JobSummary>,
    pub total: i64,
    pub page: i64,
    pub page_size: i64,
}

/// Adds the joins for the latest score and your label. `s.score`,
/// `s.blocker_count` and `lb.label` can be used after this.
fn push_from(qb: &mut QueryBuilder<Postgres>, settings: &Settings) {
    qb.push(
        " FROM jobs
          LEFT JOIN LATERAL (
              SELECT score, jsonb_array_length(hard_blockers) AS blocker_count
              FROM job_scores
              WHERE job_id = jobs.id AND model = ",
    )
    .push_bind(settings.scoring_model.clone())
    .push(" AND prompt_version = ")
    .push_bind(PROMPT_VERSION)
    .push(
        " ORDER BY created_at DESC LIMIT 1
          ) s ON TRUE
          LEFT JOIN job_labels lb ON lb.job_id = jobs.id",
    );
}

/// Adds the WHERE clause shared by the count and the list queries.
fn push_filters(
    qb: &mut QueryBuilder<Postgres>,
    p: &ListParams,
    settings: &Settings,
) -> AppResult<()> {
    qb.push(" WHERE TRUE");

    match p.status.as_deref().unwrap_or("all") {
        "all" => {
            qb.push(" AND NOT hidden");
        }
        "new" => {
            qb.push(" AND NOT hidden AND viewed_at IS NULL");
        }
        "saved" => {
            qb.push(" AND saved");
        }
        "hidden" => {
            qb.push(" AND hidden");
        }
        other => return Err(AppError::BadRequest(format!("Unknown status {other:?}."))),
    }

    if let Some(q) = p.q.as_deref().map(str::trim).filter(|q| !q.is_empty()) {
        qb.push(" AND search_vector @@ websearch_to_tsquery('english', ")
            .push_bind(q.to_string())
            .push(")");
    }
    if let Some(work_type) = p.work_type.as_deref().filter(|w| !w.is_empty()) {
        qb.push(" AND work_type = ")
            .push_bind(work_type.to_string());
    }
    if let Some(days) = p.posted_within_days {
        if !(1..=365).contains(&days) {
            return Err(AppError::BadRequest(
                "posted_within_days must be 1-365.".into(),
            ));
        }
        qb.push(" AND coalesce(listed_at, first_seen_at) >= now() - make_interval(days => ")
            .push_bind(days)
            .push(")");
    }
    if p.relevant_only == Some(true) {
        qb.push(" AND s.blocker_count = 0 AND s.score >= ")
            .push_bind(settings.relevance_threshold);
    }
    Ok(())
}

pub async fn list(
    State(state): State<AppState>,
    Query(p): Query<ListParams>,
) -> AppResult<Json<JobPage>> {
    let page = p.page.unwrap_or(1).max(1);
    let page_size = p.page_size.unwrap_or(25).clamp(1, 100);

    let settings = settings::load(&state.db).await?;

    let mut count = QueryBuilder::new("SELECT count(*)");
    push_from(&mut count, &settings);
    push_filters(&mut count, &p, &settings)?;
    let total: i64 = count.build_query_scalar().fetch_one(&state.db).await?;

    let mut qb = QueryBuilder::new(
        "SELECT id, title, company, location, work_type, salary, short_description,
                posted_label, listed_at, first_seen_at, apply_type, saved, hidden,
                viewed_at, scrape_status, s.score, s.blocker_count, lb.label,
                (s.score >= ",
    );
    qb.push_bind(settings.relevance_threshold)
        .push(" AND s.blocker_count = 0) AS relevant");
    push_from(&mut qb, &settings);
    push_filters(&mut qb, &p, &settings)?;

    let has_query = p.q.as_deref().is_some_and(|q| !q.trim().is_empty());
    match p.sort.as_deref().unwrap_or("newest") {
        "newest" => {
            qb.push(" ORDER BY coalesce(listed_at, first_seen_at) DESC, id");
        }
        "oldest" => {
            qb.push(" ORDER BY coalesce(listed_at, first_seen_at) ASC, id");
        }
        "relevance" if has_query => {
            qb.push(" ORDER BY ts_rank(search_vector, websearch_to_tsquery('english', ")
                .push_bind(p.q.clone().unwrap_or_default())
                .push(")) DESC, id");
        }
        "relevance" => {
            qb.push(" ORDER BY coalesce(listed_at, first_seen_at) DESC, id");
        }
        // Relevant jobs first, then by score; unscored jobs last.
        "best_fit" => {
            qb.push(
                " ORDER BY (s.blocker_count = 0) DESC NULLS LAST, s.score DESC NULLS LAST,
                  coalesce(listed_at, first_seen_at) DESC, id",
            );
        }
        other => return Err(AppError::BadRequest(format!("Unknown sort {other:?}."))),
    }
    qb.push(" LIMIT ")
        .push_bind(page_size)
        .push(" OFFSET ")
        .push_bind((page - 1) * page_size);

    let items = qb.build_query_as().fetch_all(&state.db).await?;
    Ok(Json(JobPage {
        items,
        total,
        page,
        page_size,
    }))
}

pub async fn get(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<JobView>> {
    fetch_detail(&state, &id).await.map(Json)
}

async fn fetch_detail(state: &AppState, id: &str) -> AppResult<JobView> {
    let job: JobDetail = sqlx::query_as(DETAIL_BY_ID)
        .bind(id)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound("Job not found.".into()))?;

    let settings = settings::load(&state.db).await?;
    let score: Option<JobScore> = sqlx::query_as(LATEST_SCORE)
        .bind(id)
        .bind(&settings.scoring_model)
        .bind(PROMPT_VERSION)
        .fetch_optional(&state.db)
        .await?;
    let score_pending: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM score_requests WHERE job_id = $1 AND model = $2)",
    )
    .bind(id)
    .bind(&settings.scoring_model)
    .fetch_one(&state.db)
    .await?;

    let relevant = score
        .as_ref()
        .map(|s| i32::from(s.score) >= settings.relevance_threshold && s.hard_blockers.is_empty());
    Ok(JobView {
        job,
        score,
        relevant,
        score_pending,
    })
}

/// Only the fields you send are changed: {"saved": true} leaves `hidden` alone.
#[derive(Debug, Deserialize)]
pub struct JobPatch {
    pub saved: Option<bool>,
    pub hidden: Option<bool>,
    /// true marks the job as viewed (once); false marks it unread again.
    pub viewed: Option<bool>,
}

pub async fn patch(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(patch): Json<JobPatch>,
) -> AppResult<Json<JobView>> {
    let result = sqlx::query(
        "UPDATE jobs SET
            saved = coalesce($2, saved),
            hidden = coalesce($3, hidden),
            viewed_at = CASE
                WHEN $4 IS TRUE THEN coalesce(viewed_at, now())
                WHEN $4 IS FALSE THEN NULL
                ELSE viewed_at END
         WHERE id = $1",
    )
    .bind(&id)
    .bind(patch.saved)
    .bind(patch.hidden)
    .bind(patch.viewed)
    .execute(&state.db)
    .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("Job not found.".into()));
    }
    fetch_detail(&state, &id).await.map(Json)
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct Facet {
    pub value: String,
    pub count: i64,
}

/// Work types that exist, with counts, for the filter dropdown.
pub async fn work_types(State(state): State<AppState>) -> AppResult<Json<Vec<Facet>>> {
    let facets = sqlx::query_as(
        "SELECT work_type AS value, count(*) AS count FROM jobs
         WHERE work_type IS NOT NULL AND NOT hidden
         GROUP BY work_type ORDER BY count(*) DESC",
    )
    .fetch_all(&state.db)
    .await?;
    Ok(Json(facets))
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct Stats {
    pub jobs_total: i64,
    pub jobs_new_24h: i64,
    pub jobs_unviewed: i64,
    pub jobs_saved: i64,
    /// Jobs scored by the active model.
    pub jobs_scored: i64,
    /// Scored jobs that pass the threshold with no hard blockers.
    pub jobs_relevant: i64,
}

/// Numbers for the dashboard cards.
pub async fn stats(State(state): State<AppState>) -> AppResult<Json<Stats>> {
    let settings = settings::load(&state.db).await?;
    let stats = sqlx::query_as(
        "WITH latest AS (
            SELECT DISTINCT ON (job_id) job_id, score,
                   jsonb_array_length(hard_blockers) AS blockers
            FROM job_scores WHERE model = $1 AND prompt_version = $2
            ORDER BY job_id, created_at DESC
         )
         SELECT
            count(*) FILTER (WHERE NOT hidden) AS jobs_total,
            count(*) FILTER (WHERE NOT hidden AND first_seen_at > now() - interval '24 hours') AS jobs_new_24h,
            count(*) FILTER (WHERE NOT hidden AND viewed_at IS NULL) AS jobs_unviewed,
            count(*) FILTER (WHERE saved) AS jobs_saved,
            count(l.job_id) FILTER (WHERE NOT hidden) AS jobs_scored,
            count(*) FILTER (WHERE NOT hidden AND l.score >= $3 AND l.blockers = 0) AS jobs_relevant
         FROM jobs LEFT JOIN latest l ON l.job_id = jobs.id",
    )
    .bind(&settings.scoring_model)
    .bind(PROMPT_VERSION)
    .bind(settings.relevance_threshold)
    .fetch_one(&state.db)
    .await?;
    Ok(Json(stats))
}
