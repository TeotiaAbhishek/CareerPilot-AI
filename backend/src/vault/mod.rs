//! The password vault: one login per site, the password encrypted with
//! AES-256-GCM (see `crypto.rs`). Lists never include passwords; you fetch
//! one at a time with "reveal", and every reveal is logged.

pub mod crypto;
pub mod password;

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    AppState,
    error::{AppError, AppResult, is_unique_violation},
};
use crypto::VaultKey;

const LOCKED: &str =
    "The vault is locked: VAULT_KEY is not set. Run `cargo run -- vault-init` in backend/.";

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct Credential {
    pub id: Uuid,
    pub site: String,
    pub login_url: String,
    pub username: String,
    pub notes: String,
    pub created_by: String,
    pub last_used_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CredentialInput {
    /// A domain ("seek.com.au") or any URL on the site.
    pub site: String,
    pub login_url: Option<String>,
    pub username: String,
    /// Required when creating. When updating, leave it out to keep the current one.
    pub password: Option<String>,
    pub notes: Option<String>,
}

fn key(state: &AppState) -> AppResult<&VaultKey> {
    state
        .vault
        .as_ref()
        .ok_or_else(|| AppError::Unavailable(LOCKED.into()))
}

/// "https://www.Seek.com.au/login" -> "seek.com.au". Sites are matched by this.
pub fn normalize_site(input: &str) -> Option<String> {
    let input = input.trim().to_lowercase();
    let host = if input.contains("://") {
        reqwest::Url::parse(&input).ok()?.host_str()?.to_string()
    } else {
        input.split(['/', '?', '#']).next()?.to_string()
    };
    let host = host.trim_start_matches("www.").trim_end_matches('.');
    let valid = host.contains('.')
        && !host.starts_with('.')
        && host
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-');
    valid.then(|| host.to_string())
}

struct Clean {
    site: String,
    login_url: String,
    username: String,
    notes: String,
}

fn validate(input: &CredentialInput) -> AppResult<Clean> {
    let site = normalize_site(&input.site).ok_or_else(|| {
        AppError::BadRequest("Site must be a domain like seek.com.au or a URL.".into())
    })?;
    let username = input.username.trim().to_string();
    if username.is_empty() {
        return Err(AppError::BadRequest(
            "Username or email is required.".into(),
        ));
    }
    let login_url = input.login_url.as_deref().unwrap_or("").trim().to_string();
    if !login_url.is_empty() && reqwest::Url::parse(&login_url).is_err() {
        return Err(AppError::BadRequest(
            "Login URL must be a full URL (https://…).".into(),
        ));
    }
    if input.password.as_deref().is_some_and(|p| p.len() > 512) {
        return Err(AppError::BadRequest("Password is too long.".into()));
    }
    Ok(Clean {
        site,
        login_url,
        username,
        notes: input.notes.as_deref().unwrap_or("").trim().to_string(),
    })
}

async fn log(state: &AppState, id: Option<Uuid>, site: &str, action: &str) -> sqlx::Result<()> {
    sqlx::query("INSERT INTO vault_access_log (credential_id, site, action) VALUES ($1, $2, $3)")
        .bind(id)
        .bind(site)
        .bind(action)
        .execute(&state.db)
        .await
        .map(|_| ())
}

fn duplicate(error: sqlx::Error, site: &str) -> AppError {
    if is_unique_violation(&error) {
        AppError::Conflict(format!("You already have a login for {site}."))
    } else {
        error.into()
    }
}

pub async fn list(State(state): State<AppState>) -> AppResult<Json<Vec<Credential>>> {
    let rows = sqlx::query_as(
        "SELECT id, site, login_url, username, notes, created_by, last_used_at,
                created_at, updated_at FROM credentials ORDER BY site",
    )
    .fetch_all(&state.db)
    .await?;
    Ok(Json(rows))
}

pub async fn create(
    State(state): State<AppState>,
    Json(input): Json<CredentialInput>,
) -> AppResult<(StatusCode, Json<Credential>)> {
    let key = key(&state)?;
    let clean = validate(&input)?;
    let password = input.password.as_deref().unwrap_or("");
    if password.is_empty() {
        return Err(AppError::BadRequest("Password is required.".into()));
    }

    let id = Uuid::new_v4();
    let sealed = key.encrypt(password, id.as_bytes())?;
    sqlx::query(
        "INSERT INTO credentials (id, site, login_url, username, secret, nonce, notes)
         VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(id)
    .bind(&clean.site)
    .bind(&clean.login_url)
    .bind(&clean.username)
    .bind(&sealed.ciphertext)
    .bind(&sealed.nonce)
    .bind(&clean.notes)
    .execute(&state.db)
    .await
    .map_err(|e| duplicate(e, &clean.site))?;

    log(&state, Some(id), &clean.site, "create").await?;
    Ok((StatusCode::CREATED, Json(fetch(&state, id).await?)))
}

pub async fn update(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(input): Json<CredentialInput>,
) -> AppResult<Json<Credential>> {
    let clean = validate(&input)?;
    let result = sqlx::query(
        "UPDATE credentials SET site = $2, login_url = $3, username = $4, notes = $5,
                updated_at = now()
         WHERE id = $1",
    )
    .bind(id)
    .bind(&clean.site)
    .bind(&clean.login_url)
    .bind(&clean.username)
    .bind(&clean.notes)
    .execute(&state.db)
    .await
    .map_err(|e| duplicate(e, &clean.site))?;
    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("Login not found.".into()));
    }

    // A new password is only stored if you typed one.
    if let Some(password) = input.password.as_deref().filter(|p| !p.is_empty()) {
        let sealed = key(&state)?.encrypt(password, id.as_bytes())?;
        sqlx::query("UPDATE credentials SET secret = $2, nonce = $3 WHERE id = $1")
            .bind(id)
            .bind(&sealed.ciphertext)
            .bind(&sealed.nonce)
            .execute(&state.db)
            .await?;
    }

    log(&state, Some(id), &clean.site, "update").await?;
    Ok(Json(fetch(&state, id).await?))
}

pub async fn delete(State(state): State<AppState>, Path(id): Path<Uuid>) -> AppResult<StatusCode> {
    let site: Option<String> =
        sqlx::query_scalar("DELETE FROM credentials WHERE id = $1 RETURNING site")
            .bind(id)
            .fetch_optional(&state.db)
            .await?;
    let site = site.ok_or_else(|| AppError::NotFound("Login not found.".into()))?;
    log(&state, None, &site, "delete").await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Serialize)]
pub struct Revealed {
    pub password: String,
}

/// Decrypt one password, and record that you looked at it.
pub async fn reveal(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<Revealed>> {
    let key = key(&state)?;
    let row: Option<(String, Vec<u8>, Vec<u8>)> =
        sqlx::query_as("SELECT site, secret, nonce FROM credentials WHERE id = $1")
            .bind(id)
            .fetch_optional(&state.db)
            .await?;
    let (site, secret, nonce) = row.ok_or_else(|| AppError::NotFound("Login not found.".into()))?;

    let password = key.decrypt(&secret, &nonce, id.as_bytes()).map_err(|e| {
        tracing::error!(%site, error = %e, "vault decrypt failed");
        AppError::Unavailable(
            "This password can't be decrypted. Was VAULT_KEY changed? Put the old key back.".into(),
        )
    })?;
    log(&state, Some(id), &site, "reveal").await?;
    Ok(Json(Revealed { password }))
}

#[derive(Deserialize)]
pub struct GenerateParams {
    pub length: Option<usize>,
}

pub async fn generate(Query(params): Query<GenerateParams>) -> Json<Revealed> {
    Json(Revealed {
        password: password::generate(params.length.unwrap_or(20)),
    })
}

#[derive(Serialize, sqlx::FromRow)]
pub struct Access {
    pub site: String,
    pub action: String,
    pub at: DateTime<Utc>,
}

#[derive(Serialize)]
pub struct Status {
    pub unlocked: bool,
    pub entries: i64,
    pub recent_access: Vec<Access>,
}

pub async fn status(State(state): State<AppState>) -> AppResult<Json<Status>> {
    let entries: i64 = sqlx::query_scalar("SELECT count(*) FROM credentials")
        .fetch_one(&state.db)
        .await?;
    let recent_access = sqlx::query_as(
        "SELECT site, action, at FROM vault_access_log ORDER BY at DESC, id DESC LIMIT 10",
    )
    .fetch_all(&state.db)
    .await?;
    Ok(Json(Status {
        unlocked: state.vault.is_some(),
        entries,
        recent_access,
    }))
}

async fn fetch(state: &AppState, id: Uuid) -> AppResult<Credential> {
    sqlx::query_as(CREDENTIAL_BY_ID)
        .bind(id)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound("Login not found.".into()))
}

const CREDENTIAL_BY_ID: &str = "SELECT id, site, login_url, username, notes, created_by,
    last_used_at, created_at, updated_at FROM credentials WHERE id = $1";

// ---------- The apply agent's side ----------

/// A login the agent can use. Debug never prints the password.
pub struct AgentLogin {
    pub username: String,
    pub password: String,
}

impl std::fmt::Debug for AgentLogin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "AgentLogin({}, ..)", self.username)
    }
}

/// Your saved login for a site (a domain like "acme.wd3.myworkdayjobs.com"),
/// if there is one. Each use is recorded in the access log.
pub async fn agent_login(
    db: &sqlx::PgPool,
    key: &VaultKey,
    site: &str,
) -> anyhow::Result<Option<AgentLogin>> {
    let row: Option<(Uuid, String, Vec<u8>, Vec<u8>)> = sqlx::query_as(
        "UPDATE credentials SET last_used_at = now() WHERE site = $1
         RETURNING id, username, secret, nonce",
    )
    .bind(site)
    .fetch_optional(db)
    .await?;
    let Some((id, username, secret, nonce)) = row else {
        return Ok(None);
    };
    let password = key.decrypt(&secret, &nonce, id.as_bytes())?;
    sqlx::query(
        "INSERT INTO vault_access_log (credential_id, site, action) VALUES ($1, $2, 'agent_use')",
    )
    .bind(id)
    .bind(site)
    .execute(db)
    .await?;
    Ok(Some(AgentLogin { username, password }))
}

/// Save a login the agent created when it signed up for you. Replaces an
/// older one the agent made for the same site (a sign-up that had to be
/// redone), never one you saved yourself.
pub async fn save_agent_login(
    db: &sqlx::PgPool,
    key: &VaultKey,
    site: &str,
    login_url: &str,
    username: &str,
    password: &str,
) -> anyhow::Result<()> {
    let id = Uuid::new_v4();
    let sealed = key.encrypt(password, id.as_bytes())?;
    sqlx::query("DELETE FROM credentials WHERE site = $1 AND created_by = 'agent'")
        .bind(site)
        .execute(db)
        .await?;
    sqlx::query(
        "INSERT INTO credentials (id, site, login_url, username, secret, nonce, notes, created_by)
         VALUES ($1, $2, $3, $4, $5, $6, 'Created by the apply agent.', 'agent')",
    )
    .bind(id)
    .bind(site)
    .bind(login_url)
    .bind(username)
    .bind(&sealed.ciphertext)
    .bind(&sealed.nonce)
    .execute(db)
    .await?;
    sqlx::query(
        "INSERT INTO vault_access_log (credential_id, site, action) VALUES ($1, $2, 'create')",
    )
    .bind(id)
    .bind(site)
    .execute(db)
    .await?;
    Ok(())
}

/// Your usual login: the email and password you use on most sites. When a
/// site has no saved login, the agent tries this once before offering to
/// create an account, and saves it for the site if it works. Stored in
/// `integrations`, encrypted like every other password.
const DEFAULT_AAD: &[u8] = b"integration:default_login";

pub async fn default_login(
    db: &sqlx::PgPool,
    key: &VaultKey,
) -> anyhow::Result<Option<AgentLogin>> {
    let row: Option<(String, Vec<u8>, Vec<u8>)> = sqlx::query_as(
        "SELECT account, secret, nonce FROM integrations WHERE name = 'default_login'",
    )
    .fetch_optional(db)
    .await?;
    let Some((username, secret, nonce)) = row else {
        return Ok(None);
    };
    let password = key.decrypt(&secret, &nonce, DEFAULT_AAD)?;
    Ok(Some(AgentLogin { username, password }))
}

#[derive(Serialize)]
pub struct DefaultLoginView {
    pub set: bool,
    /// Empty: your profile email.
    pub username: String,
}

pub async fn get_default(State(state): State<AppState>) -> AppResult<Json<DefaultLoginView>> {
    let username: Option<String> =
        sqlx::query_scalar("SELECT account FROM integrations WHERE name = 'default_login'")
            .fetch_optional(&state.db)
            .await?;
    Ok(Json(DefaultLoginView {
        set: username.is_some(),
        username: username.unwrap_or_default(),
    }))
}

#[derive(Deserialize)]
pub struct DefaultLoginInput {
    #[serde(default)]
    username: String,
    password: String,
}

pub async fn set_default(
    State(state): State<AppState>,
    Json(input): Json<DefaultLoginInput>,
) -> AppResult<Json<DefaultLoginView>> {
    let key = key(&state)?;
    let username = input.username.trim().to_string();
    if input.password.is_empty() || input.password.len() > 200 || username.len() > 200 {
        return Err(AppError::BadRequest(
            "Enter the password (at most 200 characters).".into(),
        ));
    }
    let sealed = key.encrypt(&input.password, DEFAULT_AAD)?;
    sqlx::query(
        "INSERT INTO integrations (name, account, secret, nonce) VALUES ('default_login', $1, $2, $3)
         ON CONFLICT (name) DO UPDATE SET account = EXCLUDED.account, secret = EXCLUDED.secret,
             nonce = EXCLUDED.nonce, updated_at = now()",
    )
    .bind(&username)
    .bind(&sealed.ciphertext)
    .bind(&sealed.nonce)
    .execute(&state.db)
    .await?;
    Ok(Json(DefaultLoginView {
        set: true,
        username,
    }))
}

pub async fn delete_default(State(state): State<AppState>) -> AppResult<StatusCode> {
    sqlx::query("DELETE FROM integrations WHERE name = 'default_login'")
        .execute(&state.db)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// The username saved for a site, without touching the password.
pub async fn saved_username(db: &sqlx::PgPool, site: &str) -> sqlx::Result<Option<String>> {
    sqlx::query_scalar(
        "SELECT username FROM credentials WHERE site = $1 ORDER BY created_at LIMIT 1",
    )
    .bind(site)
    .fetch_optional(db)
    .await
}

/// Save your usual login for a site once it worked there, so next time it's
/// a saved login like any other (marked as yours, not the agent's).
pub async fn save_login_for_site(
    db: &sqlx::PgPool,
    key: &VaultKey,
    site: &str,
    login_url: &str,
    username: &str,
    password: &str,
) -> anyhow::Result<()> {
    let id = Uuid::new_v4();
    let sealed = key.encrypt(password, id.as_bytes())?;
    let inserted = sqlx::query(
        "INSERT INTO credentials (id, site, login_url, username, secret, nonce, notes, created_by)
         SELECT $1, $2, $3, $4, $5, $6, 'Your usual login: it worked here, so the agent saved it.', 'you'
         WHERE NOT EXISTS (SELECT 1 FROM credentials WHERE site = $2)",
    )
    .bind(id)
    .bind(site)
    .bind(login_url)
    .bind(username)
    .bind(&sealed.ciphertext)
    .bind(&sealed.nonce)
    .execute(db)
    .await?;
    if inserted.rows_affected() > 0 {
        sqlx::query(
            "INSERT INTO vault_access_log (credential_id, site, action) VALUES ($1, $2, 'create')",
        )
        .bind(id)
        .bind(site)
        .execute(db)
        .await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sites_are_normalised() {
        assert_eq!(
            normalize_site("https://www.Seek.com.au/login?x=1").as_deref(),
            Some("seek.com.au")
        );
        assert_eq!(
            normalize_site("seek.com.au/oauth").as_deref(),
            Some("seek.com.au")
        );
        assert_eq!(
            normalize_site("acme.wd3.myworkdayjobs.com").as_deref(),
            Some("acme.wd3.myworkdayjobs.com")
        );
        assert_eq!(normalize_site("localhost"), None);
        assert_eq!(normalize_site("not a site"), None);
        assert_eq!(normalize_site(""), None);
    }
}
