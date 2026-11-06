use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use sha2::{Digest, Sha256};

use crate::platform::{Platform, Result};
use crate::text::to_hex;

/// PBKDF2 iterations for link passwords. Workers cap PBKDF2 at 100000
/// iterations; half keeps unlocks well inside the free plan CPU budget. These
/// guard silly short links, not bank vaults.
pub const PASSWORD_ITERATIONS: u32 = 50_000;

const SALT_BYTES: usize = 16;

pub fn random_token<P: Platform>(platform: &P, byte_length: usize) -> Result<String> {
    Ok(URL_SAFE_NO_PAD.encode(platform.random_bytes(byte_length)?))
}

pub fn sha256_hex(text: &str) -> String {
    to_hex(&Sha256::digest(text.as_bytes()))
}

pub async fn hash_password<P: Platform>(platform: &P, password: &str) -> Result<String> {
    let salt = platform.random_bytes(SALT_BYTES)?;
    let hash = platform
        .pbkdf2(password, &salt, PASSWORD_ITERATIONS)
        .await?;
    Ok(format!(
        "{}.{}",
        URL_SAFE_NO_PAD.encode(salt),
        URL_SAFE_NO_PAD.encode(hash)
    ))
}

/// Compares without stopping at the first difference.
pub fn timing_safe_equal(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.bytes()
        .zip(b.bytes())
        .fold(0u8, |difference, (x, y)| difference | (x ^ y))
        == 0
}

pub async fn verify_password<P: Platform>(
    platform: &P,
    password: &str,
    stored: &str,
) -> Result<bool> {
    let Some((salt, expected)) = stored.split_once('.') else {
        return Ok(false);
    };
    if salt.is_empty() || expected.is_empty() {
        return Ok(false);
    }
    let Ok(salt) = URL_SAFE_NO_PAD.decode(salt) else {
        return Ok(false);
    };
    let hash = platform
        .pbkdf2(password, &salt, PASSWORD_ITERATIONS)
        .await?;
    Ok(timing_safe_equal(&URL_SAFE_NO_PAD.encode(hash), expected))
}
