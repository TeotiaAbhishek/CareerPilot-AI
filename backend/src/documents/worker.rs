//! The drafting worker: takes queued documents (then queued emails) one at a
//! time and asks the local model to write them. Runs next to the scoring worker; both share one
//! model client, so they take turns on the GPU.

use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::{PgPool, types::Json};
use tokio::sync::Notify;
use uuid::Uuid;

use super::{
    Kind,
    check::{CheckContext, note_flags},
    draft::{LetterAnswer, ResumeAnswer, assemble_letter, assemble_resume},
    prompt::{
        JobForPrompt, LETTER_SYSTEM, LETTER_TASK, PROMPT_VERSION, RESUME_SYSTEM, RESUME_TASK,
        letter_schema, resume_schema, user_prompt,
    },
    resume::MasterResume,
};
use crate::{
    llm::{ChatOptions, LlmClient, LlmError},
    profile, settings,
};

const IDLE_WAIT: Duration = Duration::from_secs(30);
/// Drafting sends the whole resume and job ad and may think first, so it
/// needs a bigger window than scoring. 12k fits a 14B model on a 12 GB GPU.
pub const DRAFT_NUM_CTX: u32 = 12_288;

#[derive(Clone)]
pub struct Drafter {
    inner: Arc<Inner>,
}

struct Inner {
    db: PgPool,
    llm: LlmClient,
    wake: Notify,
    state: Mutex<DrafterState>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct DrafterState {
    pub current: Option<CurrentDraft>,
    /// Set when every draft would fail the same way (Ollama down, no resume).
    pub paused_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CurrentDraft {
    pub document_id: Uuid,
    pub job_id: String,
    pub kind: Kind,
    pub started_at: DateTime<Utc>,
}

/// What one call to `process_next` did.
#[derive(Debug, PartialEq, Eq)]
pub enum Step {
    Idle,
    Drafted(Uuid),
    Failed(Uuid),
    Paused(String),
}

enum Failure {
    Paused(String),
    Document(String),
}

#[derive(sqlx::FromRow)]
struct Task {
    id: Uuid,
    job_id: String,
    kind: Kind,
    note: Option<String>,
}

/// The matched / missing lists from the job's latest score.
#[derive(sqlx::FromRow)]
struct Analysis {
    matched: Json<Vec<String>>,
    missing: Json<Vec<String>>,
}

#[derive(sqlx::FromRow)]
struct JobRow {
    title: String,
    company: Option<String>,
    location: Option<String>,
    work_type: Option<String>,
    description_text: Option<String>,
}

impl Drafter {
    /// The shared model client (the assistant uses it too, so everyone takes turns on the GPU).
    pub fn llm(&self) -> &LlmClient {
        &self.inner.llm
    }

    pub fn new(db: PgPool, llm: LlmClient) -> Self {
        Self {
            inner: Arc::new(Inner {
                db,
                llm,
                wake: Notify::new(),
                state: Mutex::new(DrafterState::default()),
            }),
        }
    }

    pub fn state(&self) -> DrafterState {
        self.inner.state.lock().unwrap().clone()
    }

    pub fn wake(&self) {
        self.inner.wake.notify_one();
    }

    /// Drafts interrupted by a restart go back in the queue.
    pub async fn requeue_interrupted(&self) -> sqlx::Result<u64> {
        let done = sqlx::query("UPDATE documents SET status = 'queued' WHERE status = 'drafting'")
            .execute(&self.inner.db)
            .await?;
        let emails = sqlx::query("UPDATE emails SET status = 'queued' WHERE status = 'drafting'")
            .execute(&self.inner.db)
            .await?;
        Ok(done.rows_affected() + emails.rows_affected())
    }

    pub fn spawn(&self) {
        let drafter = self.clone();
        tokio::spawn(async move {
            loop {
                match drafter.process_next().await {
                    Ok(Step::Drafted(_) | Step::Failed(_)) => continue,
                    Ok(Step::Idle | Step::Paused(_)) => {}
                    Err(error) => tracing::error!(?error, "drafting worker error"),
                }
                tokio::select! {
                    _ = drafter.inner.wake.notified() => {}
                    _ = tokio::time::sleep(IDLE_WAIT) => {}
                }
            }
        });
    }

    /// Draft the oldest queued document, if any. Public so tests can drive it.
    pub async fn process_next(&self) -> anyhow::Result<Step> {
        let db = &self.inner.db;
        let task: Option<Task> = sqlx::query_as(
            "UPDATE documents SET status = 'drafting', updated_at = now()
             WHERE id = (SELECT id FROM documents WHERE status = 'queued'
                         ORDER BY created_at LIMIT 1 FOR UPDATE SKIP LOCKED)
             RETURNING id, job_id, kind, note",
        )
        .fetch_optional(db)
        .await?;
        let Some(task) = task else {
            // No document waiting: draft an email (F8), if one is due.
            return Ok(
                match crate::emails::draft_next(db, &self.inner.llm).await? {
                    crate::emails::Drafted::Idle => {
                        self.update(|s| s.paused_reason = None);
                        Step::Idle
                    }
                    crate::emails::Drafted::Ready(id) => {
                        self.update(|s| s.paused_reason = None);
                        Step::Drafted(id)
                    }
                    crate::emails::Drafted::Failed(id) => Step::Failed(id),
                    crate::emails::Drafted::Paused(reason) => {
                        tracing::warn!(%reason, "email drafting paused");
                        self.update(|s| s.paused_reason = Some(reason.clone()));
                        Step::Paused(reason)
                    }
                },
            );
        };

        self.update(|s| {
            s.current = Some(CurrentDraft {
                document_id: task.id,
                job_id: task.job_id.clone(),
                kind: task.kind,
                started_at: Utc::now(),
            })
        });
        let outcome = self.draft(&task).await;
        self.update(|s| s.current = None);

        match outcome {
            Ok(()) => {
                self.update(|s| s.paused_reason = None);
                Ok(Step::Drafted(task.id))
            }
            Err(Failure::Document(message)) => {
                tracing::warn!(id = %task.id, %message, "drafting failed");
                sqlx::query(
                    "UPDATE documents SET status = 'failed', error = $2, updated_at = now() WHERE id = $1",
                )
                .bind(task.id)
                .bind(&message)
                .execute(db)
                .await?;
                Ok(Step::Failed(task.id))
            }
            Err(Failure::Paused(reason)) => {
                tracing::warn!(%reason, "drafting paused");
                sqlx::query("UPDATE documents SET status = 'queued' WHERE id = $1")
                    .bind(task.id)
                    .execute(db)
                    .await?;
                self.update(|s| s.paused_reason = Some(reason.clone()));
                Ok(Step::Paused(reason))
            }
        }
    }

    async fn draft(&self, task: &Task) -> Result<(), Failure> {
        let db = &self.inner.db;
        let paused = |e: sqlx::Error| Failure::Paused(e.to_string());

        let job: JobRow = sqlx::query_as(
            "SELECT title, company, location, work_type, description_text FROM jobs WHERE id = $1",
        )
        .bind(&task.job_id)
        .fetch_one(db)
        .await
        .map_err(|e| Failure::Document(format!("could not load the job: {e}")))?;
        let Some(description) = job.description_text.as_deref() else {
            return Err(Failure::Document(
                "The job has no description to tailor to.".into(),
            ));
        };

        let resume = profile::current_resume(db)
            .await
            .map_err(paused)?
            .ok_or_else(|| Failure::Paused("Add your resume on the Profile page first.".into()))?;
        let master = MasterResume::parse(&resume.content);
        if master.experience.is_empty() && master.projects.is_empty() {
            return Err(Failure::Paused(
                "Your resume needs an \"## Experience\" or \"## Projects\" section with \"### \" entries."
                    .into(),
            ));
        }

        // The latest score's analysis helps the model focus.
        let analysis: Option<Analysis> = sqlx::query_as(
            "SELECT matched, missing FROM job_scores WHERE job_id = $1
             ORDER BY created_at DESC LIMIT 1",
        )
        .bind(&task.job_id)
        .fetch_optional(db)
        .await
        .map_err(paused)?;
        let (matched, missing) = analysis
            .map(|a| (a.matched.0, a.missing.0))
            .unwrap_or_default();

        let settings = settings::load(db).await.map_err(paused)?;
        let job_for_prompt = JobForPrompt {
            title: &job.title,
            company: job.company.as_deref(),
            location: job.location.as_deref(),
            work_type: job.work_type.as_deref(),
            description,
            matched: &matched,
            missing: &missing,
        };
        let (system, task_text, schema) = match task.kind {
            Kind::Resume => (RESUME_SYSTEM, RESUME_TASK, resume_schema()),
            Kind::CoverLetter => (LETTER_SYSTEM, LETTER_TASK, letter_schema()),
        };
        let user = user_prompt(&master, &job_for_prompt, task.note.as_deref(), task_text);

        let started = Instant::now();
        let value = self
            .inner
            .llm
            .chat_json_with(
                &settings.drafting_model,
                system,
                &user,
                &schema,
                ChatOptions {
                    think: settings.drafting_think,
                    num_ctx: Some(DRAFT_NUM_CTX),
                },
            )
            .await
            .map_err(|e| match e {
                e if e.is_unavailable() => Failure::Paused(e.to_string()),
                LlmError::BadAnswer(m) => Failure::Document(m),
                e => Failure::Document(e.to_string()),
            })?;
        let duration_ms = started.elapsed().as_millis() as i32;

        let check = CheckContext::new(
            &master,
            &job.title,
            job.company.as_deref(),
            job.location.as_deref(),
        );
        let bad_schema = |e: serde_json::Error| {
            Failure::Document(format!("the answer doesn't match the schema: {e}"))
        };
        let (content, flags) = match task.kind {
            Kind::Resume => {
                let answer: ResumeAnswer = serde_json::from_value(value).map_err(bad_schema)?;
                let (draft, notes) = assemble_resume(&master, answer);
                let mut flags = check.check_resume(&draft);
                flags.extend(note_flags(&notes));
                (serde_json::to_value(draft).expect("serialisable"), flags)
            }
            Kind::CoverLetter => {
                let answer: LetterAnswer = serde_json::from_value(value).map_err(bad_schema)?;
                if answer.paragraphs.is_empty() {
                    return Err(Failure::Document(
                        "The model returned an empty letter.".into(),
                    ));
                }
                let draft = assemble_letter(&master, answer);
                let flags = check.check_letter(&draft);
                (serde_json::to_value(draft).expect("serialisable"), flags)
            }
        };

        sqlx::query(
            "UPDATE documents SET status = 'ready', content = $2, flags = $3, model = $4,
                    prompt_version = $5, resume_version_id = $6, duration_ms = $7,
                    error = NULL, updated_at = now()
             WHERE id = $1",
        )
        .bind(task.id)
        .bind(content)
        .bind(Json(&flags))
        .bind(&settings.drafting_model)
        .bind(PROMPT_VERSION)
        .bind(resume.id)
        .bind(duration_ms)
        .execute(db)
        .await
        .map_err(|e| Failure::Document(format!("could not save the draft: {e}")))?;

        tracing::info!(id = %task.id, kind = ?task.kind, duration_ms, flags = flags.len(), "document drafted");
        Ok(())
    }

    fn update(&self, change: impl FnOnce(&mut DrafterState)) {
        change(&mut self.inner.state.lock().unwrap());
    }
}
