use crate::bootstrap::app_state::AppState;
use axum::{Json, extract::State};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct HealthResponse {
    status: &'static str,
    service: &'static str,
    port: u16,
}

pub async fn health_check(State(state): State<AppState>) -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        service: "sagiton-backend",
        port: state.config().port,
    })
}
