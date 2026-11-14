//! The scoring worker: one background task that scores jobs with a local
//! model, one at a time (the GPU can only run one big model well).
//!
//! What it picks, in order:
//! 1. `score_requests` rows (the Re-score button and eval runs), oldest first.
//! 2. If automatic scoring is on: jobs from the last 30 days that have no
//!    score yet for the active model and prompt version, newest first.
//!
//! When there's nothing to do it sleeps until woken (`wake()`) or 30 s pass,
//! so jobs saved by a scrape get picked up within half a minute.

pub mod metrics;
pub mod prompt;

use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use tokio::sync::Notify;

use crate::{
    llm::{LlmClient, LlmError},
    profile, settings,
};
use prompt::{JobForPrompt, PROMPT_VERSION, SYSTEM_PROMPT, output_schema, user_prompt};

/// Lists longer than this are cut: the UI shows them as chips.
const MAX_ITEMS: usize = 8;
const IDLE_WAIT: Duration = Duration::from_secs(30);
/// Unscored jobs with no score this long after a failure are tried again.
const RETRY_FAILED_AFTER: &str = "1 hour";

#[derive(Clone)]
pub struct Scorer {
    inner: Arc<Inner>,
}

struct Inner {
    db: PgPool,
    llm: LlmClient,
    wake: Notify,
    state: Mutex<WorkerState>,
}

/// What the worker is doing right now, for the status endpoint.
#[derive(Debug, Clone, Default, Serialize)]
pub struct WorkerState {
    pub current: Option<CurrentJob>,
    /// Set when the whole queue is stuck (Ollama down, model missing, no resume yet).
    pub paused_reason: Option<String>,
    pub last_scored_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CurrentJob {
    pub job_id: String,
    pub title: String,
    pub model: String,
    pub started_at: DateTime<Utc>,
}

/// What one call to `process_next` did.
#[derive(Debug, PartialEq, Eq)]
pub enum Step {
    Idle,
    Scored(String),
    Failed(String),
    Paused(String),
}

/// The model's answer, exactly as the schema asks for it.
#[derive(Debug, Deserialize)]
struct Answer {
    score: i64,
    summary: String,
    matched_requirements: Vec<String>,
    missing_requirements: Vec<String>,
    unknown_requirements: Vec<String>,
    hard_blockers: Vec<String>,
}

#[derive(sqlx::FromRow)]
struct JobRow {
    title: String,
    company: Option<String>,
    location: Option<String>,
    work_type: Option<String>,
    description_text: Option<String>,
}

enum Failure {
    /// Every job would fail the same way: leave the queue alone and wait.
    Paused(String),
    /// Only this job is the problem: record it and move on.
    Job(String),
}

impl Scorer {
    pub fn new(db: PgPool, llm: LlmClient) -> Self {
        Self {
            inner: Arc::new(Inner {
                db,
                llm,
                wake: Notify::new(),
                state: Mutex::new(WorkerState::default()),
            }),
        }
    }

    pub fn llm(&self) -> &LlmClient {
        &self.inner.llm
    }

    pub fn state(&self) -> WorkerState {
        self.inner.state.lock().unwrap().clone()
    }

    /// Tell the worker there is new work (it stores the signal if it's busy).
    pub fn wake(&self) {
        self.inner.wake.notify_one();
    }

    /// Run the worker forever in the background.
    pub fn spawn(&self) {
        let scorer = self.clone();
        tokio::spawn(async move {
            loop {
                match scorer.process_next().await {
                    Ok(Step::Scored(_) | Step::Failed(_)) => continue,
                    Ok(Step::Idle | Step::Paused(_)) => {}
                    Err(error) => tracing::error!(?error, "scoring worker error"),
                }
                tokio::select! {
                    _ = scorer.inner.wake.notified() => {}
                    _ = tokio::time::sleep(IDLE_WAIT) => {}
                }
            }
        });
    }

    /// Score the next job, if there is one. Public so tests can drive it step by step.
    pub async fn process_next(&self) -> anyhow::Result<Step> {
        let db = &self.inner.db;
        let settings = settings::load(db).await?;

        let Some((job_id, model, requested)) = next_task(db, &settings).await? else {
            self.update(|s| s.paused_reason = None);
            return Ok(Step::Idle);
        };

        let outcome = self.score(&job_id, &model).await;
        self.update(|s| s.current = None);

        match outcome {
            Ok(()) => {
                if requested {
                    delete_request(db, &job_id, &model).await?;
                }
                self.update(|s| {
                    s.paused_reason = None;
                    s.last_scored_at = Some(Utc::now());
                });
                Ok(Step::Scored(job_id))
            }
            Err(Failure::Job(message)) => {
                tracing::warn!(%job_id, %model, %message, "scoring failed");
                sqlx::query(
                    "UPDATE jobs SET score_error = $2, score_failed_at = now() WHERE id = $1",
                )
                .bind(&job_id)
                .bind(format!("{model}: {message}"))
                .execute(db)
                .await?;
                if requested {
                    delete_request(db, &job_id, &model).await?;
                }
                Ok(Step::Failed(job_id))
            }
            Err(Failure::Paused(reason)) => {
                tracing::warn!(%reason, "scoring paused");
                self.update(|s| s.paused_reason = Some(reason.clone()));
                Ok(Step::Paused(reason))
            }
        }
    }

    async fn score(&self, job_id: &str, model: &str) -> Result<(), Failure> {
        let db = &self.inner.db;
        let job: JobRow = sqlx::query_as(
            "SELECT title, company, location, work_type, description_text FROM jobs WHERE id = $1",
        )
        .bind(job_id)
        .fetch_one(db)
        .await
        .map_err(|e| Failure::Job(format!("could not load the job: {e}")))?;

        self.update(|s| {
            s.current = Some(CurrentJob {
                job_id: job_id.to_string(),
                title: job.title.clone(),
                model: model.to_string(),
                started_at: Utc::now(),
            })
        });

        let Some(description) = job.description_text.as_deref() else {
            return Err(Failure::Job("the job has no description".into()));
        };

        // Read on every job, so a resume or facts change applies to the next job.
        let resume = profile::current_resume(db)
            .await
            .map_err(|e| Failure::Paused(e.to_string()))?
            .ok_or_else(|| {
                Failure::Paused("Add your resume on the Profile page to start scoring.".into())
            })?;
        let facts = profile::load_facts(db)
            .await
            .map_err(|e| Failure::Paused(e.to_string()))?;
        // Without location and work rights the model would invent hard blockers.
        if facts.city.is_empty() {
            return Err(Failure::Paused(
                "Fill in your Facts (location and work rights) on the Profile page to start scoring."
                    .into(),
            ));
        }
        let profile = profile::profile_for_prompt(&facts);

        let user = user_prompt(
            &resume.content,
            &profile,
            &JobForPrompt {
                title: &job.title,
                company: job.company.as_deref(),
                location: job.location.as_deref(),
                work_type: job.work_type.as_deref(),
                description,
            },
        );

        let started = Instant::now();
        let value = self
            .inner
            .llm
            .chat_json(model, SYSTEM_PROMPT, &user, &output_schema())
            .await
            .map_err(|e| match e {
                e if e.is_unavailable() => Failure::Paused(e.to_string()),
                LlmError::BadAnswer(m) => Failure::Job(m),
                e => Failure::Job(e.to_string()),
            })?;
        let duration_ms = started.elapsed().as_millis() as i32;

        let answer: Answer = serde_json::from_value(value)
            .map_err(|e| Failure::Job(format!("answer doesn't match the schema: {e}")))?;

        sqlx::query(
            "INSERT INTO job_scores (job_id, model, prompt_version, score, summary,
                                     matched, missing, unknown, hard_blockers, duration_ms,
                                     resume_version_id)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)",
        )
        .bind(job_id)
        .bind(model)
        .bind(PROMPT_VERSION)
        .bind(answer.score.clamp(0, 100) as i16)
        .bind(clean_summary(&answer.summary))
        .bind(sqlx::types::Json(clean_list(answer.matched_requirements)))
        .bind(sqlx::types::Json(clean_list(answer.missing_requirements)))
        .bind(sqlx::types::Json(clean_list(answer.unknown_requirements)))
        .bind(sqlx::types::Json(clean_list(answer.hard_blockers)))
        .bind(duration_ms)
        .bind(resume.id)
        .execute(db)
        .await
        .map_err(|e| Failure::Job(format!("could not save the score: {e}")))?;

        sqlx::query("UPDATE jobs SET score_error = NULL, score_failed_at = NULL WHERE id = $1")
            .bind(job_id)
            .execute(db)
            .await
            .map_err(|e| Failure::Job(e.to_string()))?;

        tracing::info!(%job_id, %model, score = answer.score, duration_ms, "job scored");
        Ok(())
    }

    fn update(&self, change: impl FnOnce(&mut WorkerState)) {
        change(&mut self.inner.state.lock().unwrap());
    }
}

/// (job_id, model, came from score_requests)
async fn next_task(
    db: &PgPool,
    settings: &settings::Settings,
) -> sqlx::Result<Option<(String, String, bool)>> {
    let requested: Option<(String, String)> =
        sqlx::query_as("SELECT job_id, model FROM score_requests ORDER BY requested_at LIMIT 1")
            .fetch_optional(db)
            .await?;
    if let Some((job_id, model)) = requested {
        return Ok(Some((job_id, model, true)));
    }
    if !settings.scoring_enabled {
        return Ok(None);
    }

    let job_id: Option<String> = sqlx::query_scalar(
        "SELECT id FROM jobs j
         WHERE scrape_status = 'ok' AND NOT hidden AND description_text IS NOT NULL
           AND coalesce(listed_at, first_seen_at) > now() - interval '30 days'
           AND (score_failed_at IS NULL OR score_failed_at < now() - $3::interval)
           AND NOT EXISTS (SELECT 1 FROM job_scores s
                           WHERE s.job_id = j.id AND s.model = $1 AND s.prompt_version = $2)
         ORDER BY coalesce(listed_at, first_seen_at) DESC
         LIMIT 1",
    )
    .bind(&settings.scoring_model)
    .bind(PROMPT_VERSION)
    .bind(RETRY_FAILED_AFTER)
    .fetch_optional(db)
    .await?;
    Ok(job_id.map(|id| (id, settings.scoring_model.clone(), false)))
}

/// How many jobs automatic scoring still has to do (for the status card).
pub async fn unscored_count(db: &PgPool, model: &str) -> sqlx::Result<i64> {
    sqlx::query_scalar(
        "SELECT count(*) FROM jobs j
         WHERE scrape_status = 'ok' AND NOT hidden AND description_text IS NOT NULL
           AND coalesce(listed_at, first_seen_at) > now() - interval '30 days'
           AND NOT EXISTS (SELECT 1 FROM job_scores s
                           WHERE s.job_id = j.id AND s.model = $1 AND s.prompt_version = $2)",
    )
    .bind(model)
    .bind(PROMPT_VERSION)
    .fetch_one(db)
    .await
}

async fn delete_request(db: &PgPool, job_id: &str, model: &str) -> sqlx::Result<()> {
    sqlx::query("DELETE FROM score_requests WHERE job_id = $1 AND model = $2")
        .bind(job_id)
        .bind(model)
        .execute(db)
        .await
        .map(|_| ())
}

/// Trim, drop blanks and duplicates, keep at most MAX_ITEMS.
fn clean_list(items: Vec<String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for item in items {
        let item = item.trim().to_string();
        if !item.is_empty() && !out.contains(&item) {
            out.push(item);
        }
        if out.len() == MAX_ITEMS {
            break;
        }
    }
    out
}

fn clean_summary(summary: &str) -> String {
    let summary = summary.trim();
    if summary.chars().count() <= 600 {
        summary.to_string()
    } else {
        summary.chars().take(597).collect::<String>() + "..."
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_are_trimmed_deduplicated_and_capped() {
        let items = vec![" SQL ", "", "SQL", "Python"]
            .into_iter()
            .chain(std::iter::repeat_n("x", 3))
            .chain((0..10).map(|i| if i == 0 { "x" } else { "y" }))
            .map(String::from)
            .collect();
        assert_eq!(clean_list(items), vec!["SQL", "Python", "x", "y"]);

        let many = (0..20).map(|i| format!("skill {i}")).collect();
        assert_eq!(clean_list(many).len(), MAX_ITEMS);
    }

    #[test]
    fn long_summaries_are_cut() {
        assert_eq!(clean_summary("  ok  "), "ok");
        assert_eq!(clean_summary(&"a".repeat(1000)).chars().count(), 600);
    }
}
