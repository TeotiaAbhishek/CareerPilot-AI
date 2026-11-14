//! Your master resume (every save is kept as a version) and your application
//! facts. The scoring worker reads both from here.

use std::path::Path as FsPath;

use axum::{
    Json,
    extract::{Path, State},
};
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    AppState,
    error::{AppError, AppResult},
};

const MAX_RESUME_CHARS: usize = 50_000;

// ---------- Resume ----------

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct ResumeVersion {
    pub id: Uuid,
    pub content: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct VersionInfo {
    pub id: Uuid,
    pub created_at: DateTime<Utc>,
    pub chars: i32,
}

#[derive(Serialize)]
pub struct ResumeView {
    pub current: Option<ResumeVersion>,
    /// The latest 20 versions, newest first (without their text).
    pub versions: Vec<VersionInfo>,
}

pub async fn current_resume(db: &PgPool) -> sqlx::Result<Option<ResumeVersion>> {
    sqlx::query_as(
        "SELECT id, content, created_at FROM resume_versions ORDER BY created_at DESC LIMIT 1",
    )
    .fetch_optional(db)
    .await
}

pub async fn get_resume(State(state): State<AppState>) -> AppResult<Json<ResumeView>> {
    let current = current_resume(&state.db).await?;
    let versions = sqlx::query_as(
        "SELECT id, created_at, length(content) AS chars FROM resume_versions
         ORDER BY created_at DESC LIMIT 20",
    )
    .fetch_all(&state.db)
    .await?;
    Ok(Json(ResumeView { current, versions }))
}

pub async fn get_resume_version(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<ResumeVersion>> {
    sqlx::query_as("SELECT id, content, created_at FROM resume_versions WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.db)
        .await?
        .map(Json)
        .ok_or_else(|| AppError::NotFound("Version not found.".into()))
}

#[derive(Deserialize)]
pub struct ResumeInput {
    pub content: String,
}

/// Save a new version, unless nothing changed.
pub async fn save_resume(
    State(state): State<AppState>,
    Json(input): Json<ResumeInput>,
) -> AppResult<Json<ResumeView>> {
    let content = input.content.trim();
    if content.is_empty() {
        return Err(AppError::BadRequest("Your resume can't be empty.".into()));
    }
    if content.chars().count() > MAX_RESUME_CHARS {
        return Err(AppError::BadRequest(format!(
            "That's over {MAX_RESUME_CHARS} characters. Keep the master resume focused."
        )));
    }

    let unchanged = current_resume(&state.db)
        .await?
        .is_some_and(|r| r.content == content);
    if !unchanged {
        sqlx::query("INSERT INTO resume_versions (content) VALUES ($1)")
            .bind(content)
            .execute(&state.db)
            .await?;
    }
    get_resume(State(state)).await
}

/// On first start, copy v1's resume file into the database (only if there is none yet).
pub async fn import_resume_file(db: &PgPool, path: &FsPath) -> anyhow::Result<bool> {
    if current_resume(db).await?.is_some() {
        return Ok(false);
    }
    let Ok(content) = tokio::fs::read_to_string(path).await else {
        return Ok(false);
    };
    let content = content.trim();
    if content.is_empty() {
        return Ok(false);
    }
    sqlx::query("INSERT INTO resume_versions (content) VALUES ($1)")
        .bind(content)
        .execute(db)
        .await?;
    Ok(true)
}

// ---------- Facts ----------

#[derive(Debug, Clone, Default, Serialize, Deserialize, sqlx::FromRow)]
pub struct Facts {
    pub full_name: String,
    pub email: String,
    pub phone: String,
    pub linkedin_url: String,
    pub website_url: String,
    pub street_address: String,
    pub postcode: String,
    pub city: String,
    pub state: String,
    pub country: String,
    pub willing_to_relocate: bool,
    pub australian_citizen: bool,
    pub australian_permanent_resident: bool,
    pub new_zealand_citizen: bool,
    pub australian_work_rights: bool,
    pub visa_type: String,
    pub work_rights_expiry: Option<NaiveDate>,
    pub notice_period: String,
    pub salary_expectation: String,
    pub available_from: Option<NaiveDate>,
}

#[derive(Serialize)]
pub struct FactsView {
    #[serde(flatten)]
    pub facts: Facts,
    pub updated_at: DateTime<Utc>,
}

pub async fn load_facts(db: &PgPool) -> sqlx::Result<Facts> {
    sqlx::query_as(
        "SELECT full_name, email, phone, linkedin_url, website_url, street_address, postcode,
                city, state, country, willing_to_relocate, australian_citizen,
                australian_permanent_resident, new_zealand_citizen, australian_work_rights,
                visa_type, work_rights_expiry, notice_period, salary_expectation, available_from
         FROM profile_facts",
    )
    .fetch_one(db)
    .await
}

pub async fn get_facts(State(state): State<AppState>) -> AppResult<Json<FactsView>> {
    let facts = load_facts(&state.db).await?;
    let updated_at = sqlx::query_scalar("SELECT updated_at FROM profile_facts")
        .fetch_one(&state.db)
        .await?;
    Ok(Json(FactsView { facts, updated_at }))
}

/// Replace all the facts at once (the form always sends every field).
pub async fn save_facts(
    State(state): State<AppState>,
    Json(input): Json<Facts>,
) -> AppResult<Json<FactsView>> {
    let f = clean_facts(input)?;
    sqlx::query(
        "UPDATE profile_facts SET
            full_name = $1, email = $2, phone = $3, linkedin_url = $4, website_url = $5,
            street_address = $6, postcode = $7, city = $8, state = $9, country = $10,
            willing_to_relocate = $11, australian_citizen = $12,
            australian_permanent_resident = $13, new_zealand_citizen = $14,
            australian_work_rights = $15, visa_type = $16, work_rights_expiry = $17,
            notice_period = $18, salary_expectation = $19, available_from = $20,
            updated_at = now()",
    )
    .bind(&f.full_name)
    .bind(&f.email)
    .bind(&f.phone)
    .bind(&f.linkedin_url)
    .bind(&f.website_url)
    .bind(&f.street_address)
    .bind(&f.postcode)
    .bind(&f.city)
    .bind(&f.state)
    .bind(&f.country)
    .bind(f.willing_to_relocate)
    .bind(f.australian_citizen)
    .bind(f.australian_permanent_resident)
    .bind(f.new_zealand_citizen)
    .bind(f.australian_work_rights)
    .bind(&f.visa_type)
    .bind(f.work_rights_expiry)
    .bind(&f.notice_period)
    .bind(&f.salary_expectation)
    .bind(f.available_from)
    .execute(&state.db)
    .await?;
    get_facts(State(state)).await
}

fn clean_facts(mut f: Facts) -> AppResult<Facts> {
    for field in [
        &mut f.full_name,
        &mut f.email,
        &mut f.phone,
        &mut f.linkedin_url,
        &mut f.website_url,
        &mut f.street_address,
        &mut f.postcode,
        &mut f.city,
        &mut f.state,
        &mut f.country,
        &mut f.visa_type,
        &mut f.notice_period,
        &mut f.salary_expectation,
    ] {
        *field = field.trim().to_string();
        if field.chars().count() > 300 {
            return Err(AppError::BadRequest(
                "A field is over 300 characters.".into(),
            ));
        }
    }
    if !f.email.is_empty() && !looks_like_email(&f.email) {
        return Err(AppError::BadRequest(
            "That email address doesn't look right.".into(),
        ));
    }
    for url in [&f.linkedin_url, &f.website_url] {
        if !url.is_empty() && reqwest::Url::parse(url).is_err() {
            return Err(AppError::BadRequest(format!(
                "{url:?} isn't a full URL (https://…)."
            )));
        }
    }
    Ok(f)
}

fn looks_like_email(value: &str) -> bool {
    let Some((user, domain)) = value.split_once('@') else {
        return false;
    };
    !user.is_empty() && domain.contains('.') && !domain.starts_with('.') && !value.contains(' ')
}

/// The candidate profile as the scoring prompt sees it: the same YAML layout
/// as v1's `candidate_profile.yaml`. Contact details are deliberately left out:
/// the model doesn't need your phone number to judge fit.
pub fn profile_for_prompt(f: &Facts) -> String {
    let text = |v: &str| {
        if v.is_empty() {
            "unknown".to_string()
        } else {
            format!("{v:?}")
        }
    };
    let date = |d: Option<NaiveDate>| d.map_or("unknown".to_string(), |d| format!("\"{d}\""));
    format!(
        "eligibility:\n  australian_citizen: {}\n  australian_permanent_resident: {}\n  \
         new_zealand_citizen: {}\n\n  australian_work_rights: {}\n  visa_type: {}\n  \
         work_rights_expiry: {}\n\nlocation:\n  city: {}\n  state: {}\n  country: {}\n\n\
         preferences:\n  willing_to_relocate: {}\n",
        f.australian_citizen,
        f.australian_permanent_resident,
        f.new_zealand_citizen,
        f.australian_work_rights,
        text(&f.visa_type),
        date(f.work_rights_expiry),
        text(&f.city),
        text(&f.state),
        text(&f.country),
        f.willing_to_relocate,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_profile_matches_v1_layout_and_skips_contact_details() {
        let facts = Facts {
            full_name: "Test Person".into(),
            phone: "0400 000 000".into(),
            australian_work_rights: true,
            visa_type: "Temporary Graduate visa (subclass 485)".into(),
            work_rights_expiry: NaiveDate::from_ymd_opt(2029, 1, 16),
            city: "Sydney".into(),
            state: "NSW".into(),
            country: "Australia".into(),
            ..Facts::default()
        };
        let yaml = profile_for_prompt(&facts);
        assert!(yaml.contains("australian_work_rights: true"));
        assert!(yaml.contains("visa_type: \"Temporary Graduate visa (subclass 485)\""));
        assert!(yaml.contains("work_rights_expiry: \"2029-01-16\""));
        assert!(yaml.contains("city: \"Sydney\""));
        assert!(!yaml.contains("Test Person"));
        assert!(!yaml.contains("0400"));
    }

    #[test]
    fn email_check() {
        assert!(looks_like_email("a@b.com"));
        assert!(!looks_like_email("a@b"));
        assert!(!looks_like_email("@b.com"));
        assert!(!looks_like_email("a b@c.com"));
    }
}
