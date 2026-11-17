//! Tailored documents (F5): queue drafts, review and edit them, approve them,
//! and download them as Word files. Only approved documents will be used by
//! the apply agents (F6).

pub mod check;
pub mod compare;
pub mod docx;
pub mod draft;
pub mod html;
pub mod pdf;
pub mod prompt;
pub mod resume;
pub mod style;
pub mod worker;

use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, HeaderValue, header},
    response::IntoResponse,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    AppState,
    error::{AppError, AppResult},
};
use check::{CheckContext, Flag, Severity};
use draft::{LetterDraft, ResumeDraft, tidy_letter, tidy_resume};
use resume::{Evidence, MasterResume};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "text", rename_all = "snake_case")]
pub enum Kind {
    Resume,
    CoverLetter,
}

impl Kind {
    fn label(self) -> &'static str {
        match self {
            Kind::Resume => "Resume",
            Kind::CoverLetter => "Cover letter",
        }
    }
}

/// `SELECT <every Document column> FROM documents d JOIN jobs j ... <tail>`.
/// A macro (not format!) because sqlx only accepts SQL that is fixed at compile time.
macro_rules! select_documents {
    ($tail:literal) => {
        concat!(
            "SELECT d.id, d.job_id, d.kind, d.status, d.note, d.model, d.prompt_version,
                    d.resume_version_id, d.content, d.flags, d.dismissed_flags, d.edited, d.error,
                    d.duration_ms, d.created_at, d.updated_at, d.approved_at,
                    j.title AS job_title, j.company
             FROM documents d JOIN jobs j ON j.id = d.job_id ",
            $tail
        )
    };
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct Document {
    pub id: Uuid,
    pub job_id: String,
    pub kind: Kind,
    pub status: String,
    pub note: Option<String>,
    pub model: Option<String>,
    pub prompt_version: Option<String>,
    pub resume_version_id: Option<Uuid>,
    pub content: Option<Value>,
    pub flags: sqlx::types::Json<Vec<Flag>>,
    pub dismissed_flags: Vec<String>,
    pub edited: bool,
    pub error: Option<String>,
    pub duration_ms: Option<i32>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub approved_at: Option<DateTime<Utc>>,
    pub job_title: String,
    pub company: Option<String>,
}

impl Document {
    /// Error flags you haven't dismissed. Approval needs this to be 0.
    pub fn open_errors(&self) -> usize {
        self.flags
            .iter()
            .filter(|f| f.severity == Severity::Error && !self.dismissed_flags.contains(&f.key))
            .count()
    }
}

/// A document plus what the review screen needs alongside it.
#[derive(Serialize)]
pub struct DocumentView {
    #[serde(flatten)]
    pub document: Document,
    pub open_errors: usize,
    /// The evidence the draft cites, for the hover cards.
    pub evidence: Vec<Evidence>,
    /// Earlier drafts of the same kind for this job.
    pub previous_drafts: i64,
}

pub(crate) async fn load(db: &PgPool, id: Uuid) -> AppResult<Document> {
    sqlx::query_as(select_documents!("WHERE d.id = $1"))
        .bind(id)
        .fetch_optional(db)
        .await?
        .ok_or_else(|| AppError::NotFound("Document not found.".into()))
}

async fn master_for(db: &PgPool, document: &Document) -> AppResult<MasterResume> {
    let content: Option<String> = match document.resume_version_id {
        Some(id) => {
            sqlx::query_scalar("SELECT content FROM resume_versions WHERE id = $1")
                .bind(id)
                .fetch_optional(db)
                .await?
        }
        None => None,
    };
    let content = match content {
        Some(c) => Some(c),
        None => crate::profile::current_resume(db).await?.map(|r| r.content),
    };
    Ok(MasterResume::parse(&content.unwrap_or_default()))
}

async fn view(db: &PgPool, document: Document) -> AppResult<DocumentView> {
    let evidence = if document.content.is_some() {
        master_for(db, &document).await?.evidence()
    } else {
        Vec::new()
    };
    let previous_drafts = sqlx::query_scalar(
        "SELECT count(*) FROM documents WHERE job_id = $1 AND kind = $2 AND created_at < $3",
    )
    .bind(&document.job_id)
    .bind(document.kind)
    .bind(document.created_at)
    .fetch_one(db)
    .await?;
    Ok(DocumentView {
        open_errors: document.open_errors(),
        document,
        evidence,
        previous_drafts,
    })
}

// ---------- Queue ----------

#[derive(Deserialize)]
pub struct PrepareInput {
    #[serde(default = "yes")]
    pub resume: bool,
    #[serde(default)]
    pub cover_letter: bool,
    pub note: Option<String>,
}

fn yes() -> bool {
    true
}

/// "Prepare application": queue a resume, and a cover letter if asked.
/// Also used to regenerate one document (with an optional note).
pub async fn prepare(
    State(state): State<AppState>,
    Path(job_id): Path<String>,
    Json(input): Json<PrepareInput>,
) -> AppResult<Json<JobDocuments>> {
    let job: Option<Option<String>> =
        sqlx::query_scalar("SELECT description_text FROM jobs WHERE id = $1")
            .bind(&job_id)
            .fetch_optional(&state.db)
            .await?;
    match job {
        None => return Err(AppError::NotFound("Job not found.".into())),
        Some(None) => {
            return Err(AppError::BadRequest(
                "This job has no description yet, so there's nothing to tailor to.".into(),
            ));
        }
        Some(Some(_)) => {}
    }
    if !input.resume && !input.cover_letter {
        return Err(AppError::BadRequest(
            "Choose a resume, a cover letter or both.".into(),
        ));
    }
    let note = input
        .note
        .map(|n| n.trim().to_string())
        .filter(|n| !n.is_empty());
    if note.as_ref().is_some_and(|n| n.chars().count() > 500) {
        return Err(AppError::BadRequest(
            "Keep the note under 500 characters.".into(),
        ));
    }

    for (wanted, kind) in [
        (input.resume, Kind::Resume),
        (input.cover_letter, Kind::CoverLetter),
    ] {
        if !wanted {
            continue;
        }
        let busy: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM documents
                            WHERE job_id = $1 AND kind = $2 AND status IN ('queued', 'drafting'))",
        )
        .bind(&job_id)
        .bind(kind)
        .fetch_one(&state.db)
        .await?;
        if busy {
            continue; // Already on its way: don't queue a duplicate.
        }
        sqlx::query("INSERT INTO documents (job_id, kind, note) VALUES ($1, $2, $3)")
            .bind(&job_id)
            .bind(kind)
            .bind(&note)
            .execute(&state.db)
            .await?;
    }
    crate::tracker::track(&state.db, &job_id).await?;
    state.drafter.wake();
    job_documents(State(state), Path(job_id)).await
}

/// The latest resume and cover letter for one job.
#[derive(Serialize)]
pub struct JobDocuments {
    pub resume: Option<DocumentView>,
    pub cover_letter: Option<DocumentView>,
    /// Drafts waiting before this job's (so the UI can say "2nd in line").
    pub queue_ahead: i64,
}

async fn latest(db: &PgPool, job_id: &str, kind: Kind) -> AppResult<Option<DocumentView>> {
    let document: Option<Document> = sqlx::query_as(select_documents!(
        "WHERE d.job_id = $1 AND d.kind = $2 ORDER BY d.created_at DESC LIMIT 1"
    ))
    .bind(job_id)
    .bind(kind)
    .fetch_optional(db)
    .await?;
    match document {
        Some(d) => Ok(Some(view(db, d).await?)),
        None => Ok(None),
    }
}

pub async fn job_documents(
    State(state): State<AppState>,
    Path(job_id): Path<String>,
) -> AppResult<Json<JobDocuments>> {
    let resume = latest(&state.db, &job_id, Kind::Resume).await?;
    let cover_letter = latest(&state.db, &job_id, Kind::CoverLetter).await?;
    let first_queued = [&resume, &cover_letter]
        .into_iter()
        .flatten()
        .filter(|d| d.document.status == "queued")
        .map(|d| d.document.created_at)
        .min();
    let queue_ahead = match first_queued {
        Some(at) => {
            sqlx::query_scalar(
                "SELECT count(*) FROM documents WHERE status = 'queued' AND created_at < $1",
            )
            .bind(at)
            .fetch_one(&state.db)
            .await?
        }
        None => 0,
    };
    Ok(Json(JobDocuments {
        resume,
        cover_letter,
        queue_ahead,
    }))
}

// ---------- List and status ----------

#[derive(Serialize, sqlx::FromRow)]
pub struct DocumentSummary {
    pub id: Uuid,
    pub job_id: String,
    pub job_title: String,
    pub company: Option<String>,
    pub kind: Kind,
    pub status: String,
    pub open_errors: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub approved_at: Option<DateTime<Utc>>,
}

/// The latest draft of each kind for every job, most recently changed first.
pub async fn list(State(state): State<AppState>) -> AppResult<Json<Vec<DocumentSummary>>> {
    let rows = sqlx::query_as(
        "SELECT DISTINCT ON (d.job_id, d.kind)
                d.id, d.job_id, j.title AS job_title, j.company, d.kind, d.status,
                (SELECT count(*) FROM jsonb_array_elements(d.flags) f
                  WHERE f->>'severity' = 'error' AND NOT (f->>'key' = ANY (d.dismissed_flags))) AS open_errors,
                d.created_at, d.updated_at, d.approved_at
         FROM documents d JOIN jobs j ON j.id = d.job_id
         ORDER BY d.job_id, d.kind, d.created_at DESC",
    )
    .fetch_all(&state.db)
    .await?;
    let mut rows: Vec<DocumentSummary> = rows;
    rows.sort_by_key(|r| std::cmp::Reverse(r.updated_at));
    Ok(Json(rows))
}

#[derive(Serialize)]
pub struct DrafterStatus {
    pub model: String,
    pub think: bool,
    pub queued: i64,
    pub worker: worker::DrafterState,
}

pub async fn status(State(state): State<AppState>) -> AppResult<Json<DrafterStatus>> {
    let settings = crate::settings::load(&state.db).await?;
    let queued =
        sqlx::query_scalar("SELECT count(*) FROM documents WHERE status IN ('queued', 'drafting')")
            .fetch_one(&state.db)
            .await?;
    Ok(Json(DrafterStatus {
        model: settings.drafting_model,
        think: settings.drafting_think,
        queued,
        worker: state.drafter.state(),
    }))
}

// ---------- One document ----------

pub async fn get(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<DocumentView>> {
    let document = load(&state.db, id).await?;
    Ok(Json(view(&state.db, document).await?))
}

#[derive(Deserialize)]
pub struct SaveInput {
    pub content: Value,
    #[serde(default)]
    pub dismissed_flags: Vec<String>,
}

/// Save your edits. The checker runs again, and an approved document goes
/// back to "ready" so you approve what you changed.
pub async fn save(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(input): Json<SaveInput>,
) -> AppResult<Json<DocumentView>> {
    let document = load(&state.db, id).await?;
    if !matches!(document.status.as_str(), "ready" | "approved") {
        return Err(AppError::Conflict(
            "Only a finished draft can be edited.".into(),
        ));
    }
    let master = master_for(&state.db, &document).await?;
    let location: Option<String> = sqlx::query_scalar("SELECT location FROM jobs WHERE id = $1")
        .bind(&document.job_id)
        .fetch_one(&state.db)
        .await?;
    let check = CheckContext::new(
        &master,
        &document.job_title,
        document.company.as_deref(),
        location.as_deref(),
    );
    let invalid =
        |e: serde_json::Error| AppError::BadRequest(format!("That draft isn't valid: {e}"));

    let (content, flags) = match document.kind {
        Kind::Resume => {
            let draft =
                tidy_resume(serde_json::from_value::<ResumeDraft>(input.content).map_err(invalid)?);
            let flags = check.check_resume(&draft);
            (serde_json::to_value(draft).expect("serialisable"), flags)
        }
        Kind::CoverLetter => {
            let draft =
                tidy_letter(serde_json::from_value::<LetterDraft>(input.content).map_err(invalid)?);
            if draft.paragraphs.is_empty() {
                return Err(AppError::BadRequest(
                    "A cover letter needs at least one paragraph.".into(),
                ));
            }
            let flags = check.check_letter(&draft);
            (serde_json::to_value(draft).expect("serialisable"), flags)
        }
    };
    // Keep "info" notes from drafting (e.g. a removed skill) until the next draft.
    let mut flags = flags;
    flags.extend(
        document
            .flags
            .0
            .iter()
            .filter(|f| f.key.starts_with("note|"))
            .cloned(),
    );
    // Only keep dismissals that still match a flag.
    let dismissed: Vec<String> = input
        .dismissed_flags
        .into_iter()
        .filter(|k| flags.iter().any(|f| &f.key == k))
        .collect();

    sqlx::query(
        "UPDATE documents SET content = $2, flags = $3, dismissed_flags = $4, edited = TRUE,
                status = 'ready', approved_at = NULL, updated_at = now()
         WHERE id = $1",
    )
    .bind(id)
    .bind(content)
    .bind(sqlx::types::Json(&flags))
    .bind(&dismissed)
    .execute(&state.db)
    .await?;
    get(State(state), Path(id)).await
}

pub async fn approve(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<DocumentView>> {
    let document = load(&state.db, id).await?;
    if document.status != "ready" && document.status != "approved" {
        return Err(AppError::Conflict(
            "Only a finished draft can be approved.".into(),
        ));
    }
    let open = document.open_errors();
    if open > 0 {
        return Err(AppError::Conflict(format!(
            "{open} flagged line{} still need{} a look. Fix or dismiss them first.",
            if open == 1 { "" } else { "s" },
            if open == 1 { "s" } else { "" },
        )));
    }
    sqlx::query(
        "UPDATE documents SET status = 'approved', approved_at = coalesce(approved_at, now()), updated_at = now()
         WHERE id = $1",
    )
    .bind(id)
    .execute(&state.db)
    .await?;
    get(State(state), Path(id)).await
}

/// Download as a Word file.
pub async fn download_docx(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<impl IntoResponse> {
    let (filename, bytes) = docx_file(&state.db, id).await?;
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static(
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        ),
    );
    headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&format!("attachment; filename=\"{filename}\""))
            .map_err(|e| AppError::Other(e.into()))?,
    );
    Ok((headers, bytes))
}

/// A document as a Word file: (file name, bytes). The apply agent uploads these.
pub async fn docx_file(db: &PgPool, id: Uuid) -> AppResult<(String, Vec<u8>)> {
    let document = load(db, id).await?;
    let Some(content) = document.content.clone() else {
        return Err(AppError::Conflict("This draft isn't ready yet.".into()));
    };
    let broken =
        |e: serde_json::Error| AppError::Other(anyhow::anyhow!("stored draft is invalid: {e}"));
    let (bytes, name) = match document.kind {
        Kind::Resume => {
            let draft: ResumeDraft = serde_json::from_value(content).map_err(broken)?;
            (docx::resume_docx(&draft)?, draft.name)
        }
        Kind::CoverLetter => {
            let draft: LetterDraft = serde_json::from_value(content).map_err(broken)?;
            let date = Utc::now().format("%-d %B %Y").to_string();
            (docx::letter_docx(&draft, &date)?, draft.name)
        }
    };
    Ok((file_name(&name, document.kind, "docx"), bytes))
}

/// "Jane Citizen - Resume.pdf", ASCII only so every browser and OS accepts
/// it. No company in the name: the agent tells copies apart by when they
/// were added, and employers just see a clean file name.
pub fn file_name(name: &str, kind: Kind, extension: &str) -> String {
    let parts: Vec<&str> = [name, kind.label()]
        .into_iter()
        .filter(|p| !p.trim().is_empty())
        .collect();
    let safe: String = parts
        .join(" - ")
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || " -_.&()".contains(c) {
                c
            } else {
                ' '
            }
        })
        .collect();
    let safe = safe.split_whitespace().collect::<Vec<_>>().join(" ");
    format!(
        "{}.{extension}",
        if safe.is_empty() { "Document" } else { &safe }
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_names_are_safe() {
        assert_eq!(
            file_name("Jane Citizen™", Kind::Resume, "pdf"),
            "Jane Citizen - Resume.pdf"
        );
        assert_eq!(
            file_name("", Kind::CoverLetter, "docx"),
            "Cover letter.docx"
        );
    }
}
