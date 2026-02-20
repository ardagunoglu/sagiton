use crate::bootstrap::app_state::AppState;
use crate::shared::error::{AppError, AppResult};
use axum::http::{HeaderMap, header};
use uuid::Uuid;

/// Extracts authenticated user id from bearer token.
///
/// # Parameters
/// - `state`: Application state containing auth use-case.
/// - `headers`: HTTP headers expected to carry `Authorization: Bearer <token>`.
pub fn authenticated_user_id(state: &AppState, headers: &HeaderMap) -> AppResult<Uuid> {
    let token = extract_bearer_token(headers)?;
    state.auth_use_case().verify_access_token(token)
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
