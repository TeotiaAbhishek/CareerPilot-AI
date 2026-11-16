//! Browser apply (F6a): click Apply on a job and the agent fills in the
//! application form in its own browser window, then stops at the final
//! Submit button for you. It never clicks Submit and never solves a CAPTCHA.

pub mod auth;
pub mod browser;
pub mod cleanup;
pub mod experience;
pub mod fill;
pub mod prompt;
pub mod review;
pub mod runner;

use std::{convert::Infallible, time::Duration};

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
    response::sse::{self, KeepAlive, Sse},
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use tokio_stream::{Stream, StreamExt, wrappers::BroadcastStream};
use uuid::Uuid;

use crate::{
    AppState,
    error::{AppError, AppResult, is_unique_violation},
    settings,
};
use fill::match_option;
use runner::{Applier, Event};

/// `SELECT <every Application column> FROM applications a JOIN jobs j ... <tail>`.
macro_rules! select_applications {
    ($tail:literal) => {
        concat!(
            "SELECT a.id, a.job_id, a.status, a.manual_reason, a.message, a.start_url,
                    a.current_url, a.tab_id, a.resume_id, a.cover_letter_id, a.steps, a.practice,
                    a.started_at, a.ready_at, a.submitted_at, a.created_at, a.updated_at,
                    j.title AS job_title, j.company,
                    (SELECT count(*) FROM application_questions q
                     WHERE q.application_id = a.id AND q.answer IS NULL) AS open_questions
             FROM applications a JOIN jobs j ON j.id = a.job_id ",
            $tail
        )
    };
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct Application {
    pub id: Uuid,
    pub job_id: String,
    pub status: String,
    pub manual_reason: Option<String>,
    pub message: Option<String>,
    pub start_url: String,
    pub current_url: Option<String>,
    pub tab_id: Option<String>,
    pub resume_id: Option<Uuid>,
    pub cover_letter_id: Option<Uuid>,
    pub steps: i32,
    pub practice: bool,
    pub started_at: Option<DateTime<Utc>>,
    pub ready_at: Option<DateTime<Utc>>,
    pub submitted_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub job_title: String,
    pub company: Option<String>,
    pub open_questions: i64,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct Question {
    pub id: Uuid,
    pub label: String,
    pub kind: String,
    pub options: Vec<String>,
    pub answer: Option<String>,
    pub answered_at: Option<DateTime<Utc>>,
    /// False for the agent's own questions (like "Include a cover letter?"),
    /// whose answers only count for this run.
    pub savable: bool,
}

#[derive(Serialize)]
pub struct ApplicationView {
    #[serde(flatten)]
    pub application: Application,
    pub questions: Vec<Question>,
    pub events: Vec<Event>,
}

async fn load(db: &PgPool, id: Uuid) -> AppResult<Application> {
    sqlx::query_as(select_applications!("WHERE a.id = $1"))
        .bind(id)
        .fetch_optional(db)
        .await?
        .ok_or_else(|| AppError::NotFound("Application not found.".into()))
}

async fn view(db: &PgPool, application: Application) -> AppResult<ApplicationView> {
    let questions = sqlx::query_as(
        "SELECT id, label, kind, options, answer, answered_at, field_id NOT LIKE 'cp-%' AS savable
         FROM application_questions
         WHERE application_id = $1 ORDER BY created_at",
    )
    .bind(application.id)
    .fetch_all(db)
    .await?;
    let events = sqlx::query_as(
        "SELECT id, application_id, kind, message, at FROM application_events
         WHERE application_id = $1 ORDER BY id",
    )
    .bind(application.id)
    .fetch_all(db)
    .await?;
    Ok(ApplicationView {
        application,
        questions,
        events,
    })
}

pub async fn get(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<ApplicationView>> {
    let application = load(&state.db, id).await?;
    Ok(Json(view(&state.db, application).await?))
}

#[derive(Deserialize)]
pub struct ListParams {
    pub job_id: Option<String>,
}

pub async fn list(
    State(state): State<AppState>,
    Query(params): Query<ListParams>,
) -> AppResult<Json<Vec<Application>>> {
    let rows = match params.job_id {
        Some(job_id) => {
            sqlx::query_as(select_applications!(
                "WHERE a.job_id = $1 ORDER BY a.created_at DESC"
            ))
            .bind(job_id)
            .fetch_all(&state.db)
            .await?
        }
        None => {
            sqlx::query_as(select_applications!("ORDER BY a.updated_at DESC LIMIT 200"))
                .fetch_all(&state.db)
                .await?
        }
    };
    Ok(Json(rows))
}

// ---------- Starting a run ----------

/// The newest approved document of a kind for a job.
async fn approved(db: &PgPool, job_id: &str, kind: &str) -> sqlx::Result<Option<Uuid>> {
    sqlx::query_scalar(
        "SELECT id FROM documents WHERE job_id = $1 AND kind = $2 AND status = 'approved'
         ORDER BY approved_at DESC LIMIT 1",
    )
    .bind(job_id)
    .bind(kind)
    .fetch_optional(db)
    .await
}

async fn create(
    state: &AppState,
    job_id: &str,
    start_url: &str,
    resume_id: Uuid,
    cover_letter_id: Option<Uuid>,
    practice: bool,
) -> AppResult<(StatusCode, Json<ApplicationView>)> {
    let inserted: Result<Uuid, sqlx::Error> = sqlx::query_scalar(
        "INSERT INTO applications (job_id, start_url, resume_id, cover_letter_id, practice)
         VALUES ($1, $2, $3, $4, $5) RETURNING id",
    )
    .bind(job_id)
    .bind(start_url)
    .bind(resume_id)
    .bind(cover_letter_id)
    .bind(practice)
    .fetch_one(&state.db)
    .await;
    let id = match inserted {
        Ok(id) => id,
        Err(e) if is_unique_violation(&e) => {
            return Err(AppError::Conflict(
                "This job already has an application in progress.".into(),
            ));
        }
        Err(e) => return Err(e.into()),
    };
    state.applier.log(id, "info", "Queued.").await;
    state.applier.wake();
    let application = load(&state.db, id).await?;
    Ok((
        StatusCode::CREATED,
        Json(view(&state.db, application).await?),
    ))
}

/// Click Apply: needs an approved resume for the job. The approved cover
/// letter goes too, if there is one.
pub async fn start(
    State(state): State<AppState>,
    Path(job_id): Path<String>,
) -> AppResult<(StatusCode, Json<ApplicationView>)> {
    let job: Option<(String, Option<String>)> = sqlx::query_as(
        "SELECT job_url, apply_url FROM jobs WHERE id = $1 AND source <> 'practice'",
    )
    .bind(&job_id)
    .fetch_optional(&state.db)
    .await?;
    let Some((job_url, apply_url)) = job else {
        return Err(AppError::NotFound("Job not found.".into()));
    };
    let Some(resume_id) = approved(&state.db, &job_id, "resume").await? else {
        return Err(AppError::Conflict(
            "Approve a resume for this job first (Prepare application, then Review).".into(),
        ));
    };
    let cover_letter_id = approved(&state.db, &job_id, "cover_letter").await?;
    let start_url = apply_url.unwrap_or(job_url);
    crate::tracker::track(&state.db, &job_id).await?;
    create(
        &state,
        &job_id,
        &start_url,
        resume_id,
        cover_letter_id,
        false,
    )
    .await
}

/// A practice run on the worker's practice form, with your newest approved resume.
#[derive(Deserialize)]
pub struct PracticeParams {
    /// "careers": the practice careers site with a sign-in, a sign-up and an
    /// email code. "code": one that signs you in with an emailed code only.
    /// Otherwise the practice Quick Apply form.
    site: Option<String>,
}

pub async fn practice(
    State(state): State<AppState>,
    Query(params): Query<PracticeParams>,
) -> AppResult<(StatusCode, Json<ApplicationView>)> {
    let resume_id: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM documents WHERE kind = 'resume' AND status = 'approved'
         ORDER BY approved_at DESC LIMIT 1",
    )
    .fetch_optional(&state.db)
    .await?;
    let Some(resume_id) = resume_id else {
        return Err(AppError::Conflict(
            "Approve a resume on any job first: the practice run uploads it.".into(),
        ));
    };
    // A new practice run replaces an unfinished one.
    let old: Vec<Option<String>> = sqlx::query_scalar(
        "UPDATE applications SET status = 'cancelled', updated_at = now()
         WHERE practice AND status IN ('queued', 'waiting_for_you', 'ready_to_submit', 'needs_manual')
         RETURNING tab_id",
    )
    .fetch_all(&state.db)
    .await?;
    for tab in old.into_iter().flatten() {
        let _ = state.applier.browser().close(&tab).await;
    }
    let page = match params.site.as_deref() {
        Some("careers") => "careers-login",
        Some("code") => "careers-code",
        _ => "quick-apply",
    };
    let url = format!("{}/practice/{page}", state.applier.browser().base_url());
    create(&state, "practice", &url, resume_id, None, true).await
}

// ---------- Your part ----------

#[derive(Deserialize)]
pub struct AnswerInput {
    pub question_id: Uuid,
    pub answer: String,
    /// Also save it to your answer bank, so no form asks you again.
    #[serde(default = "yes")]
    pub save: bool,
}

fn yes() -> bool {
    true
}

#[derive(Deserialize)]
pub struct AnswersInput {
    pub answers: Vec<AnswerInput>,
}

/// Answer the agent's questions. When none are left, the run carries on.
pub async fn answer(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(input): Json<AnswersInput>,
) -> AppResult<Json<ApplicationView>> {
    let application = load(&state.db, id).await?;
    if application.status != "waiting_for_you" {
        return Err(AppError::Conflict(
            "This run isn't waiting for answers.".into(),
        ));
    }
    let mut tx = state.db.begin().await?;
    for item in &input.answers {
        let question: Option<(String, String, Vec<String>, String)> = sqlx::query_as(
            "SELECT label, kind, options, field_id FROM application_questions
             WHERE id = $1 AND application_id = $2",
        )
        .bind(item.question_id)
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?;
        let Some((label, kind, options, field_id)) = question else {
            return Err(AppError::BadRequest(
                "That question isn't part of this run.".into(),
            ));
        };
        let mut answer = item.answer.trim().to_string();
        if answer.is_empty() {
            continue;
        }
        if answer.chars().count() > 5000 {
            return Err(AppError::BadRequest("That answer is too long.".into()));
        }
        if matches!(kind.as_str(), "select" | "radio" | "checkbox") {
            answer = match_option(&answer, &options).ok_or_else(|| {
                AppError::BadRequest(format!("\"{label}\" needs one of: {}.", options.join(", ")))
            })?;
        }
        sqlx::query(
            "UPDATE application_questions SET answer = $2, answered_at = now() WHERE id = $1",
        )
        .bind(item.question_id)
        .bind(&answer)
        .execute(&mut *tx)
        .await?;
        if item.save && !field_id.starts_with("cp-") {
            let existing: Option<Uuid> =
                sqlx::query_scalar("SELECT id FROM answers WHERE lower(question) = lower($1)")
                    .bind(&label)
                    .fetch_optional(&mut *tx)
                    .await?;
            match existing {
                Some(answer_id) => {
                    sqlx::query("UPDATE answers SET answer = $2, updated_at = now() WHERE id = $1")
                        .bind(answer_id)
                        .bind(&answer)
                        .execute(&mut *tx)
                        .await?;
                }
                None => {
                    sqlx::query(
                        "INSERT INTO answers (question, answer, source) VALUES ($1, $2, 'agent')",
                    )
                    .bind(&label)
                    .bind(&answer)
                    .execute(&mut *tx)
                    .await?;
                }
            }
        }
    }
    let open: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM application_questions WHERE application_id = $1 AND answer IS NULL",
    )
    .bind(id)
    .fetch_one(&mut *tx)
    .await?;
    if open == 0 {
        sqlx::query(
            "UPDATE applications SET status = 'queued', message = NULL, updated_at = now() WHERE id = $1",
        )
        .bind(id)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    if open == 0 {
        state
            .applier
            .log(id, "answer", "Thanks. Carrying on with your answers.")
            .await;
        state.applier.wake();
    }
    get(State(state), Path(id)).await
}

async fn set_status(
    state: &AppState,
    id: Uuid,
    from: &[&str],
    to: &str,
    message: &str,
) -> AppResult<Application> {
    let application = load(&state.db, id).await?;
    if !from.contains(&application.status.as_str()) {
        return Err(AppError::Conflict(format!(
            "Can't do that while it's {}.",
            application.status.replace('_', " ")
        )));
    }
    sqlx::query(
        "UPDATE applications SET status = $2, manual_reason = NULL, message = NULL,
                submitted_at = CASE WHEN $2 = 'submitted' THEN now() ELSE submitted_at END,
                updated_at = now()
         WHERE id = $1",
    )
    .bind(id)
    .bind(to)
    .execute(&state.db)
    .await?;
    state.applier.log(id, "info", message).await;
    Ok(application)
}

/// Try again after a CAPTCHA, login or a stuck form (same tab if it's open).
pub async fn retry(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<ApplicationView>> {
    set_status(
        &state,
        id,
        &["needs_manual", "failed"],
        "queued",
        "Trying again.",
    )
    .await?;
    state.applier.wake();
    get(State(state), Path(id)).await
}

/// You clicked Submit yourself (or finished it by hand).
pub async fn submitted(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<ApplicationView>> {
    set_status(
        &state,
        id,
        &[
            "ready_to_submit",
            "needs_manual",
            "waiting_for_you",
            "failed",
        ],
        "submitted",
        "You submitted it.",
    )
    .await?;
    let job_id: String = sqlx::query_scalar("SELECT job_id FROM applications WHERE id = $1")
        .bind(id)
        .fetch_one(&state.db)
        .await?;
    crate::tracker::mark_submitted(&state.db, &job_id).await?;
    get(State(state), Path(id)).await
}

pub async fn cancel(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<ApplicationView>> {
    let application = set_status(
        &state,
        id,
        &[
            "queued",
            "running",
            "waiting_for_you",
            "ready_to_submit",
            "needs_manual",
            "failed",
        ],
        "cancelled",
        "Cancelled.",
    )
    .await?;
    if let Some(tab) = application.tab_id {
        let _ = state.applier.browser().close(&tab).await;
    }
    get(State(state), Path(id)).await
}

/// Bring the run's tab to the front of the agent's browser.
pub async fn focus(State(state): State<AppState>, Path(id): Path<Uuid>) -> AppResult<StatusCode> {
    let application = load(&state.db, id).await?;
    let Some(tab) = application.tab_id else {
        return Err(AppError::Conflict("This run has no open tab.".into()));
    };
    match state.applier.browser().focus(&tab).await {
        Ok(()) => Ok(StatusCode::NO_CONTENT),
        Err(browser::BrowserError::TabClosed) => Err(AppError::Conflict(
            "That tab was closed. Try again opens a new one.".into(),
        )),
        Err(e) => Err(AppError::Unavailable(e.to_string())),
    }
}

// ---------- Live updates ----------

#[derive(Deserialize)]
pub struct EventsParams {
    /// Only events after this id (the run view sends the last one it has).
    pub after: Option<i64>,
}

/// Server-Sent Events: the run log as it happens, plus status changes.
pub async fn events(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(params): Query<EventsParams>,
) -> AppResult<Sse<impl Stream<Item = Result<sse::Event, Infallible>>>> {
    load(&state.db, id).await?;
    // Subscribe before reading the backlog, so nothing falls in between.
    let live = BroadcastStream::new(state.applier.subscribe());
    let backlog: Vec<Event> = sqlx::query_as(
        "SELECT id, application_id, kind, message, at FROM application_events
         WHERE application_id = $1 AND id > $2 ORDER BY id",
    )
    .bind(id)
    .bind(params.after.unwrap_or(0))
    .fetch_all(&state.db)
    .await?;
    let last = backlog.last().map_or(params.after.unwrap_or(0), |e| e.id);

    let backlog = tokio_stream::iter(backlog.into_iter().map(runner::LiveEvent::Log));
    let live = live.filter_map(move |item| match item {
        Ok(event) if event.application_id() == id => match &event {
            runner::LiveEvent::Log(e) if e.id <= last => None,
            _ => Some(event),
        },
        // Fell behind (very busy): the run view reloads on the next status change.
        _ => None,
    });
    let stream = backlog.chain(live).map(|event| {
        Ok(sse::Event::default()
            .json_data(&event)
            .unwrap_or_else(|_| sse::Event::default().data("{}")))
    });
    Ok(Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(15))))
}

// ---------- Status and the browser ----------

#[derive(Serialize)]
pub struct ApplyStatus {
    pub worker_url: String,
    pub worker_ok: bool,
    pub worker_error: Option<String>,
    pub running: usize,
    pub queued: i64,
    pub waiting_for_you: i64,
    pub ready_to_submit: i64,
    pub started_today: i64,
    pub daily_cap: i32,
    pub parallel: i32,
    pub paused_reason: Option<String>,
}

pub async fn status(State(state): State<AppState>) -> AppResult<Json<ApplyStatus>> {
    let settings = settings::load(&state.db).await?;
    let counts: (i64, i64, i64) = sqlx::query_as(
        "SELECT count(*) FILTER (WHERE status = 'queued'),
                count(*) FILTER (WHERE status = 'waiting_for_you'),
                count(*) FILTER (WHERE status = 'ready_to_submit')
         FROM applications",
    )
    .fetch_one(&state.db)
    .await?;
    let health = state.applier.browser().health().await;
    let runner = state.applier.state();
    Ok(Json(ApplyStatus {
        worker_url: state.applier.browser().base_url().to_string(),
        worker_ok: health.is_ok(),
        worker_error: health.err().map(|e| e.to_string()),
        running: runner.running,
        queued: counts.0,
        waiting_for_you: counts.1,
        ready_to_submit: counts.2,
        started_today: Applier::started_today(&state.db).await?,
        daily_cap: settings.apply_daily_cap,
        parallel: settings.apply_parallel,
        paused_reason: runner.paused_reason,
    }))
}

#[derive(Deserialize)]
pub struct OpenInput {
    pub url: String,
}

/// Open a page in the agent's browser, e.g. to log in to SEEK once.
pub async fn open_browser(
    State(state): State<AppState>,
    Json(input): Json<OpenInput>,
) -> AppResult<StatusCode> {
    let url = input.url.trim();
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return Err(AppError::BadRequest(
            "That needs to be a web address (https://…).".into(),
        ));
    }
    state
        .applier
        .browser()
        .open(url, true)
        .await
        .map_err(|e| AppError::Unavailable(e.to_string()))?;
    Ok(StatusCode::NO_CONTENT)
}
