//! POST /api/scrape starts a run in the background.
//! GET /api/scrape/status powers the progress UI.

use axum::{Json, extract::State, http::StatusCode};
use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::{
    AppState,
    error::AppResult,
    scraper::{self, ScrapeRun, Trigger, schedule::next_run_after},
};

pub async fn start(State(state): State<AppState>) -> AppResult<(StatusCode, Json<ScrapeRun>)> {
    let run = scraper::start_run(&state.db, Trigger::Manual).await?;
    // Answer straight away; the run carries on in the background.
    tokio::spawn(scraper::execute_run(
        state.db.clone(),
        state.seek.clone(),
        run.id,
    ));
    Ok((StatusCode::ACCEPTED, Json(run)))
}

#[derive(Serialize)]
pub struct Schedule {
    /// "08:00", or null when the daily scrape is off.
    pub daily_at: Option<String>,
    pub timezone: String,
    pub next_run_at: Option<DateTime<Utc>>,
}

#[derive(Serialize)]
pub struct Status {
    pub current: Option<ScrapeRun>,
    pub recent: Vec<ScrapeRun>,
    pub schedule: Schedule,
}

pub async fn status(State(state): State<AppState>) -> AppResult<Json<Status>> {
    let schedule = Schedule {
        daily_at: state
            .schedule
            .daily_at
            .map(|t| t.format("%H:%M").to_string()),
        timezone: state.schedule.timezone.name().to_string(),
        next_run_at: state
            .schedule
            .daily_at
            .map(|at| next_run_after(Utc::now(), at, state.schedule.timezone)),
    };

    Ok(Json(Status {
        current: scraper::current_run(&state.db).await?,
        recent: scraper::recent_runs(&state.db, 5).await?,
        schedule,
    }))
}
