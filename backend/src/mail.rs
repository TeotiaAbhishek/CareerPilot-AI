//! Email codes and links for sign-ups: "We've emailed you a 6-digit code" or
//! "Click the link to verify your email".
//!
//! Real sites: your Gmail, read-only (the `gmail.readonly` scope, so the app
//! can't send, change or delete anything). You connect it once on Settings;
//! the refresh token is encrypted with your VAULT_KEY like a password.
//! Practice runs: the browser worker's practice mailbox, so they need no inbox.

use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use axum::{
    Json,
    extract::{Query, State},
    http::StatusCode,
    response::Redirect,
};
use base64::Engine;
use chrono::{DateTime, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::PgPool;

use crate::{
    AppState,
    error::{AppError, AppResult},
    vault::crypto::VaultKey,
};

const SCOPE: &str = "https://www.googleapis.com/auth/gmail.readonly";
const AAD: &[u8] = b"integration:gmail";

/// Your Google OAuth client (a "Desktop app" client from Google Cloud).
#[derive(Clone, Debug)]
pub struct GmailConfig {
    pub client_id: String,
    pub client_secret: String,
    pub auth_url: String,
    pub token_url: String,
    pub api_url: String,
    /// Where Google sends you back: this API's /api/mail/callback.
    pub redirect_uri: String,
    /// Where the callback sends you afterwards: the web app's Settings page.
    pub web_origin: String,
}

#[derive(Clone)]
pub struct Mail {
    inner: Arc<Inner>,
}

struct Inner {
    http: reqwest::Client,
    db: PgPool,
    key: Option<VaultKey>,
    gmail: Option<GmailConfig>,
    /// The browser worker, for the practice mailbox.
    worker_url: String,
    /// OAuth "state" values handed out by connect, checked by callback.
    states: Mutex<Vec<String>>,
    token: Mutex<Option<(String, Instant)>>,
}

/// One email, as plain text.
#[derive(Debug, Clone)]
pub struct Email {
    pub from: String,
    pub subject: String,
    pub body: String,
    pub received: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Found {
    Code(String),
    Link(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Want {
    Code,
    Link,
}

impl Mail {
    pub fn new(
        db: PgPool,
        key: Option<VaultKey>,
        gmail: Option<GmailConfig>,
        worker_url: &str,
    ) -> anyhow::Result<Self> {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(20))
            .build()?;
        Ok(Self {
            inner: Arc::new(Inner {
                http,
                db,
                key,
                gmail,
                worker_url: worker_url.trim_end_matches('/').to_string(),
                states: Mutex::new(Vec::new()),
                token: Mutex::new(None),
            }),
        })
    }

    /// The Gmail account you connected, if any.
    pub async fn connected_account(&self) -> sqlx::Result<Option<String>> {
        sqlx::query_scalar("SELECT account FROM integrations WHERE name = 'gmail'")
            .fetch_optional(&self.inner.db)
            .await
    }

    /// Wait for an email (newer than `since`) with a code or a link in it.
    /// `hint` (the site, e.g. "practice-careers") picks the right email when
    /// several arrive. None after `timeout`.
    pub async fn wait_for(
        &self,
        practice: bool,
        since: DateTime<Utc>,
        hint: &str,
        want: Want,
        timeout: Duration,
    ) -> anyhow::Result<Option<Found>> {
        let start = Instant::now();
        loop {
            let emails = if practice {
                self.practice_emails().await?
            } else {
                self.gmail_emails().await?
            };
            if let Some(found) = pick(&emails, since, hint, want) {
                return Ok(Some(found));
            }
            if start.elapsed() >= timeout {
                return Ok(None);
            }
            tokio::time::sleep(Duration::from_secs(if practice { 1 } else { 5 })).await;
        }
    }

    async fn practice_emails(&self) -> anyhow::Result<Vec<Email>> {
        #[derive(Deserialize)]
        struct Box {
            emails: Vec<PracticeEmail>,
        }
        #[derive(Deserialize)]
        struct PracticeEmail {
            from: String,
            subject: String,
            body: String,
            at: DateTime<Utc>,
        }
        let url = format!("{}/practice/mailbox", self.inner.worker_url);
        let mailbox: Box = self.inner.http.get(url).send().await?.json().await?;
        Ok(mailbox
            .emails
            .into_iter()
            .map(|e| Email {
                from: e.from,
                subject: e.subject,
                body: e.body,
                received: e.at,
            })
            .collect())
    }

    fn gmail(&self) -> anyhow::Result<&GmailConfig> {
        self.inner
            .gmail
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Gmail isn't set up (GMAIL_CLIENT_ID in backend/.env)"))
    }

    /// The last 10 emails from the past day, newest first.
    async fn gmail_emails(&self) -> anyhow::Result<Vec<Email>> {
        let gmail = self.gmail()?;
        let token = self.access_token().await?;
        let list: Value = self
            .inner
            .http
            .get(reqwest::Url::parse_with_params(
                &format!("{}/gmail/v1/users/me/messages", gmail.api_url),
                &[("q", "newer_than:1d"), ("maxResults", "10")],
            )?)
            .bearer_auth(&token)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        let ids: Vec<String> = list["messages"]
            .as_array()
            .map(|m| {
                m.iter()
                    .filter_map(|x| x["id"].as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        let mut out = Vec::new();
        for id in ids {
            let message: Value = self
                .inner
                .http
                .get(format!(
                    "{}/gmail/v1/users/me/messages/{id}?format=full",
                    gmail.api_url
                ))
                .bearer_auth(&token)
                .send()
                .await?
                .error_for_status()?
                .json()
                .await?;
            if let Some(email) = parse_gmail(&message) {
                out.push(email);
            }
        }
        Ok(out)
    }

    async fn access_token(&self) -> anyhow::Result<String> {
        if let Some((token, until)) = self.inner.token.lock().unwrap().clone()
            && Instant::now() < until
        {
            return Ok(token);
        }
        let gmail = self.gmail()?;
        let refresh = self
            .refresh_token()
            .await?
            .ok_or_else(|| anyhow::anyhow!("Gmail isn't connected (Settings → Email codes)"))?;
        let answer = self
            .token_request(
                gmail,
                &[
                    ("client_id", gmail.client_id.as_str()),
                    ("client_secret", gmail.client_secret.as_str()),
                    ("refresh_token", refresh.as_str()),
                    ("grant_type", "refresh_token"),
                ],
            )
            .await?;
        let lifetime = answer.expires_in.unwrap_or(3600).saturating_sub(60);
        *self.inner.token.lock().unwrap() = Some((
            answer.access_token.clone(),
            Instant::now() + Duration::from_secs(lifetime),
        ));
        Ok(answer.access_token)
    }

    async fn token_request(
        &self,
        gmail: &GmailConfig,
        form: &[(&str, &str)],
    ) -> anyhow::Result<TokenAnswer> {
        let body = reqwest::Url::parse_with_params("http://x/", form)?
            .query()
            .unwrap_or("")
            .to_string();
        let response = self
            .inner
            .http
            .post(&gmail.token_url)
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(body)
            .send()
            .await?;
        if !response.status().is_success() {
            let text = response.text().await.unwrap_or_default();
            anyhow::bail!(
                "Google said no: {}",
                text.chars().take(200).collect::<String>()
            );
        }
        Ok(response.json().await?)
    }

    async fn refresh_token(&self) -> anyhow::Result<Option<String>> {
        let key = self
            .inner
            .key
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("the vault is locked (VAULT_KEY)"))?;
        let row: Option<(Vec<u8>, Vec<u8>)> =
            sqlx::query_as("SELECT secret, nonce FROM integrations WHERE name = 'gmail'")
                .fetch_optional(&self.inner.db)
                .await?;
        row.map(|(secret, nonce)| key.decrypt(&secret, &nonce, AAD))
            .transpose()
    }
}

#[derive(Deserialize)]
struct TokenAnswer {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: Option<u64>,
}

/// The newest email after `since` with what we want, preferring one that
/// mentions the site.
pub fn pick(emails: &[Email], since: DateTime<Utc>, hint: &str, want: Want) -> Option<Found> {
    let hint = hint.to_lowercase();
    let mut fresh: Vec<&Email> = emails.iter().filter(|e| e.received >= since).collect();
    fresh.sort_by_key(|e| std::cmp::Reverse(e.received));
    let mentions = |e: &&Email| {
        !hint.is_empty()
            && format!("{} {} {}", e.from, e.subject, e.body)
                .to_lowercase()
                .contains(&hint)
    };
    let find = |e: &Email| {
        let text = format!("{}\n{}", e.subject, e.body);
        match want {
            Want::Code => find_code(&text).map(Found::Code),
            Want::Link => find_link(&text).map(Found::Link),
        }
    };
    fresh
        .iter()
        .copied()
        .filter(mentions)
        .find_map(find)
        .or_else(|| fresh.iter().copied().find_map(find))
}

/// "Your verification code is 482913" -> "482913". Only a 4 to 8 digit number
/// close to the word "code" counts, so dates and phone numbers don't.
pub fn find_code(text: &str) -> Option<String> {
    let lower = text.to_lowercase();
    for (at, _) in lower.match_indices("code") {
        let start = lower[..at]
            .char_indices()
            .rev()
            .nth(60)
            .map_or(0, |(i, _)| i);
        let end = lower[at..]
            .char_indices()
            .nth(120)
            .map_or(lower.len(), |(i, _)| at + i);
        let digits = |part: &str| {
            part.split(|c: char| !c.is_ascii_digit())
                .find(|t| (4..=8).contains(&t.len()))
                .map(str::to_string)
        };
        // After the word first ("code is 123456"), then before ("123456 is your code").
        if let Some(code) = digits(&lower[at..end]).or_else(|| digits(&lower[start..at])) {
            return Some(code);
        }
    }
    None
}

/// The first link that verifies or activates an account.
pub fn find_link(text: &str) -> Option<String> {
    const WORDS: [&str; 5] = ["verif", "confirm", "activat", "validat", "registration"];
    for (at, _) in text.match_indices("http") {
        let link: String = text[at..]
            .chars()
            .take_while(|c| !c.is_whitespace() && !matches!(c, '"' | '\'' | '<' | '>' | ')' | ']'))
            .collect();
        let link = link.trim_end_matches(['.', ',', ';']).to_string();
        let lower = link.to_lowercase();
        if (lower.starts_with("https://") || lower.starts_with("http://"))
            && WORDS.iter().any(|w| lower.contains(w))
        {
            return Some(link);
        }
    }
    None
}

/// A Gmail API message (format=full) as plain text.
fn parse_gmail(message: &Value) -> Option<Email> {
    let ms: i64 = message["internalDate"].as_str()?.parse().ok()?;
    let received = Utc.timestamp_millis_opt(ms).single()?;
    let header = |name: &str| {
        message["payload"]["headers"]
            .as_array()
            .and_then(|hs| {
                hs.iter().find(|h| {
                    h["name"]
                        .as_str()
                        .is_some_and(|n| n.eq_ignore_ascii_case(name))
                })
            })
            .and_then(|h| h["value"].as_str())
            .unwrap_or("")
            .to_string()
    };
    let mut plain = String::new();
    let mut html = String::new();
    collect_text(&message["payload"], &mut plain, &mut html);
    let body = if !plain.trim().is_empty() {
        plain
    } else {
        // Keep the links: html2text writes them out as [n]: https://...
        html2text::from_read(html.as_bytes(), 200).unwrap_or_default()
    };
    Some(Email {
        from: header("From"),
        subject: header("Subject"),
        body,
        received,
    })
}

fn collect_text(part: &Value, plain: &mut String, html: &mut String) {
    let mime = part["mimeType"].as_str().unwrap_or("");
    if let Some(data) = part["body"]["data"].as_str()
        && let Ok(bytes) =
            base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(data.trim_end_matches('='))
    {
        let text = String::from_utf8_lossy(&bytes);
        match mime {
            "text/plain" => plain.push_str(&text),
            "text/html" => html.push_str(&text),
            _ => {}
        }
    }
    if let Some(parts) = part["parts"].as_array() {
        for p in parts {
            collect_text(p, plain, html);
        }
    }
}

// ---------- Routes: Settings → Email codes ----------

#[derive(Serialize)]
pub struct MailStatus {
    /// GMAIL_CLIENT_ID and GMAIL_CLIENT_SECRET are set.
    pub configured: bool,
    /// The connected Gmail address, if any.
    pub account: Option<String>,
}

pub async fn status(State(state): State<AppState>) -> AppResult<Json<MailStatus>> {
    Ok(Json(MailStatus {
        configured: state.mail.inner.gmail.is_some(),
        account: state.mail.connected_account().await?,
    }))
}

/// Start connecting: off to Google's consent screen.
pub async fn connect(State(state): State<AppState>) -> AppResult<Redirect> {
    let mail = &state.mail;
    let gmail = mail
        .gmail()
        .map_err(|e| AppError::Unavailable(e.to_string()))?;
    if mail.inner.key.is_none() {
        return Err(AppError::Unavailable(
            "Set up the vault first (VAULT_KEY): the Gmail token is stored encrypted.".into(),
        ));
    }
    let state_value = uuid::Uuid::new_v4().to_string();
    {
        let mut states = mail.inner.states.lock().unwrap();
        states.push(state_value.clone());
        if states.len() > 5 {
            states.remove(0);
        }
    }
    let url = reqwest::Url::parse_with_params(
        &gmail.auth_url,
        &[
            ("client_id", gmail.client_id.as_str()),
            ("redirect_uri", gmail.redirect_uri.as_str()),
            ("response_type", "code"),
            ("scope", SCOPE),
            ("access_type", "offline"),
            ("prompt", "consent"),
            ("state", state_value.as_str()),
        ],
    )
    .map_err(|e| AppError::Other(e.into()))?;
    Ok(Redirect::to(url.as_str()))
}

#[derive(Deserialize)]
pub struct Callback {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
}

/// Google sends you back here with a one-time code: swap it for a refresh
/// token, store that encrypted, and return to Settings.
pub async fn callback(
    State(state): State<AppState>,
    Query(params): Query<Callback>,
) -> AppResult<Redirect> {
    let mail = &state.mail;
    let gmail = mail
        .gmail()
        .map_err(|e| AppError::Unavailable(e.to_string()))?;
    let back =
        |result: &str| Redirect::to(&format!("{}/settings?gmail={result}", gmail.web_origin));
    let known = params.state.as_ref().is_some_and(|s| {
        let mut states = mail.inner.states.lock().unwrap();
        let found = states.contains(s);
        states.retain(|x| x != s);
        found
    });
    let (Some(code), true, None) = (params.code, known, params.error) else {
        return Ok(back("cancelled"));
    };
    let key = mail
        .inner
        .key
        .as_ref()
        .ok_or_else(|| AppError::Unavailable("The vault is locked (VAULT_KEY).".into()))?;
    let answer = match mail
        .token_request(
            gmail,
            &[
                ("client_id", gmail.client_id.as_str()),
                ("client_secret", gmail.client_secret.as_str()),
                ("code", code.as_str()),
                ("redirect_uri", gmail.redirect_uri.as_str()),
                ("grant_type", "authorization_code"),
            ],
        )
        .await
    {
        Ok(a) => a,
        Err(error) => {
            tracing::warn!(%error, "gmail connect failed");
            return Ok(back("failed"));
        }
    };
    let Some(refresh) = answer.refresh_token else {
        return Ok(back("failed"));
    };
    let profile: Value = mail
        .inner
        .http
        .get(format!("{}/gmail/v1/users/me/profile", gmail.api_url))
        .bearer_auth(&answer.access_token)
        .send()
        .await
        .map_err(|e| AppError::Other(e.into()))?
        .json()
        .await
        .map_err(|e| AppError::Other(e.into()))?;
    let account = profile["emailAddress"].as_str().unwrap_or("").to_string();
    let sealed = key.encrypt(&refresh, AAD)?;
    sqlx::query(
        "INSERT INTO integrations (name, account, secret, nonce) VALUES ('gmail', $1, $2, $3)
         ON CONFLICT (name) DO UPDATE SET account = $1, secret = $2, nonce = $3, updated_at = now()",
    )
    .bind(&account)
    .bind(&sealed.ciphertext)
    .bind(&sealed.nonce)
    .execute(&state.db)
    .await?;
    *mail.inner.token.lock().unwrap() = None;
    Ok(back("connected"))
}

/// Forget the Gmail connection. (Also remove the app's access in your Google
/// account settings to revoke it on Google's side.)
pub async fn disconnect(State(state): State<AppState>) -> AppResult<StatusCode> {
    sqlx::query("DELETE FROM integrations WHERE name = 'gmail'")
        .execute(&state.db)
        .await?;
    *state.mail.inner.token.lock().unwrap() = None;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn email(minutes_ago: i64, from: &str, subject: &str, body: &str) -> Email {
        Email {
            from: from.into(),
            subject: subject.into(),
            body: body.into(),
            received: Utc::now() - chrono::Duration::minutes(minutes_ago),
        }
    }

    #[test]
    fn codes_are_found_next_to_the_word_code() {
        assert_eq!(
            find_code("Hi, your verification code is 482913. It expires in 10 minutes."),
            Some("482913".into())
        );
        assert_eq!(
            find_code("482913 is your Workday code"),
            Some("482913".into())
        );
        assert_eq!(find_code("Call us on 0400 000 000 by 2026."), None);
    }

    #[test]
    fn verify_links_are_found_and_other_links_skipped() {
        let body = "Welcome! https://acme.com/careers\nVerify here: https://acme.com/account/verify?t=abc123.\n";
        assert_eq!(
            find_link(body),
            Some("https://acme.com/account/verify?t=abc123".into())
        );
        assert_eq!(find_link("Visit https://acme.com/jobs"), None);
    }

    #[test]
    fn the_newest_email_after_the_sign_up_wins_and_the_site_name_helps() {
        let since = Utc::now() - chrono::Duration::minutes(5);
        let emails = vec![
            email(60, "old@acme.com", "Code", "Your code is 111111"),
            email(2, "news@other.com", "Your code", "Your code is 222222"),
            email(3, "no-reply@acme.com", "Verify", "Your code is 333333"),
        ];
        assert_eq!(
            pick(&emails, since, "acme", Want::Code),
            Some(Found::Code("333333".into()))
        );
        assert_eq!(
            pick(&emails, since, "", Want::Code),
            Some(Found::Code("222222".into()))
        );
        assert_eq!(pick(&emails, Utc::now(), "acme", Want::Code), None);
    }

    #[test]
    fn gmail_messages_become_plain_text() {
        let data = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode("Your verification code is 482913");
        let message = serde_json::json!({
            "internalDate": "1763100000000",
            "payload": {
                "mimeType": "multipart/alternative",
                "headers": [{"name": "From", "value": "Acme <no-reply@acme.com>"},
                            {"name": "Subject", "value": "Verify your email"}],
                "parts": [{"mimeType": "text/plain", "body": {"data": data}}]
            }
        });
        let email = parse_gmail(&message).unwrap();
        assert_eq!(email.subject, "Verify your email");
        assert!(email.from.contains("acme.com"));
        assert_eq!(find_code(&email.body), Some("482913".into()));
    }
}
