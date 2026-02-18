use crate::application::ports::auth::RefreshTokenPort;
use hmac::{Hmac, Mac};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

pub struct RefreshTokenAdapter {
    pepper: String,
}

impl RefreshTokenAdapter {
    /// Creates refresh token adapter for random token generation.
    ///
    /// # Parameters
    /// - `pepper`: Server-side secret used in refresh token hash derivation.
    pub fn new(pepper: &str) -> Self {
        Self {
            pepper: pepper.to_string(),
        }
    }
}

impl RefreshTokenPort for RefreshTokenAdapter {
    fn generate_refresh_token(&self) -> String {
        let bytes: [u8; 48] = rand::random();
        to_hex(&bytes)
    }

    fn hash_refresh_token(&self, token: &str) -> String {
        let mut mac = HmacSha256::new_from_slice(self.pepper.as_bytes())
            .expect("pepper key must be a valid HMAC key");
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
