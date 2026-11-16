//! PDF versions of a resume and a cover letter, printed by the browser
//! worker's headless Chromium. The resume is fitted to one page: printed a
//! little smaller first, then, if it still runs over, the bullets that matter
//! least (the last project's, then the oldest job's) are left out of the PDF.
//! The draft itself isn't changed; the review page lists what was left out.

use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, HeaderValue, header},
    response::IntoResponse,
};
use chrono::Utc;
use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

use super::{
    Kind,
    draft::{LetterDraft, ResumeDraft},
    file_name, html, load,
};
use crate::{
    AppState,
    apply::browser::BrowserClient,
    error::{AppError, AppResult},
};

/// Print sizes to try, largest first. 0.88 is still easy to read.
const SCALES: [f64; 3] = [1.0, 0.94, 0.88];
/// Give up trimming after this many bullets: the PDF then has 2 pages.
const MAX_TRIMS: usize = 12;
/// Every job keeps at least this many bullets.
const MIN_JOB_BULLETS: usize = 2;

pub struct Pdf {
    pub name: String,
    pub bytes: Vec<u8>,
    pub pages: u32,
    /// Bullets left out to fit one page.
    pub left_out: Vec<String>,
}

/// Leave out one bullet: the last project's (or the whole last project once
/// it has none), then the oldest job's, keeping MIN_JOB_BULLETS per job.
/// Returns what was left out, or None when there's nothing left to trim.
pub fn trim_one(draft: &mut ResumeDraft) -> Option<String> {
    if let Some(project) = draft.projects.last_mut() {
        if let Some(bullet) = project.bullets.pop() {
            return Some(bullet.text);
        }
        let project = draft.projects.pop()?;
        return Some(format!("Project: {}", project.heading));
    }
    let job = draft
        .experience
        .iter_mut()
        .rev()
        .find(|e| e.bullets.len() > MIN_JOB_BULLETS)?;
    job.bullets.pop().map(|b| b.text)
}

async fn print(browser: &BrowserClient, html: &str, scale: f64) -> AppResult<(Vec<u8>, u32)> {
    browser
        .pdf(html, scale)
        .await
        .map_err(|e| AppError::Unavailable(format!("Couldn't make the PDF: {e}")))
}

/// A document as a PDF. Resumes are fitted to one page.
pub async fn pdf_file(db: &PgPool, browser: &BrowserClient, id: Uuid) -> AppResult<Pdf> {
    let document = load(db, id).await?;
    let Some(content) = document.content.clone() else {
        return Err(AppError::Conflict("This draft isn't ready yet.".into()));
    };
    let broken =
        |e: serde_json::Error| AppError::Other(anyhow::anyhow!("stored draft is invalid: {e}"));
    match document.kind {
        Kind::CoverLetter => {
            let draft: LetterDraft = serde_json::from_value(content).map_err(broken)?;
            let date = Utc::now().format("%-d %B %Y").to_string();
            let (bytes, pages) = print(browser, &html::letter_html(&draft, &date), 1.0).await?;
            Ok(Pdf {
                name: file_name(&draft.name, Kind::CoverLetter, "pdf"),
                bytes,
                pages,
                left_out: vec![],
            })
        }
        Kind::Resume => {
            let mut draft: ResumeDraft = serde_json::from_value(content).map_err(broken)?;
            let name = file_name(&draft.name, Kind::Resume, "pdf");
            let mut left_out = Vec::new();
            for scale in SCALES {
                let (bytes, pages) = print(browser, &html::resume_html(&draft), scale).await?;
                if pages <= 1 {
                    return Ok(Pdf {
                        name,
                        bytes,
                        pages,
                        left_out,
                    });
                }
            }
            let smallest = SCALES[SCALES.len() - 1];
            loop {
                let trimmed = (left_out.len() < MAX_TRIMS)
                    .then(|| trim_one(&mut draft))
                    .flatten();
                let done = trimmed.is_none();
                if let Some(text) = trimmed {
                    left_out.push(text);
                }
                let (bytes, pages) = print(browser, &html::resume_html(&draft), smallest).await?;
                if pages <= 1 || done {
                    return Ok(Pdf {
                        name,
                        bytes,
                        pages,
                        left_out,
                    });
                }
            }
        }
    }
}

/// Download as a PDF (the resume fitted to one page).
pub async fn download(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<impl IntoResponse> {
    let pdf = pdf_file(&state.db, state.applier.browser(), id).await?;
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/pdf"),
    );
    headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&format!("attachment; filename=\"{}\"", pdf.name))
            .map_err(|e| AppError::Other(e.into()))?,
    );
    Ok((headers, pdf.bytes))
}

#[derive(Serialize)]
pub struct PdfCheck {
    pub pages: u32,
    pub left_out: Vec<String>,
}

/// How the PDF comes out: its pages, and any bullets left out to fit one page.
pub async fn check(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<PdfCheck>> {
    let pdf = pdf_file(&state.db, state.applier.browser(), id).await?;
    Ok(Json(PdfCheck {
        pages: pdf.pages,
        left_out: pdf.left_out,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::documents::draft::{DraftEntry, Line};

    fn entry(heading: &str, bullets: usize) -> DraftEntry {
        DraftEntry {
            heading: heading.into(),
            bullets: (1..=bullets)
                .map(|i| Line {
                    text: format!("{heading} {i}"),
                    evidence: vec![],
                })
                .collect(),
            ..DraftEntry::default()
        }
    }

    #[test]
    fn projects_go_first_then_the_oldest_job_keeping_two_bullets_each() {
        let mut draft = ResumeDraft {
            experience: vec![entry("New job", 3), entry("Old job", 3)],
            projects: vec![entry("Project", 1)],
            ..ResumeDraft::default()
        };
        let mut out = Vec::new();
        while let Some(text) = trim_one(&mut draft) {
            out.push(text);
        }
        assert_eq!(
            out,
            vec!["Project 1", "Project: Project", "Old job 3", "New job 3"]
        );
        assert!(draft.experience.iter().all(|e| e.bullets.len() == 2));
    }
}
