//! Raw byte framing — splits an arbitrary byte stream into complete lines.
//!
//! [`LineSplitter`] accumulates bytes and yields complete `\n`-terminated lines.
//! A 4 MB cap prevents OOM from a malformed or adversarial upstream.

/// Maximum byte length of a single line before [`LineSplitter`] returns an error.
pub const MAX_LINE_BYTES: usize = 4 * 1024 * 1024;

/// Error produced by [`LineSplitter`].
#[derive(Debug, thiserror::Error)]
pub enum FramingError {
    /// The accumulated line bytes exceeded [`MAX_LINE_BYTES`] without a `\n`.
    #[error("line exceeded maximum length of {MAX_LINE_BYTES} bytes")]
    LineTooLong,
}

/// Accumulates raw bytes and yields complete UTF-8 lines.
///
/// Call [`push_bytes`] as new bytes arrive; collect yielded lines via the
/// iterator return value. Call [`flush`] when the upstream source is exhausted
/// to emit any partial final line.
///
/// [`push_bytes`]: LineSplitter::push_bytes
/// [`flush`]: LineSplitter::flush
#[derive(Default)]
pub struct LineSplitter {
    buf: Vec<u8>,
}

impl LineSplitter {
    /// Creates a new empty [`LineSplitter`].
    pub fn new() -> Self {
        Self::default()
    }

    /// Append `bytes` to the internal buffer and return any newly complete lines.
    ///
    /// # Errors
    ///
    /// Returns [`FramingError::LineTooLong`] if the buffer exceeds [`MAX_LINE_BYTES`]
    /// without containing a `\n`.
    pub fn push_bytes(&mut self, bytes: &[u8]) -> Result<Vec<String>, FramingError> {
        self.buf.extend_from_slice(bytes);
        if self.buf.len() > MAX_LINE_BYTES && !self.buf.contains(&b'\n') {
            return Err(FramingError::LineTooLong);
        }
        Ok(self.drain_lines())
    }

    /// Emit any remaining bytes as a final (possibly unterminated) line.
    pub fn flush(&mut self) -> Option<String> {
        if self.buf.is_empty() {
            return None;
        }
        let line = String::from_utf8_lossy(&self.buf).into_owned();
        self.buf.clear();
        Some(line)
    }

    fn drain_lines(&mut self) -> Vec<String> {
        let mut lines = Vec::new();
        while let Some(pos) = self.buf.iter().position(|&b| b == b'\n') {
            let line_bytes = self.buf.drain(..=pos).collect::<Vec<_>>();
            let line = String::from_utf8_lossy(&line_bytes)
                .trim_end_matches(['\n', '\r'])
                .to_owned();
            lines.push(line);
        }
        lines
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn splits_complete_lines() {
        let mut s = LineSplitter::new();
        let lines = s.push_bytes(b"hello\nworld\n").unwrap();
        assert_eq!(lines, ["hello", "world"]);
    }

    #[test]
    fn splits_across_chunks() {
        let mut s = LineSplitter::new();
        assert!(s.push_bytes(b"hel").unwrap().is_empty());
        assert!(s.push_bytes(b"lo").unwrap().is_empty());
        let lines = s.push_bytes(b"\n").unwrap();
        assert_eq!(lines, ["hello"]);
    }

    #[test]
    fn flush_returns_partial_line() {
        let mut s = LineSplitter::new();
        s.push_bytes(b"no newline here").unwrap();
        assert_eq!(s.flush(), Some("no newline here".to_owned()));
    }

    #[test]
    fn rejects_oversized_line() {
        let mut s = LineSplitter::new();
        let big = vec![b'x'; MAX_LINE_BYTES + 1];
        assert!(matches!(s.push_bytes(&big), Err(FramingError::LineTooLong)));
    }

    #[test]
    fn empty_flush_returns_none() {
        let mut s = LineSplitter::new();
        assert!(s.flush().is_none());
    }
}
