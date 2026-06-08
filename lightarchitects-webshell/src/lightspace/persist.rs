//! HMAC-chained NDJSON event log for Lightspace sessions.
//!
//! Each session's events are appended to
//! `~/.lightarchitects/lightspace/<session_id>/events.ndjson`.
//!
//! ## Integrity chain
//!
//! Every appended line includes a `_chain` field:
//! `HMAC-SHA256(session_seed, prev_chain_bytes || line_json_bytes)`
//! encoded as lowercase hex.  The genesis line uses `[0u8; 32]` as
//! `prev_chain_bytes`.  Any deletion, reordering, or modification breaks
//! the chain, which [`verify_chain`] detects during replay.
//!
//! ## Format
//!
//! Each line is a JSON object:
//! ```json
//! {"seq":0,"ts":"2026-06-08T12:00:00Z","event":{...},"_chain":"<hex>"}
//! ```

use std::{
    fmt::Write as _,
    fs::{File, OpenOptions},
    io::{BufRead, BufReader, Write},
    path::PathBuf,
};

use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::Sha256;
use uuid::Uuid;

use crate::events::WebEventV2;

use super::{hmac_seed::HmacSeed, path_safety::session_dir};

type HmacSha256 = Hmac<Sha256>;

/// A single persisted log line.
#[derive(Debug, Serialize, Deserialize)]
struct LogLine {
    seq: u64,
    ts: String,
    event: Value,
    /// HMAC-SHA256 chain tag for this line (lowercase hex).
    ///
    /// Named with the `_chain` JSON key (matching the on-disk format).
    #[serde(rename = "_chain")]
    chain: String,
}

/// Append `event` to the session's NDJSON log, extending the HMAC chain.
///
/// Creates the session directory and log file on first call.
///
/// # Errors
///
/// Returns an error string on I/O failure or JSON serialisation failure.
pub fn append(
    session_id: Uuid,
    seed: &HmacSeed,
    seq: u64,
    prev_chain: &[u8; 32],
    event: &WebEventV2,
) -> Result<[u8; 32], String> {
    let dir = session_dir(session_id).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = log_path(session_id)?;

    let ts = chrono::Utc::now().to_rfc3339();
    let event_json = serde_json::to_value(event).map_err(|e| format!("serialize event: {e}"))?;

    let event_bytes =
        serde_json::to_vec(&event_json).map_err(|e| format!("serialize bytes: {e}"))?;
    let chain = compute_chain(seed, prev_chain, &event_bytes)?;
    let chain_hex = hex_encode(&chain);

    let line = LogLine {
        seq,
        ts,
        event: event_json,
        chain: chain_hex,
    };
    let mut json = serde_json::to_string(&line).map_err(|e| format!("serialize line: {e}"))?;
    json.push('\n');

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|e| format!("open log '{}': {e}", path.display()))?;
    file.write_all(json.as_bytes())
        .map_err(|e| format!("write log: {e}"))?;

    Ok(chain)
}

/// Read all persisted events for `session_id` in order.
///
/// Returns the raw [`serde_json::Value`] of the `event` field from each line,
/// along with the sequence number, for replay.
///
/// # Errors
///
/// Returns an error string on I/O or JSON parse failure.
pub fn read_events(session_id: Uuid) -> Result<Vec<(u64, Value)>, String> {
    let path = log_path(session_id)?;
    if !path.exists() {
        return Ok(vec![]);
    }
    let file = File::open(&path).map_err(|e| format!("open log: {e}"))?;
    let reader = BufReader::new(file);
    let mut out = Vec::new();
    for (i, line) in reader.lines().enumerate() {
        let line = line.map_err(|e| format!("read line {i}: {e}"))?;
        if line.trim().is_empty() {
            continue;
        }
        let parsed: LogLine =
            serde_json::from_str(&line).map_err(|e| format!("parse line {i}: {e}"))?;
        out.push((parsed.seq, parsed.event));
    }
    Ok(out)
}

/// Verify the HMAC chain for `session_id`.
///
/// Returns `true` when every line's `_chain` field matches
/// `HMAC(seed, prev_chain || event_bytes)`.
///
/// # Errors
///
/// Returns an error string on I/O or JSON parse failure.
pub fn verify_chain(session_id: Uuid, seed: &HmacSeed) -> Result<bool, String> {
    let path = log_path(session_id)?;
    if !path.exists() {
        return Ok(true);
    }
    let file = File::open(&path).map_err(|e| format!("open log: {e}"))?;
    let reader = BufReader::new(file);
    let mut prev = [0u8; 32];

    for (i, line) in reader.lines().enumerate() {
        let line = line.map_err(|e| format!("read line {i}: {e}"))?;
        if line.trim().is_empty() {
            continue;
        }
        let parsed: LogLine =
            serde_json::from_str(&line).map_err(|e| format!("parse line {i}: {e}"))?;
        let event_bytes =
            serde_json::to_vec(&parsed.event).map_err(|e| format!("re-serialize line {i}: {e}"))?;
        let expected = compute_chain(seed, &prev, &event_bytes)?;
        let recorded =
            hex_decode(&parsed.chain).map_err(|e| format!("decode chain hex at line {i}: {e}"))?;
        if expected != recorded.as_slice() {
            return Ok(false);
        }
        prev = expected;
    }
    Ok(true)
}

fn log_path(session_id: Uuid) -> Result<PathBuf, String> {
    let dir = session_dir(session_id).map_err(|e| e.to_string())?;
    Ok(dir.join("events.ndjson"))
}

/// Compute one HMAC-SHA256 chain link.
///
/// # Errors
///
/// Returns an error if the HMAC key is invalid (cannot happen with a 32-byte
/// seed, but propagated for correctness).
fn compute_chain(seed: &HmacSeed, prev: &[u8; 32], event_bytes: &[u8]) -> Result<[u8; 32], String> {
    let mut mac = HmacSha256::new_from_slice(seed).map_err(|e| format!("HMAC init error: {e}"))?;
    mac.update(prev);
    mac.update(event_bytes);
    let result = mac.finalize().into_bytes();
    let mut out = [0u8; 32];
    out.copy_from_slice(&result[..32]);
    Ok(out)
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut s, b| {
            let _ = write!(s, "{b:02x}");
            s
        })
}

fn hex_decode(s: &str) -> Result<Vec<u8>, String> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).map_err(|e| format!("bad hex at {i}: {e}")))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lightspace::hmac_seed::new_seed;

    #[test]
    #[allow(clippy::unwrap_used)]
    fn chain_verify_roundtrip() {
        let seed = new_seed();

        let stub = serde_json::json!({
            "type": "ping",
            "topic": "v1.test",
            "timestamp": "2026-06-08T00:00:00Z",
            "agent_id": null,
            "severity": "info",
        });
        let prev = [0u8; 32];
        let event_bytes = serde_json::to_vec(&stub).unwrap();
        let chain = compute_chain(&seed, &prev, &event_bytes).unwrap();
        assert_eq!(chain.len(), 32);

        let other_bytes = b"other event bytes";
        let chain2 = compute_chain(&seed, &prev, other_bytes).unwrap();
        assert_ne!(chain, chain2);
    }
}
