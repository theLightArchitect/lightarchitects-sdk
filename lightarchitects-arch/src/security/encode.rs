//! HTML output encoding — H2 (XSS prevention) + S-4 (seed text sanitization).
//!
//! All text that originates from source files or narrative seeds must pass through
//! [`encode`] before being embedded in generated HTML.  This applies to:
//! - Identifier names, doc-comment text, module paths (embedded in `<code>` tags)
//! - Narrative seed TOML values (embedded in `<p>` tags or HTML attributes)
//! - URL-like values in `href`/`src` attributes
//!
//! The caller selects the appropriate [`EncodeContext`]; incorrect context choice is a
//! type-level mistake rather than a runtime one.

use thiserror::Error;

/// The HTML context in which encoded output will be placed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncodeContext {
    /// Inside an HTML text node — encodes `<`, `>`, `&`, `"`, `'`.
    HtmlText,
    /// Inside an HTML attribute value (double-quoted) — same as `HtmlText` plus `"`.
    HtmlAttr,
    /// In an `href` or `src` attribute — additionally rejects `javascript:` and `data:` schemes.
    HtmlUrl,
}

/// Errors from the encoding layer.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum EncodeError {
    /// The URL begins with a forbidden scheme (`javascript:`, `data:`).
    #[error("URL scheme is not permitted: '{0}'")]
    ForbiddenScheme(String),
}

/// Encodes `input` for safe embedding in the given HTML `context`.
///
/// For [`EncodeContext::HtmlUrl`] this returns an [`Err`] when the URL starts with a
/// forbidden scheme.  For other contexts it is infallible — the `Result` is returned for
/// API consistency.
///
/// # Errors
///
/// [`EncodeError::ForbiddenScheme`] when context is `HtmlUrl` and `input` begins with
/// `javascript:` or `data:` (case-insensitive).
pub fn encode(input: &str, context: EncodeContext) -> Result<String, EncodeError> {
    if context == EncodeContext::HtmlUrl {
        let lower = input.trim_start().to_ascii_lowercase();
        for forbidden in &["javascript:", "data:"] {
            if lower.starts_with(forbidden) {
                return Err(EncodeError::ForbiddenScheme(
                    // Return only the scheme portion for safe error display.
                    input.get(..forbidden.len()).unwrap_or(input).to_owned(),
                ));
            }
        }
    }

    let mut out = String::with_capacity(input.len() + 16);
    for ch in input.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#x27;"),
            other => out.push(other),
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;

    #[test]
    fn encodes_html_metacharacters() {
        let s = encode("<script>alert('xss')</script>", EncodeContext::HtmlText).unwrap();
        assert_eq!(s, "&lt;script&gt;alert(&#x27;xss&#x27;)&lt;/script&gt;");
    }

    #[test]
    fn encodes_ampersand() {
        let s = encode("foo & bar", EncodeContext::HtmlAttr).unwrap();
        assert_eq!(s, "foo &amp; bar");
    }

    #[test]
    fn rejects_javascript_scheme() {
        let err = encode("javascript:alert(1)", EncodeContext::HtmlUrl).unwrap_err();
        assert_eq!(err, EncodeError::ForbiddenScheme("javascript:".to_owned()));
    }

    #[test]
    fn rejects_data_scheme() {
        let err = encode("data:text/html,<h1>pwned</h1>", EncodeContext::HtmlUrl).unwrap_err();
        assert_eq!(err, EncodeError::ForbiddenScheme("data:".to_owned()));
    }

    #[test]
    fn rejects_javascript_scheme_case_insensitive() {
        assert!(encode("JAVASCRIPT:void(0)", EncodeContext::HtmlUrl).is_err());
    }

    #[test]
    fn accepts_https_url() {
        let s = encode("https://example.com/path?q=1&r=2", EncodeContext::HtmlUrl).unwrap();
        assert!(s.contains("&amp;"));
    }

    #[test]
    fn passthrough_plain_text() {
        let s = encode("hello world", EncodeContext::HtmlText).unwrap();
        assert_eq!(s, "hello world");
    }
}
