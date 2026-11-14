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
