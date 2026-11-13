//! One-off import of v1 data from Supabase into the local database.
//! Safe to run twice: rows that already exist are skipped.
//!
//!   SUPABASE_DATABASE_URL=postgres://... cargo run -- import-supabase

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::{FromRow, PgPool, postgres::PgPoolOptions};

use crate::scraper::posted::listed_at_from_label;

#[derive(FromRow)]
struct OldJob {
    job_id: String,
    title: Option<String>,
    company: Option<String>,
    location: Option<String>,
    work_type: Option<String>,
    classification: Option<String>,
    subclassification: Option<String>,
    short_description: Option<String>,
    description: Option<String>,
    posted: Option<String>,
    job_url: Option<String>,
    apply_url: Option<String>,
    apply_type: Option<String>,
    date_found: Option<DateTime<Utc>>,
    scrape_status: Option<String>,
    scrape_error: Option<String>,
}

#[derive(FromRow)]
struct OldAnalysis {
    job_id: String,
    relevance_score: i16,
    kept: bool,
    analysis: Value,
    model: String,
    analysed_at: DateTime<Utc>,
}

#[derive(FromRow)]
struct OldApplication {
    job_id: String,
    status: String,
    applied_at: Option<DateTime<Utc>>,
    updated_at: DateTime<Utc>,
}

pub async fn run(local: &PgPool) -> Result<()> {
    let url = std::env::var("SUPABASE_DATABASE_URL")
        .context("Set SUPABASE_DATABASE_URL to your Supabase connection string")?;
    let supabase = PgPoolOptions::new()
        .max_connections(2)
        .connect(&url)
        .await
        .context("could not connect to Supabase")?;

    let jobs: Vec<OldJob> = sqlx::query_as(
        "SELECT job_id, title, company, location, work_type, classification, subclassification,
                short_description, description, posted, job_url, apply_url, apply_type,
                date_found, scrape_status, scrape_error
         FROM jobs",
    )
    .fetch_all(&supabase)
    .await?;

    let mut imported = 0;
    for job in &jobs {
        let found = job.date_found.unwrap_or_else(Utc::now);
        let listed_at = job
            .posted
            .as_deref()
            .and_then(|label| listed_at_from_label(label, found));
        let ok = job.scrape_status.as_deref() == Some("success");

        let result = sqlx::query(
            "INSERT INTO jobs (id, title, company, location, work_type, classification,
                subclassification, short_description, description_text, posted_label, listed_at,
                job_url, apply_url, apply_type, first_seen_at, last_seen_at, scrape_status,
                scrape_error)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$15,$16,$17)
             ON CONFLICT (id) DO NOTHING",
        )
        .bind(&job.job_id)
        .bind(job.title.clone().unwrap_or_else(|| "(untitled job)".into()))
        .bind(&job.company)
        .bind(&job.location)
        .bind(&job.work_type)
        .bind(&job.classification)
        .bind(&job.subclassification)
        .bind(&job.short_description)
        .bind(&job.description)
        .bind(&job.posted)
        .bind(listed_at)
        .bind(
            job.job_url
                .clone()
                .unwrap_or_else(|| format!("https://www.seek.com.au/job/{}", job.job_id)),
        )
        .bind(&job.apply_url)
        .bind(&job.apply_type)
        .bind(found)
        .bind(if ok { "ok" } else { "failed" })
        .bind(&job.scrape_error)
        .execute(local)
        .await?;
        imported += result.rows_affected();
    }
    println!(
        "Jobs:         {imported} imported, {} already there",
        jobs.len() as u64 - imported
    );

    let analyses: Vec<OldAnalysis> = sqlx::query_as(
        "SELECT job_id, relevance_score, kept, analysis, model, analysed_at FROM job_analysis",
    )
    .fetch_all(&supabase)
    .await?;
    let mut imported = 0;
    for a in &analyses {
        imported += sqlx::query(
            "INSERT INTO legacy_job_analysis (job_id, relevance_score, kept, analysis, model, analysed_at)
             VALUES ($1, $2, $3, $4, $5, $6) ON CONFLICT (job_id) DO NOTHING",
        )
        .bind(&a.job_id)
        .bind(a.relevance_score)
        .bind(a.kept)
        .bind(&a.analysis)
        .bind(&a.model)
        .bind(a.analysed_at)
        .execute(local)
        .await?
        .rows_affected();
    }
    println!(
        "Analyses:     {imported} imported, {} already there",
        analyses.len() as u64 - imported
    );

    let applications: Vec<OldApplication> =
        sqlx::query_as("SELECT job_id, status, applied_at, updated_at FROM applications")
            .fetch_all(&supabase)
            .await?;
    let mut imported = 0;
    for a in &applications {
        imported += sqlx::query(
            "INSERT INTO legacy_applications (job_id, status, applied_at, updated_at)
             VALUES ($1, $2, $3, $4) ON CONFLICT (job_id) DO NOTHING",
        )
        .bind(&a.job_id)
        .bind(&a.status)
        .bind(a.applied_at)
        .bind(a.updated_at)
        .execute(local)
        .await?
        .rows_affected();
    }
    println!(
        "Applications: {imported} imported, {} already there",
        applications.len() as u64 - imported
    );

    Ok(())
}
