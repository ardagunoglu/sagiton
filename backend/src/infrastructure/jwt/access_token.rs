use crate::application::ports::auth::JwtPort;
use crate::shared::error::{AppError, AppResult};
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use uuid::Uuid;

const ACCESS_TOKEN_TTL_SECONDS: u64 = 15 * 60;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AccessTokenClaims {
    sub: String,
    exp: u64,
    iat: u64,
    jti: String,
}

pub struct JwtAdapter {
    encoding_key: EncodingKey,
    decoding_key: DecodingKey,
    validation: Validation,
}

impl JwtAdapter {
    /// Builds HS256 JWT adapter from configured secret.
    pub fn new(secret: &str) -> Self {
        let mut validation = Validation::new(jsonwebtoken::Algorithm::HS256);
        validation.validate_exp = true;

        Self {
            encoding_key: EncodingKey::from_secret(secret.as_bytes()),
            decoding_key: DecodingKey::from_secret(secret.as_bytes()),
            validation,
        }
    }
}

impl JwtPort for JwtAdapter {
    fn issue_access_token(&self, user_id: Uuid) -> AppResult<String> {
        let now = unix_now()?;
        let claims = AccessTokenClaims {
            sub: user_id.to_string(),
            exp: now + ACCESS_TOKEN_TTL_SECONDS,
            iat: now,
            jti: Uuid::new_v4().to_string(),
        };

        encode(&Header::default(), &claims, &self.encoding_key)
            .map_err(|e| AppError::internal(format!("failed to sign jwt: {e}")))
    }

    fn verify_access_token(&self, token: &str) -> AppResult<Uuid> {
        let data = decode::<AccessTokenClaims>(token, &self.decoding_key, &self.validation)
            .map_err(|_| AppError::unauthorized("invalid access token"))?;

        Uuid::parse_str(&data.claims.sub)
            .map_err(|_| AppError::unauthorized("invalid access token subject"))
    }
}

fn unix_now() -> AppResult<u64> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| AppError::internal(format!("system clock error: {e}")))?
        .as_secs())
}

#[allow(dead_code)]
fn _duration_from_seconds(seconds: u64) -> Duration {
    Duration::from_secs(seconds)
}
