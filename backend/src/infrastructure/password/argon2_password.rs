use crate::application::ports::auth::PasswordPort;
use crate::shared::error::{AppError, AppResult};
use argon2::Argon2;
use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};

pub struct Argon2PasswordAdapter {
    argon2: Argon2<'static>,
}

impl Argon2PasswordAdapter {
    /// Creates Argon2id password adapter with default safe profile.
    pub fn new() -> Self {
        Self {
            argon2: Argon2::default(),
        }
    }
}

impl PasswordPort for Argon2PasswordAdapter {
    fn hash_password(&self, password: &str) -> AppResult<String> {
        let salt = SaltString::generate(&mut OsRng);
        self.argon2
            .hash_password(password.as_bytes(), &salt)
            .map(|hash| hash.to_string())
            .map_err(|e| AppError::internal(format!("failed to hash password: {e}")))
    }

    fn verify_password(&self, password: &str, password_hash: &str) -> AppResult<bool> {
        let parsed_hash = match PasswordHash::new(password_hash) {
            Ok(value) => value,
            Err(_) => return Ok(false),
        };

        Ok(self
            .argon2
            .verify_password(password.as_bytes(), &parsed_hash)
            .is_ok())
    }
}
