use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const TOKEN_MAX_AGE_MS: i64 = 3_600_000; // 1 hour

fn signing_key() -> &'static str {
    static KEY: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    KEY.get_or_init(|| {
        std::env::var("CLICK_SIGNING_KEY")
            .unwrap_or_else(|_| "dev-only-click-signing-key".to_string())
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClickPayload {
    pub query: String,
    pub position: usize,
    pub target_url: String,
    pub timestamp_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SignedPayload {
    payload: ClickPayload,
    sig: String,
}

pub fn encode_click_payload(query: &str, position: usize, target_url: &str) -> String {
    let payload = ClickPayload {
        query: query.to_string(),
        position,
        target_url: target_url.to_string(),
        timestamp_ms: now_ms(),
    };

    let sig = sign_payload(&payload);
    let signed = SignedPayload { payload, sig };
    let json = serde_json::to_vec(&signed).unwrap_or_default();
    URL_SAFE_NO_PAD.encode(json)
}

pub fn decode_click_payload(token: &str) -> Option<ClickPayload> {
    let bytes = URL_SAFE_NO_PAD.decode(token).ok()?;
    let signed: SignedPayload = serde_json::from_slice(&bytes).ok()?;

    if sign_payload(&signed.payload) != signed.sig {
        return None;
    }

    // Reject expired tokens
    let age = now_ms() - signed.payload.timestamp_ms;
    if age > TOKEN_MAX_AGE_MS || age < -60_000 {
        return None;
    }

    Some(signed.payload)
}

/// Keyed hash using length-prefixed construction to prevent extension attacks:
/// SHA256(len(key) || key || canonical_data)
fn sign_payload(payload: &ClickPayload) -> String {
    let key = signing_key();
    let canonical = format!(
        "{}|{}|{}|{}",
        payload.query, payload.position, payload.target_url, payload.timestamp_ms
    );
    let mut hasher = Sha256::new();
    hasher.update((key.len() as u64).to_le_bytes());
    hasher.update(key.as_bytes());
    hasher.update(canonical.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn now_ms() -> i64 {
    let now = std::time::SystemTime::now();
    now.duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or_default()
}
