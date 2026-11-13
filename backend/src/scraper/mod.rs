//! A scrape run: search SEEK for every enabled search, then fetch the
//! details of jobs we haven't seen before. Progress is written to
//! `scrape_runs` as it goes, so the UI can show it live.

pub mod posted;
pub mod schedule;
pub mod seek;

use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    error::{AppError, AppResult, is_unique_violation},
    searches::Search,
};
use seek::{FetchError, JobCard, JobDetail, SeekClient};

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct ScrapeRun {
    pub id: Uuid,
    pub trigger: String,
    pub status: String,
    pub message: Option<String>,
    pub pages_fetched: i32,
    pub jobs_found: i32,
    pub jobs_new: i32,
    pub jobs_failed: i32,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy)]
pub enum Trigger {
    Manual,
    Schedule,
}

impl Trigger {
    fn as_str(self) -> &'static str {
        match self {
            Trigger::Manual => "manual",
            Trigger::Schedule => "schedule",
        }
    }
}

/// Record a new run, or fail with Conflict if one is already running.
/// The unique index `scrape_runs_one_running` makes this safe even if two
/// requests arrive at the same moment.
pub async fn start_run(db: &PgPool, trigger: Trigger) -> AppResult<ScrapeRun> {
    sqlx::query_as::<_, ScrapeRun>(
        "INSERT INTO scrape_runs (trigger, message) VALUES ($1, 'Starting…') RETURNING *",
    )
    .bind(trigger.as_str())
    .fetch_one(db)
    .await
    .map_err(|e| {
        if is_unique_violation(&e) {
            AppError::Conflict("A scrape is already running.".into())
        } else {
            e.into()
        }
    })
}

/// A run left as 'running' means the API stopped mid-run. Close it on startup.
pub async fn close_interrupted_runs(db: &PgPool) -> sqlx::Result<u64> {
    let result = sqlx::query(
        "UPDATE scrape_runs
         SET status = 'failed', message = 'Interrupted: the API stopped during this run.',
             finished_at = now()
         WHERE status = 'running'",
    )
    .execute(db)
    .await?;
    Ok(result.rows_affected())
}

pub async fn current_run(db: &PgPool) -> sqlx::Result<Option<ScrapeRun>> {
    sqlx::query_as("SELECT * FROM scrape_runs WHERE status = 'running' LIMIT 1")
        .fetch_optional(db)
        .await
}

pub async fn recent_runs(db: &PgPool, limit: i64) -> sqlx::Result<Vec<ScrapeRun>> {
    sqlx::query_as("SELECT * FROM scrape_runs ORDER BY started_at DESC LIMIT $1")
        .bind(limit)
        .fetch_all(db)
        .await
}

/// Do the whole run and record how it ended. Never panics, never returns an error:
/// whatever happens ends up in the run's status and message.
pub async fn execute_run(db: PgPool, client: SeekClient, run_id: Uuid) {
    let (status, message) = match scrape(&db, &client, run_id).await {
        Ok(Outcome::Finished { found, new, failed }) => (
            "succeeded",
            format!("Found {found} jobs: {new} new, {failed} failed."),
        ),
        Ok(Outcome::RateLimited { new }) => (
            "rate_limited",
            format!("Stopped early: SEEK is rate limiting. Saved {new} new jobs. Try again later."),
        ),
        Ok(Outcome::NoSearches) => (
            "succeeded",
            "No enabled searches. Add one in Settings.".to_string(),
        ),
        Err(error) => {
            tracing::error!(?error, "scrape failed");
            ("failed", format!("Failed: {error}"))
        }
    };

    if let Err(error) = sqlx::query(
        "UPDATE scrape_runs SET status = $2, message = $3, finished_at = now() WHERE id = $1",
    )
    .bind(run_id)
    .bind(status)
    .bind(&message)
    .execute(&db)
    .await
    {
        tracing::error!(?error, "could not record the end of a scrape run");
    }
    tracing::info!(%run_id, status, message, "scrape run finished");
}

enum Outcome {
    Finished {
        found: usize,
        new: usize,
        failed: usize,
    },
    RateLimited {
        new: usize,
    },
    NoSearches,
}

async fn scrape(db: &PgPool, client: &SeekClient, run_id: Uuid) -> anyhow::Result<Outcome> {
    let searches: Vec<Search> =
        sqlx::query_as("SELECT * FROM searches WHERE enabled ORDER BY created_at")
            .fetch_all(db)
            .await?;
    if searches.is_empty() {
        return Ok(Outcome::NoSearches);
    }

    // 1. Search pages: collect unique job cards across every search.
    let mut cards: Vec<JobCard> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    let mut pages_fetched = 0;
    let mut first_request = true;

    for search in &searches {
        for page in 1..=search.max_pages as u32 {
            if !first_request {
                client.politeness.pause().await;
            }
            first_request = false;

            progress(
                db,
                run_id,
                &format!("Searching \"{}\" page {page}", search.keywords),
            )
            .await?;

            let html = match client
                .fetch(&client.search_url(&search.keywords, &search.location, page))
                .await
            {
                Ok(html) => html,
                Err(FetchError::RateLimited) => return Ok(Outcome::RateLimited { new: 0 }),
                Err(error) => anyhow::bail!("search page failed: {error}"),
            };
            pages_fetched += 1;

            let mut new_on_page = 0;
            for card in client.parse_search_page(&html) {
                if seen.insert(card.id.clone()) {
                    cards.push(card);
                    new_on_page += 1;
                }
            }

            sqlx::query("UPDATE scrape_runs SET pages_fetched = $2, jobs_found = $3 WHERE id = $1")
                .bind(run_id)
                .bind(pages_fetched)
                .bind(cards.len() as i32)
                .execute(db)
                .await?;

            // SEEK repeats the last page forever, so stop when nothing new appears.
            if new_on_page == 0 {
                break;
            }
        }
    }

    // 2. Jobs we already have: just mark them as seen again.
    let ids: Vec<String> = cards.iter().map(|c| c.id.clone()).collect();
    let known: HashMap<String, String> = sqlx::query_as::<_, (String, String)>(
        "SELECT id, scrape_status FROM jobs WHERE id = ANY($1)",
    )
    .bind(&ids)
    .fetch_all(db)
    .await?
    .into_iter()
    .collect();

    sqlx::query("UPDATE jobs SET last_seen_at = now() WHERE id = ANY($1)")
        .bind(&ids)
        .execute(db)
        .await?;

    // 3. New jobs (and ones that failed last time): fetch their detail page.
    let to_fetch: Vec<&JobCard> = cards
        .iter()
        .filter(|c| known.get(&c.id).is_none_or(|status| status == "failed"))
        .collect();

    let (mut new, mut failed) = (0usize, 0usize);
    for (index, card) in to_fetch.iter().enumerate() {
        client.politeness.pause().await;
        progress(
            db,
            run_id,
            &format!("Fetching job details {}/{}", index + 1, to_fetch.len()),
        )
        .await?;

        match client.fetch(&card.job_url).await {
            Ok(html) => {
                let detail = client.parse_job_detail(&html);
                if detail.description_text.is_none() {
                    save_failed(db, card, "Job description could not be parsed.").await?;
                    failed += 1;
                } else {
                    save_job(db, card, &detail).await?;
                    new += 1;
                }
            }
            Err(FetchError::RateLimited) => {
                bump_counts(db, run_id, new, failed).await?;
                return Ok(Outcome::RateLimited { new });
            }
            Err(error) => {
                save_failed(db, card, &error.to_string()).await?;
                failed += 1;
            }
        }
        bump_counts(db, run_id, new, failed).await?;
    }

    Ok(Outcome::Finished {
        found: cards.len(),
        new,
        failed,
    })
}

async fn progress(db: &PgPool, run_id: Uuid, message: &str) -> sqlx::Result<()> {
    sqlx::query("UPDATE scrape_runs SET message = $2 WHERE id = $1")
        .bind(run_id)
        .bind(message)
        .execute(db)
        .await
        .map(|_| ())
}

async fn bump_counts(db: &PgPool, run_id: Uuid, new: usize, failed: usize) -> sqlx::Result<()> {
    sqlx::query("UPDATE scrape_runs SET jobs_new = $2, jobs_failed = $3 WHERE id = $1")
        .bind(run_id)
        .bind(new as i32)
        .bind(failed as i32)
        .execute(db)
        .await
        .map(|_| ())
}

/// Insert or complete a job. The detail page wins over the search card where both have a value.
async fn save_job(db: &PgPool, card: &JobCard, d: &JobDetail) -> sqlx::Result<()> {
    let now = Utc::now();
    let listed_at = d
        .posted_label
        .as_deref()
        .and_then(|label| posted::listed_at_from_label(label, now));
    let title = d
        .title
        .clone()
        .or_else(|| card.title.clone())
        .unwrap_or_else(|| "(untitled job)".into());

    sqlx::query(
        "INSERT INTO jobs (
            id, title, company, location, work_type, classification, subclassification,
            salary, short_description, description_text, description_html,
            posted_label, listed_at, job_url, apply_url, apply_type, scrape_status, scrape_error
         ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,'ok',NULL)
         ON CONFLICT (id) DO UPDATE SET
            title = EXCLUDED.title, company = EXCLUDED.company, location = EXCLUDED.location,
            work_type = EXCLUDED.work_type, classification = EXCLUDED.classification,
            subclassification = EXCLUDED.subclassification, salary = EXCLUDED.salary,
            short_description = EXCLUDED.short_description,
            description_text = EXCLUDED.description_text,
            description_html = EXCLUDED.description_html,
            posted_label = EXCLUDED.posted_label, listed_at = EXCLUDED.listed_at,
            apply_url = EXCLUDED.apply_url, apply_type = EXCLUDED.apply_type,
            scrape_status = 'ok', scrape_error = NULL, last_seen_at = now()",
    )
    .bind(&card.id)
    .bind(title)
    .bind(d.company.as_ref().or(card.company.as_ref()))
    .bind(d.location.as_ref().or(card.location.as_ref()))
    .bind(&d.work_type)
    .bind(d.classification.as_ref().or(card.classification.as_ref()))
    .bind(&card.subclassification)
    .bind(&d.salary)
    .bind(&card.short_description)
    .bind(&d.description_text)
    .bind(&d.description_html)
    .bind(&d.posted_label)
    .bind(listed_at)
    .bind(&card.job_url)
    .bind(&d.apply_url)
    .bind(&d.apply_type)
    .execute(db)
    .await
    .map(|_| ())
}

/// Keep what the search card told us, and remember why the details failed.
async fn save_failed(db: &PgPool, card: &JobCard, error: &str) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO jobs (id, title, company, location, short_description, classification,
                           subclassification, job_url, scrape_status, scrape_error)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, 'failed', $9)
         ON CONFLICT (id) DO UPDATE SET scrape_status = 'failed', scrape_error = EXCLUDED.scrape_error",
    )
    .bind(&card.id)
    .bind(card.title.clone().unwrap_or_else(|| "(untitled job)".into()))
    .bind(&card.company)
    .bind(&card.location)
    .bind(&card.short_description)
    .bind(&card.classification)
    .bind(&card.subclassification)
    .bind(&card.job_url)
    .bind(error)
    .execute(db)
    .await
    .map(|_| ())
}
