use crate::bootstrap::app_state::AppState;
use crate::interfaces::http::handlers::auth::{login, logout, me, refresh, register};
use crate::interfaces::http::handlers::health::health_check;
use axum::{
    Router,
    routing::{get, post},
};

pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health_check))
        .route("/api/auth/register", post(register))
        .route("/api/auth/login", post(login))
        .route("/api/auth/refresh", post(refresh))
        .route("/api/auth/logout", post(logout))
        .route("/api/auth/me", get(me))
        .with_state(state)
}
