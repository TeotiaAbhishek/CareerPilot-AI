//! Talks to the browser worker (browser-worker/, TypeScript + Playwright).
//! The worker only does what it's told: open, read, fill, click. Every
//! decision is made here in Rust.

use std::time::Duration;

use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum FieldKind {
    Text,
    Textarea,
    Email,
    Tel,
    Number,
    Date,
    Url,
    Select,
    Radio,
    Checkbox,
    File,
}

impl FieldKind {
    /// A choice from a fixed list (the answer must be one of the options).
    pub fn is_choice(&self) -> bool {
        matches!(self, Self::Select | Self::Radio | Self::Checkbox)
    }

    /// "checkbox", "text box" and so on, for the run log.
    pub fn describe(&self) -> &'static str {
        match self {
            FieldKind::Checkbox => "checkbox",
            FieldKind::Radio => "set of options",
            FieldKind::Select => "drop-down",
            FieldKind::File => "file box",
            _ => "text box",
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Field {
    pub id: String,
    pub kind: FieldKind,
    pub label: String,
    pub required: bool,
    #[serde(default)]
    pub options: Vec<String>,
    #[serde(default)]
    pub value: String,
}

impl Field {
    /// Empty, or a checkbox that isn't ticked yet.
    pub fn is_empty(&self) -> bool {
        match self.kind {
            FieldKind::Checkbox => self.value != "Yes",
            _ => self.value.trim().is_empty(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ButtonKind {
    Submit,
    Next,
    Back,
    /// "Apply now" on a job ad (a page with no form yet): opens the form.
    Start,
    Other,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Button {
    pub id: String,
    pub text: String,
    pub kind: ButtonKind,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Snapshot {
    pub url: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub headings: Vec<String>,
    pub fields: Vec<Field>,
    pub buttons: Vec<Button>,
    #[serde(default)]
    pub errors: Vec<String>,
    pub captcha: bool,
    pub password: bool,
    /// Ids of the visible password boxes. Filled like fields, never logged.
    #[serde(default)]
    pub secrets: Vec<String>,
    /// The page's visible text (shortened), e.g. "We've emailed you a link".
    #[serde(default)]
    pub text: String,
}

/// A resumé saved on the job board's profile page.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SavedResume {
    pub name: String,
    /// The rest of its row, e.g. "Added 3 days ago".
    pub added: String,
    pub index: usize,
}

#[derive(Debug, Deserialize)]
pub struct FillResult {
    pub id: String,
    pub ok: bool,
    pub error: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum BrowserError {
    #[error(
        "The browser worker isn't running at {0}. Start it with `npm start` in browser-worker/."
    )]
    Unreachable(String),
    #[error("The browser tab was closed.")]
    TabClosed,
    #[error("Blocked: {0}")]
    SubmitBlocked(String),
    #[error("The browser worker said: {0}")]
    Worker(String),
}

#[derive(Clone)]
pub struct BrowserClient {
    http: reqwest::Client,
    base_url: String,
}

#[derive(Deserialize)]
struct WorkerError {
    error: String,
    code: Option<String>,
}

impl BrowserClient {
    pub fn new(base_url: &str) -> anyhow::Result<Self> {
        let http = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(3))
            // Opening a slow careers site can take a while.
            .timeout(Duration::from_secs(90))
            .build()?;
        Ok(Self {
            http,
            base_url: base_url.trim_end_matches('/').to_string(),
        })
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    async fn call(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Option<Value>,
    ) -> Result<Value, BrowserError> {
        let mut request = self
            .http
            .request(method, format!("{}{path}", self.base_url));
        if let Some(body) = body {
            request = request.json(&body);
        }
        let response = request.send().await.map_err(|e| {
            if e.is_connect() {
                BrowserError::Unreachable(self.base_url.clone())
            } else {
                BrowserError::Worker(e.to_string())
            }
        })?;
        let status = response.status();
        let value: Value = response
            .json()
            .await
            .map_err(|e| BrowserError::Worker(format!("bad response: {e}")))?;
        if status.is_success() {
            return Ok(value);
        }
        let error: WorkerError = serde_json::from_value(value).unwrap_or(WorkerError {
            error: format!("HTTP {status}"),
            code: None,
        });
        Err(match error.code.as_deref() {
            Some("tab_not_found") => BrowserError::TabClosed,
            Some("submit_blocked") => BrowserError::SubmitBlocked(error.error),
            _ => BrowserError::Worker(error.error),
        })
    }

    pub async fn health(&self) -> Result<(), BrowserError> {
        self.call(reqwest::Method::GET, "/health", None)
            .await
            .map(|_| ())
    }

    /// Open a new tab and return its id.
    /// Open a tab. The agent's window stays minimised unless `show`.
    pub async fn open(&self, url: &str, show: bool) -> Result<String, BrowserError> {
        let value = self
            .call(
                reqwest::Method::POST,
                "/tabs",
                Some(json!({ "url": url, "show": show })),
            )
            .await?;
        value["tab"]
            .as_str()
            .map(str::to_string)
            .ok_or_else(|| BrowserError::Worker("no tab id".into()))
    }

    pub async fn has_tab(&self, tab: &str) -> Result<bool, BrowserError> {
        match self
            .call(reqwest::Method::GET, &format!("/tabs/{tab}"), None)
            .await
        {
            Ok(_) => Ok(true),
            Err(BrowserError::TabClosed) => Ok(false),
            Err(e) => Err(e),
        }
    }

    pub async fn snapshot(&self, tab: &str) -> Result<Snapshot, BrowserError> {
        let value = self
            .call(reqwest::Method::GET, &format!("/tabs/{tab}/snapshot"), None)
            .await?;
        serde_json::from_value(value)
            .map_err(|e| BrowserError::Worker(format!("bad snapshot: {e}")))
    }

    pub async fn fill(
        &self,
        tab: &str,
        fields: &[(String, String)],
    ) -> Result<Vec<FillResult>, BrowserError> {
        let fields: Vec<Value> = fields
            .iter()
            .map(|(id, value)| json!({ "id": id, "value": value }))
            .collect();
        let value = self
            .call(
                reqwest::Method::POST,
                &format!("/tabs/{tab}/fill"),
                Some(json!({ "fields": fields })),
            )
            .await?;
        serde_json::from_value(value["results"].clone())
            .map_err(|e| BrowserError::Worker(format!("bad fill response: {e}")))
    }

    pub async fn upload(
        &self,
        tab: &str,
        field_id: &str,
        filename: &str,
        bytes: &[u8],
    ) -> Result<(), BrowserError> {
        let content = base64::engine::general_purpose::STANDARD.encode(bytes);
        self.call(
            reqwest::Method::POST,
            &format!("/tabs/{tab}/upload"),
            Some(json!({ "id": field_id, "filename": filename, "content_base64": content })),
        )
        .await
        .map(|_| ())
    }

    /// Click a Next / Continue button. The worker refuses Submit buttons.
    pub async fn click(&self, tab: &str, button_id: &str) -> Result<(), BrowserError> {
        self.call(
            reqwest::Method::POST,
            &format!("/tabs/{tab}/click"),
            Some(json!({ "id": button_id })),
        )
        .await
        .map(|_| ())
    }

    /// Click Sign in / Create account / Verify on a login, sign-up or code
    /// page. The worker allows a button labelled "Submit" there only.
    pub async fn click_auth(&self, tab: &str, button_id: &str) -> Result<(), BrowserError> {
        self.call(
            reqwest::Method::POST,
            &format!("/tabs/{tab}/click"),
            Some(json!({ "id": button_id, "auth": true })),
        )
        .await
        .map(|_| ())
    }

    /// Go to a link in the tab (an email's "verify your account" link).
    pub async fn goto(&self, tab: &str, url: &str) -> Result<(), BrowserError> {
        self.call(
            reqwest::Method::POST,
            &format!("/tabs/{tab}/goto"),
            Some(json!({ "url": url })),
        )
        .await
        .map(|_| ())
    }

    /// The resumés saved on the profile page in this tab whose names start
    /// with one of `prefixes`, in page order.
    pub async fn saved_resumes(
        &self,
        tab: &str,
        prefixes: &[String],
    ) -> Result<Vec<SavedResume>, BrowserError> {
        let value = self
            .call(
                reqwest::Method::POST,
                &format!("/tabs/{tab}/resumes"),
                Some(json!({ "prefixes": prefixes })),
            )
            .await?;
        serde_json::from_value(value["resumes"].clone())
            .map_err(|e| BrowserError::Worker(format!("bad response: {e}")))
    }

    /// Delete the saved resumé at `index`, if it's still called `name`.
    pub async fn delete_resume(
        &self,
        tab: &str,
        prefixes: &[String],
        index: usize,
        name: &str,
    ) -> Result<bool, BrowserError> {
        let value = self
            .call(
                reqwest::Method::POST,
                &format!("/tabs/{tab}/delete-resume"),
                Some(json!({ "prefixes": prefixes, "index": index, "name": name })),
            )
            .await?;
        Ok(value["deleted"].as_bool().unwrap_or(false))
    }

    /// Print HTML to an A4 PDF at `scale` (0.8 to 1): (bytes, pages).
    pub async fn pdf(&self, html: &str, scale: f64) -> Result<(Vec<u8>, u32), BrowserError> {
        let value = self
            .call(
                reqwest::Method::POST,
                "/pdf",
                Some(json!({ "html": html, "scale": scale })),
            )
            .await?;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(value["pdf_base64"].as_str().unwrap_or(""))
            .map_err(|e| BrowserError::Worker(format!("bad PDF: {e}")))?;
        let pages = value["pages"].as_u64().unwrap_or(0) as u32;
        Ok((bytes, pages))
    }

    pub async fn focus(&self, tab: &str) -> Result<(), BrowserError> {
        self.call(reqwest::Method::POST, &format!("/tabs/{tab}/focus"), None)
            .await
            .map(|_| ())
    }

    pub async fn close(&self, tab: &str) -> Result<(), BrowserError> {
        self.call(reqwest::Method::DELETE, &format!("/tabs/{tab}"), None)
            .await
            .map(|_| ())
    }
}
