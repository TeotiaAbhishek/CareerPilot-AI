use std::{net::SocketAddr, path::PathBuf};

use anyhow::{Context, Result, bail};
use chrono::NaiveTime;
use chrono_tz::Tz;

/// Everything the API needs from the environment, read once at startup.
#[derive(Clone, Debug)]
pub struct Config {
    pub database_url: String,
    pub bind_addr: SocketAddr,
    pub web_origin: String,
    /// Local time to run the daily scrape. `None` turns the schedule off.
    pub scrape_daily_at: Option<NaiveTime>,
    pub scrape_timezone: Tz,
    pub seek_base_url: String,
    pub ollama_url: String,
    /// The model's context window in tokens. Resume + profile + job is ~4-6k.
    pub ollama_num_ctx: u32,
    /// v1's resume file, copied into the database on first start.
    pub resume_path: PathBuf,
    /// Base64 AES-256 key for the password vault. None leaves the vault locked.
    pub vault_key: Option<String>,
    /// The browser worker (browser-worker/, `npm start`).
    pub browser_worker_url: String,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let database_url = std::env::var("DATABASE_URL").context("DATABASE_URL is not set")?;

        let bind_addr = std::env::var("API_BIND_ADDR")
            .unwrap_or_else(|_| "127.0.0.1:8080".to_string())
            .parse()
            .context("API_BIND_ADDR must look like 127.0.0.1:8080")?;

        let web_origin =
            std::env::var("WEB_ORIGIN").unwrap_or_else(|_| "http://localhost:3000".to_string());

        let scrape_daily_at =
            parse_daily_at(&std::env::var("SCRAPE_DAILY_AT").unwrap_or_else(|_| "08:00".into()))?;

        let scrape_timezone: Tz = std::env::var("SCRAPE_TIMEZONE")
            .unwrap_or_else(|_| "Australia/Sydney".into())
            .parse()
            .map_err(|_| anyhow::anyhow!("SCRAPE_TIMEZONE must be like Australia/Sydney"))?;

        let seek_base_url = std::env::var("SEEK_BASE_URL")
            .unwrap_or_else(|_| "https://www.seek.com.au".into())
            .trim_end_matches('/')
            .to_string();

        let ollama_url =
            std::env::var("OLLAMA_URL").unwrap_or_else(|_| "http://localhost:11434".into());
        let ollama_num_ctx = std::env::var("OLLAMA_NUM_CTX")
            .unwrap_or_else(|_| "8192".into())
            .parse()
            .context("OLLAMA_NUM_CTX must be a number like 8192")?;
        let resume_path = std::env::var("RESUME_PATH")
            .unwrap_or_else(|_| "../data/master_resume.md".into())
            .into();
        let vault_key = std::env::var("VAULT_KEY")
            .ok()
            .filter(|k| !k.trim().is_empty());
        let browser_worker_url =
            std::env::var("BROWSER_WORKER_URL").unwrap_or_else(|_| "http://127.0.0.1:8090".into());

        Ok(Self {
            database_url,
            bind_addr,
            web_origin,
            scrape_daily_at,
            scrape_timezone,
            seek_base_url,
            ollama_url,
            ollama_num_ctx,
            resume_path,
            vault_key,
            browser_worker_url,
        })
    }
}

/// "08:00" -> Some(08:00). "off" or "" -> None (no scheduled scrape).
pub fn parse_daily_at(value: &str) -> Result<Option<NaiveTime>> {
    let value = value.trim();
    if value.is_empty() || value.eq_ignore_ascii_case("off") {
        return Ok(None);
    }
    match NaiveTime::parse_from_str(value, "%H:%M") {
        Ok(time) => Ok(Some(time)),
        Err(_) => bail!("SCRAPE_DAILY_AT must be HH:MM (24h) or \"off\", got {value:?}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn daily_at_parses_time_or_off() {
        assert_eq!(
            parse_daily_at("08:30").unwrap(),
            NaiveTime::from_hms_opt(8, 30, 0)
        );
        assert_eq!(parse_daily_at("off").unwrap(), None);
        assert_eq!(parse_daily_at("").unwrap(), None);
        assert!(parse_daily_at("8am").is_err());
    }
}
