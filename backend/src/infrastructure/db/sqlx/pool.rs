use anyhow::Context;
use sqlx::{PgPool, postgres::PgPoolOptions};
use std::time::Duration;

/// Builds PostgreSQL connection pool for the application lifecycle.
///
/// # Parameters
/// - `database_url`: PostgreSQL connection string.
/// - `max_connections`: Maximum number of concurrent DB connections.
pub async fn create_pg_pool(database_url: &str, max_connections: u32) -> anyhow::Result<PgPool> {
    PgPoolOptions::new()
        .max_connections(max_connections)
        .acquire_timeout(Duration::from_secs(5))
        .connect(database_url)
        .await
        .context("failed to connect to postgres")
}

/// Executes SQLx migrations on startup.
pub async fn run_migrations(pool: &PgPool) -> anyhow::Result<()> {
    sqlx::migrate!("./migrations")
        .run(pool)
        .await
        .context("failed to run database migrations")
}
