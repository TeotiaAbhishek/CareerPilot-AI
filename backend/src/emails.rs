//! F8: email drafts and follow-ups. Drafts only: you send them yourself
//! (Copy, or Open in Gmail) and mark them sent here.
//!
//! - **application**: when the job ad lists an email address, on request.
//! - **follow_up_1 / follow_up_2**: 7 and 14 days after you applied, while
//!   the card is still in Submitted (the second can be switched off).
//! - **thank_you**: when you move a card to Interview.
//!
//! The address comes from the job ad (found by code, never by the model).
//! The model writes the subject and the paragraphs, citing your resume, and
//! the same checker as the resume flags anything your resume doesn't back up.

use std::time::Duration;

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{PgPool, types::Json as SqlJson};
use uuid::Uuid;

use crate::{
    AppState,
    documents::{
        check::{CheckContext, Flag, Severity},
        draft::{LetterDraft, Line, clean_text, tidy_letter},
        prompt::{JobForPrompt, user_prompt},
        resume::{Evidence, MasterResume},
        worker::DRAFT_NUM_CTX,
    },
    error::{AppError, AppResult},
    llm::{ChatOptions, LlmClient, LlmError},
    profile, settings,
};

/// Bump when a prompt changes.
pub const EMAIL_PROMPT_VERSION: &str = "v1-emails";
/// Emails stay short. Longer ones get an "info" flag.
pub const MAX_EMAIL_WORDS: usize = 150;
/// Follow-ups are only planned for applications this recent, so importing
/// old history doesn't queue a pile of pointless follow-ups.
const FOLLOW_UP_WINDOW_DAYS: i32 = 21;
const PLAN_EVERY: Duration = Duration::from_secs(60);

// ---------- Kinds ----------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "text", rename_all = "snake_case")]
pub enum EmailKind {
    Application,
    // snake_case alone would make these "follow_up1".
    #[serde(rename = "follow_up_1")]
    #[sqlx(rename = "follow_up_1")]
    FollowUp1,
    #[serde(rename = "follow_up_2")]
    #[sqlx(rename = "follow_up_2")]
    FollowUp2,
    ThankYou,
}

impl EmailKind {
    fn task(self, applied: Option<DateTime<Utc>>) -> String {
        let applied = applied
            .map(|a| format!("The candidate applied on {}.", a.format("%-d %B %Y")))
            .unwrap_or_default();
        match self {
            EmailKind::Application => "Write a short email applying for this job, to send with the resume attached. \
Say which role it's for, give the two strongest genuine reasons the candidate fits, and say the resume is attached."
                .to_string(),
            EmailKind::FollowUp1 => format!(
                "Write a short, polite follow-up email about the application for this job. {applied} \
Restate interest in one sentence, add one strong genuine reason the candidate fits, and ask whether there's an update on next steps."
            ),
            EmailKind::FollowUp2 => format!(
                "Write a brief second follow-up email about the application for this job. {applied} \
An earlier follow-up got no reply. Keep it to two short paragraphs: still interested, happy to provide anything else."
            ),
            EmailKind::ThankYou => "Write a short thank-you email after an interview for this job. \
Thank them for their time, mention one genuine strength that fits the role, and say the candidate looks forward to hearing back."
                .to_string(),
        }
    }
}

pub const EMAIL_SYSTEM: &str = "You are writing a short, professional email from a job candidate.

The EVIDENCE list is the ONLY source of truth about the candidate. Each item has an id in square brackets.

YOU MUST NOT:
- invent skills, technologies, achievements, metrics, employers or dates
- change or add numbers: every number must appear in the evidence you cite
- claim knowledge of the company beyond what the job description says
- invent names of people, interviewers or recruiters

STYLE:
- Australian English. Warm, direct, not pushy. 60-120 words in 2-3 short paragraphs.
- A clear subject line naming the role, at most 10 words.
- No greeting or sign-off: those are added for you. No em dashes or en dashes.
- Every paragraph must list the ids of the evidence it uses (a closing sentence can cite the job you mention, e.g. W1).
- Return only JSON that matches the schema.";

pub fn email_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "subject": { "type": "string" },
            "paragraphs": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "text": { "type": "string" },
                        "evidence": { "type": "array", "items": { "type": "string" } }
                    },
                    "required": ["text", "evidence"]
                }
            }
        },
        "required": ["subject", "paragraphs"]
    })
}

#[derive(Debug, Deserialize)]
struct EmailAnswer {
    subject: String,
    paragraphs: Vec<Line>,
}

// ---------- The address in the ad ----------

/// The first email address in the job ad that looks like a person or a
/// hiring inbox (not a no-reply, privacy or job board address).
pub fn find_address(text: &str) -> Option<String> {
    const SKIP: &[&str] = &[
        "noreply",
        "no-reply",
        "donotreply",
        "privacy",
        "unsubscribe",
        "seek.com",
        "example.",
    ];
    text.split(|c: char| c.is_whitespace() || "<>()[]{},;:\"'".contains(c))
        .map(|w| w.trim_matches(|c: char| !c.is_ascii_alphanumeric()))
        .filter(|w| is_address(w))
        .map(str::to_lowercase)
        .find(|w| !SKIP.iter().any(|s| w.contains(s)))
}

fn is_address(word: &str) -> bool {
    let Some((local, domain)) = word.split_once('@') else {
        return false;
    };
    !local.is_empty()
        && local
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "._%+-".contains(c))
        && domain.contains('.')
        && !domain.starts_with('.')
        && !domain.ends_with('.')
        && domain
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || ".-".contains(c))
        && domain.rsplit('.').next().is_some_and(|tld| tld.len() >= 2)
}

// ---------- Checking ----------

/// Everyday email words that start sentences ("Happy to…", "Thanks for…").
/// They aren't claims, so the checker doesn't need them in your resume.
const EMAIL_WORDS: &[&str] = &[
    "happy",
    "thank",
    "thanks",
    "please",
    "looking",
    "hope",
    "hoping",
    "keen",
    "glad",
    "would",
    "could",
    "let",
    "following",
    "just",
    "since",
    "if",
    "as",
    "after",
    "having",
    "again",
    "any",
];

/// The resume checker on each paragraph, plus a gentle length check.
pub fn check_email(check: &CheckContext, paragraphs: &[Line]) -> Vec<Flag> {
    let letter = LetterDraft {
        paragraphs: paragraphs.to_vec(),
        ..Default::default()
    };
    let everyday = |f: &Flag| {
        f.key
            .strip_prefix("term|")
            .and_then(|rest| rest.rsplit('|').next())
            .is_some_and(|word| EMAIL_WORDS.contains(&word.to_lowercase().as_str()))
    };
    let mut flags: Vec<Flag> = check
        .check_letter(&letter)
        .into_iter()
        .filter(|f| f.path != "document" && !everyday(f))
        .collect();
    let words: usize = paragraphs
        .iter()
        .map(|p| p.text.split_whitespace().count())
        .sum();
    if words > MAX_EMAIL_WORDS {
        flags.push(Flag {
            key: "length|document|".into(),
            path: "document".into(),
            severity: Severity::Info,
            message: format!("{words} words. Emails work best under {MAX_EMAIL_WORDS}."),
        });
    }
    flags
}

// ---------- Planning: what's due ----------

/// Plan follow-ups and thank-yous, cancel the ones no longer needed, and
/// queue what's due. Safe to run any time; returns how many it queued.
pub async fn plan(db: &PgPool) -> sqlx::Result<u64> {
    let second = settings::load(db).await?.follow_up_second;

    // Follow-ups for recent applications still waiting to hear back.
    sqlx::query(
        "INSERT INTO emails (job_id, kind, due_at)
         SELECT t.job_id, k.kind, t.applied_at + make_interval(days => k.days)
         FROM tracker t
         CROSS JOIN (VALUES ('follow_up_1', 7), ('follow_up_2', 14)) AS k (kind, days)
         WHERE t.stage = 'submitted' AND t.applied_at > now() - make_interval(days => $1)
           AND (k.kind = 'follow_up_1' OR $2)
         ON CONFLICT (job_id, kind) DO UPDATE SET due_at = EXCLUDED.due_at
             WHERE emails.status = 'scheduled'",
    )
    .bind(FOLLOW_UP_WINDOW_DAYS)
    .bind(second)
    .execute(db)
    .await?;

    // A thank-you when a card has just moved to Interview.
    sqlx::query(
        "INSERT INTO emails (job_id, kind, due_at)
         SELECT job_id, 'thank_you', stage_changed_at FROM tracker
         WHERE stage = 'interview' AND stage_changed_at > now() - interval '3 days'
         ON CONFLICT (job_id, kind) DO NOTHING",
    )
    .execute(db)
    .await?;

    // Follow-ups stop once the card moves on (or the second is switched off).
    sqlx::query(
        "UPDATE emails e SET status = 'cancelled', updated_at = now()
         FROM tracker t
         WHERE t.job_id = e.job_id
           AND e.kind IN ('follow_up_1', 'follow_up_2')
           AND e.status IN ('scheduled', 'queued', 'ready', 'failed')
           AND (t.stage <> 'submitted' OR (e.kind = 'follow_up_2' AND NOT $1))",
    )
    .bind(second)
    .execute(db)
    .await?;

    // Due ones go to the drafting worker.
    let queued = sqlx::query(
        "UPDATE emails SET status = 'queued', updated_at = now()
         WHERE status = 'scheduled' AND due_at <= now()",
    )
    .execute(db)
    .await?;
    Ok(queued.rows_affected())
}

/// Plan every minute, and wake the drafter when something is due.
pub fn spawn_planner(state: AppState) {
    tokio::spawn(async move {
        loop {
            match plan(&state.db).await {
                Ok(n) if n > 0 => state.drafter.wake(),
                Ok(_) => {}
                Err(e) => tracing::warn!(error = %e, "email planner"),
            }
            tokio::time::sleep(PLAN_EVERY).await;
        }
    });
}

// ---------- Drafting ----------

/// What drafting the next email did.
#[derive(Debug, PartialEq, Eq)]
pub enum Drafted {
    Idle,
    Ready(Uuid),
    Failed(Uuid),
    /// Every draft would fail the same way: try again later.
    Paused(String),
}

#[derive(sqlx::FromRow)]
struct EmailTask {
    id: Uuid,
    job_id: String,
    kind: EmailKind,
    to_address: Option<String>,
}

#[derive(sqlx::FromRow)]
struct EmailJob {
    title: String,
    company: Option<String>,
    location: Option<String>,
    work_type: Option<String>,
    description_text: Option<String>,
    applied_at: Option<DateTime<Utc>>,
}

/// Draft the oldest queued email, if any. Called by the drafting worker when
/// no document is waiting, so documents and emails take turns on the GPU.
pub async fn draft_next(db: &PgPool, llm: &LlmClient) -> anyhow::Result<Drafted> {
    let task: Option<EmailTask> = sqlx::query_as(
        "UPDATE emails SET status = 'drafting', updated_at = now()
         WHERE id = (SELECT id FROM emails WHERE status = 'queued'
                     ORDER BY due_at LIMIT 1 FOR UPDATE SKIP LOCKED)
         RETURNING id, job_id, kind, to_address",
    )
    .fetch_optional(db)
    .await?;
    let Some(task) = task else {
        return Ok(Drafted::Idle);
    };

    match draft(db, llm, &task).await {
        Ok(()) => Ok(Drafted::Ready(task.id)),
        Err(Err(reason)) => {
            sqlx::query("UPDATE emails SET status = 'queued' WHERE id = $1")
                .bind(task.id)
                .execute(db)
                .await?;
            Ok(Drafted::Paused(reason))
        }
        Err(Ok(message)) => {
            tracing::warn!(id = %task.id, %message, "email draft failed");
            sqlx::query(
                "UPDATE emails SET status = 'failed', error = $2, updated_at = now() WHERE id = $1",
            )
            .bind(task.id)
            .bind(&message)
            .execute(db)
            .await?;
            Ok(Drafted::Failed(task.id))
        }
    }
}

/// `Err(Ok(message))`: this email failed. `Err(Err(reason))`: pause and retry.
async fn draft(
    db: &PgPool,
    llm: &LlmClient,
    task: &EmailTask,
) -> Result<(), Result<String, String>> {
    let pause = |e: sqlx::Error| Err(e.to_string());
    let job: EmailJob = sqlx::query_as(
        "SELECT j.title, j.company, j.location, j.work_type, j.description_text, t.applied_at
         FROM jobs j LEFT JOIN tracker t ON t.job_id = j.id WHERE j.id = $1",
    )
    .bind(&task.job_id)
    .fetch_one(db)
    .await
    .map_err(pause)?;
    let description = job.description_text.as_deref().unwrap_or("Not available.");
    let resume = profile::current_resume(db)
        .await
        .map_err(pause)?
        .ok_or_else(|| Err("Add your resume on the Profile page first.".to_string()))?;
    let master = MasterResume::parse(&resume.content);
    let settings = settings::load(db).await.map_err(pause)?;

    let job_for_prompt = JobForPrompt {
        title: &job.title,
        company: job.company.as_deref(),
        location: job.location.as_deref(),
        work_type: job.work_type.as_deref(),
        description,
        matched: &[],
        missing: &[],
    };
    let user = user_prompt(
        &master,
        &job_for_prompt,
        None,
        &task.kind.task(job.applied_at),
    );
    let value = llm
        .chat_json_with(
            &settings.drafting_model,
            EMAIL_SYSTEM,
            &user,
            &email_schema(),
            ChatOptions {
                think: settings.drafting_think,
                num_ctx: Some(DRAFT_NUM_CTX),
            },
        )
        .await
        .map_err(|e| match e {
            e if e.is_unavailable() => Err(e.to_string()),
            LlmError::BadAnswer(m) => Ok(m),
            e => Ok(e.to_string()),
        })?;
    let answer: EmailAnswer = serde_json::from_value(value)
        .map_err(|e| Ok(format!("the answer doesn't match the schema: {e}")))?;
    let paragraphs = tidy(answer.paragraphs);
    if paragraphs.is_empty() {
        return Err(Ok("The model returned an empty email.".into()));
    }
    let subject = clean_text(&answer.subject);
    let check = CheckContext::new(
        &master,
        &job.title,
        job.company.as_deref(),
        job.location.as_deref(),
    );
    let flags = check_email(&check, &paragraphs);
    // The address comes from the ad, by code. Kept if you already typed one.
    let to = task
        .to_address
        .clone()
        .or_else(|| job.description_text.as_deref().and_then(find_address));

    sqlx::query(
        "UPDATE emails SET status = 'ready', subject = $2, paragraphs = $3, flags = $4,
                dismissed_flags = '{}', to_address = $5, model = $6, error = NULL, updated_at = now()
         WHERE id = $1",
    )
    .bind(task.id)
    .bind(&subject)
    .bind(SqlJson(&paragraphs))
    .bind(SqlJson(&flags))
    .bind(&to)
    .bind(&settings.drafting_model)
    .execute(db)
    .await
    .map_err(|e| Ok(format!("could not save the draft: {e}")))?;
    tracing::info!(id = %task.id, kind = ?task.kind, flags = flags.len(), "email drafted");
    Ok(())
}

/// "[LinkedIn](https://…) | [GitHub](https://…)" from your resume's Markdown
/// becomes "https://… | https://…", which reads fine in a plain email.
pub fn plain_links(line: &str) -> String {
    let mut out = String::new();
    let mut rest = line;
    while let Some(open) = rest.find('[') {
        let Some(close) = rest[open..].find("](").map(|i| open + i) else {
            break;
        };
        let Some(end) = rest[close..].find(')').map(|i| close + i) else {
            break;
        };
        out.push_str(&rest[..open]);
        out.push_str(&rest[close + 2..end]);
        rest = &rest[end + 1..];
    }
    out.push_str(rest);
    out
}

fn tidy(paragraphs: Vec<Line>) -> Vec<Line> {
    tidy_letter(LetterDraft {
        paragraphs,
        ..Default::default()
    })
    .paragraphs
}

// ---------- The API ----------

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct EmailView {
    pub id: Uuid,
    pub job_id: String,
    pub kind: EmailKind,
    pub status: String,
    pub due_at: DateTime<Utc>,
    pub to_address: Option<String>,
    pub subject: String,
    pub paragraphs: SqlJson<Vec<Line>>,
    pub flags: SqlJson<Vec<Flag>>,
    pub dismissed_flags: Vec<String>,
    pub error: Option<String>,
    pub sent_at: Option<DateTime<Utc>>,
    pub updated_at: DateTime<Utc>,
    pub title: String,
    pub company: Option<String>,
}

macro_rules! select_emails {
    ($tail:literal) => {
        concat!(
            "SELECT e.id, e.job_id, e.kind, e.status, e.due_at, e.to_address, e.subject,
                    e.paragraphs, e.flags, e.dismissed_flags, e.error, e.sent_at, e.updated_at,
                    j.title, j.company
             FROM emails e JOIN jobs j ON j.id = e.job_id ",
            $tail
        )
    };
}

/// The emails on a job page: the address in the ad, your sign-off, and drafts.
#[derive(Debug, Serialize)]
pub struct JobEmails {
    pub address: Option<String>,
    pub greeting: String,
    /// "Kind regards," then your name and contact lines, from your resume.
    pub sign_off: String,
    /// Your resume's numbered lines, to show what each paragraph cites.
    pub evidence: Vec<Evidence>,
    pub emails: Vec<EmailView>,
}

/// Your sign-off and evidence, from your current resume.
async fn from_resume(db: &PgPool) -> AppResult<(String, Vec<Evidence>)> {
    let mut lines = vec!["Kind regards,".to_string()];
    let mut evidence = Vec::new();
    if let Some(resume) = profile::current_resume(db).await? {
        let master = MasterResume::parse(&resume.content);
        evidence = master.evidence();
        lines.push(master.name);
        lines.extend(master.contact.iter().map(|c| plain_links(c)));
    }
    Ok((lines.join("\n"), evidence))
}

/// GET /api/jobs/{id}/emails
pub async fn for_job(
    State(state): State<AppState>,
    Path(job_id): Path<String>,
) -> AppResult<Json<JobEmails>> {
    let description: Option<Option<String>> =
        sqlx::query_scalar("SELECT description_text FROM jobs WHERE id = $1")
            .bind(&job_id)
            .fetch_optional(&state.db)
            .await?;
    let Some(description) = description else {
        return Err(AppError::NotFound("Job not found.".into()));
    };
    let emails = sqlx::query_as(select_emails!(
        "WHERE e.job_id = $1 AND e.status <> 'scheduled' ORDER BY e.due_at"
    ))
    .bind(&job_id)
    .fetch_all(&state.db)
    .await?;
    let (sign_off, evidence) = from_resume(&state.db).await?;
    Ok(Json(JobEmails {
        address: description.as_deref().and_then(find_address),
        greeting: "Dear Hiring Manager,".into(),
        sign_off,
        evidence,
        emails,
    }))
}

#[derive(Deserialize)]
pub struct DraftInput {
    kind: EmailKind,
}

/// POST /api/jobs/{id}/emails: draft one now (or draft it again).
pub async fn draft_now(
    State(state): State<AppState>,
    Path(job_id): Path<String>,
    Json(input): Json<DraftInput>,
) -> AppResult<(StatusCode, Json<EmailView>)> {
    let source: Option<String> = sqlx::query_scalar("SELECT source FROM jobs WHERE id = $1")
        .bind(&job_id)
        .fetch_optional(&state.db)
        .await?;
    match source.as_deref() {
        None => return Err(AppError::NotFound("Job not found.".into())),
        Some("practice") => {
            return Err(AppError::BadRequest(
                "The practice job has no emails.".into(),
            ));
        }
        Some(_) => {}
    }
    let sent: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM emails WHERE job_id = $1 AND kind = $2 AND status = 'sent')",
    )
    .bind(&job_id)
    .bind(input.kind)
    .fetch_one(&state.db)
    .await?;
    if sent {
        return Err(AppError::Conflict("You've already sent this one.".into()));
    }
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO emails (job_id, kind, status, due_at) VALUES ($1, $2, 'queued', now())
         ON CONFLICT (job_id, kind) DO UPDATE SET status = 'queued', due_at = now(), error = NULL,
             updated_at = now()
         RETURNING id",
    )
    .bind(&job_id)
    .bind(input.kind)
    .fetch_one(&state.db)
    .await?;
    state.drafter.wake();
    Ok((StatusCode::ACCEPTED, Json(one(&state.db, id).await?)))
}

async fn one(db: &PgPool, id: Uuid) -> AppResult<EmailView> {
    sqlx::query_as(select_emails!("WHERE e.id = $1"))
        .bind(id)
        .fetch_optional(db)
        .await?
        .ok_or_else(|| AppError::NotFound("Email not found.".into()))
}

#[derive(Deserialize)]
pub struct SaveInput {
    to_address: Option<String>,
    subject: String,
    paragraphs: Vec<Line>,
    #[serde(default)]
    dismissed_flags: Vec<String>,
}

/// PUT /api/emails/{id}: your edits, checked again.
pub async fn save(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(input): Json<SaveInput>,
) -> AppResult<Json<EmailView>> {
    let current = one(&state.db, id).await?;
    if current.status != "ready" {
        return Err(AppError::Conflict(
            "Only a finished draft can be edited.".into(),
        ));
    }
    let to = input
        .to_address
        .map(|a| a.trim().to_lowercase())
        .filter(|a| !a.is_empty());
    if to.as_deref().is_some_and(|a| !is_address(a)) {
        return Err(AppError::BadRequest(
            "That doesn't look like an email address.".into(),
        ));
    }
    let paragraphs = tidy(input.paragraphs);
    let job: (String, Option<String>, Option<String>) =
        sqlx::query_as("SELECT title, company, location FROM jobs WHERE id = $1")
            .bind(&current.job_id)
            .fetch_one(&state.db)
            .await?;
    let flags = match profile::current_resume(&state.db).await? {
        Some(resume) => {
            let master = MasterResume::parse(&resume.content);
            let check = CheckContext::new(&master, &job.0, job.1.as_deref(), job.2.as_deref());
            check_email(&check, &paragraphs)
        }
        None => Vec::new(),
    };
    sqlx::query(
        "UPDATE emails SET to_address = $2, subject = $3, paragraphs = $4, flags = $5,
                dismissed_flags = $6, updated_at = now()
         WHERE id = $1",
    )
    .bind(id)
    .bind(&to)
    .bind(clean_text(&input.subject))
    .bind(SqlJson(&paragraphs))
    .bind(SqlJson(&flags))
    .bind(&input.dismissed_flags)
    .execute(&state.db)
    .await?;
    one(&state.db, id).await.map(Json)
}

/// POST /api/emails/{id}/sent: you sent it.
pub async fn mark_sent(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<EmailView>> {
    let done = sqlx::query(
        "UPDATE emails SET status = 'sent', sent_at = now(), updated_at = now()
         WHERE id = $1 AND status = 'ready'",
    )
    .bind(id)
    .execute(&state.db)
    .await?;
    if done.rows_affected() == 0 {
        return Err(AppError::Conflict(
            "Only a finished draft can be marked sent.".into(),
        ));
    }
    one(&state.db, id).await.map(Json)
}

/// POST /api/emails/{id}/skip: not sending this one.
pub async fn skip(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<EmailView>> {
    sqlx::query(
        "UPDATE emails SET status = 'cancelled', updated_at = now()
         WHERE id = $1 AND status <> 'sent'",
    )
    .bind(id)
    .execute(&state.db)
    .await?;
    one(&state.db, id).await.map(Json)
}

/// GET /api/emails/due: follow-ups and thank-yous that are due, oldest first.
pub async fn due(State(state): State<AppState>) -> AppResult<Json<Vec<EmailView>>> {
    let rows = sqlx::query_as(select_emails!(
        "WHERE e.status IN ('queued', 'drafting', 'ready', 'failed') AND e.due_at <= now()
         ORDER BY e.due_at"
    ))
    .fetch_all(&state.db)
    .await?;
    Ok(Json(rows))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_hiring_address_in_an_ad() {
        assert_eq!(
            find_address("Send your CV to Jobs@Acme.com.au (subject: Analyst)."),
            Some("jobs@acme.com.au".into())
        );
        assert_eq!(
            find_address("Questions? <noreply@acme.com> or talent.team@acme.io"),
            Some("talent.team@acme.io".into())
        );
        assert_eq!(find_address("Apply via SEEK. privacy@seek.com.au"), None);
        assert_eq!(find_address("Follow @acme on X"), None);
        assert_eq!(find_address("No address here."), None);
    }

    #[test]
    fn markdown_links_become_plain_urls() {
        assert_eq!(
            plain_links("[LinkedIn](https://l.in/me) | [GitHub](https://gh.com/me)"),
            "https://l.in/me | https://gh.com/me"
        );
        assert_eq!(plain_links("0400 000 000"), "0400 000 000");
        assert_eq!(plain_links("[broken"), "[broken");
    }

    #[test]
    fn checks_addresses() {
        assert!(is_address("a.b+c@x-y.com"));
        assert!(!is_address("a@b"));
        assert!(!is_address("@b.com"));
        assert!(!is_address("a@b.c"));
    }

    #[test]
    fn long_emails_get_an_info_flag() {
        let master =
            MasterResume::parse("# Jane\n\n## Experience\n\n### Acme\n\n* Built reports.\n");
        let check = CheckContext::new(&master, "Analyst", Some("Acme"), None);
        let short = vec![
            Line {
                text: "I built reports at Acme.".into(),
                evidence: vec!["W1.1".into()],
            },
            Line {
                text: "Happy to help. Thanks for your time.".into(),
                evidence: vec!["W1".into()],
            },
        ];
        assert!(
            check_email(&check, &short).is_empty(),
            "everyday words aren't claims"
        );
        let claim = vec![Line {
            text: "I led a Kubernetes migration.".into(),
            evidence: vec!["W1".into()],
        }];
        assert_eq!(check_email(&check, &claim).len(), 1);
        let long = vec![Line {
            text: "word ".repeat(160),
            evidence: vec!["W1.1".into()],
        }];
        let flags = check_email(&check, &long);
        assert_eq!(flags.len(), 1);
        assert_eq!(flags[0].severity, Severity::Info);
    }
}
