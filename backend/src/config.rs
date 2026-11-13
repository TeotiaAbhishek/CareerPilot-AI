use std::net::SocketAddr;

use anyhow::{Context, Result};

/// Everything the API needs from the environment, read once at startup.
#[derive(Clone, Debug)]
pub struct Config {
    pub database_url: String,
    pub bind_addr: SocketAddr,
    pub web_origin: String,
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

        Ok(Self {
            database_url,
            bind_addr,
            web_origin,
        })
    }
}
