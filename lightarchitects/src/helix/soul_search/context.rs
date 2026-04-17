//! Context formatter — token-aware excerpt formatting for RAG context windows.
//!
//! Takes a list of Steps and formats them into a context string within a
//! token budget. Includes citation markers `[{N}]` for source attribution.

use crate::helix::types::Step;

// ============================================================================
// ContextFormatter
// ============================================================================

/// Formats Steps into a context string for LLM consumption.
///
/// Output format per step:
/// ```text
/// [{N}] [{owner}/{date}] {title}
/// {excerpt}
/// ---
/// ```
///
/// Approximate token count: `content.len() / 4`.
pub struct ContextFormatter {
    /// Maximum total tokens for the formatted context.
    token_budget: usize,
}

impl ContextFormatter {
    /// Create a formatter with the given token budget.
    #[must_use]
    pub fn new(token_budget: usize) -> Self {
        Self { token_budget }
    }

    /// Create a formatter with default budget (4096 tokens).
    #[must_use]
    pub fn default_budget() -> Self {
        Self::new(4096)
    }

    /// Format steps into a context string, respecting the token budget.
    ///
    /// Steps are included in order until the budget is exhausted.
    /// Returns the formatted context and metadata about what was included.
    #[must_use]
    pub fn format(&self, steps: &[Step]) -> FormattedContext {
        let mut output = String::new();
        let mut included = 0;
        let mut tokens_used = 0;

        for (i, step) in steps.iter().enumerate() {
            let entry = format_entry(i + 1, step);
            let entry_tokens = estimate_tokens(&entry);

            if tokens_used + entry_tokens > self.token_budget {
                break;
            }

            output.push_str(&entry);
            tokens_used += entry_tokens;
            included += 1;
        }

        FormattedContext {
            context: output,
            steps_included: included,
            steps_total: steps.len(),
            tokens_estimated: tokens_used,
            token_budget: self.token_budget,
        }
    }
}

/// Format a single step entry with citation marker.
fn format_entry(citation_num: usize, step: &Step) -> String {
    let owner = step
        .metadata
        .get("owner")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("unknown");

    let date = step
        .step_date
        .map_or_else(|| "n/d".to_owned(), |d| d.to_string());

    let title = step
        .title
        .as_deref()
        .filter(|t| !t.is_empty())
        .unwrap_or("(untitled)");

    // Truncate content to ~500 chars for excerpt
    let excerpt = if step.content.len() > 500 {
        let truncated = &step.content[..500];
        // Find last sentence boundary
        let end = truncated
            .rfind(". ")
            .or_else(|| truncated.rfind(".\n"))
            .map_or(500, |pos| pos + 1);
        format!("{}...", &step.content[..end])
    } else {
        step.content.clone()
    };

    format!("[{citation_num}] [{owner}/{date}] {title}\n{excerpt}\n---\n")
}

/// Estimate token count (~4 chars per token).
fn estimate_tokens(text: &str) -> usize {
    text.len().div_ceil(4)
}

// ============================================================================
// Result Type
// ============================================================================

/// Result from context formatting.
#[derive(Debug, Clone)]
pub struct FormattedContext {
    /// The formatted context string.
    pub context: String,
    /// Number of steps included in the context.
    pub steps_included: usize,
    /// Total steps available.
    pub steps_total: usize,
    /// Estimated tokens used.
    pub tokens_estimated: usize,
    /// Token budget that was applied.
    pub token_budget: usize,
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn make_step(id: &str, title: &str, content: &str) -> Step {
        Step {
            id: id.into(),
            helix_id: "test-helix".into(),
            title: Some(title.into()),
            content: content.into(),
            significance: 5.0,
            created_at: chrono::Utc::now(),
            step_date: Some(NaiveDate::from_ymd_opt(2026, 3, 8).expect("valid date")),
            step_index: None,
            community_id: None,
            expires: None,
            metadata: serde_json::json!({"owner": "eva"}),
        }
    }

    #[test]
    fn test_format_single_step() {
        let formatter = ContextFormatter::default_budget();
        let steps = vec![make_step("s1", "Test Title", "Some content here.")];
        let result = formatter.format(&steps);

        assert_eq!(result.steps_included, 1);
        assert!(result.context.contains("[1]"));
        assert!(result.context.contains("Test Title"));
        assert!(result.context.contains("Some content here."));
        assert!(result.context.contains("eva/2026-03-08"));
    }

    #[test]
    fn test_format_respects_budget() {
        // Tiny budget — should include very few steps
        let formatter = ContextFormatter::new(10);
        let steps = vec![
            make_step(
                "s1",
                "Title 1",
                "Content that is long enough to exceed budget",
            ),
            make_step("s2", "Title 2", "More content"),
        ];
        let result = formatter.format(&steps);
        assert!(result.steps_included <= 1);
        assert!(result.tokens_estimated <= 10);
    }

    #[test]
    fn test_format_empty_steps() {
        let formatter = ContextFormatter::default_budget();
        let result = formatter.format(&[]);
        assert_eq!(result.steps_included, 0);
        assert!(result.context.is_empty());
    }

    #[test]
    fn test_format_long_content_truncated() {
        let long_content = "A".repeat(1000) + ". End sentence.";
        let steps = vec![make_step("s1", "Long", &long_content)];
        let formatter = ContextFormatter::default_budget();
        let result = formatter.format(&steps);
        // Should have "..." indicating truncation
        assert!(result.context.len() < long_content.len() + 100);
    }

    #[test]
    fn test_estimate_tokens() {
        assert_eq!(estimate_tokens("hello world"), 3); // 11 chars → ~3 tokens
        assert_eq!(estimate_tokens(""), 0); // (0+3)/4 = 0
    }

    #[test]
    fn test_citation_numbering() {
        let formatter = ContextFormatter::default_budget();
        let steps = vec![
            make_step("s1", "First", "Content 1."),
            make_step("s2", "Second", "Content 2."),
            make_step("s3", "Third", "Content 3."),
        ];
        let result = formatter.format(&steps);
        assert!(result.context.contains("[1]"));
        assert!(result.context.contains("[2]"));
        assert!(result.context.contains("[3]"));
        assert_eq!(result.steps_included, 3);
    }
}
