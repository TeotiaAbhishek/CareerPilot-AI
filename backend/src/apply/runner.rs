//! The apply runner: takes queued applications and drives the browser through
//! each form, up to `apply_parallel` at once, one tab each.
//!
//! A run stops (and frees its slot) when it reaches the final Submit button,
//! needs an answer from you, hits a CAPTCHA or login page, or gets stuck.
//! The tab stays open, so after you answer, the run carries on where it was.

use std::{
    collections::HashSet,
    sync::{Arc, Mutex},
    time::Duration,
};

use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::PgPool;
use tokio::{
    sync::{Notify, broadcast},
    task::JoinSet,
};
use uuid::Uuid;

use super::{
    browser::{BrowserClient, BrowserError, ButtonKind, Field, FieldKind, Snapshot},
    experience,
    fill::{self, Action, Doc, FillContext, KnownAnswer},
    prompt::{self, PromptInput},
};
use crate::{
    documents::{self, prompt::evidence_block, resume::MasterResume},
    llm::{ChatOptions, LlmClient},
    profile, settings,
};

/// More than this many pages or fill rounds in one go and something is wrong.
pub const MAX_STEPS: u32 = 40;
const IDLE_WAIT: Duration = Duration::from_secs(20);

#[derive(Clone)]
pub struct Applier {
    inner: Arc<Inner>,
}

struct Inner {
    db: PgPool,
    llm: LlmClient,
    browser: BrowserClient,
    wake: Notify,
    state: Mutex<ApplierState>,
    events: broadcast::Sender<LiveEvent>,
    /// Short random pauses between actions, like a person. Off in tests.
    pacing: bool,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct ApplierState {
    pub running: usize,
    /// Why nothing new starts: the worker isn't running, or the daily limit.
    pub paused_reason: Option<String>,
}

/// One line of the run log, as the run view receives it.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Event {
    pub id: i64,
    pub application_id: Uuid,
    pub kind: String,
    pub message: String,
    pub at: DateTime<Utc>,
}

/// Sent live to the run view: a new log line, or a status change.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LiveEvent {
    Log(Event),
    Status {
        application_id: Uuid,
        status: String,
    },
}

impl LiveEvent {
    pub fn application_id(&self) -> Uuid {
        match self {
            LiveEvent::Log(e) => e.application_id,
            LiveEvent::Status { application_id, .. } => *application_id,
        }
    }
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Run {
    pub id: Uuid,
    pub job_id: String,
    pub start_url: String,
    pub tab_id: Option<String>,
    pub resume_id: Option<Uuid>,
    pub cover_letter_id: Option<Uuid>,
    pub job_title: String,
    pub company: Option<String>,
    pub job_description: Option<String>,
}

/// How a run ended this time.
#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    /// At the final Submit button: your turn.
    Ready,
    /// Questions only you can answer.
    Waiting(usize),
    /// A CAPTCHA, a login page, or a form it couldn't get through.
    Manual {
        reason: &'static str,
        message: String,
    },
    /// You cancelled it while it ran.
    Cancelled,
}

enum RunError {
    /// The worker is down: the run goes back in the queue.
    WorkerDown(String),
    Failed(String),
}

impl From<BrowserError> for RunError {
    fn from(error: BrowserError) -> Self {
        match error {
            BrowserError::Unreachable(_) => RunError::WorkerDown(error.to_string()),
            other => RunError::Failed(other.to_string()),
        }
    }
}

impl From<sqlx::Error> for RunError {
    fn from(error: sqlx::Error) -> Self {
        RunError::Failed(format!("database error: {error}"))
    }
}

struct Documents {
    resume: (String, Vec<u8>),
    cover_letter: Option<(String, Vec<u8>)>,
}

impl Applier {
    pub fn new(db: PgPool, llm: LlmClient, browser: BrowserClient, pacing: bool) -> Self {
        let (events, _) = broadcast::channel(256);
        Self {
            inner: Arc::new(Inner {
                db,
                llm,
                browser,
                wake: Notify::new(),
                state: Mutex::new(ApplierState::default()),
                events,
                pacing,
            }),
        }
    }

    pub fn browser(&self) -> &BrowserClient {
        &self.inner.browser
    }

    pub fn state(&self) -> ApplierState {
        self.inner.state.lock().unwrap().clone()
    }

    pub fn wake(&self) {
        self.inner.wake.notify_one();
    }

    pub fn subscribe(&self) -> broadcast::Receiver<LiveEvent> {
        self.inner.events.subscribe()
    }

    /// Runs interrupted by a restart go back in the queue (in a new tab).
    pub async fn requeue_interrupted(&self) -> sqlx::Result<u64> {
        let done = sqlx::query(
            "UPDATE applications SET status = 'queued', tab_id = NULL, updated_at = now()
             WHERE status = 'running'",
        )
        .execute(&self.inner.db)
        .await?;
        Ok(done.rows_affected())
    }

    pub fn spawn(&self) {
        let applier = self.clone();
        tokio::spawn(async move {
            let mut runs = JoinSet::new();
            loop {
                if let Err(error) = applier.start_runs(&mut runs).await {
                    tracing::error!(?error, "apply runner error");
                }
                tokio::select! {
                    _ = applier.inner.wake.notified() => {}
                    Some(_) = runs.join_next(), if !runs.is_empty() => {}
                    _ = tokio::time::sleep(IDLE_WAIT) => {}
                }
                applier.inner.state.lock().unwrap().running = runs.len();
            }
        });
    }

    /// Start queued runs while there are free slots.
    async fn start_runs(&self, runs: &mut JoinSet<()>) -> anyhow::Result<()> {
        let queued: i64 =
            sqlx::query_scalar("SELECT count(*) FROM applications WHERE status = 'queued'")
                .fetch_one(&self.inner.db)
                .await?;
        if queued == 0 {
            self.set_paused(None);
            return Ok(());
        }
        if let Err(error) = self.inner.browser.health().await {
            self.set_paused(Some(error.to_string()));
            return Ok(());
        }
        let settings = settings::load(&self.inner.db).await?;
        while runs.len() < settings.apply_parallel as usize {
            let Some(run) = self.claim(settings.apply_daily_cap).await? else {
                break;
            };
            let applier = self.clone();
            runs.spawn(async move { applier.run(run).await });
        }
        self.inner.state.lock().unwrap().running = runs.len();
        Ok(())
    }

    fn set_paused(&self, reason: Option<String>) {
        self.inner.state.lock().unwrap().paused_reason = reason;
    }

    /// Applications started in the last 24 hours (practice runs don't count).
    pub async fn started_today(db: &PgPool) -> sqlx::Result<i64> {
        sqlx::query_scalar(
            "SELECT count(*) FROM applications
             WHERE NOT practice AND started_at > now() - interval '24 hours'",
        )
        .fetch_one(db)
        .await
    }

    /// Take the next queued application. Runs carrying on after your answers
    /// go first, and new ones only start while under the daily limit.
    async fn claim(&self, daily_cap: i32) -> sqlx::Result<Option<Run>> {
        let db = &self.inner.db;
        let under_cap = Self::started_today(db).await? < i64::from(daily_cap);
        let id: Option<Uuid> = sqlx::query_scalar(
            "UPDATE applications SET status = 'running', message = NULL, manual_reason = NULL,
                    started_at = coalesce(started_at, now()), updated_at = now()
             WHERE id = (
                SELECT id FROM applications
                WHERE status = 'queued' AND ($1 OR started_at IS NOT NULL OR practice)
                ORDER BY started_at IS NULL, created_at
                FOR UPDATE SKIP LOCKED LIMIT 1
             )
             RETURNING id",
        )
        .bind(under_cap)
        .fetch_optional(db)
        .await?;
        if id.is_none() && !under_cap {
            self.set_paused(Some(format!(
                "Daily limit reached ({daily_cap} applications in 24 hours). Change it on Settings."
            )));
        }
        let Some(id) = id else { return Ok(None) };
        self.status_changed(id, "running");
        self.load_run(id).await
    }

    async fn load_run(&self, id: Uuid) -> sqlx::Result<Option<Run>> {
        sqlx::query_as(
            "SELECT a.id, a.job_id, a.start_url, a.tab_id, a.resume_id, a.cover_letter_id,
                    j.title AS job_title, j.company, j.description_text AS job_description
             FROM applications a JOIN jobs j ON j.id = a.job_id WHERE a.id = $1",
        )
        .bind(id)
        .fetch_optional(&self.inner.db)
        .await
    }

    /// Claim and run one application, start to stop, in this task.
    /// Public so tests can drive the runner step by step.
    pub async fn run_next(&self) -> anyhow::Result<Option<Uuid>> {
        let settings = settings::load(&self.inner.db).await?;
        let Some(run) = self.claim(settings.apply_daily_cap).await? else {
            return Ok(None);
        };
        let id = run.id;
        self.run(run).await;
        Ok(Some(id))
    }

    async fn run(&self, run: Run) {
        let id = run.id;
        let result = self.drive(&run).await;
        let db = &self.inner.db;
        let outcome = match result {
            Ok(outcome) => outcome,
            Err(RunError::WorkerDown(message)) => {
                self.log(
                    id,
                    "warn",
                    &format!("{message} The run will carry on when it's back."),
                )
                .await;
                let _ = sqlx::query(
                    "UPDATE applications SET status = 'queued', tab_id = NULL, updated_at = now() WHERE id = $1",
                )
                .bind(id)
                .execute(db)
                .await;
                self.set_paused(Some(message));
                self.status_changed(id, "queued");
                return;
            }
            Err(RunError::Failed(message)) => {
                self.log(id, "error", &message).await;
                self.finish(id, "failed", None, &message).await;
                return;
            }
        };

        match outcome {
            Outcome::Ready => {
                let written: i64 = sqlx::query_scalar(
                    "SELECT count(*) FROM application_events WHERE application_id = $1 AND kind = 'written'",
                )
                .bind(id)
                .fetch_one(db)
                .await
                .unwrap_or(0);
                let message = match written {
                    0 => "Ready for you: check every answer in the browser tab, then click Submit yourself.".to_string(),
                    n => format!(
                        "Ready for you: check every answer in the browser tab, especially the {n} the model wrote, then click Submit yourself."
                    ),
                };
                let message = message.as_str();
                self.log(id, "info", message).await;
                // No pulling the window to the front here: on WSL, a window
                // raised by itself looks focused but doesn't take clicks.
                // "Show in the browser" switches to the tab when you want it.
                let _ = sqlx::query("UPDATE applications SET ready_at = now() WHERE id = $1")
                    .bind(id)
                    .execute(db)
                    .await;
                self.finish(id, "ready_to_submit", None, message).await;
            }
            Outcome::Waiting(n) => {
                let message = format!(
                    "{n} question{} for you. Answer {} on the run page and it carries on.",
                    if n == 1 { "" } else { "s" },
                    if n == 1 { "it" } else { "them" }
                );
                self.log(id, "info", &message).await;
                self.finish(id, "waiting_for_you", None, &message).await;
            }
            Outcome::Manual { reason, message } => {
                self.log(id, "warn", &message).await;
                self.finish(id, "needs_manual", Some(reason), &message)
                    .await;
            }
            Outcome::Cancelled => {
                self.log(id, "info", "Stopped: you cancelled it.").await;
            }
        }
    }

    async fn finish(&self, id: Uuid, status: &str, reason: Option<&str>, message: &str) {
        let done = sqlx::query(
            "UPDATE applications SET status = $2, manual_reason = $3, message = $4, updated_at = now()
             WHERE id = $1 AND status = 'running'",
        )
        .bind(id)
        .bind(status)
        .bind(reason)
        .bind(message)
        .execute(&self.inner.db)
        .await;
        if let Err(error) = done {
            tracing::error!(?error, %id, "couldn't save the run's status");
        }
        self.status_changed(id, status);
    }

    fn status_changed(&self, id: Uuid, status: &str) {
        let _ = self.inner.events.send(LiveEvent::Status {
            application_id: id,
            status: status.to_string(),
        });
    }

    /// Add a line to the run log (and send it to anyone watching).
    pub async fn log(&self, id: Uuid, kind: &str, message: &str) {
        let event: sqlx::Result<Event> = sqlx::query_as(
            "INSERT INTO application_events (application_id, kind, message) VALUES ($1, $2, $3)
             RETURNING id, application_id, kind, message, at",
        )
        .bind(id)
        .bind(kind)
        .bind(message)
        .fetch_one(&self.inner.db)
        .await;
        match event {
            Ok(event) => {
                let _ = self.inner.events.send(LiveEvent::Log(event));
            }
            Err(error) => tracing::error!(?error, "couldn't save a run event"),
        }
    }

    async fn cancelled(&self, id: Uuid) -> bool {
        let status: Option<String> =
            sqlx::query_scalar("SELECT status FROM applications WHERE id = $1")
                .bind(id)
                .fetch_optional(&self.inner.db)
                .await
                .ok()
                .flatten();
        status.as_deref() != Some("running")
    }

    async fn pause(&self, min_ms: u64, max_ms: u64) {
        if self.inner.pacing {
            let ms = rand::random_range(min_ms..max_ms);
            tokio::time::sleep(Duration::from_millis(ms)).await;
        }
    }

    async fn load_documents(&self, run: &Run) -> Result<Documents, RunError> {
        let db = &self.inner.db;
        let Some(resume_id) = run.resume_id else {
            return Err(RunError::Failed("No approved resume for this job.".into()));
        };
        let resume = documents::docx_file(db, resume_id)
            .await
            .map_err(|e| RunError::Failed(format!("Couldn't build the resume file: {e}")))?;
        let cover_letter = match run.cover_letter_id {
            Some(id) => Some(documents::docx_file(db, id).await.map_err(|e| {
                RunError::Failed(format!("Couldn't build the cover letter file: {e}"))
            })?),
            None => None,
        };
        Ok(Documents {
            resume,
            cover_letter,
        })
    }

    /// Answers the agent may reuse: the ones you gave for this application
    /// first, then the answer bank.
    /// Your answer to "Do you want to include a cover letter?" for this run:
    /// None if you haven't been asked yet, Some(true) for "Yes, I'll add one".
    async fn cover_letter_decision(&self, id: Uuid) -> sqlx::Result<Option<bool>> {
        let answer: Option<String> = sqlx::query_scalar(
            "SELECT answer FROM application_questions
             WHERE application_id = $1 AND field_id = $2 AND answer IS NOT NULL
             ORDER BY answered_at DESC LIMIT 1",
        )
        .bind(id)
        .bind(COVER_LETTER_QUESTION_ID)
        .fetch_optional(&self.inner.db)
        .await?;
        Ok(answer.map(|a| a == COVER_LETTER_YES))
    }

    async fn known_answers(&self, id: Uuid) -> sqlx::Result<Vec<KnownAnswer>> {
        let db = &self.inner.db;
        let mine: Vec<(String, String)> = sqlx::query_as(
            "SELECT label, answer FROM application_questions
             WHERE application_id = $1 AND answer IS NOT NULL ORDER BY answered_at DESC",
        )
        .bind(id)
        .fetch_all(db)
        .await?;
        let bank: Vec<(Uuid, String, String)> = sqlx::query_as(
            "SELECT id, question, answer FROM answers ORDER BY times_used DESC, updated_at DESC",
        )
        .fetch_all(db)
        .await?;
        Ok(mine
            .into_iter()
            .map(|(question, answer)| KnownAnswer {
                id: None,
                question,
                answer,
            })
            .chain(bank.into_iter().map(|(id, question, answer)| KnownAnswer {
                id: Some(id),
                question,
                answer,
            }))
            .collect())
    }

    async fn open_tab(&self, run: &Run) -> Result<String, RunError> {
        if let Some(tab) = &run.tab_id
            && self.inner.browser.has_tab(tab).await?
        {
            self.log(run.id, "info", "Carrying on in the same browser tab.")
                .await;
            return Ok(tab.clone());
        }
        self.log(run.id, "info", &format!("Opening {}", run.start_url))
            .await;
        let tab = self.inner.browser.open(&run.start_url, false).await?;
        sqlx::query("UPDATE applications SET tab_id = $2, updated_at = now() WHERE id = $1")
            .bind(run.id)
            .bind(&tab)
            .execute(&self.inner.db)
            .await?;
        Ok(tab)
    }

    async fn drive(&self, run: &Run) -> Result<Outcome, RunError> {
        let db = &self.inner.db;
        let docs = self.load_documents(run).await?;
        let facts = profile::load_facts(db).await?;
        let tab = self.open_tab(run).await?;
        let total_years = profile::current_resume(db).await?.and_then(|r| {
            experience::total_years(
                &MasterResume::parse(&r.content),
                chrono::Local::now().date_naive(),
            )
        });

        let mut last_click: Option<String> = None;
        let mut stuck = 0;
        // Optional fields nobody could answer: don't ask the model again.
        let mut skipped: HashSet<String> = HashSet::new();
        // Fields already filled once in this run. A checkbox answered "No", or
        // a box the site clears again, still looks empty on the next look:
        // filling it again would loop forever, so each field gets one go.
        let mut filled: HashSet<String> = HashSet::new();
        let mut uploaded: Vec<Doc> = Vec::new();
        for _ in 0..MAX_STEPS {
            if self.cancelled(run.id).await {
                return Ok(Outcome::Cancelled);
            }
            sqlx::query(
                "UPDATE applications SET steps = steps + 1, updated_at = now() WHERE id = $1",
            )
            .bind(run.id)
            .execute(db)
            .await?;

            let snap = self.inner.browser.snapshot(&tab).await?;
            sqlx::query("UPDATE applications SET current_url = $2 WHERE id = $1")
                .bind(run.id)
                .bind(&snap.url)
                .execute(db)
                .await?;

            if snap.captcha {
                return Ok(Outcome::Manual {
                    reason: "captcha",
                    message: "A CAPTCHA appeared. The agent never solves those: finish this one yourself in the browser tab.".into(),
                });
            }
            if snap.password {
                return Ok(Outcome::Manual {
                    reason: "login",
                    message: "This site wants you to log in. Log in yourself in the agent's browser tab, then click Try again.".into(),
                });
            }

            let answers = self.known_answers(run.id).await?;
            let ctx = FillContext {
                facts: &facts,
                answers: &answers,
                has_cover_letter: docs.cover_letter.is_some(),
                resume_file: &docs.resume.0,
                cover_letter_file: docs.cover_letter.as_ref().map(|(name, _)| name.as_str()),
                uploaded: &uploaded,
            };
            let mut plan = fill::plan(&snap.fields, &ctx);
            plan.actions.retain(|a| match a {
                Action::Fill { field_id, .. } => !filled.contains(field_id),
                Action::Upload { .. } => true,
            });
            plan.unknown
                .retain(|f| !skipped.contains(&f.id) && !filled.contains(&f.id));
            // A field with no label can't be answered by anyone: leave it.
            for field in plan.unknown.iter().filter(|f| f.label.trim().is_empty()) {
                skipped.insert(field.id.clone());
                self.log(
                    run.id,
                    "info",
                    &format!("Left a {} with no label alone.", field.kind.describe()),
                )
                .await;
            }
            plan.unknown.retain(|f| !f.label.trim().is_empty());
            // "Years' experience as a <this job>": counted, not guessed.
            if let Some(total) = total_years {
                plan.unknown.retain(
                    |f| match experience::years_answer(f, &run.job_title, total) {
                        Some(value) => {
                            plan.actions.push(Action::Fill {
                                field_id: f.id.clone(),
                                label: f.label.clone(),
                                value,
                                source: fill::Source::ResumeDates,
                            });
                            false
                        }
                        None => true,
                    },
                );
            }
            if plan.needs_cover_letter {
                return Ok(Outcome::Manual {
                    reason: "cover_letter",
                    message: "This form needs a cover letter. Add one on the job, approve it, then click Try again.".into(),
                });
            }
            // An optional cover letter, and none approved: your call, once per run.
            if plan.cover_letter_optional {
                match self.cover_letter_decision(run.id).await? {
                    None => {
                        let asked = self.ask_you(run.id, &[cover_letter_question()]).await?;
                        return Ok(Outcome::Waiting(asked));
                    }
                    Some(true) => {
                        return Ok(Outcome::Manual {
                            reason: "cover_letter",
                            message: "You want a cover letter for this one. On the job, prepare one and approve it, then click Try again here.".into(),
                        });
                    }
                    Some(false) => {}
                }
            }

            let mut did_something = self.apply_plan(run.id, &tab, &plan.actions, &docs).await?;
            for action in &plan.actions {
                match action {
                    Action::Upload { doc, .. } => uploaded.push(*doc),
                    Action::Fill { field_id, .. } => {
                        filled.insert(field_id.clone());
                    }
                }
            }

            // What code couldn't fill: ask the model, then you.
            let (text_unknown, file_unknown): (Vec<Field>, Vec<Field>) = plan
                .unknown
                .into_iter()
                .partition(|f| f.kind != FieldKind::File);
            if let Some(file) = file_unknown.iter().find(|f| f.required) {
                return Ok(Outcome::Manual {
                    reason: "stuck",
                    message: format!(
                        "This form wants a file the agent doesn't have: \"{}\".",
                        file.label
                    ),
                });
            }
            // An optional tick box ("Make this my default resumé", "Send me
            // job alerts") stays as the site set it: not worth a model call.
            let text_unknown: Vec<Field> = text_unknown
                .into_iter()
                .filter(|f| {
                    let leave = f.kind == FieldKind::Checkbox && !f.required;
                    if leave {
                        skipped.insert(f.id.clone());
                    }
                    !leave
                })
                .collect();
            let mut still_unknown = text_unknown.clone();
            if !text_unknown.is_empty() {
                let answered = self.ask_model(run, &facts, &answers, &text_unknown).await;
                if !answered.is_empty() {
                    let answered_ids = self
                        .fill_fields(run.id, &tab, &answered, &text_unknown)
                        .await?;
                    did_something |= !answered_ids.is_empty();
                    still_unknown.retain(|f| !answered_ids.contains(&f.id));
                    filled.extend(answered_ids);
                }
            }
            let (required, optional): (Vec<Field>, Vec<Field>) =
                still_unknown.into_iter().partition(|f| f.required);
            for field in &optional {
                skipped.insert(field.id.clone());
                if field.kind != FieldKind::Checkbox {
                    self.log(
                        run.id,
                        "info",
                        &format!("Left \"{}\" empty (optional).", field.label),
                    )
                    .await;
                }
            }

            if !required.is_empty() {
                let asked = self.ask_you(run.id, &required).await?;
                return Ok(Outcome::Waiting(asked));
            }

            // New fields can appear after filling (an upload box after "Upload a
            // resumé"), so read the page again before moving on.
            if did_something {
                continue;
            }

            if let Some(next) = snap.buttons.iter().find(|b| b.kind == ButtonKind::Next) {
                let signature = page_signature(&snap);
                if last_click.as_deref() == Some(signature.as_str()) {
                    stuck += 1;
                    if stuck >= 2 {
                        let why = if snap.errors.is_empty() {
                            "the page didn't change".to_string()
                        } else {
                            snap.errors.join(" ")
                        };
                        return Ok(Outcome::Manual {
                            reason: "stuck",
                            message: format!(
                                "The form didn't accept the answers ({why}). Finish this one yourself, or fix the answer and Try again."
                            ),
                        });
                    }
                } else {
                    stuck = 0;
                }
                self.pause(800, 2000).await;
                self.inner.browser.click(&tab, &next.id).await?;
                self.log(run.id, "click", &format!("Clicked \"{}\".", next.text))
                    .await;
                last_click = Some(signature);
                continue;
            }

            if snap.buttons.iter().any(|b| b.kind == ButtonKind::Submit) {
                return Ok(Outcome::Ready);
            }
            return Ok(Outcome::Manual {
                reason: "stuck",
                message: "Couldn't find a Continue or Submit button on this page. Finish this one yourself.".into(),
            });
        }
        Ok(Outcome::Manual {
            reason: "step_limit",
            message: format!(
                "Stopped after {MAX_STEPS} steps without reaching Submit. Finish this one yourself."
            ),
        })
    }

    /// Do the uploads and fills that code decided on. True if anything changed.
    async fn apply_plan(
        &self,
        id: Uuid,
        tab: &str,
        actions: &[Action],
        docs: &Documents,
    ) -> Result<bool, RunError> {
        let mut changed = false;
        let mut fills = Vec::new();
        for action in actions {
            match action {
                Action::Upload {
                    field_id,
                    label,
                    doc,
                } => {
                    let (name, bytes) = match doc {
                        Doc::Resume => &docs.resume,
                        Doc::CoverLetter => docs
                            .cover_letter
                            .as_ref()
                            .expect("planned only with a letter"),
                    };
                    self.pause(600, 1500).await;
                    self.inner
                        .browser
                        .upload(tab, field_id, name, bytes)
                        .await?;
                    self.log(id, "fill", &format!("Uploaded {name} to \"{label}\"."))
                        .await;
                    changed = true;
                }
                Action::Fill {
                    field_id,
                    label,
                    value,
                    source,
                } => {
                    fills.push((
                        field_id.clone(),
                        value.clone(),
                        label.clone(),
                        source.describe(),
                    ));
                }
            }
        }
        for (field_id, value, label, source) in fills {
            self.pause(300, 900).await;
            let result = self
                .inner
                .browser
                .fill(tab, &[(field_id, value.clone())])
                .await?;
            if result.first().is_some_and(|r| r.ok) {
                self.log(
                    id,
                    "fill",
                    &format!("\"{label}\": {} ({source}).", shorten(&value)),
                )
                .await;
                changed = true;
            } else {
                let error = result
                    .first()
                    .and_then(|r| r.error.clone())
                    .unwrap_or_default();
                self.log(id, "warn", &format!("Couldn't fill \"{label}\": {error}"))
                    .await;
            }
        }
        Ok(changed)
    }

    /// Fill (id, value) pairs and return the ids that worked.
    async fn fill_fields(
        &self,
        id: Uuid,
        tab: &str,
        values: &[(String, String)],
        fields: &[Field],
    ) -> Result<Vec<String>, RunError> {
        let mut filled = Vec::new();
        for (field_id, value) in values {
            let Some(field) = fields.iter().find(|f| &f.id == field_id) else {
                continue;
            };
            let label = field.label.as_str();
            self.pause(300, 900).await;
            let result = self
                .inner
                .browser
                .fill(tab, &[(field_id.clone(), value.clone())])
                .await?;
            if result.first().is_some_and(|r| r.ok) {
                // A sentence the model wrote gets its own line, so you
                // know to read it before you submit.
                // Shown in full, so you can read it on the run page.
                let line = if prompt::is_written(field, value) {
                    let line = format!(
                        "\"{label}\" (written by the model, read it before you submit): {value}"
                    );
                    ("written", line)
                } else {
                    let line = format!("\"{label}\": {} (the model's answer).", shorten(value));
                    ("model", line)
                };
                self.log(id, line.0, &line.1).await;
                filled.push(field_id.clone());
            } else {
                let error = result
                    .first()
                    .and_then(|r| r.error.clone())
                    .unwrap_or_default();
                self.log(id, "warn", &format!("Couldn't fill \"{label}\": {error}"))
                    .await;
            }
        }
        Ok(filled)
    }

    /// The model's answers for fields code couldn't fill. Any problem with
    /// the model just means those questions come to you instead.
    async fn ask_model(
        &self,
        run: &Run,
        facts: &profile::Facts,
        answers: &[KnownAnswer],
        fields: &[Field],
    ) -> Vec<(String, String)> {
        let db = &self.inner.db;
        let settings = match settings::load(db).await {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        let master = profile::current_resume(db)
            .await
            .ok()
            .flatten()
            .map(|r| MasterResume::parse(&r.content));
        let today = chrono::Local::now().date_naive();
        let evidence = master
            .as_ref()
            .map(|m| {
                format!(
                    "{}\n{}",
                    evidence_block(m),
                    experience::experience_block(m, today)
                )
            })
            .unwrap_or_default();
        let user = prompt::user_prompt(&PromptInput {
            job_title: &run.job_title,
            company: run.company.as_deref(),
            job_description: run.job_description.as_deref(),
            facts,
            answers,
            evidence: &evidence,
            fields,
        });
        self.log(
            run.id,
            "model",
            &format!(
                "Asking {} about {} field{}.",
                settings.apply_model,
                fields.len(),
                if fields.len() == 1 { "" } else { "s" }
            ),
        )
        .await;
        let options = ChatOptions {
            think: settings.apply_think,
            num_ctx: None,
        };
        match self
            .inner
            .llm
            .chat_json_with(
                &settings.apply_model,
                prompt::SYSTEM,
                &user,
                &prompt::schema(),
                options,
            )
            .await
        {
            Ok(raw) => match prompt::usable_answers(raw, fields) {
                Ok(values) => values,
                Err(error) => {
                    self.log(
                        run.id,
                        "warn",
                        &format!(
                            "The model's answer wasn't usable ({error}), so these come to you."
                        ),
                    )
                    .await;
                    Vec::new()
                }
            },
            Err(error) => {
                self.log(
                    run.id,
                    "warn",
                    &format!("{error} These questions come to you instead."),
                )
                .await;
                Vec::new()
            }
        }
    }

    /// Save the questions only you can answer. Returns how many are open.
    async fn ask_you(&self, id: Uuid, fields: &[Field]) -> Result<usize, RunError> {
        let db = &self.inner.db;
        for field in fields {
            let exists: bool = sqlx::query_scalar(
                "SELECT EXISTS (SELECT 1 FROM application_questions
                                WHERE application_id = $1 AND label = $2 AND answer IS NULL)",
            )
            .bind(id)
            .bind(&field.label)
            .fetch_one(db)
            .await?;
            if exists {
                continue;
            }
            let kind = serde_json::to_value(&field.kind)
                .ok()
                .and_then(|v| v.as_str().map(str::to_string))
                .unwrap_or_else(|| "text".into());
            sqlx::query(
                "INSERT INTO application_questions (application_id, field_id, label, kind, options)
                 VALUES ($1, $2, $3, $4, $5)",
            )
            .bind(id)
            .bind(&field.id)
            .bind(&field.label)
            .bind(kind)
            .bind(&field.options)
            .execute(db)
            .await?;
            self.log(
                id,
                "answer",
                &format!("Question for you: \"{}\"", field.label),
            )
            .await;
        }
        let open: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM application_questions WHERE application_id = $1 AND answer IS NULL",
        )
        .bind(id)
        .fetch_one(db)
        .await?;
        Ok(open as usize)
    }
}

/// Identifies "the same page with the same fields" to notice when clicking
/// Continue doesn't get anywhere.
fn page_signature(snap: &Snapshot) -> String {
    let fields: Vec<&str> = snap.fields.iter().map(|f| f.id.as_str()).collect();
    format!(
        "{}|{}|{}",
        snap.url,
        snap.headings.join("/"),
        fields.join(",")
    )
}

/// Questions the agent asks itself (not from the page) have ids starting
/// with "cp-". Their answers belong to one run, never to the answer bank.
pub const COVER_LETTER_QUESTION_ID: &str = "cp-cover-letter";
const COVER_LETTER_YES: &str = "Yes, I'll add one";

fn cover_letter_question() -> Field {
    Field {
        id: COVER_LETTER_QUESTION_ID.into(),
        kind: FieldKind::Radio,
        label: "This job accepts a cover letter, and there isn't one approved for it. Include one?"
            .into(),
        required: true,
        options: vec!["No, skip it".into(), COVER_LETTER_YES.into()],
        value: String::new(),
    }
}

fn shorten(value: &str) -> String {
    let short: String = value.chars().take(80).collect();
    if short.len() < value.len() {
        format!("{short}…")
    } else {
        short
    }
}
