pub mod middleware;

use hmac::{Hmac, Mac};
use sha2::Sha256;
use rand::Rng;
use chrono::Utc;
use serde::{Deserialize, Serialize};

type HmacSha256 = Hmac<Sha256>;

/// A generated API key pair
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKeyPair {
    pub user_id: String,
    pub api_key: String,
    pub secret_key: String,
    pub created_at: String,
}

/// Generate a random hex string
fn random_hex(len: usize) -> String {
    let bytes: Vec<u8> = (0..len).map(|_| rand::thread_rng().gen()).collect();
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

/// Generate a new API key pair for a user
pub fn generate_api_key(user_id: &str) -> ApiKeyPair {
    ApiKeyPair {
        user_id: user_id.to_string(),
        api_key: format!("trd_{}", random_hex(16)),
        secret_key: random_hex(32),
        created_at: Utc::now().to_rfc3339(),
    }
}

/// Sign a message with the given secret key
pub fn sign_message(secret_key: &str, message: &str) -> String {
    let mut mac = HmacSha256::new_from_slice(secret_key.as_bytes())
        .expect("HMAC key");
    mac.update(message.as_bytes());
    let result = mac.finalize();
    let code_bytes = result.into_bytes();
    hex::encode(code_bytes)
}

/// Verify a signed message against the expected signature
pub fn verify_signature(secret_key: &str, message: &str, signature: &str) -> bool {
    let expected = sign_message(secret_key, message);
    // Constant-time comparison
    expected.len() == signature.len() && {
        let e = expected.as_bytes();
        let s = signature.as_bytes();
        let mut result = 0u8;
        for i in 0..e.len() {
            result |= e[i] ^ s[i];
        }
        result == 0
    }
}

/// Generate a nonce for replay protection
pub fn generate_nonce() -> String {
    Utc::now().timestamp_millis().to_string()
}

/// Validate that a timestamp is within the allowed window (5 minutes)
pub fn validate_timestamp(timestamp_ms: i64) -> bool {
    let now = Utc::now().timestamp_millis();
    let diff = (now - timestamp_ms).abs();
    diff < 300_000 // 5 minutes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_api_key() {
        let pair = generate_api_key("alice");
        assert!(pair.api_key.starts_with("trd_"));
        assert_eq!(pair.user_id, "alice");
        assert_eq!(pair.secret_key.len(), 64); // 32 bytes hex encoded = 64 chars
    }

    #[test]
    fn test_sign_and_verify() {
        let pair = generate_api_key("bob");
        let message = format!("GET/api/v1/balances/bob{}", generate_nonce());
        let sig = sign_message(&pair.secret_key, &message);
        assert!(verify_signature(&pair.secret_key, &message, &sig));
        assert!(!verify_signature(&pair.secret_key, &message, &(sig[..sig.len()-1].to_string() + "0")));
    }

    #[test]
    fn test_timestamp_validation() {
        assert!(validate_timestamp(Utc::now().timestamp_millis()));
        // 1 hour ago should fail
        let old = Utc::now().timestamp_millis() - 3_600_000;
        assert!(!validate_timestamp(old));
    }
}
