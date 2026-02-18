use anyhow::Context;
use backend::bootstrap::{app_state::AppState, config::AppConfig};
use backend::interfaces::http::router::build_router;
use std::net::SocketAddr;
use tokio::net::TcpListener;
use tracing::info;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = AppConfig::from_env()?;
    backend::bootstrap::telemetry::init(&config.log_level);

    let state = AppState::new(config.clone())
        .await
        .context("failed to initialize app state")?;
    let app = build_router(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], config.port));
    let listener = TcpListener::bind(addr)
        .await
        .with_context(|| format!("failed to bind tcp listener at {addr}"))?;

    info!(%addr, "backend listening");
    axum::serve(listener, app)
        .await
        .context("axum server terminated unexpectedly")?;

    Ok(())
}
