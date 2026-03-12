use anyhow::Context;
use std::env;

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct AppConfig {
    pub port: u16,
    pub app_env: String,
    pub log_level: String,
    pub db_max_connections: u32,
    pub http_body_limit_bytes: usize,
    pub ws_max_message_bytes: usize,
    pub database_url: String,
    pub redis_url: String,
    pub jwt_secret: String,
    pub refresh_token_pepper: String,
    pub docs_enabled: bool,
}

impl AppConfig {
    pub fn from_env() -> anyhow::Result<Self> {
        dotenvy::dotenv().ok();

        let port = env::var("APP_PORT")
            .unwrap_or_else(|_| "8080".to_string())
            .parse::<u16>()
            .context("APP_PORT must be a valid u16")?;

        Ok(Self {
            app_env: env::var("APP_ENV").unwrap_or_else(|_| "development".to_string()),
            port,
            log_level: env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string()),
            db_max_connections: env::var("APP_DB_MAX_CONNECTIONS")
                .unwrap_or_else(|_| "20".to_string())
                .parse::<u32>()
                .context("APP_DB_MAX_CONNECTIONS must be a valid u32")?,
            http_body_limit_bytes: env::var("APP_HTTP_BODY_LIMIT_BYTES")
                .unwrap_or_else(|_| "1048576".to_string())
                .parse::<usize>()
                .context("APP_HTTP_BODY_LIMIT_BYTES must be a valid usize")?,
            ws_max_message_bytes: env::var("APP_WS_MAX_MESSAGE_BYTES")
                .unwrap_or_else(|_| "16384".to_string())
                .parse::<usize>()
                .context("APP_WS_MAX_MESSAGE_BYTES must be a valid usize")?,
            database_url: env::var("DATABASE_URL")
                .context("DATABASE_URL is required in environment")?,
            redis_url: env::var("REDIS_URL").context("REDIS_URL is required in environment")?,
            jwt_secret: env::var("JWT_SECRET").context("JWT_SECRET is required in environment")?,
            refresh_token_pepper: env::var("REFRESH_TOKEN_PEPPER")
                .unwrap_or_else(|_| env::var("JWT_SECRET").unwrap_or_default()),
            docs_enabled: env::var("APP_DOCS_ENABLED")
                .map(|raw| {
                    matches!(
                        raw.trim().to_ascii_lowercase().as_str(),
                        "1" | "true" | "yes"
                    )
                })
                .unwrap_or(false),
        })
    }
}
