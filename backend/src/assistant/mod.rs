//! F8b: the suggestions assistant on the Ask page.
//!
//! A small agent loop: the model reads your question, calls read-only tools
//! (tools.rs) to look things up, and answers. It runs at most `MAX_ROUNDS`
//! rounds of tool calls, then has to answer with what it has. The answer is
//! streamed to the page as it's written, one JSON object per line.

pub mod tools;

use std::convert::Infallible;

use axum::{
    Json,
    body::Body,
    extract::{Path, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool, types::Json as SqlJson};
use tokio::sync::mpsc;
use tokio_stream::{StreamExt, wrappers::UnboundedReceiverStream};
use uuid::Uuid;

use crate::{
    AppState,
    documents::worker::DRAFT_NUM_CTX,
    error::{AppError, AppResult},
    llm::{ChatOptions, LlmClient, LlmError, Message},
    settings::{self, Settings},
};
use tools::Chip;

/// Rounds of tool calls before the model must answer.
pub const MAX_ROUNDS: usize = 6;
/// Tool calls the model may make in one round.
const MAX_CALLS_PER_ROUND: usize = 4;
/// Earlier messages of the chat sent along with a new question.
const HISTORY: i64 = 10;
const MAX_QUESTION_CHARS: usize = 2_000;

pub fn system_prompt(settings: &Settings, today: &str) -> String {
    format!(
        "You are the assistant inside CareerPilot, an app that finds jobs on SEEK, scores how \
well each fits the user, drafts tailored resumes and applies with a browser agent. You help the \
user decide what to do next: which jobs to apply to first, what keeps coming up missing, how their \
applications are going.

Today is {today}. A job is \"relevant\" when its fit score is at least {threshold} and it has no \
hard blockers.

Rules:
- Look things up with the tools before you answer. Never make up jobs, companies, scores or numbers. \
If the tools don't have it, say so.
- Link every job you mention as [Title at Company](/jobs/ID), using the link the tools give you.
- Be brief and concrete: a sentence or two, then a short bulleted list if it helps. Say why for \
each suggestion (score, missing requirement, how old the ad is).
- You can't apply, save, label or email anything. Say where in the app the user can do it \
(the job's page, Applications, Settings).
- Use Australian English. No em dashes.",
        threshold = settings.relevance_threshold,
    )
}

// ---------- Saved chats ----------

#[derive(Debug, Serialize, FromRow)]
pub struct Chat {
    pub id: Uuid,
    pub title: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct ChatMessage {
    pub id: i64,
    pub role: String,
    pub content: String,
    pub tools: SqlJson<Vec<Chip>>,
    pub failed: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Serialize)]
pub struct ChatView {
    #[serde(flatten)]
    pub chat: Chat,
    pub messages: Vec<ChatMessage>,
}

pub async fn list(State(state): State<AppState>) -> AppResult<Json<Vec<Chat>>> {
    let chats = sqlx::query_as(
        "SELECT id, title, created_at, updated_at FROM chats ORDER BY updated_at DESC LIMIT 100",
    )
    .fetch_all(&state.db)
    .await?;
    Ok(Json(chats))
}

async fn load_chat(db: &PgPool, id: Uuid) -> AppResult<Chat> {
    sqlx::query_as("SELECT id, title, created_at, updated_at FROM chats WHERE id = $1")
        .bind(id)
        .fetch_optional(db)
        .await?
        .ok_or_else(|| AppError::NotFound("Conversation not found.".into()))
}

pub async fn get(State(state): State<AppState>, Path(id): Path<Uuid>) -> AppResult<Json<ChatView>> {
    let chat = load_chat(&state.db, id).await?;
    let messages = sqlx::query_as(
        "SELECT id, role, content, tools, failed, created_at FROM chat_messages
         WHERE chat_id = $1 ORDER BY id",
    )
    .bind(id)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(ChatView { chat, messages }))
}

#[derive(Deserialize)]
pub struct Rename {
    pub title: String,
}

pub async fn rename(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(input): Json<Rename>,
) -> AppResult<Json<Chat>> {
    let title = input.title.trim();
    if title.is_empty() || title.chars().count() > 100 {
        return Err(AppError::BadRequest(
            "A title needs 1-100 characters.".into(),
        ));
    }
    sqlx::query_as(
        "UPDATE chats SET title = $2 WHERE id = $1
         RETURNING id, title, created_at, updated_at",
    )
    .bind(id)
    .bind(title)
    .fetch_optional(&state.db)
    .await?
    .map(Json)
    .ok_or_else(|| AppError::NotFound("Conversation not found.".into()))
}

pub async fn delete(State(state): State<AppState>, Path(id): Path<Uuid>) -> AppResult<StatusCode> {
    let deleted = sqlx::query("DELETE FROM chats WHERE id = $1")
        .bind(id)
        .execute(&state.db)
        .await?
        .rows_affected();
    if deleted == 0 {
        return Err(AppError::NotFound("Conversation not found.".into()));
    }
    Ok(StatusCode::NO_CONTENT)
}

/// The first question, cut at a word boundary, names a new chat.
pub fn title_for(question: &str) -> String {
    let words = question.split_whitespace().collect::<Vec<_>>().join(" ");
    if words.chars().count() <= 60 {
        return words;
    }
    let mut title = String::new();
    for word in words.split(' ') {
        if title.chars().count() + word.chars().count() + 1 > 57 {
            break;
        }
        if !title.is_empty() {
            title.push(' ');
        }
        title.push_str(word);
    }
    if title.is_empty() {
        title = words.chars().take(57).collect();
    }
    format!("{}...", title.trim_end_matches(['?', ',', '.', ' ']))
}

// ---------- Asking ----------

#[derive(Deserialize)]
pub struct Ask {
    /// Leave out to start a new conversation.
    pub chat_id: Option<Uuid>,
    pub question: String,
}

/// What the page receives, one per line.
#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    /// The conversation (new or existing) and your saved question.
    Start {
        chat: Chat,
        question: ChatMessage,
    },
    /// The assistant looked something up.
    Tool {
        chip: Chip,
    },
    /// More of the answer.
    Text {
        text: String,
    },
    /// Throw away the text so far: it was thinking out loud before a tool call.
    Reset,
    Done {
        message: ChatMessage,
    },
    Error {
        message: ChatMessage,
    },
}

pub async fn ask(State(state): State<AppState>, Json(input): Json<Ask>) -> AppResult<Response> {
    let question = input.question.trim().to_string();
    if question.is_empty() {
        return Err(AppError::BadRequest("Type a question first.".into()));
    }
    if question.chars().count() > MAX_QUESTION_CHARS {
        return Err(AppError::BadRequest(format!(
            "Keep questions under {MAX_QUESTION_CHARS} characters."
        )));
    }
    let settings = settings::load(&state.db).await?;

    let chat =
        match input.chat_id {
            Some(id) => load_chat(&state.db, id).await?,
            None => sqlx::query_as(
                "INSERT INTO chats (title) VALUES ($1) RETURNING id, title, created_at, updated_at",
            )
            .bind(title_for(&question))
            .fetch_one(&state.db)
            .await?,
        };
    // Earlier turns that worked, oldest first. Failed answers and the
    // questions that led to them are left out.
    let earlier: Vec<(String, String)> = sqlx::query_as(
        "SELECT role, content FROM (
            SELECT m.id, m.role, m.content FROM chat_messages m
            WHERE m.chat_id = $1 AND NOT m.failed
              AND NOT EXISTS (SELECT 1 FROM chat_messages n
                              WHERE n.chat_id = m.chat_id AND n.failed
                                AND n.id = (SELECT min(o.id) FROM chat_messages o
                                            WHERE o.chat_id = m.chat_id AND o.id > m.id))
            ORDER BY m.id DESC LIMIT $2
         ) recent ORDER BY id",
    )
    .bind(chat.id)
    .bind(HISTORY)
    .fetch_all(&state.db)
    .await?;
    let saved = save_message(&state.db, chat.id, "user", &question, &[], false).await?;

    let today = Utc::now()
        .with_timezone(&state.schedule.timezone)
        .format("%A %-d %B %Y")
        .to_string();
    let mut messages = vec![Message::System(system_prompt(&settings, &today))];
    for (role, content) in earlier {
        messages.push(if role == "user" {
            Message::User(content)
        } else {
            Message::Assistant {
                content,
                tool_calls: Vec::new(),
                raw: None,
            }
        });
    }
    messages.push(Message::User(question));

    let chat_id = chat.id;
    let (events, receiver) = mpsc::unbounded_channel();
    let _ = events.send(Event::Start {
        chat,
        question: saved,
    });
    let db = state.db.clone();
    let llm = state.drafter.llm().clone();
    // Its own task: if you close the page, the answer is still finished and saved.
    tokio::spawn(finish(db, llm, settings, chat_id, messages, events));

    let stream = UnboundedReceiverStream::new(receiver).map(|event| {
        let mut line = serde_json::to_string(&event).unwrap_or_else(|_| "{}".into());
        line.push('\n');
        Ok::<_, Infallible>(line)
    });
    Ok((
        [
            (header::CONTENT_TYPE, "application/x-ndjson"),
            (header::CACHE_CONTROL, "no-cache"),
        ],
        Body::from_stream(stream),
    )
        .into_response())
}

async fn save_message(
    db: &PgPool,
    chat_id: Uuid,
    role: &str,
    content: &str,
    chips: &[Chip],
    failed: bool,
) -> sqlx::Result<ChatMessage> {
    let message = sqlx::query_as(
        "INSERT INTO chat_messages (chat_id, role, content, tools, failed)
         VALUES ($1, $2, $3, $4, $5)
         RETURNING id, role, content, tools, failed, created_at",
    )
    .bind(chat_id)
    .bind(role)
    .bind(content)
    .bind(SqlJson(chips))
    .bind(failed)
    .fetch_one(db)
    .await?;
    sqlx::query("UPDATE chats SET updated_at = now() WHERE id = $1")
        .bind(chat_id)
        .execute(db)
        .await?;
    Ok(message)
}

/// Run the agent, then save the answer (or what went wrong) and tell the page.
async fn finish(
    db: PgPool,
    llm: LlmClient,
    settings: Settings,
    chat_id: Uuid,
    messages: Vec<Message>,
    events: mpsc::UnboundedSender<Event>,
) {
    let result = run_agent(&db, &llm, &settings, messages, &events).await;
    let (content, chips, failed) = match result {
        Ok((answer, chips)) => (answer, chips, false),
        Err((problem, chips)) => (problem, chips, true),
    };
    match save_message(&db, chat_id, "assistant", &content, &chips, failed).await {
        Ok(message) if failed => {
            let _ = events.send(Event::Error { message });
        }
        Ok(message) => {
            let _ = events.send(Event::Done { message });
        }
        Err(error) => tracing::error!(?error, "couldn't save the assistant's answer"),
    }
}

/// The loop. Ok is the answer; Err is a message for you about what went wrong.
/// Both carry the chips for the tools that ran.
pub async fn run_agent(
    db: &PgPool,
    llm: &LlmClient,
    settings: &Settings,
    mut messages: Vec<Message>,
    events: &mpsc::UnboundedSender<Event>,
) -> Result<(String, Vec<Chip>), (String, Vec<Chip>)> {
    let specs = tools::specs();
    let options = ChatOptions {
        think: settings.assistant_think,
        // Room for tool results. The same size as drafting, so Ollama
        // doesn't reload the model when the two take turns.
        num_ctx: Some(DRAFT_NUM_CTX),
    };
    let mut chips: Vec<Chip> = Vec::new();

    for round in 0..=MAX_ROUNDS {
        let last = round == MAX_ROUNDS;
        if last {
            messages.push(Message::System(
                "You've used all your lookups. Answer now with what you found.".into(),
            ));
        }
        let mut on_text = |text: &str| {
            let _ = events.send(Event::Text {
                text: text.to_string(),
            });
        };
        let turn = llm
            .chat_tools(
                &settings.assistant_model,
                &messages,
                if last { &[] } else { &specs },
                options,
                &mut on_text,
            )
            .await
            .map_err(|e| (problem(&e), chips.clone()))?;

        if turn.tool_calls.is_empty() {
            let answer = turn.content.trim().to_string();
            if answer.is_empty() {
                return Err((
                    "The model didn't answer. Try asking another way.".into(),
                    chips,
                ));
            }
            return Ok((answer, chips));
        }

        if !turn.content.trim().is_empty() {
            let _ = events.send(Event::Reset);
        }
        let calls: Vec<_> = turn
            .tool_calls
            .into_iter()
            .take(MAX_CALLS_PER_ROUND)
            .collect();
        messages.push(Message::Assistant {
            content: turn.content,
            tool_calls: calls.clone(),
            raw: turn.raw,
        });
        for call in calls {
            let output = tools::run(db, settings, &call.name, &call.arguments)
                .await
                .map_err(|error| {
                    tracing::error!(?error, tool = %call.name, "assistant tool failed");
                    (
                        "Something went wrong reading your data. Try again.".to_string(),
                        chips.clone(),
                    )
                })?;
            let _ = events.send(Event::Tool {
                chip: output.chip.clone(),
            });
            chips.push(output.chip);
            messages.push(Message::Tool {
                name: call.name,
                content: output.content,
            });
        }
    }
    unreachable!("the last round has no tools, so it always returns")
}

/// What to tell you when the model call fails.
fn problem(error: &LlmError) -> String {
    match error {
        LlmError::Unreachable(m) | LlmError::ModelMissing(m) => m.clone(),
        LlmError::Timeout => "The model took too long. Try a shorter question.".into(),
        other => format!("The model's answer didn't work: {other}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_questions_are_their_own_title() {
        assert_eq!(title_for("  Which jobs   first? "), "Which jobs first?");
    }

    #[test]
    fn long_questions_are_cut_at_a_word() {
        let title = title_for(
            "Which of the data analyst jobs from this week should I apply to first, given my Power BI experience?",
        );
        assert!(title.ends_with("..."), "{title}");
        assert!(title.chars().count() <= 60, "{title}");
        assert!(title.starts_with("Which of the data analyst jobs"));
    }
}
