use crate::application::ports::auth::RefreshTokenPort;
use anyhow::{Result, bail};
use hmac::{Hmac, Mac};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;
const MIN_PEPPER_LENGTH: usize = 16;

pub struct RefreshTokenAdapter {
    pepper: String,
}

impl RefreshTokenAdapter {
    /// Creates refresh token adapter for random token generation.
    ///
    /// # Parameters
    /// - `pepper`: Server-side secret used in refresh token hash derivation.
    pub fn new(pepper: &str) -> Result<Self> {
        if pepper.trim().len() < MIN_PEPPER_LENGTH {
            bail!("REFRESH_TOKEN_PEPPER must be at least {MIN_PEPPER_LENGTH} characters long");
        }

        Ok(Self {
            pepper: pepper.to_string(),
        })
    }
}

impl RefreshTokenPort for RefreshTokenAdapter {
    fn generate_refresh_token(&self) -> String {
        let bytes: [u8; 48] = rand::random();
        to_hex(&bytes)
    }

    fn hash_refresh_token(&self, token: &str) -> String {
        let Ok(mut mac) = HmacSha256::new_from_slice(self.pepper.as_bytes()) else {
            return String::new();
        };
        mac.update(token.as_bytes());
        let digest = mac.finalize().into_bytes();
        to_hex(&digest)
    }
}

fn to_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        output.push(HEX[(b >> 4) as usize] as char);
        output.push(HEX[(b & 0x0f) as usize] as char);
    }
    output
}
