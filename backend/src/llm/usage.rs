//! What paid model calls cost: prices, the usage log, and the monthly caps.

use axum::{Json, extract::State};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

use crate::{
    AppState,
    error::{AppError, AppResult},
};

/// A paid model you can pick in Settings, and its price in US dollars per
/// million tokens (input, output). Prices as of November 2026.
pub struct CloudModel {
    pub id: &'static str,
    pub provider: &'static str,
    pub input_per_mtok: f64,
    pub output_per_mtok: f64,
}

pub const CLOUD_MODELS: [CloudModel; 7] = [
    CloudModel {
        id: "claude-haiku-5-5",
        provider: "claude",
        input_per_mtok: 0.10,
        output_per_mtok: 0.50,
    },
    CloudModel {
        id: "claude-sonnet-5-5",
        provider: "claude",
        input_per_mtok: 2.0,
        output_per_mtok: 10.0,
    },
    CloudModel {
        id: "claude-opus-5-5",
        provider: "claude",
        input_per_mtok: 4.0,
        output_per_mtok: 20.0,
    },
    CloudModel {
        id: "gpt-6-luna",
        provider: "openai",
        input_per_mtok: 0.10,
        output_per_mtok: 0.50,
    },
    CloudModel {
        id: "gpt-6.1-terra",
        provider: "openai",
        input_per_mtok: 1.0,
        output_per_mtok: 5.0,
    },
    CloudModel {
        id: "gpt-6.1-sol",
        provider: "openai",
        input_per_mtok: 2.0,
        output_per_mtok: 10.0,
    },
    CloudModel {
        id: "gpt-6-astra",
        provider: "openai",
        input_per_mtok: 10.0,
        output_per_mtok: 50.0,
    },
];

/// Tokens one call used.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Usage {
    pub input_tokens: u32,
    pub output_tokens: u32,
}

/// What `usage` cost on `model`, in US dollars. Models not in the list
/// (a newer one you typed in) are priced like the dearest model of their
/// provider, so the cap errs on the safe side.
pub fn cost_usd(model: &str, usage: Usage) -> f64 {
    let provider = super::provider_of(model).name();
    let price = CLOUD_MODELS.iter().find(|m| m.id == model).or_else(|| {
        CLOUD_MODELS
            .iter()
            .filter(|m| m.provider == provider)
            .max_by(|a, b| a.output_per_mtok.total_cmp(&b.output_per_mtok))
    });
    match price {
        Some(p) => {
            (usage.input_tokens as f64 * p.input_per_mtok
                + usage.output_tokens as f64 * p.output_per_mtok)
                / 1_000_000.0
        }
        None => 0.0,
    }
}

/// Write one call to the usage log. Returns its cost.
pub async fn record(db: &PgPool, model: &str, usage: Usage) -> sqlx::Result<f64> {
    let cost = cost_usd(model, usage);
    sqlx::query(
        "INSERT INTO llm_usage (provider, model, input_tokens, output_tokens, cost_usd)
         VALUES ($1, $2, $3, $4, $5::float8::numeric)",
    )
    .bind(super::provider_of(model).name())
    .bind(model)
    .bind(usage.input_tokens as i32)
    .bind(usage.output_tokens as i32)
    .bind(cost)
    .execute(db)
    .await?;
    Ok(cost)
}

/// True when `provider` has spent its cap this calendar month (server time).
pub async fn over_cap(db: &PgPool, provider: &str) -> sqlx::Result<bool> {
    sqlx::query_scalar(
        "SELECT coalesce((SELECT sum(cost_usd) FROM llm_usage
                          WHERE provider = $1 AND created_at >= date_trunc('month', now())), 0)
                >= (SELECT monthly_cap_usd FROM llm_caps WHERE provider = $1)",
    )
    .bind(provider)
    .fetch_one(db)
    .await
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct ProviderUsage {
    pub provider: String,
    /// True when its API key is in backend/.env. The key itself never leaves the server.
    #[sqlx(skip)]
    pub key_set: bool,
    pub monthly_cap_usd: f64,
    pub spent_this_month_usd: f64,
    pub calls_this_month: i64,
}

#[derive(Debug, Serialize)]
pub struct UsageReport {
    pub providers: Vec<ProviderUsage>,
    /// Paid models with their prices, for the Settings page.
    pub models: Vec<ModelPrice>,
}

#[derive(Debug, Serialize)]
pub struct ModelPrice {
    pub id: &'static str,
    pub provider: &'static str,
    pub input_per_mtok: f64,
    pub output_per_mtok: f64,
}

pub async fn report(State(state): State<AppState>) -> AppResult<Json<UsageReport>> {
    let llm = state.scorer.llm();
    let mut providers: Vec<ProviderUsage> = sqlx::query_as(
        "SELECT c.provider, c.monthly_cap_usd::float8 AS monthly_cap_usd,
                coalesce(sum(u.cost_usd), 0)::float8 AS spent_this_month_usd,
                count(u.id) AS calls_this_month
         FROM llm_caps c
         LEFT JOIN llm_usage u ON u.provider = c.provider
                              AND u.created_at >= date_trunc('month', now())
         GROUP BY c.provider, c.monthly_cap_usd
         ORDER BY c.provider",
    )
    .fetch_all(&state.db)
    .await?;
    for p in &mut providers {
        p.key_set = llm.has_key(&p.provider);
    }
    let models = CLOUD_MODELS
        .iter()
        .map(|m| ModelPrice {
            id: m.id,
            provider: m.provider,
            input_per_mtok: m.input_per_mtok,
            output_per_mtok: m.output_per_mtok,
        })
        .collect();
    Ok(Json(UsageReport { providers, models }))
}

#[derive(Debug, Deserialize)]
pub struct CapPatch {
    pub provider: String,
    pub monthly_cap_usd: f64,
}

pub async fn set_cap(
    State(state): State<AppState>,
    Json(patch): Json<CapPatch>,
) -> AppResult<Json<UsageReport>> {
    if !(0.0..=1000.0).contains(&patch.monthly_cap_usd) {
        return Err(AppError::BadRequest("The cap must be $0-1000.".into()));
    }
    let updated = sqlx::query(
        "UPDATE llm_caps SET monthly_cap_usd = $2::float8::numeric WHERE provider = $1",
    )
    .bind(&patch.provider)
    .bind(patch.monthly_cap_usd)
    .execute(&state.db)
    .await?;
    if updated.rows_affected() == 0 {
        return Err(AppError::BadRequest("Unknown provider.".into()));
    }
    report(State(state)).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cost_uses_the_price_list() {
        let usage = Usage {
            input_tokens: 4_000,
            output_tokens: 500,
        };
        // 4000 * 0.10 / 1M + 500 * 0.50 / 1M
        assert!((cost_usd("claude-haiku-5-5", usage) - 0.00065).abs() < 1e-12);
        assert!((cost_usd("gpt-6.1-sol", usage) - 0.013).abs() < 1e-12);
    }

    #[test]
    fn unknown_models_cost_like_the_dearest_of_their_provider() {
        let usage = Usage {
            input_tokens: 1_000_000,
            output_tokens: 0,
        };
        assert_eq!(cost_usd("claude-something-new", usage), 4.0);
        assert_eq!(cost_usd("gpt-7", usage), 10.0);
        assert_eq!(cost_usd("qwen3:14b", usage), 0.0);
    }
}
