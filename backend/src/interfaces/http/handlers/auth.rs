use crate::application::use_cases::auth::{
    LoginCommand, LogoutCommand, RefreshCommand, RegisterCommand,
};
use crate::bootstrap::app_state::AppState;
use crate::domain::auth::SessionContext;
use crate::interfaces::http::dto::auth::{
    LoginRequest, LogoutRequest, MessageResponse, RefreshRequest, RegisterRequest,
};
use crate::shared::error::{AppError, AppResult};
use axum::{
    Json,
    extract::State,
    http::{HeaderMap, header},
};

/// Registers user with username/email/password payload.
///
/// # Parameters
/// - `payload`: Registration input containing username, optional email and password.
pub async fn register(
    State(state): State<AppState>,
    Json(payload): Json<RegisterRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let user = state
        .auth_use_case()
        .register(RegisterCommand {
            username: payload.username,
            email: payload.email,
            password: payload.password,
        })
        .await?;

    Ok(Json(serde_json::json!({ "user": user })))
}

/// Creates access and refresh tokens for valid credentials.
///
/// # Parameters
/// - `payload`: Login input with username/email and password.
/// - `headers`: Request headers used to capture session context metadata.
pub async fn login(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<LoginRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let tokens = state
        .auth_use_case()
        .login(LoginCommand {
            username_or_email: payload.username_or_email,
            password: payload.password,
            context: build_session_context(&headers),
        })
        .await?;

    Ok(Json(serde_json::json!(tokens)))
}

/// Rotates refresh token and returns fresh token pair.
///
/// # Parameters
/// - `payload`: Refresh input containing previous refresh token.
/// - `headers`: Request headers used to capture session context metadata.
pub async fn refresh(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<RefreshRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let tokens = state
        .auth_use_case()
        .refresh(RefreshCommand {
            refresh_token: payload.refresh_token,
            context: build_session_context(&headers),
        })
        .await?;

    Ok(Json(serde_json::json!(tokens)))
}

/// Revokes session represented by provided refresh token.
pub async fn logout(
    State(state): State<AppState>,
    Json(payload): Json<LogoutRequest>,
) -> AppResult<Json<MessageResponse>> {
    state
        .auth_use_case()
        .logout(LogoutCommand {
            refresh_token: payload.refresh_token,
        })
        .await?;

    Ok(Json(MessageResponse {
        message: "logout successful",
    }))
}

/// Returns current user profile from access token.
pub async fn me(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> AppResult<Json<serde_json::Value>> {
    let token = extract_bearer_token(&headers)?;
    let user_id = state.auth_use_case().verify_access_token(token)?;
    let user = state.auth_use_case().me(user_id).await?;

    Ok(Json(serde_json::json!({ "user": user })))
}

fn build_session_context(headers: &HeaderMap) -> SessionContext {
    SessionContext {
        user_agent: read_header(headers, header::USER_AGENT),
        ip: read_header(headers, "x-forwarded-for"),
    }
}

fn extract_bearer_token(headers: &HeaderMap) -> AppResult<&str> {
    let header_value = headers
        .get(header::AUTHORIZATION)
        .ok_or_else(|| AppError::unauthorized("authorization header is required"))?
        .to_str()
        .map_err(|_| AppError::unauthorized("authorization header is invalid"))?;

    let token = header_value
        .strip_prefix("Bearer ")
        .ok_or_else(|| AppError::unauthorized("bearer token is required"))?;

    if token.trim().is_empty() {
        return Err(AppError::unauthorized("bearer token is required"));
    }

    Ok(token)
}

fn read_header(headers: &HeaderMap, key: impl axum::http::header::AsHeaderName) -> Option<String> {
    headers
        .get(key)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}
