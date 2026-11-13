use axum::{Json, extract::State};
use serde::Serialize;

use crate::AppState;

#[derive(Serialize)]
pub struct Health {
    pub status: &'static str,
    pub database: &'static str,
    pub version: &'static str,
}

/// GET /api/health
/// Always answers 200 so the UI can show *which* part is down.
pub async fn health(State(state): State<AppState>) -> Json<Health> {
    let database = match sqlx::query_scalar::<_, i32>("SELECT 1")
        .fetch_one(&state.db)
        .await
    {
        Ok(_) => "ok",
        Err(error) => {
            tracing::warn!(?error, "health check: database unreachable");
            "unreachable"
        }
    };

    Json(Health {
        status: "ok",
        database,
        version: env!("CARGO_PKG_VERSION"),
    })
}
