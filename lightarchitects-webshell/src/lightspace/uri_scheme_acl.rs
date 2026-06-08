//! URI scheme allowlist for Lightspace content URIs.
//!
//! Any URI appearing in a [`CanvasEvent`] payload (e.g. a file reference in
//! [`DrawerFileData`]) is validated here before the event is applied to the
//! reducer or persisted.  Schemes not on the allowlist are rejected to prevent
//! `javascript:`, `data:`, and other injection vectors (OWASP A03:2021).
//!
//! [`CanvasEvent`]: lightarchitects_lightspace::CanvasEvent
//! [`DrawerFileData`]: lightarchitects_lightspace::types::DrawerFileData

/// The set of URI schemes permitted in Lightspace content fields.
///
/// Only lowercase comparisons are performed; callers should normalise the
/// scheme to lowercase before calling [`is_allowed`].
const ALLOWED_SCHEMES: &[&str] = &["https", "file", "lightarchitects"];

/// Return `true` if the leading scheme of `uri` is on the allowlist.
///
/// Returns `false` for empty strings and URIs without a recognisable scheme.
#[must_use]
pub fn is_allowed(uri: &str) -> bool {
    let lower = uri.to_lowercase();
    ALLOWED_SCHEMES
        .iter()
        .any(|s| lower.starts_with(&format!("{s}:")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allowed_schemes_pass() {
        assert!(is_allowed("https://example.com/path"));
        assert!(is_allowed("file:///home/user/file.txt"));
        assert!(is_allowed("lightarchitects://canvas/session/abc"));
        // case-insensitive
        assert!(is_allowed("HTTPS://example.com"));
    }

    #[test]
    fn blocked_schemes_rejected() {
        assert!(!is_allowed("javascript:alert(1)"));
        assert!(!is_allowed("data:text/html,<script>"));
        assert!(!is_allowed("ftp://example.com"));
        assert!(!is_allowed(""));
        assert!(!is_allowed("no-scheme-here"));
    }
}
