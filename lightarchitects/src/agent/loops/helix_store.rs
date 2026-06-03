//! `HelixMemoryStore` — adapter that wraps `HelixSessionMemory` to satisfy
//! the [`MemoryStore`] trait used by `ReactWithMemoryStrategy`.
//!
//! LTM read returns recent conversation turns (no Neo4j dependency at this
//! layer — semantic HNSW recall is deferred until `HelixDb` is wired through).
//! LTM write persists via `HelixSessionMemory::push()` which appends to today's
//! session file synchronously.
//!
//! Per SCRUM 2026-06-02:
//! - SOUL HIGH C2: secret redaction is delegated to `HelixSessionMemory`
//!   `redact_secrets` (5 baseline patterns). Tool outputs are pre-shielded by
//!   `StrategyToolExecutor`, but operators MUST extend redaction patterns
//!   before treating helix sessions as a knowledge-graph promotion source.
//! - SOUL HIGH C1: SSM fan-out — this adapter does NOT coalesce STM writes;
//!   callers should batch observations into a single `write_stm` at iteration
//!   close, not push per-token. The `ReactWithMemoryStrategy` already writes
//!   one summary per `ReAct` step.

#![cfg(feature = "agent-cli")]

use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::Mutex;

use crate::agent::conversation::memory::ConversationMemory as _;
use crate::agent::conversation::{HelixSessionMemory, MessageRole};
use crate::agent::loops::error::LoopError;
use crate::agent::loops::react_with_memory::MemoryStore;

/// Adapter wrapping `HelixSessionMemory` for `MemoryStore` consumption.
///
/// Thread-safe via `Arc<Mutex<...>>`. Construct one per build session.
#[derive(Clone)]
pub struct HelixMemoryStore {
    inner: Arc<Mutex<HelixSessionMemory>>,
}

impl HelixMemoryStore {
    /// Wrap an existing `HelixSessionMemory`.
    #[must_use]
    pub fn new(memory: HelixSessionMemory) -> Self {
        Self {
            inner: Arc::new(Mutex::new(memory)),
        }
    }

    /// Wrap an existing shared memory cell (for cases where the webshell
    /// already holds an `Arc<Mutex<HelixSessionMemory>>`).
    #[must_use]
    pub fn from_shared(inner: Arc<Mutex<HelixSessionMemory>>) -> Self {
        Self { inner }
    }

    /// Number of turns currently loaded in memory.
    pub async fn len(&self) -> usize {
        self.inner.lock().await.turns().len()
    }

    /// Returns `true` if no turns are loaded.
    pub async fn is_empty(&self) -> bool {
        self.len().await == 0
    }
}

#[async_trait]
impl MemoryStore for HelixMemoryStore {
    /// Return the most recent `limit` turns as LTM context.
    ///
    /// The `query` parameter is currently unused — semantic recall via SSM +
    /// HNSW (`HelixSessionMemory::session_context_block`) requires a `HelixDb`
    /// handle which is not threaded into this layer. Future enhancement.
    async fn read_ltm(&self, _query: &str, limit: usize) -> Result<Vec<String>, LoopError> {
        let mem = self.inner.lock().await;
        let turns = mem.turns();
        if turns.is_empty() || limit == 0 {
            return Ok(Vec::new());
        }
        let take = limit.min(turns.len());
        let start = turns.len() - take;
        Ok(turns[start..]
            .iter()
            .map(|t| {
                let role = match t.role {
                    MessageRole::User => "user",
                    MessageRole::Assistant => "agent",
                    MessageRole::System => "system",
                };
                format!("[{role}] {}", t.content)
            })
            .collect())
    }

    /// Append an agent observation to the session file.
    ///
    /// Routes through `HelixSessionMemory::push` which applies baseline secret
    /// redaction + SSM state update + synchronous disk append.
    async fn write_stm(&self, content: String) -> Result<(), LoopError> {
        let mut mem = self.inner.lock().await;
        mem.push(MessageRole::Assistant, content);
        Ok(())
    }

    /// No-op: `HelixSessionMemory.push()` persists every write synchronously,
    /// so the STM ring buffer is already on disk. Nothing to flush.
    async fn persist_stm_to_ltm(&self, _entries: Vec<String>) -> Result<(), LoopError> {
        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn temp_cwd() -> PathBuf {
        let mut p = std::env::temp_dir();
        let suffix = format!(
            "helix-store-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        p.push(suffix);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    #[tokio::test]
    async fn write_then_read_round_trip() {
        let cwd = temp_cwd();
        let mem = HelixSessionMemory::open(&cwd, 10);
        let store = HelixMemoryStore::new(mem);
        store
            .write_stm("agent observed a leak".to_owned())
            .await
            .unwrap();
        store
            .write_stm("agent observed a fix landed".to_owned())
            .await
            .unwrap();
        let ltm = store.read_ltm("leak", 5).await.unwrap();
        assert_eq!(ltm.len(), 2);
        assert!(ltm.iter().any(|s| s.contains("leak")));
        assert!(ltm.iter().any(|s| s.contains("fix landed")));
    }

    #[tokio::test]
    async fn read_empty_returns_empty_vec() {
        let cwd = temp_cwd();
        let mem = HelixSessionMemory::open(&cwd, 10);
        let store = HelixMemoryStore::new(mem);
        let ltm = store.read_ltm("anything", 5).await.unwrap();
        assert!(ltm.is_empty());
    }

    #[tokio::test]
    async fn read_limit_caps_returned_entries() {
        let cwd = temp_cwd();
        let mem = HelixSessionMemory::open(&cwd, 100);
        let store = HelixMemoryStore::new(mem);
        for i in 0..10 {
            store.write_stm(format!("turn {i}")).await.unwrap();
        }
        let ltm = store.read_ltm("turn", 3).await.unwrap();
        assert_eq!(ltm.len(), 3);
        // Should be the most-recent 3.
        assert!(ltm.iter().any(|s| s.contains("turn 9")));
        assert!(ltm.iter().any(|s| s.contains("turn 8")));
        assert!(ltm.iter().any(|s| s.contains("turn 7")));
    }

    #[tokio::test]
    async fn persist_stm_to_ltm_is_noop() {
        let cwd = temp_cwd();
        let mem = HelixSessionMemory::open(&cwd, 10);
        let store = HelixMemoryStore::new(mem);
        store
            .persist_stm_to_ltm(vec!["entry".to_owned()])
            .await
            .unwrap();
    }
}
