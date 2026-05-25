//! Integration tests for `SsmState` + `HelixSessionMemory` Phase 5 Wave C.
//!
//! Validates:
//! - (a) SSM state persists across `HelixSessionMemory::open()`
//! - (b) State file is binary format (not text)
//! - (c) `parse_turns` and `ssm_state` are consistent (no drift)
//! - (d) `clear()` resets both turns and SSM state to zero

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::missing_errors_doc)]

use lightarchitects::agent::conversation::{
    HelixSessionMemory,
    helix_memory::SsmState,
    memory::{ConversationMemory, MessageRole},
};

/// (a) SSM turn count persists across `HelixSessionMemory::open()`.
#[test]
fn ssm_state_persists_across_open() {
    let dir = tempfile::TempDir::new().unwrap();
    let cwd = dir.path().to_path_buf();

    {
        let mut mem = HelixSessionMemory::open(&cwd, 20);
        assert_eq!(mem.ssm_turn_count(), 0, "fresh session must start at 0");
        mem.push(MessageRole::User, "first message".to_owned());
        mem.push(MessageRole::Assistant, "first reply".to_owned());
        assert_eq!(mem.ssm_turn_count(), 2);
    }

    // Re-open the same session directory.
    let mem2 = HelixSessionMemory::open(&cwd, 20);
    assert_eq!(
        mem2.ssm_turn_count(),
        2,
        "SSM turn_count must persist across open()"
    );
}

/// (b) The `.ssm` sidecar is binary (magic header present, fixed byte length).
///
/// Uses `SsmState::save` directly — `HelixSessionMemory` writes to the global
/// sessions directory, not to `cwd`, so the format test targets the storage API.
#[test]
fn ssm_state_file_is_binary_not_text() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("test.ssm");

    let mut s = SsmState::new();
    s.update(&vec![0.5f32; 384]);
    s.save(&path).expect("save must succeed");

    let bytes = std::fs::read(&path).unwrap();

    // Magic bytes at offset 0.
    assert_eq!(&bytes[0..4], b"SSM\0", "magic header mismatch");
    // Version byte.
    assert_eq!(bytes[4], 1, "version must be 1");
    // Exact binary size: magic(4) + version(1) + dim(2) + turn_count(8) + h(256) + checksum(4) = 275
    assert_eq!(bytes.len(), 275, "SSM state file must be exactly 275 bytes");
    // File must NOT be valid UTF-8 in the h-vector region (raw f32 bytes).
    // We just confirm the magic is binary (null byte) rather than text.
    assert!(bytes.contains(&0u8), "binary file must contain null bytes");
}

/// (c) Number of turns in the turns file equals SSM `turn_count` after equal pushes.
#[test]
fn ssm_state_and_turns_consistent() {
    let dir = tempfile::TempDir::new().unwrap();
    let cwd = dir.path().to_path_buf();

    let mut mem = HelixSessionMemory::open(&cwd, 20);
    mem.push(MessageRole::User, "msg one".to_owned());
    mem.push(MessageRole::Assistant, "reply one".to_owned());
    mem.push(MessageRole::User, "msg two".to_owned());

    assert_eq!(mem.turns().len(), 3, "turns count must match push count");
    assert_eq!(
        mem.ssm_turn_count(),
        3,
        "ssm turn_count must match push count"
    );
}

/// (d) `clear()` resets both turns and SSM state to zero.
#[test]
fn clear_drops_both_turns_and_ssm() {
    let dir = tempfile::TempDir::new().unwrap();
    let cwd = dir.path().to_path_buf();

    let mut mem = HelixSessionMemory::open(&cwd, 20);
    mem.push(MessageRole::User, "before clear".to_owned());
    assert_eq!(mem.ssm_turn_count(), 1);
    assert_eq!(mem.turns().len(), 1);

    mem.clear();
    assert_eq!(
        mem.ssm_turn_count(),
        0,
        "clear() must reset SSM turn_count to 0"
    );
    assert!(mem.turns().is_empty(), "clear() must empty the turns list");
}

/// Round-trip: `SsmState::save` then `SsmState::load` preserves h exactly.
#[test]
fn ssm_roundtrip_h_vector_exact() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("rt.ssm");

    // Build a state with known input so h is deterministic.
    let mut s = SsmState::new();
    let x = vec![0.42f32; 384];
    s.update(&x);
    s.update(&x);
    s.save(&path).unwrap();

    let s2 = SsmState::try_load(&path).expect("load must succeed after save");
    assert_eq!(s2.turn_count, 2);
    for (i, (a, b)) in s.h.iter().zip(s2.h.iter()).enumerate() {
        assert!(
            (a - b).abs() < 1e-6,
            "h[{i}] mismatch after round-trip: {a} vs {b}"
        );
    }
}

/// Corrupted checksum causes `try_load` to return `None`.
#[test]
fn ssm_corrupted_checksum_returns_none() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("corrupt.ssm");

    let mut s = SsmState::new();
    let x = vec![0.1f32; 384];
    s.update(&x);
    s.save(&path).unwrap();

    // Flip a byte in the h region.
    let mut bytes = std::fs::read(&path).unwrap();
    bytes[20] ^= 0xFF;
    std::fs::write(&path, &bytes).unwrap();

    assert!(
        SsmState::try_load(&path).is_none(),
        "corrupted file must not load"
    );
}

/// Decay property: repeated identical updates must converge (h remains finite and bounded).
#[test]
fn ssm_decay_convergence_property() {
    let mut s = SsmState::new();
    let x = vec![1.0f32; 384];
    for _ in 0..200 {
        s.update(&x);
    }
    for (i, &v) in s.h.iter().enumerate() {
        assert!(v.is_finite(), "h[{i}] must be finite after 200 updates");
        // With A=0.9, geometric series converges to B·x / (1 - 0.9) = 10 × B·x
        assert!(
            v.abs() < 2000.0,
            "h[{i}] must be bounded after convergence; got {v}"
        );
    }
}
