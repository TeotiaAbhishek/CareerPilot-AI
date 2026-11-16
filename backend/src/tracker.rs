//! F7: one-click Apply, bulk apply, the application board and insights.
//!
//! One-click Apply: you click once, and the supervisor does the rest. It
//! queues a resume draft if the job has none, approves it when the checker
//! found nothing wrong (Settings: "Auto-approve clean drafts"), then starts
//! the apply agent. A draft with flagged lines waits for you; once you
//! approve it, the agent starts by itself. You still click Submit.
//!
//! The board: every job you've started on has a `tracker` row. Before
//! Submit the column comes from the job's documents and latest run; after
//! Submit you move the card yourself (Interview, Offer, Rejected).

use std::time::Duration;

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    AppState,
    documents::{self, Kind},
    error::{AppError, AppResult},
    settings,
};

/// How often the supervisor checks one-click jobs.
const TICK: Duration = Duration::from_secs(5);
/// Most jobs in one "Apply to selected".
const MAX_BULK: usize = 50;

// ---------- Columns ----------

/// A board column. The first three are worked out; the rest you set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Column {
    Ready,
    Applying,
    YourTurn,
    Submitted,
    Interview,
    Offer,
    Rejected,
    Withdrawn,
}

/// What the board needs to place an `active` card.
#[derive(Debug, Default)]
pub struct Progress<'a> {
    pub one_click: bool,
    /// Latest resume: queued, drafting, ready, approved or failed.
    pub resume: Option<&'a str>,
    pub resume_open_errors: i64,
    /// Latest non-practice run's status.
    pub run: Option<&'a str>,
    pub run_manual_reason: Option<&'a str>,
    pub note: Option<&'a str>,
}

/// Where an unsent job goes, and the one line under its title.
pub fn place(p: &Progress) -> (Column, String) {
    match p.run {
        Some("queued") => return (Column::Applying, "Queued for the agent".into()),
        Some("running") => return (Column::Applying, "The agent is filling in the form".into()),
        Some("waiting_for_you") => return (Column::YourTurn, "A question for you".into()),
        Some("ready_to_submit") => {
            return (
                Column::YourTurn,
                "Ready to submit: check it and click Submit".into(),
            );
        }
        Some("needs_manual") => {
            let why = match p.run_manual_reason {
                Some("captcha") => "a CAPTCHA",
                Some("login") => "a login",
                Some("cover_letter") => "the cover letter",
                _ => "a step it couldn't finish",
            };
            return (Column::YourTurn, format!("Needs you: {why}"));
        }
        Some("submitted") => return (Column::Submitted, "Submitted".into()),
        _ => {}
    }
    if let Some(note) = p.note {
        return (Column::YourTurn, note.to_string());
    }
    let failed_run = p.run == Some("failed");
    match p.resume {
        Some("queued") | Some("drafting") => (Column::Applying, "Drafting your resume".into()),
        Some("ready") if p.resume_open_errors > 0 => (
            Column::YourTurn,
            format!(
                "Review the resume: {} line{} to check",
                p.resume_open_errors,
                if p.resume_open_errors == 1 { "" } else { "s" }
            ),
        ),
        Some("ready") if p.one_click => (Column::Applying, "Approving your resume".into()),
        Some("ready") => (Column::YourTurn, "Review and approve the resume".into()),
        Some("failed") => (
            Column::YourTurn,
            "The resume draft failed: regenerate it".into(),
        ),
        Some("approved") if failed_run => (Column::YourTurn, "The run failed: try again".into()),
        Some("approved") if p.one_click => (Column::Applying, "Starting the agent".into()),
        Some("approved") => (Column::Ready, "Resume approved: ready to apply".into()),
        _ if p.one_click => (Column::Applying, "Drafting your resume".into()),
        _ => (Column::Ready, "Not started".into()),
    }
}

fn column_of_stage(stage: &str) -> Option<Column> {
    Some(match stage {
        "submitted" => Column::Submitted,
        "interview" => Column::Interview,
        "offer" => Column::Offer,
        "rejected" => Column::Rejected,
        "withdrawn" => Column::Withdrawn,
        _ => return None,
    })
}

// ---------- Keeping the board up to date ----------

/// Put a job on the board (no-op if it's there). Called when you prepare
/// documents or start a run.
pub async fn track(db: &PgPool, job_id: &str) -> sqlx::Result<()> {
    if job_id == "practice" {
        return Ok(());
    }
    sqlx::query("INSERT INTO tracker (job_id) VALUES ($1) ON CONFLICT (job_id) DO NOTHING")
        .bind(job_id)
        .execute(db)
        .await?;
    Ok(())
}

/// Move a card, recording the move. `replied_at` is set the first time it
/// leaves Submitted for Interview, Offer or Rejected.
async fn move_to(db: &PgPool, job_id: &str, stage: &str) -> sqlx::Result<()> {
    let mut tx = db.begin().await?;
    let from: Option<String> =
        sqlx::query_scalar("SELECT stage FROM tracker WHERE job_id = $1 FOR UPDATE")
            .bind(job_id)
            .fetch_optional(&mut *tx)
            .await?;
    if from.as_deref() == Some(stage) {
        return Ok(());
    }
    sqlx::query(
        "INSERT INTO tracker (job_id, stage, applied_at)
         VALUES ($1, $2, CASE WHEN $2 IN ('submitted', 'interview', 'offer', 'rejected') THEN now() END)
         ON CONFLICT (job_id) DO UPDATE SET
             stage = EXCLUDED.stage,
             one_click = FALSE,
             note = NULL,
             applied_at = CASE WHEN EXCLUDED.stage IN ('submitted', 'interview', 'offer', 'rejected')
                               THEN coalesce(tracker.applied_at, now()) ELSE tracker.applied_at END,
             replied_at = CASE WHEN tracker.stage = 'submitted'
                                    AND EXCLUDED.stage IN ('interview', 'offer', 'rejected')
                               THEN coalesce(tracker.replied_at, now()) ELSE tracker.replied_at END,
             stage_changed_at = now(),
             updated_at = now()",
    )
    .bind(job_id)
    .bind(stage)
    .execute(&mut *tx)
    .await?;
    sqlx::query("INSERT INTO tracker_moves (job_id, from_stage, to_stage) VALUES ($1, $2, $3)")
        .bind(job_id)
        .bind(&from)
        .bind(stage)
        .execute(&mut *tx)
        .await?;
    tx.commit().await
}

/// You clicked "I submitted it" on a run.
pub async fn mark_submitted(db: &PgPool, job_id: &str) -> sqlx::Result<()> {
    if job_id == "practice" {
        return Ok(());
    }
    let stage: Option<String> = sqlx::query_scalar("SELECT stage FROM tracker WHERE job_id = $1")
        .bind(job_id)
        .fetch_optional(db)
        .await?;
    // Don't move a card back from Interview to Submitted.
    if matches!(stage.as_deref(), None | Some("active")) {
        move_to(db, job_id, "submitted").await?;
    }
    Ok(())
}

// ---------- One-click Apply ----------

#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Started {
    /// The agent is applying now (the resume was already approved).
    Applying,
    /// A resume is being drafted; the agent starts once it's approved.
    Drafting,
    /// The resume is waiting for you to review it.
    WaitingForYou,
}

/// One-click Apply for one job. Errors say why it couldn't start.
pub async fn one_click_job(state: &AppState, job_id: &str) -> AppResult<Started> {
    let db = &state.db;
    let job: Option<(String, Option<String>)> =
        sqlx::query_as("SELECT source, description_text FROM jobs WHERE id = $1")
            .bind(job_id)
            .fetch_optional(db)
            .await?;
    match job {
        None => return Err(AppError::NotFound("Job not found.".into())),
        Some((source, _)) if source == "practice" => {
            return Err(AppError::BadRequest(
                "Use Practice run on Settings for the practice job.".into(),
            ));
        }
        Some((_, None)) => {
            return Err(AppError::BadRequest(
                "This job has no description yet, so there's nothing to tailor the resume to."
                    .into(),
            ));
        }
        Some(_) => {}
    }
    let busy: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM applications WHERE job_id = $1
                        AND status IN ('queued', 'running', 'waiting_for_you', 'ready_to_submit'))",
    )
    .bind(job_id)
    .fetch_one(db)
    .await?;
    if busy {
        return Err(AppError::Conflict(
            "This job already has an application in progress.".into(),
        ));
    }
    // Once you've applied (even if it ended in a rejection), don't apply again.
    let card: Option<(String, Option<DateTime<Utc>>)> =
        sqlx::query_as("SELECT stage, applied_at FROM tracker WHERE job_id = $1")
            .bind(job_id)
            .fetch_optional(db)
            .await?;
    if card.as_ref().is_some_and(|(_, applied)| applied.is_some()) {
        return Err(AppError::Conflict(
            "You've already applied for this job.".into(),
        ));
    }
    let stage = card.map(|(stage, _)| stage);
    if stage.is_some_and(|s| s != "active") {
        move_to(db, job_id, "active").await?;
    }
    sqlx::query(
        "INSERT INTO tracker (job_id, one_click) VALUES ($1, TRUE)
         ON CONFLICT (job_id) DO UPDATE SET one_click = TRUE, note = NULL, updated_at = now()",
    )
    .bind(job_id)
    .execute(db)
    .await?;

    // A resume on its way or done? Otherwise draft one.
    let resume: Option<String> = sqlx::query_scalar(
        "SELECT status FROM documents WHERE job_id = $1 AND kind = 'resume'
         ORDER BY created_at DESC LIMIT 1",
    )
    .bind(job_id)
    .fetch_optional(db)
    .await?;
    if matches!(resume.as_deref(), None | Some("failed")) {
        sqlx::query("INSERT INTO documents (job_id, kind) VALUES ($1, $2)")
            .bind(job_id)
            .bind(Kind::Resume)
            .execute(db)
            .await?;
        state.drafter.wake();
    }
    // Do the next step now rather than in 5 seconds.
    supervise(state, job_id).await
}

/// One step of one-click Apply for one job.
async fn supervise(state: &AppState, job_id: &str) -> AppResult<Started> {
    let db = &state.db;
    let doc: Option<(Uuid, String)> = sqlx::query_as(
        "SELECT id, status FROM documents WHERE job_id = $1 AND kind = 'resume'
         ORDER BY created_at DESC LIMIT 1",
    )
    .bind(job_id)
    .fetch_optional(db)
    .await?;
    let Some((doc_id, status)) = doc else {
        return Ok(Started::Drafting);
    };
    match status.as_str() {
        "queued" | "drafting" => Ok(Started::Drafting),
        "failed" => {
            stop(
                db,
                job_id,
                "The resume draft failed: regenerate it, then click Apply again",
            )
            .await?;
            Ok(Started::WaitingForYou)
        }
        "ready" => {
            let open = documents::load(db, doc_id).await?.open_errors();
            let auto = settings::load(db).await?.auto_approve;
            if open > 0 || !auto {
                // Stays one-click: once you approve it, the agent starts.
                return Ok(Started::WaitingForYou);
            }
            sqlx::query(
                "UPDATE documents SET status = 'approved', approved_at = coalesce(approved_at, now()),
                        updated_at = now()
                 WHERE id = $1 AND status = 'ready'",
            )
            .bind(doc_id)
            .execute(db)
            .await?;
            start_run(state, job_id).await
        }
        "approved" => start_run(state, job_id).await,
        _ => Ok(Started::Drafting),
    }
}

async fn start_run(state: &AppState, job_id: &str) -> AppResult<Started> {
    let started = crate::apply::start(State(state.clone()), Path(job_id.to_string())).await;
    let db = &state.db;
    match started {
        Ok(_) => {
            sqlx::query("UPDATE tracker SET one_click = FALSE, note = NULL, updated_at = now() WHERE job_id = $1")
                .bind(job_id)
                .execute(db)
                .await?;
            Ok(Started::Applying)
        }
        // Already running: nothing left for one-click to do.
        Err(AppError::Conflict(_)) => {
            sqlx::query(
                "UPDATE tracker SET one_click = FALSE, updated_at = now() WHERE job_id = $1",
            )
            .bind(job_id)
            .execute(db)
            .await?;
            Ok(Started::Applying)
        }
        Err(e) => Err(e),
    }
}

async fn stop(db: &PgPool, job_id: &str, note: &str) -> sqlx::Result<()> {
    sqlx::query(
        "UPDATE tracker SET one_click = FALSE, note = $2, updated_at = now() WHERE job_id = $1",
    )
    .bind(job_id)
    .bind(note)
    .execute(db)
    .await?;
    Ok(())
}

/// Every few seconds: move one-click jobs along (a draft finished, you
/// approved a resume).
pub fn spawn_supervisor(state: AppState) {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(TICK).await;
            let jobs: Vec<String> = match sqlx::query_scalar(
                "SELECT job_id FROM tracker WHERE one_click AND stage = 'active'",
            )
            .fetch_all(&state.db)
            .await
            {
                Ok(jobs) => jobs,
                Err(e) => {
                    tracing::warn!(error = %e, "one-click supervisor");
                    continue;
                }
            };
            for job_id in jobs {
                if let Err(e) = supervise(&state, &job_id).await {
                    tracing::warn!(job_id, error = %e, "one-click step failed");
                    let _ = stop(
                        &state.db,
                        &job_id,
                        &format!("Couldn't start the agent: {e}"),
                    )
                    .await;
                }
            }
        }
    });
}

/// POST /api/jobs/{id}/one-click
pub async fn one_click(
    State(state): State<AppState>,
    Path(job_id): Path<String>,
) -> AppResult<Json<OneClick>> {
    let started = one_click_job(&state, &job_id).await?;
    Ok(Json(OneClick { started }))
}

#[derive(Serialize)]
pub struct OneClick {
    pub started: Started,
}

#[derive(Deserialize)]
pub struct BulkInput {
    job_ids: Vec<String>,
}

#[derive(Serialize)]
pub struct Skipped {
    pub job_id: String,
    pub reason: String,
}

#[derive(Serialize)]
pub struct BulkResult {
    pub started: usize,
    pub skipped: Vec<Skipped>,
}

/// POST /api/apply/bulk: one-click Apply for each ticked job.
pub async fn bulk(
    State(state): State<AppState>,
    Json(input): Json<BulkInput>,
) -> AppResult<Json<BulkResult>> {
    if input.job_ids.is_empty() || input.job_ids.len() > MAX_BULK {
        return Err(AppError::BadRequest(format!(
            "Pick between 1 and {MAX_BULK} jobs."
        )));
    }
    let mut result = BulkResult {
        started: 0,
        skipped: vec![],
    };
    let mut seen = std::collections::HashSet::new();
    for job_id in input.job_ids {
        if !seen.insert(job_id.clone()) {
            continue;
        }
        match one_click_job(&state, &job_id).await {
            Ok(_) => result.started += 1,
            Err(e) => result.skipped.push(Skipped {
                job_id,
                reason: e.to_string(),
            }),
        }
    }
    Ok(Json(result))
}

// ---------- The board ----------

#[derive(sqlx::FromRow)]
struct CardRow {
    job_id: String,
    title: String,
    company: Option<String>,
    location: Option<String>,
    apply_type: Option<String>,
    score: Option<i16>,
    stage: String,
    one_click: bool,
    note: Option<String>,
    applied_at: Option<DateTime<Utc>>,
    replied_at: Option<DateTime<Utc>>,
    stage_changed_at: DateTime<Utc>,
    created_at: DateTime<Utc>,
    resume_status: Option<String>,
    resume_open_errors: Option<i64>,
    application_id: Option<Uuid>,
    emails_due: i64,
    run_status: Option<String>,
    manual_reason: Option<String>,
    run_updated_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize)]
pub struct Card {
    pub job_id: String,
    pub title: String,
    pub company: Option<String>,
    pub location: Option<String>,
    pub apply_type: Option<String>,
    pub score: Option<i16>,
    pub column: Column,
    /// What's happening, in one line.
    pub detail: String,
    pub application_id: Option<Uuid>,
    /// Email drafts (follow-ups, a thank-you) ready for you to send.
    pub emails_due: i64,
    pub applied_at: Option<DateTime<Utc>>,
    pub replied_at: Option<DateTime<Utc>>,
    /// When it last moved (or the run last changed, before Submit).
    pub updated_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

macro_rules! cards {
    ($tail:literal) => {
        concat!("SELECT t.job_id, j.title, j.company, j.location, j.apply_type,
        coalesce(s.score, l.relevance_score) AS score,
        t.stage, t.one_click, t.note, t.applied_at, t.replied_at, t.stage_changed_at, t.created_at,
        d.status AS resume_status, d.open_errors AS resume_open_errors,
        a.id AS application_id, a.status AS run_status, a.manual_reason, a.updated_at AS run_updated_at,
        (SELECT count(*) FROM emails e WHERE e.job_id = t.job_id AND e.status = 'ready'
                                         AND e.due_at <= now()) AS emails_due
    FROM tracker t
    JOIN jobs j ON j.id = t.job_id
    LEFT JOIN legacy_job_analysis l ON l.job_id = t.job_id
    LEFT JOIN LATERAL (
        SELECT score FROM job_scores WHERE job_id = t.job_id ORDER BY created_at DESC LIMIT 1
    ) s ON TRUE
    LEFT JOIN LATERAL (
        SELECT d.status,
               (SELECT count(*) FROM jsonb_array_elements(d.flags) f
                WHERE f->>'severity' = 'error' AND NOT (f->>'key' = ANY (d.dismissed_flags))) AS open_errors
        FROM documents d WHERE d.job_id = t.job_id AND d.kind = 'resume'
        ORDER BY d.created_at DESC LIMIT 1
    ) d ON TRUE
    LEFT JOIN LATERAL (
        SELECT id, status, manual_reason, updated_at FROM applications
        WHERE job_id = t.job_id AND NOT practice AND status <> 'cancelled'
        ORDER BY created_at DESC LIMIT 1
    ) a ON TRUE", " ", $tail)
    };
}

fn card(r: CardRow) -> Card {
    let (column, detail) = match column_of_stage(&r.stage) {
        Some(column) => {
            let detail = match column {
                Column::Submitted => "Waiting to hear back",
                Column::Interview => "Interview",
                Column::Offer => "Offer",
                Column::Rejected => "Not this time",
                _ => "Withdrawn",
            };
            (column, detail.to_string())
        }
        None => place(&Progress {
            one_click: r.one_click,
            resume: r.resume_status.as_deref(),
            resume_open_errors: r.resume_open_errors.unwrap_or(0),
            run: r.run_status.as_deref(),
            run_manual_reason: r.manual_reason.as_deref(),
            note: r.note.as_deref(),
        }),
    };
    let updated_at = match (r.stage.as_str(), r.run_updated_at) {
        ("active", Some(run)) => run.max(r.stage_changed_at),
        _ => r.stage_changed_at,
    };
    Card {
        job_id: r.job_id,
        title: r.title,
        company: r.company,
        location: r.location,
        apply_type: r.apply_type,
        score: r.score,
        column,
        detail,
        application_id: r.application_id,
        emails_due: r.emails_due,
        applied_at: r.applied_at,
        replied_at: r.replied_at,
        updated_at,
        created_at: r.created_at,
    }
}

/// GET /api/tracker: every card, most recently changed first.
pub async fn board(State(state): State<AppState>) -> AppResult<Json<Vec<Card>>> {
    let rows: Vec<CardRow> = sqlx::query_as(cards!("")).fetch_all(&state.db).await?;
    let mut cards: Vec<Card> = rows.into_iter().map(card).collect();
    cards.sort_by_key(|c| std::cmp::Reverse(c.updated_at));
    Ok(Json(cards))
}

#[derive(Deserialize)]
pub struct MoveInput {
    stage: String,
}

/// PUT /api/tracker/{job_id}: move a card yourself.
pub async fn update(
    State(state): State<AppState>,
    Path(job_id): Path<String>,
    Json(input): Json<MoveInput>,
) -> AppResult<Json<Card>> {
    if column_of_stage(&input.stage).is_none() {
        return Err(AppError::BadRequest(
            "Move it to submitted, interview, offer, rejected or withdrawn.".into(),
        ));
    }
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM jobs WHERE id = $1 AND source <> 'practice')",
    )
    .bind(&job_id)
    .fetch_one(&state.db)
    .await?;
    if !exists {
        return Err(AppError::NotFound("Job not found.".into()));
    }
    move_to(&state.db, &job_id, &input.stage).await?;
    // A move to Interview plans a thank-you; moving on cancels follow-ups.
    if crate::emails::plan(&state.db).await? > 0 {
        state.drafter.wake();
    }
    one_card(&state.db, &job_id).await.map(Json)
}

async fn one_card(db: &PgPool, job_id: &str) -> AppResult<Card> {
    let row: Option<CardRow> = sqlx::query_as(cards!("WHERE t.job_id = $1"))
        .bind(job_id)
        .fetch_optional(db)
        .await?;
    row.map(card)
        .ok_or_else(|| AppError::NotFound("That job isn't on your board.".into()))
}

/// GET /api/tracker/{job_id}: one card, for the job page.
pub async fn get(
    State(state): State<AppState>,
    Path(job_id): Path<String>,
) -> AppResult<Json<Card>> {
    one_card(&state.db, &job_id).await.map(Json)
}

/// DELETE /api/tracker/{job_id}: take it off the board. Its documents and
/// runs stay.
pub async fn remove(
    State(state): State<AppState>,
    Path(job_id): Path<String>,
) -> AppResult<StatusCode> {
    sqlx::query("DELETE FROM tracker WHERE job_id = $1")
        .bind(&job_id)
        .execute(&state.db)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

// ---------- Insights ----------

#[derive(Serialize, sqlx::FromRow)]
pub struct DayCount {
    pub day: NaiveDate,
    pub count: i64,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct Breakdown {
    pub label: String,
    pub applied: i64,
    pub replied: i64,
    pub interviews: i64,
}

#[derive(Serialize)]
pub struct Funnel {
    pub applied: i64,
    pub replied: i64,
    pub interview: i64,
    pub offer: i64,
    pub rejected: i64,
}

#[derive(Serialize)]
pub struct Insights {
    /// The last 60 days, one entry per day (zeros included).
    pub per_day: Vec<DayCount>,
    /// The last 12 weeks, by the Monday each week starts.
    pub per_week: Vec<DayCount>,
    pub funnel: Funnel,
    /// Replied / applied, 0-1. None before your first application.
    pub response_rate: Option<f64>,
    /// Median days from applying to the first reply.
    pub median_days_to_reply: Option<f64>,
    pub by_score: Vec<Breakdown>,
    pub by_apply_type: Vec<Breakdown>,
    pub by_source: Vec<Breakdown>,
}

/// The applications that were sent: everything after Submit.
macro_rules! applied {
    ($select:literal, $and:literal) => {
        concat!(
            $select,
            " FROM tracker t JOIN jobs j ON j.id = t.job_id
              WHERE t.applied_at IS NOT NULL AND t.stage <> 'active' ",
            $and
        )
    };
}

/// Applied, replied and interviews per group (score band, apply type, source).
macro_rules! breakdown {
    ($group:literal, $order:literal) => {
        concat!(
            "SELECT ", $group, " AS label, count(*) AS applied,
                    count(*) FILTER (WHERE t.replied_at IS NOT NULL OR t.stage IN ('interview', 'offer', 'rejected')) AS replied,
                    count(*) FILTER (WHERE t.stage IN ('interview', 'offer')
                                        OR EXISTS (SELECT 1 FROM tracker_moves m WHERE m.job_id = t.job_id AND m.to_stage = 'interview')) AS interviews
             FROM tracker t JOIN jobs j ON j.id = t.job_id
             LEFT JOIN legacy_job_analysis l ON l.job_id = t.job_id
             LEFT JOIN LATERAL (
                 SELECT score FROM job_scores WHERE job_id = t.job_id ORDER BY created_at DESC LIMIT 1
             ) s ON TRUE
             WHERE t.applied_at IS NOT NULL AND t.stage <> 'active'
             GROUP BY 1 ORDER BY ", $order
        )
    };
}

/// GET /api/insights
pub async fn insights(State(state): State<AppState>) -> AppResult<Json<Insights>> {
    let db = &state.db;
    let per_day: Vec<DayCount> = sqlx::query_as(
        "SELECT d::date AS day, count(t.job_id) AS count
         FROM generate_series(current_date - 59, current_date, interval '1 day') d
         LEFT JOIN tracker t ON t.applied_at::date = d::date AND t.stage <> 'active'
         GROUP BY d ORDER BY d",
    )
    .fetch_all(db)
    .await?;
    let per_week: Vec<DayCount> = sqlx::query_as(
        "SELECT w::date AS day, count(t.job_id) AS count
         FROM generate_series(date_trunc('week', current_date) - interval '11 weeks',
                              date_trunc('week', current_date), interval '1 week') w
         LEFT JOIN tracker t ON date_trunc('week', t.applied_at) = w AND t.stage <> 'active'
         GROUP BY w ORDER BY w",
    )
    .fetch_all(db)
    .await?;
    let (applied, replied, interview, offer, rejected): (i64, i64, i64, i64, i64) =
        sqlx::query_as(applied!(
            "SELECT count(*),
                count(*) FILTER (WHERE t.replied_at IS NOT NULL OR t.stage IN ('interview', 'offer', 'rejected')),
                count(*) FILTER (WHERE t.stage IN ('interview', 'offer')
                                    OR EXISTS (SELECT 1 FROM tracker_moves m WHERE m.job_id = t.job_id AND m.to_stage = 'interview')),
                count(*) FILTER (WHERE t.stage = 'offer'),
                count(*) FILTER (WHERE t.stage = 'rejected')",
            ""
        ))
        .fetch_one(db)
        .await?;
    let median_days_to_reply: Option<f64> = sqlx::query_scalar(applied!(
        "SELECT percentile_cont(0.5) WITHIN GROUP (
                ORDER BY extract(epoch FROM t.replied_at - t.applied_at) / 86400.0)::float8",
        "AND t.replied_at IS NOT NULL"
    ))
    .fetch_one(db)
    .await?;

    let mut by_score: Vec<Breakdown> = sqlx::query_as(breakdown!(
        "CASE WHEN coalesce(s.score, l.relevance_score) IS NULL THEN 'Not scored'
              WHEN coalesce(s.score, l.relevance_score) >= 80 THEN '80-100'
              WHEN coalesce(s.score, l.relevance_score) >= 70 THEN '70-79'
              WHEN coalesce(s.score, l.relevance_score) >= 50 THEN '50-69'
              ELSE 'Under 50' END",
        "applied DESC"
    ))
    .fetch_all(db)
    .await?;
    const BANDS: [&str; 5] = ["80-100", "70-79", "50-69", "Under 50", "Not scored"];
    by_score.sort_by_key(|b| BANDS.iter().position(|x| *x == b.label));
    let by_apply_type: Vec<Breakdown> = sqlx::query_as(breakdown!(
        "CASE WHEN j.apply_type ILIKE '%quick%' THEN 'Quick Apply' ELSE 'Employer site' END",
        "applied DESC"
    ))
    .fetch_all(db)
    .await?;
    let by_source: Vec<Breakdown> = sqlx::query_as(breakdown!(
        "CASE j.source WHEN 'seek' THEN 'SEEK' ELSE initcap(j.source) END",
        "applied DESC"
    ))
    .fetch_all(db)
    .await?;

    Ok(Json(Insights {
        per_day,
        per_week,
        response_rate: (applied > 0).then(|| replied as f64 / applied as f64),
        median_days_to_reply,
        funnel: Funnel {
            applied,
            replied,
            interview,
            offer,
            rejected,
        },
        by_score,
        by_apply_type,
        by_source,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn progress() -> Progress<'static> {
        Progress::default()
    }

    #[test]
    fn unsent_jobs_land_in_the_right_column() {
        assert_eq!(place(&progress()).0, Column::Ready);
        let p = Progress {
            resume: Some("drafting"),
            ..progress()
        };
        assert_eq!(place(&p), (Column::Applying, "Drafting your resume".into()));
        let p = Progress {
            resume: Some("ready"),
            resume_open_errors: 2,
            one_click: true,
            ..progress()
        };
        assert_eq!(
            place(&p),
            (
                Column::YourTurn,
                "Review the resume: 2 lines to check".into()
            )
        );
        let p = Progress {
            resume: Some("approved"),
            ..progress()
        };
        assert_eq!(place(&p).0, Column::Ready);
        let p = Progress {
            resume: Some("approved"),
            one_click: true,
            ..progress()
        };
        assert_eq!(place(&p).0, Column::Applying);
    }

    #[test]
    fn the_run_wins_over_the_documents() {
        let p = Progress {
            resume: Some("approved"),
            run: Some("running"),
            ..progress()
        };
        assert_eq!(place(&p).0, Column::Applying);
        let p = Progress {
            resume: Some("approved"),
            run: Some("needs_manual"),
            run_manual_reason: Some("captcha"),
            ..progress()
        };
        assert_eq!(place(&p), (Column::YourTurn, "Needs you: a CAPTCHA".into()));
        let p = Progress {
            resume: Some("approved"),
            run: Some("failed"),
            ..progress()
        };
        assert_eq!(
            place(&p),
            (Column::YourTurn, "The run failed: try again".into())
        );
        let p = Progress {
            note: Some("The resume draft failed"),
            ..progress()
        };
        assert_eq!(place(&p).0, Column::YourTurn);
    }

    #[test]
    fn only_after_submit_stages_can_be_set_by_hand() {
        assert_eq!(column_of_stage("interview"), Some(Column::Interview));
        assert_eq!(column_of_stage("active"), None);
        assert_eq!(column_of_stage("applying"), None);
    }
}
