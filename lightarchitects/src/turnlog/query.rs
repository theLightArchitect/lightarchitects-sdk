//! Filtered read API for turnlog sessions.
//!
//! [`Query`] builds a declarative filter over [`TurnEntry`] records. Callers
//! construct a query with zero or more predicates, then call [`Query::run`]
//! to execute it against a [`TurnLogReader`].
//!
//! # Example
//!
//! ```no_run
//! use lightarchitects::turnlog::{TurnLogReader, StoreLayout, query::Query};
//! use lightarchitects::turnlog::entry::EntryKind;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let reader = TurnLogReader::new(StoreLayout::new("/tmp/turnlog".into()));
//!
//! // All tool results from seq 10..=50 in session "abc-123".
//! let results = Query::new("abc-123")
//!     .seq_range(10..=50)
//!     .kind(EntryKind::ToolResult)
//!     .run(&reader)
//!     .await?;
//!
//! for entry in results {
//!     println!("seq={} action={}", entry.seq, entry.span.action);
//! }
//! # Ok(()) }
//! ```

use std::ops::RangeInclusive;

use crate::turnlog::entry::EntryKind;
use crate::turnlog::entry::TurnEntry;
use crate::turnlog::error::Result;
use crate::turnlog::reader::TurnLogReader;

/// Declarative filter over [`TurnEntry`] records within a single session.
///
/// Predicates are AND-combined: an entry must match *all* specified filters
/// to be included in the result set. Unset predicates are no-ops (match
/// everything).
#[derive(Debug, Clone)]
pub struct Query {
    /// Session ID to query (required).
    session_id: String,
    /// Optional seq range filter.
    seq_range: Option<RangeInclusive<u64>>,
    /// Optional entry kind filter.
    kind: Option<EntryKind>,
    /// Optional parent_seq filter.
    parent_seq: Option<u64>,
    /// Optional actor filter (substring match on `span.actor.id`).
    actor: Option<String>,
    /// Optional action prefix filter (starts-with match on `span.action`).
    action_prefix: Option<String>,
    /// Invert the filter (NOT all predicates).
    negate: bool,
}

impl Query {
    /// Create a new query targeting the given session.
    #[must_use]
    pub fn new(session_id: impl Into<String>) -> Self {
        Self {
            session_id: session_id.into(),
            seq_range: None,
            kind: None,
            parent_seq: None,
            actor: None,
            action_prefix: None,
            negate: false,
        }
    }

    /// Restrict results to entries whose `seq` falls within the given range.
    #[must_use]
    pub fn seq_range(mut self, range: RangeInclusive<u64>) -> Self {
        self.seq_range = Some(range);
        self
    }

    /// Restrict results to entries matching the given [`EntryKind`].
    #[must_use]
    pub fn kind(mut self, kind: EntryKind) -> Self {
        self.kind = Some(kind);
        self
    }

    /// Restrict results to entries whose `parent_seq` equals the given value.
    #[must_use]
    pub fn parent_seq(mut self, seq: u64) -> Self {
        self.parent_seq = Some(seq);
        self
    }

    /// Restrict results to entries whose `span.actor.id` contains the given
    /// substring (case-sensitive).
    #[must_use]
    pub fn actor(mut self, actor: impl Into<String>) -> Self {
        self.actor = Some(actor.into());
        self
    }

    /// Restrict results to entries whose `span.action` starts with the given
    /// prefix (case-sensitive).
    #[must_use]
    pub fn action_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.action_prefix = Some(prefix.into());
        self
    }

    /// Negate the entire filter: return entries that do **not** match all
    /// predicates.
    #[must_use]
    pub fn negate(mut self) -> Self {
        self.negate = true;
        self
    }

    /// Execute the query against the given reader.
    ///
    /// Reads all entries for the session, then applies the filter predicates
    /// in memory. For large sessions, consider using [`Self::run_with_limit`]
    /// to cap result size.
    ///
    /// # Errors
    ///
    /// Returns [`lightarchitects::turnlog::error::TurnLogError`] if the session file cannot be
    /// read or parsed.
    pub async fn run(&self, reader: &TurnLogReader) -> Result<Vec<TurnEntry>> {
        let entries = reader.read_all(&self.session_id).await?;
        Ok(self.filter(entries))
    }

    /// Execute the query, returning at most `limit` matching entries.
    ///
    /// Stops scanning once `limit` matches have been collected — useful for
    /// "find the first N security events" patterns.
    ///
    /// # Errors
    ///
    /// Same as [`Self::run`].
    pub async fn run_with_limit(
        &self,
        reader: &TurnLogReader,
        limit: usize,
    ) -> Result<Vec<TurnEntry>> {
        let entries = reader.read_all(&self.session_id).await?;
        if self.negate {
            // Cannot early-exit on negated queries — must scan all entries.
            let filtered = self.filter(entries);
            Ok(filtered.into_iter().take(limit).collect())
        } else {
            let mut results = Vec::with_capacity(limit.min(entries.len()));
            for entry in entries {
                if self.matches(&entry) {
                    results.push(entry);
                    if results.len() >= limit {
                        return Ok(results);
                    }
                }
            }
            Ok(results)
        }
    }

    /// Apply all predicates to a pre-loaded entry slice.
    ///
    /// Useful when entries are already loaded (e.g. from `read_all`) and you
    /// want to apply query filters without re-reading the session file.
    /// The `session_id` field on the `Query` is ignored for filtering — only
    /// the predicates matter.
    #[must_use]
    pub fn filter(&self, entries: Vec<TurnEntry>) -> Vec<TurnEntry> {
        entries
            .into_iter()
            .filter(|e| {
                let matches = self.matches(e);
                if self.negate { !matches } else { matches }
            })
            .collect()
    }

    /// Test whether a single entry matches all predicates.
    fn matches(&self, entry: &TurnEntry) -> bool {
        if let Some(ref range) = self.seq_range {
            if !range.contains(&entry.seq) {
                return false;
            }
        }
        if let Some(ref kind) = self.kind {
            if entry.kind() != *kind {
                return false;
            }
        }
        if let Some(parent) = self.parent_seq {
            if entry.parent_seq != Some(parent) {
                return false;
            }
        }
        if let Some(ref actor) = self.actor {
            if !entry.span.actor.name().contains(actor) {
                return false;
            }
        }
        if let Some(ref prefix) = self.action_prefix {
            if !entry.span.action.starts_with(prefix) {
                return false;
            }
        }
        true
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::turnlog::entry::TurnEntry;
    use ayin::span::{Actor, TraceContext, TraceOutcome};

    fn make_entry(seq: u64, action: &str, parent_seq: Option<u64>) -> TurnEntry {
        let span = TraceContext::new(Actor::claude(), action)
            .outcome(TraceOutcome::Continue)
            .finish()
            .expect("span construction must succeed in tests");
        TurnEntry {
            seq,
            parent_seq,
            span,
            hmac_prev: String::new(),
            hmac_self: String::new(),
        }
    }

    fn make_entry_with_actor(seq: u64, action: &str, actor_id: &str) -> TurnEntry {
        let span = TraceContext::new(Actor::new(actor_id), action)
            .outcome(TraceOutcome::Continue)
            .finish()
            .expect("span construction must succeed in tests");
        TurnEntry {
            seq,
            parent_seq: None,
            span,
            hmac_prev: String::new(),
            hmac_self: String::new(),
        }
    }

    #[test]
    fn query_kind_filter() {
        let entries = vec![
            make_entry(0, "turn.user", None),
            make_entry(1, "tool_result", Some(1)),
            make_entry(2, "turn.assistant", None),
        ];
        let q = Query::new("test").kind(EntryKind::ToolResult);
        let results = q.filter(entries);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].seq, 1);
    }

    #[test]
    fn query_seq_range_filter() {
        let entries = vec![
            make_entry(0, "session_start", None),
            make_entry(5, "turn.user", None),
            make_entry(10, "turn.assistant", None),
            make_entry(15, "session_ended", None),
        ];
        let q = Query::new("test").seq_range(5..=10);
        let results = q.filter(entries);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].seq, 5);
        assert_eq!(results[1].seq, 10);
    }

    #[test]
    fn query_parent_seq_filter() {
        let entries = vec![
            make_entry(0, "turn.user", None),
            make_entry(1, "tool_result", Some(0)),
            make_entry(2, "tool_result", Some(0)),
            make_entry(3, "tool_result", Some(1)),
        ];
        let q = Query::new("test").parent_seq(0);
        let results = q.filter(entries);
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn query_action_prefix_filter() {
        let entries = vec![
            make_entry(0, "session_start", None),
            make_entry(1, "turn.user", None),
            make_entry(2, "turn.assistant", None),
            make_entry(3, "session_ended", None),
        ];
        let q = Query::new("test").action_prefix("turn.");
        let results = q.filter(entries);
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn query_negate_filter() {
        let entries = vec![
            make_entry(0, "session_start", None),
            make_entry(1, "turn.user", None),
            make_entry(2, "tool_result", Some(1)),
            make_entry(3, "session_ended", None),
        ];
        let q = Query::new("test").kind(EntryKind::ToolResult).negate();
        let results = q.filter(entries);
        assert_eq!(results.len(), 3);
    }

    #[test]
    fn query_combined_filters() {
        let entries = vec![
            make_entry(0, "session_start", None),
            make_entry(5, "tool_result", Some(0)),
            make_entry(10, "tool_result", Some(5)),
            make_entry(15, "reflection", None),
        ];
        // Tool results with seq in 0..=10
        let q = Query::new("test")
            .kind(EntryKind::ToolResult)
            .seq_range(0..=10);
        let results = q.filter(entries);
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn query_actor_filter() {
        let entries = vec![
            make_entry_with_actor(0, "turn.user", "claude"),
            make_entry_with_actor(1, "turn.user", "corso"),
            make_entry_with_actor(2, "turn.user", "eva"),
        ];
        let q = Query::new("test").actor("corso");
        let results = q.filter(entries);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].seq, 1);
    }

    #[test]
    fn query_no_filters_returns_all() {
        let entries = vec![
            make_entry(0, "session_start", None),
            make_entry(1, "turn.user", None),
        ];
        let q = Query::new("test");
        let results = q.filter(entries);
        assert_eq!(results.len(), 2);
    }
}
