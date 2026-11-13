use std::time::Duration;

use anyhow::{Context, Result};
use sqlx::{PgPool, postgres::PgPoolOptions};

/// Open a connection pool and bring the schema up to date.
pub async fn connect_and_migrate(database_url: &str) -> Result<PgPool> {
    let pool = PgPoolOptions::new()
        .max_connections(10)
        .acquire_timeout(Duration::from_secs(5))
        .connect(database_url)
        .await
        .context("could not connect to Postgres (is `docker compose up -d` running?)")?;

    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .context("database migrations failed")?;

    Ok(pool)
}
