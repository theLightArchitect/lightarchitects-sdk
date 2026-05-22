//! Conversation memory — turn history storage for a [`ConversationSession`].
//!
//! [`ConversationMemory`] is a sync trait (no async needed for in-memory
//! operations); implementors that persist to disk or a database may wrap
//! synchronous I/O or implement their own async flush strategy.
//!
//! [`ConversationSession`]: super::session::ConversationSession

// ── Types ─────────────────────────────────────────────────────────────────────

/// Speaker role in a conversation turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageRole {
    /// Operator / human caller.
    User,
    /// LLM assistant response.
    Assistant,
    /// Session system prompt (stored for reference only; not re-sent per turn).
    System,
}

impl std::fmt::Display for MessageRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::User => "user",
            Self::Assistant => "assistant",
            Self::System => "system",
        })
    }
}

/// One conversational exchange stored in [`ConversationMemory`].
#[derive(Debug, Clone)]
pub struct Turn {
    /// Who spoke.
    pub role: MessageRole,
    /// Text content of the turn.
    pub content: String,
}

// ── Trait ─────────────────────────────────────────────────────────────────────

/// Storage backend for conversation turn history.
///
/// Implementors must be `Send + Sync + 'static` so sessions can be moved
/// across async task boundaries and shared behind an `Arc`.
pub trait ConversationMemory: Send + Sync + 'static {
    /// Append a new turn.
    fn push(&mut self, role: MessageRole, content: String);

    /// Read-only view of all stored turns, oldest first.
    fn turns(&self) -> &[Turn];

    /// Number of turns stored.
    fn turn_count(&self) -> usize {
        self.turns().len()
    }

    /// Clear all stored turns.
    fn clear(&mut self);

    /// Build a simple flat transcript (for prompt injection).
    fn to_transcript(&self) -> String {
        let mut buf = String::new();
        for t in self.turns() {
            buf.push_str(&t.role.to_string());
            buf.push_str(": ");
            buf.push_str(&t.content);
            buf.push('\n');
        }
        buf
    }
}

// ── InMemoryConversationMemory ─────────────────────────────────────────────────

/// In-memory [`ConversationMemory`] backed by a `Vec<Turn>`.
///
/// Clears automatically on `drop`. Suitable for ephemeral sessions; for
/// persistent sessions, replace with a database-backed implementation.
#[derive(Debug, Default, Clone)]
pub struct InMemoryConversationMemory {
    turns: Vec<Turn>,
}

impl InMemoryConversationMemory {
    /// Create a new empty memory store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl ConversationMemory for InMemoryConversationMemory {
    fn push(&mut self, role: MessageRole, content: String) {
        self.turns.push(Turn { role, content });
    }

    fn turns(&self) -> &[Turn] {
        &self.turns
    }

    fn clear(&mut self) {
        self.turns.clear();
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn in_memory_push_and_turns() {
        let mut mem = InMemoryConversationMemory::new();
        assert_eq!(mem.turn_count(), 0);
        mem.push(MessageRole::User, "hello".into());
        mem.push(MessageRole::Assistant, "hi there".into());
        assert_eq!(mem.turn_count(), 2);
        assert_eq!(mem.turns()[0].role, MessageRole::User);
        assert_eq!(mem.turns()[1].content, "hi there");
    }

    #[test]
    fn in_memory_clear() {
        let mut mem = InMemoryConversationMemory::new();
        mem.push(MessageRole::User, "x".into());
        mem.clear();
        assert_eq!(mem.turn_count(), 0);
    }

    #[test]
    fn transcript_format() {
        let mut mem = InMemoryConversationMemory::new();
        mem.push(MessageRole::User, "ping".into());
        mem.push(MessageRole::Assistant, "pong".into());
        let t = mem.to_transcript();
        assert!(t.contains("user: ping"));
        assert!(t.contains("assistant: pong"));
    }
}
