//! The assistant's tools. Every one only reads: each runs inside a
//! READ ONLY transaction, so even a bug here can't change your data.
//! Contact details never leave this file: `my_profile` sends the same
//! evidence and facts the drafting prompts use, without them.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{FromRow, PgPool, Postgres, QueryBuilder, Transaction};

use crate::{
    documents::{prompt::evidence_block, resume::MasterResume},
    llm::ToolSpec,
    profile,
    scoring::prompt::PROMPT_VERSION,
    settings::Settings,
};

/// No tool sends the model more than this many rows.
pub const MAX_ROWS: i64 = 20;
const DESCRIPTION_CHARS: usize = 2_500;
const RESUME_CHARS: usize = 6_000;

/// What the page shows under an answer for each tool call, e.g. "Looked at 18 jobs".
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Chip {
    pub tool: String,
    pub label: String,
}

/// A tool's answer for the model, and the chip for the page.
pub struct Output {
    pub content: String,
    pub chip: Chip,
}

pub fn specs() -> Vec<ToolSpec> {
    vec![
        ToolSpec {
            name: "search_jobs",
            description: "Find jobs in the user's database. Returns at most 20, with the fit score (0-100), \
                whether it's relevant, the user's label and where it is on their board. Use it for \
                'which jobs should I apply to', 'anything new with Python', and so on.",
            parameters: json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Words to search titles, companies and descriptions for, e.g. \"power bi\"." },
                    "min_score": { "type": "integer", "description": "Only jobs scored at least this (0-100)." },
                    "relevant_only": { "type": "boolean", "description": "Only jobs at or above the user's threshold with no hard blockers." },
                    "stage": { "type": "string", "enum": ["not_applied", "active", "submitted", "interview", "offer", "rejected", "withdrawn"], "description": "not_applied = not sent yet." },
                    "label": { "type": "string", "enum": ["apply", "maybe", "no"], "description": "The user's own label." },
                    "posted_within_days": { "type": "integer", "description": "Only jobs posted in the last N days (1-365)." },
                    "sort": { "type": "string", "enum": ["best_fit", "newest"], "description": "Default best_fit." },
                    "limit": { "type": "integer", "description": "1-20, default 10." }
                }
            }),
        },
        ToolSpec {
            name: "job_details",
            description: "Everything about one job: the ad text, its score with matched and missing \
                requirements, hard blockers, and the user's documents, application and emails for it.",
            parameters: json!({
                "type": "object",
                "properties": { "job_id": { "type": "string", "description": "The id from search_jobs or my_applications." } },
                "required": ["job_id"]
            }),
        },
        ToolSpec {
            name: "pipeline",
            description: "The user's application numbers: how many sent this week and month, replies, \
                interviews, offers, the response rate, and how replies vary with the fit score.",
            parameters: json!({ "type": "object", "properties": {} }),
        },
        ToolSpec {
            name: "missing_skills",
            description: "The requirements that come up most often as missing (and as matched) across \
                scored jobs. Use it for 'what skill should I learn' or 'what keeps holding me back'.",
            parameters: json!({
                "type": "object",
                "properties": {
                    "posted_within_days": { "type": "integer", "description": "Only jobs posted in the last N days. Default 60." },
                    "min_score": { "type": "integer", "description": "Only jobs scored at least this. Default 0." }
                }
            }),
        },
        ToolSpec {
            name: "my_applications",
            description: "The jobs on the user's board (Applications page) with their stage and dates, \
                and the follow-up emails that are ready to send.",
            parameters: json!({
                "type": "object",
                "properties": {
                    "stage": { "type": "string", "enum": ["active", "submitted", "interview", "offer", "rejected", "withdrawn"] }
                }
            }),
        },
        ToolSpec {
            name: "my_profile",
            description: "The user's resume (summary, skills, experience, projects, education) and work \
                rights. Contact details are left out.",
            parameters: json!({ "type": "object", "properties": {} }),
        },
    ]
}

/// Run one tool call. A bad argument comes back as an error for the model to
/// read and fix, not as a failure of the whole answer.
pub async fn run(
    db: &PgPool,
    settings: &Settings,
    name: &str,
    args: &Value,
) -> sqlx::Result<Output> {
    let mut tx = db.begin().await?;
    sqlx::query("SET TRANSACTION READ ONLY")
        .execute(&mut *tx)
        .await?;
    let result = match name {
        "search_jobs" => search_jobs(&mut tx, settings, args).await,
        "job_details" => job_details(&mut tx, settings, args).await,
        "pipeline" => pipeline(&mut tx, settings).await,
        "missing_skills" => missing_skills(&mut tx, settings, args).await,
        "my_applications" => my_applications(&mut tx, args).await,
        "my_profile" => my_profile(&mut tx).await,
        _ => Ok(Err(format!("There is no tool called {name:?}."))),
    };
    tx.rollback().await?;
    Ok(match result? {
        Ok(output) => output,
        Err(problem) => Output {
            content: json!({ "error": problem }).to_string(),
            chip: Chip {
                tool: name.to_string(),
                label: format!("Couldn't run {}", name.replace('_', " ")),
            },
        },
    })
}

/// Ok(Err(..)) is a problem with the arguments, for the model to fix.
type ToolResult = sqlx::Result<Result<Output, String>>;
type Tx = Transaction<'static, Postgres>;

fn int_arg(
    args: &Value,
    key: &str,
    range: std::ops::RangeInclusive<i64>,
) -> Result<Option<i64>, String> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => {
            // Models sometimes send numbers as strings.
            let n = v
                .as_i64()
                .or_else(|| v.as_f64().map(|f| f as i64))
                .or_else(|| v.as_str().and_then(|s| s.trim().parse().ok()))
                .ok_or_else(|| format!("{key} must be a whole number."))?;
            if range.contains(&n) {
                Ok(Some(n))
            } else {
                Err(format!("{key} must be {}-{}.", range.start(), range.end()))
            }
        }
    }
}

fn text_arg(args: &Value, key: &str) -> Option<String> {
    args.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

fn one_of(args: &Value, key: &str, allowed: &[&str]) -> Result<Option<String>, String> {
    match text_arg(args, key) {
        None => Ok(None),
        Some(v) if allowed.contains(&v.as_str()) => Ok(Some(v)),
        Some(v) => Err(format!("{key} {v:?} isn't one of {}.", allowed.join(", "))),
    }
}

fn bool_arg(args: &Value, key: &str) -> bool {
    match args.get(key) {
        Some(Value::Bool(b)) => *b,
        Some(Value::String(s)) => s == "true",
        _ => false,
    }
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

fn link(id: &str) -> String {
    format!("/jobs/{id}")
}

// ---------- search_jobs ----------

#[derive(FromRow)]
struct JobRow {
    id: String,
    title: String,
    company: Option<String>,
    location: Option<String>,
    salary: Option<String>,
    work_type: Option<String>,
    apply_type: Option<String>,
    posted: Option<DateTime<Utc>>,
    score: Option<i16>,
    relevant: Option<bool>,
    label: Option<String>,
    stage: Option<String>,
}

async fn search_jobs(tx: &mut Tx, settings: &Settings, args: &Value) -> ToolResult {
    let parse = || -> Result<_, String> {
        Ok((
            text_arg(args, "query"),
            int_arg(args, "min_score", 0..=100)?,
            one_of(
                args,
                "stage",
                &[
                    "not_applied",
                    "active",
                    "submitted",
                    "interview",
                    "offer",
                    "rejected",
                    "withdrawn",
                ],
            )?,
            one_of(args, "label", &["apply", "maybe", "no"])?,
            int_arg(args, "posted_within_days", 1..=365)?,
            one_of(args, "sort", &["best_fit", "newest"])?,
            int_arg(args, "limit", 1..=MAX_ROWS)?.unwrap_or(10),
        ))
    };
    let (query, min_score, stage, label, days, sort, limit) = match parse() {
        Ok(p) => p,
        Err(e) => return Ok(Err(e)),
    };

    let mut qb: QueryBuilder<Postgres> = QueryBuilder::new(
        "SELECT j.id, j.title, j.company, j.location, j.salary, j.work_type, j.apply_type,
                coalesce(j.listed_at, j.first_seen_at) AS posted, s.score,
                (s.score >= ",
    );
    qb.push_bind(settings.relevance_threshold).push(
        " AND s.blocker_count = 0) AS relevant, lb.label, t.stage
          FROM jobs j
          LEFT JOIN LATERAL (
              SELECT score, jsonb_array_length(hard_blockers) AS blocker_count
              FROM job_scores WHERE job_id = j.id AND model = ",
    );
    qb.push_bind(settings.scoring_model.clone())
        .push(" AND prompt_version = ")
        .push_bind(PROMPT_VERSION)
        .push(
            " ORDER BY created_at DESC LIMIT 1) s ON TRUE
              LEFT JOIN job_labels lb ON lb.job_id = j.id
              LEFT JOIN tracker t ON t.job_id = j.id
              WHERE j.source <> 'practice' AND NOT j.hidden",
        );
    if let Some(q) = &query {
        qb.push(" AND j.search_vector @@ websearch_to_tsquery('english', ")
            .push_bind(q.clone())
            .push(")");
    }
    if let Some(min) = min_score {
        qb.push(" AND s.score >= ").push_bind(min as i32);
    }
    if bool_arg(args, "relevant_only") {
        qb.push(" AND s.blocker_count = 0 AND s.score >= ")
            .push_bind(settings.relevance_threshold);
    }
    match stage.as_deref() {
        None => {}
        Some("not_applied") => {
            qb.push(" AND (t.job_id IS NULL OR (t.stage = 'active' AND t.applied_at IS NULL))");
        }
        Some(s) => {
            qb.push(" AND t.stage = ").push_bind(s.to_string());
        }
    }
    if let Some(l) = &label {
        qb.push(" AND lb.label = ").push_bind(l.clone());
    }
    if let Some(d) = days {
        qb.push(" AND coalesce(j.listed_at, j.first_seen_at) >= now() - make_interval(days => ")
            .push_bind(d as i32)
            .push(")");
    }
    if sort.as_deref() == Some("newest") {
        qb.push(" ORDER BY coalesce(j.listed_at, j.first_seen_at) DESC, j.id");
    } else {
        qb.push(
            " ORDER BY (s.blocker_count = 0) DESC NULLS LAST, s.score DESC NULLS LAST,
              coalesce(j.listed_at, j.first_seen_at) DESC, j.id",
        );
    }
    qb.push(" LIMIT ").push_bind(limit);

    let rows: Vec<JobRow> = qb.build_query_as().fetch_all(&mut **tx).await?;
    let jobs: Vec<Value> = rows
        .iter()
        .map(|r| {
            json!({
                "id": r.id,
                "title": r.title,
                "company": r.company,
                "location": r.location,
                "salary": r.salary,
                "work_type": r.work_type,
                "quick_apply": r.apply_type.as_deref().is_some_and(|a| a.to_lowercase().contains("quick")),
                "posted": r.posted.map(|p| p.date_naive().to_string()),
                "score": r.score,
                "relevant": r.relevant,
                "your_label": r.label,
                "board_stage": r.stage.clone().unwrap_or_else(|| "not on board".into()),
                "link": link(&r.id),
            })
        })
        .collect();
    let n = jobs.len();
    Ok(Ok(Output {
        content:
            json!({ "count": n, "relevance_threshold": settings.relevance_threshold, "jobs": jobs })
                .to_string(),
        chip: Chip {
            tool: "search_jobs".into(),
            label: format!("Looked at {}", plural(n, "job", "jobs")),
        },
    }))
}

// ---------- job_details ----------

#[derive(FromRow)]
struct DetailRow {
    title: String,
    company: Option<String>,
    location: Option<String>,
    salary: Option<String>,
    work_type: Option<String>,
    apply_type: Option<String>,
    posted: Option<DateTime<Utc>>,
    description_text: Option<String>,
    label: Option<String>,
    stage: Option<String>,
    applied_at: Option<DateTime<Utc>>,
}

#[derive(FromRow)]
struct ScoreRow {
    score: i16,
    summary: String,
    matched: Value,
    missing: Value,
    hard_blockers: Value,
}

async fn job_details(tx: &mut Tx, settings: &Settings, args: &Value) -> ToolResult {
    let Some(id) = text_arg(args, "job_id") else {
        return Ok(Err("job_id is required.".into()));
    };
    let job: Option<DetailRow> = sqlx::query_as(
        "SELECT j.title, j.company, j.location, j.salary, j.work_type, j.apply_type,
                coalesce(j.listed_at, j.first_seen_at) AS posted, j.description_text,
                lb.label, t.stage, t.applied_at
         FROM jobs j
         LEFT JOIN job_labels lb ON lb.job_id = j.id
         LEFT JOIN tracker t ON t.job_id = j.id
         WHERE j.id = $1 AND j.source <> 'practice'",
    )
    .bind(&id)
    .fetch_optional(&mut **tx)
    .await?;
    let Some(job) = job else {
        return Ok(Err(format!(
            "No job with id {id:?}. Use search_jobs to find ids."
        )));
    };
    let score: Option<ScoreRow> = sqlx::query_as(
        "SELECT score, summary, matched, missing, hard_blockers FROM job_scores
         WHERE job_id = $1 AND model = $2 AND prompt_version = $3
         ORDER BY created_at DESC LIMIT 1",
    )
    .bind(&id)
    .bind(&settings.scoring_model)
    .bind(PROMPT_VERSION)
    .fetch_optional(&mut **tx)
    .await?;
    let documents: Vec<(String, String)> = sqlx::query_as(
        "SELECT DISTINCT ON (kind) kind, status FROM documents
         WHERE job_id = $1 ORDER BY kind, created_at DESC",
    )
    .bind(&id)
    .fetch_all(&mut **tx)
    .await?;
    let run: Option<(String,)> = sqlx::query_as(
        "SELECT status FROM applications WHERE job_id = $1 AND NOT practice
         ORDER BY created_at DESC LIMIT 1",
    )
    .bind(&id)
    .fetch_optional(&mut **tx)
    .await?;
    let emails: Vec<(String, String)> =
        sqlx::query_as("SELECT kind, status FROM emails WHERE job_id = $1 ORDER BY due_at")
            .bind(&id)
            .fetch_all(&mut **tx)
            .await?;

    let description: String = job
        .description_text
        .as_deref()
        .unwrap_or("")
        .chars()
        .take(DESCRIPTION_CHARS)
        .collect();
    let content = json!({
        "id": id,
        "title": job.title,
        "company": job.company,
        "location": job.location,
        "salary": job.salary,
        "work_type": job.work_type,
        "quick_apply": job.apply_type.as_deref().is_some_and(|a| a.to_lowercase().contains("quick")),
        "posted": job.posted.map(|p| p.date_naive().to_string()),
        "link": link(&id),
        "your_label": job.label,
        "board_stage": job.stage.unwrap_or_else(|| "not on board".into()),
        "applied_on": job.applied_at.map(|a| a.date_naive().to_string()),
        "score": score.as_ref().map(|s| json!({
            "score": s.score,
            "relevant": i32::from(s.score) >= settings.relevance_threshold
                && s.hard_blockers.as_array().is_none_or(Vec::is_empty),
            "summary": s.summary,
            "matched": s.matched,
            "missing": s.missing,
            "hard_blockers": s.hard_blockers,
        })),
        "documents": documents.iter().map(|(k, s)| json!({ "kind": k, "status": s })).collect::<Vec<_>>(),
        "agent_run": run.map(|r| r.0),
        "emails": emails.iter().map(|(k, s)| json!({ "kind": k, "status": s })).collect::<Vec<_>>(),
        "ad_text": description,
    });
    Ok(Ok(Output {
        content: content.to_string(),
        chip: Chip {
            tool: "job_details".into(),
            label: format!("Read {}", job.title),
        },
    }))
}

// ---------- pipeline ----------

#[derive(FromRow, Serialize)]
struct Funnel {
    on_board_not_sent: i64,
    sent_total: i64,
    sent_last_7_days: i64,
    sent_last_30_days: i64,
    replied: i64,
    interviews: i64,
    offers: i64,
    rejected: i64,
    median_days_to_reply: Option<f64>,
    relevant_jobs_not_on_board: i64,
}

#[derive(FromRow, Serialize)]
struct Band {
    score_band: String,
    sent: i64,
    replied: i64,
    interviews: i64,
}

async fn pipeline(tx: &mut Tx, settings: &Settings) -> ToolResult {
    let funnel: Funnel = sqlx::query_as(
        "WITH latest AS (
            SELECT DISTINCT ON (job_id) job_id, score, jsonb_array_length(hard_blockers) AS blockers
            FROM job_scores WHERE model = $1 AND prompt_version = $2
            ORDER BY job_id, created_at DESC
         )
         SELECT
            (SELECT count(*) FROM tracker WHERE stage = 'active') AS on_board_not_sent,
            count(*) AS sent_total,
            count(*) FILTER (WHERE t.applied_at > now() - interval '7 days') AS sent_last_7_days,
            count(*) FILTER (WHERE t.applied_at > now() - interval '30 days') AS sent_last_30_days,
            count(*) FILTER (WHERE t.replied_at IS NOT NULL OR t.stage IN ('interview', 'offer', 'rejected')) AS replied,
            count(*) FILTER (WHERE t.stage IN ('interview', 'offer')
                OR EXISTS (SELECT 1 FROM tracker_moves m WHERE m.job_id = t.job_id AND m.to_stage = 'interview')) AS interviews,
            count(*) FILTER (WHERE t.stage = 'offer') AS offers,
            count(*) FILTER (WHERE t.stage = 'rejected') AS rejected,
            (percentile_cont(0.5) WITHIN GROUP (
                ORDER BY extract(epoch FROM t.replied_at - t.applied_at) / 86400.0))::float8 AS median_days_to_reply,
            (SELECT count(*) FROM jobs j JOIN latest l ON l.job_id = j.id
              LEFT JOIN tracker t2 ON t2.job_id = j.id
              WHERE NOT j.hidden AND j.source <> 'practice' AND t2.job_id IS NULL
                AND l.score >= $3 AND l.blockers = 0) AS relevant_jobs_not_on_board
         FROM tracker t WHERE t.stage <> 'active'",
    )
    .bind(&settings.scoring_model)
    .bind(PROMPT_VERSION)
    .bind(settings.relevance_threshold)
    .fetch_one(&mut **tx)
    .await?;
    let bands: Vec<Band> = sqlx::query_as(
        "WITH latest AS (
            SELECT DISTINCT ON (job_id) job_id, score
            FROM job_scores WHERE model = $1 AND prompt_version = $2
            ORDER BY job_id, created_at DESC
         )
         SELECT CASE WHEN coalesce(l.score, la.relevance_score) IS NULL THEN 'not scored'
                     WHEN coalesce(l.score, la.relevance_score) >= 80 THEN '80-100'
                     WHEN coalesce(l.score, la.relevance_score) >= 70 THEN '70-79'
                     WHEN coalesce(l.score, la.relevance_score) >= 50 THEN '50-69'
                     ELSE 'under 50' END AS score_band,
                count(*) AS sent,
                count(*) FILTER (WHERE t.replied_at IS NOT NULL OR t.stage IN ('interview', 'offer', 'rejected')) AS replied,
                count(*) FILTER (WHERE t.stage IN ('interview', 'offer')) AS interviews
         FROM tracker t
         LEFT JOIN latest l ON l.job_id = t.job_id
         LEFT JOIN legacy_job_analysis la ON la.job_id = t.job_id
         WHERE t.stage <> 'active'
         GROUP BY 1 ORDER BY 1",
    )
    .bind(&settings.scoring_model)
    .bind(PROMPT_VERSION)
    .fetch_all(&mut **tx)
    .await?;
    let response_rate = (funnel.sent_total > 0)
        .then(|| (funnel.replied as f64 / funnel.sent_total as f64 * 100.0).round());
    Ok(Ok(Output {
        content:
            json!({ "numbers": funnel, "response_rate_percent": response_rate, "by_score": bands })
                .to_string(),
        chip: Chip {
            tool: "pipeline".into(),
            label: "Checked your application numbers".into(),
        },
    }))
}

// ---------- missing_skills ----------

async fn missing_skills(tx: &mut Tx, settings: &Settings, args: &Value) -> ToolResult {
    let (days, min_score) = match (|| -> Result<_, String> {
        Ok((
            int_arg(args, "posted_within_days", 1..=365)?.unwrap_or(60),
            int_arg(args, "min_score", 0..=100)?.unwrap_or(0),
        ))
    })() {
        Ok(p) => p,
        Err(e) => return Ok(Err(e)),
    };

    // The model writes requirements a little differently each time, so they're
    // grouped case-insensitively. The model is told to merge near-duplicates.
    macro_rules! counted {
        ($column:literal) => {
            concat!(
                "WITH latest AS (
                    SELECT DISTINCT ON (s.job_id) s.job_id, s.score, s.", $column, " AS items
                    FROM job_scores s JOIN jobs j ON j.id = s.job_id
                    WHERE s.model = $1 AND s.prompt_version = $2 AND NOT j.hidden
                      AND j.source <> 'practice'
                      AND coalesce(j.listed_at, j.first_seen_at) >= now() - make_interval(days => $3)
                    ORDER BY s.job_id, s.created_at DESC
                 )
                 SELECT min(item COLLATE \"C\") AS skill, count(DISTINCT job_id) AS jobs
                 FROM latest, jsonb_array_elements_text(items) AS item
                 WHERE score >= $4
                 GROUP BY lower(trim(item))
                 ORDER BY jobs DESC, skill
                 LIMIT 15"
            )
        };
    }
    let considered: i64 = sqlx::query_scalar(
        "SELECT count(DISTINCT s.job_id) FROM job_scores s JOIN jobs j ON j.id = s.job_id
         WHERE s.model = $1 AND s.prompt_version = $2 AND NOT j.hidden AND j.source <> 'practice'
           AND coalesce(j.listed_at, j.first_seen_at) >= now() - make_interval(days => $3)
           AND s.score >= $4",
    )
    .bind(&settings.scoring_model)
    .bind(PROMPT_VERSION)
    .bind(days as i32)
    .bind(min_score as i32)
    .fetch_one(&mut **tx)
    .await?;
    let mut lists = Vec::new();
    for sql in [counted!("missing"), counted!("matched")] {
        let rows: Vec<(String, i64)> = sqlx::query_as(sql)
            .bind(&settings.scoring_model)
            .bind(PROMPT_VERSION)
            .bind(days as i32)
            .bind(min_score as i32)
            .fetch_all(&mut **tx)
            .await?;
        lists.push(
            rows.into_iter()
                .map(|(skill, jobs)| json!({ "requirement": skill, "jobs": jobs }))
                .collect::<Vec<_>>(),
        );
    }
    let matched = lists.pop().unwrap_or_default();
    let missing = lists.pop().unwrap_or_default();
    Ok(Ok(Output {
        content: json!({
            "jobs_considered": considered,
            "posted_within_days": days,
            "most_often_missing": missing,
            "most_often_matched": matched,
        })
        .to_string(),
        chip: Chip {
            tool: "missing_skills".into(),
            label: format!(
                "Counted requirements across {}",
                plural(considered as usize, "job", "jobs")
            ),
        },
    }))
}

// ---------- my_applications ----------

#[derive(FromRow)]
struct CardRow {
    job_id: String,
    title: String,
    company: Option<String>,
    stage: String,
    applied_at: Option<DateTime<Utc>>,
    stage_changed_at: DateTime<Utc>,
}

async fn my_applications(tx: &mut Tx, args: &Value) -> ToolResult {
    let stage = match one_of(
        args,
        "stage",
        &[
            "active",
            "submitted",
            "interview",
            "offer",
            "rejected",
            "withdrawn",
        ],
    ) {
        Ok(s) => s,
        Err(e) => return Ok(Err(e)),
    };
    let counts: Vec<(String, i64)> =
        sqlx::query_as("SELECT stage, count(*) FROM tracker GROUP BY stage ORDER BY stage")
            .fetch_all(&mut **tx)
            .await?;
    let rows: Vec<CardRow> = sqlx::query_as(
        "SELECT t.job_id, j.title, j.company, t.stage, t.applied_at, t.stage_changed_at
         FROM tracker t JOIN jobs j ON j.id = t.job_id
         WHERE ($1::text IS NULL OR t.stage = $1)
         ORDER BY t.stage_changed_at DESC LIMIT $2",
    )
    .bind(&stage)
    .bind(MAX_ROWS)
    .fetch_all(&mut **tx)
    .await?;
    let due: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT e.job_id, j.title, e.kind FROM emails e JOIN jobs j ON j.id = e.job_id
         WHERE e.status = 'ready' AND e.due_at <= now() ORDER BY e.due_at LIMIT 20",
    )
    .fetch_all(&mut **tx)
    .await?;
    let n = rows.len();
    let content = json!({
        "count_by_stage": counts.into_iter()
            .map(|(k, v)| (k, json!(v))).collect::<serde_json::Map<_, _>>(),
        "applications": rows.iter().map(|r| json!({
            "job_id": r.job_id,
            "title": r.title,
            "company": r.company,
            "stage": r.stage,
            "applied_on": r.applied_at.map(|a| a.date_naive().to_string()),
            "days_in_stage": (Utc::now() - r.stage_changed_at).num_days(),
            "link": link(&r.job_id),
        })).collect::<Vec<_>>(),
        "emails_ready_to_send": due.iter().map(|(id, title, kind)| json!({
            "job_id": id, "title": title, "kind": kind, "link": link(id),
        })).collect::<Vec<_>>(),
    });
    Ok(Ok(Output {
        content: content.to_string(),
        chip: Chip {
            tool: "my_applications".into(),
            label: format!("Looked at {}", plural(n, "application", "applications")),
        },
    }))
}

// ---------- my_profile ----------

async fn my_profile(tx: &mut Tx) -> ToolResult {
    let resume: Option<(String,)> =
        sqlx::query_as("SELECT content FROM resume_versions ORDER BY created_at DESC LIMIT 1")
            .fetch_optional(&mut **tx)
            .await?;
    let facts = profile::load_facts(&mut **tx).await?;
    let Some((markdown,)) = resume else {
        return Ok(Err(
            "The user hasn't saved a resume yet (Profile page).".into()
        ));
    };
    // The evidence block has no name or contact lines.
    let evidence: String = evidence_block(&MasterResume::parse(&markdown))
        .chars()
        .take(RESUME_CHARS)
        .collect();
    Ok(Ok(Output {
        content: format!(
            "RESUME\n{evidence}\nFACTS\n{}",
            profile::profile_for_prompt(&facts)
        ),
        chip: Chip {
            tool: "my_profile".into(),
            label: "Read your resume".into(),
        },
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_can_arrive_as_strings() {
        let args = json!({ "min_score": "70", "limit": 5.0 });
        assert_eq!(int_arg(&args, "min_score", 0..=100), Ok(Some(70)));
        assert_eq!(int_arg(&args, "limit", 1..=20), Ok(Some(5)));
        assert_eq!(int_arg(&args, "missing", 1..=20), Ok(None));
        assert!(int_arg(&json!({ "limit": 50 }), "limit", 1..=20).is_err());
    }

    #[test]
    fn unknown_choices_are_explained() {
        let error = one_of(&json!({ "stage": "sent" }), "stage", &["submitted"]).unwrap_err();
        assert!(error.contains("submitted"));
    }

    #[test]
    fn every_tool_has_an_object_schema() {
        for spec in specs() {
            assert_eq!(spec.parameters["type"], "object", "{}", spec.name);
        }
        assert_eq!(specs().len(), 6);
    }
}
