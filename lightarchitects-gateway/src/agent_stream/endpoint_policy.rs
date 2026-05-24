//! Endpoint allowlist for outbound LLM connections (B5).
//!
//! Prevents SSRF-class attacks and OWASP-LLM05/LLM06 model substitution by
//! restricting which base URLs providers may connect to. The default allowlist
//! covers the canonical Anthropic API and local Ollama. Operators may add a
//! custom endpoint via `--allow-custom-endpoint`; doing so triggers a HITL
//! banner for the first three turns of the session.
//!
//! ## References
//! - OWASP-LLM05 (Sensitive Information Disclosure — exfil via rogue endpoint)
//! - OWASP-LLM06 (Excessive Agency — unintended model at trusted-looking URL)
//! - Security Guardrails §4.2 (endpoint trust boundaries)

use std::borrow::Cow;

/// Hosts accepted without any operator acknowledgement.
///
/// Both `api.anthropic.com` (production) and the two Ollama localhost variants
/// are trusted by default. Port numbers are intentionally absent here so the
/// check works against URL host-only extractions.
const DEFAULT_ALLOWLIST: &[&str] = &["api.anthropic.com", "localhost", "127.0.0.1", "::1"];

/// Extract the host component from a URL string.
///
/// Returns `Cow::Borrowed` when the host is a bare hostname (no scheme). For
/// full URLs (`https://host/path`) splits on `://` and takes the host segment.
/// Port numbers are stripped so `localhost:11434` matches `"localhost"`.
fn extract_host(url: &str) -> Cow<'_, str> {
    let after_scheme = if let Some(pos) = url.find("://") {
        &url[pos + 3..]
    } else {
        url
    };
    // Strip path.
    let host_port = after_scheme.split('/').next().unwrap_or(after_scheme);
    // Strip port.
    // IPv6 addresses: `[::1]:port` — strip surrounding brackets too.
    if let Some(stripped) = host_port.strip_prefix('[') {
        let host = stripped.split(']').next().unwrap_or(stripped);
        Cow::Owned(host.to_owned())
    } else {
        let host = host_port.split(':').next().unwrap_or(host_port);
        Cow::Borrowed(host)
    }
}

/// Returns `true` when `url` connects to a default-allowlisted host.
///
/// Case-insensitive host comparison; port numbers are ignored so
/// `http://localhost:11434` passes the same as `http://localhost`.
pub fn is_default_allowed(url: &str) -> bool {
    let host = extract_host(url);
    DEFAULT_ALLOWLIST
        .iter()
        .any(|allowed| host.eq_ignore_ascii_case(allowed))
}

/// Returns `true` when `url` is allowed under the current policy.
///
/// - `allow_custom`: operator has passed `--allow-custom-endpoint`; all URLs
///   are permitted (HITL banner must be shown separately by the caller).
/// - Otherwise: only `DEFAULT_ALLOWLIST` hosts pass.
pub fn is_allowed(url: &str, allow_custom: bool) -> bool {
    allow_custom || is_default_allowed(url)
}

/// Banner emitted to the operator for the first three turns when a custom
/// endpoint has been granted. Ends with a newline.
pub fn custom_endpoint_banner(url: &str) -> String {
    format!(
        "⚠  CUSTOM ENDPOINT ACTIVE: {url}\n\
         Requests are being sent to a non-default host. Ensure you trust this\n\
         endpoint before sharing sensitive context (OWASP-LLM05/LLM06).\n"
    )
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_anthropic_allowed() {
        assert!(is_allowed("https://api.anthropic.com/v1/messages", false));
    }

    #[test]
    fn localhost_allowed() {
        assert!(is_allowed("http://localhost:11434/api/chat", false));
        assert!(is_allowed(
            "http://127.0.0.1:11434/v1/chat/completions",
            false
        ));
    }

    #[test]
    fn custom_host_blocked_by_default() {
        assert!(!is_allowed("https://evil.example.com/v1/messages", false));
    }

    #[test]
    fn custom_host_allowed_with_flag() {
        assert!(is_allowed(
            "https://my-llm-proxy.internal/v1/messages",
            true
        ));
    }

    #[test]
    fn ipv6_loopback_allowed() {
        assert!(is_allowed("http://[::1]:8080/v1/messages", false));
    }

    #[test]
    fn banner_contains_url() {
        let b = custom_endpoint_banner("https://proxy.example.com");
        assert!(b.contains("https://proxy.example.com"));
        assert!(b.contains("OWASP-LLM05"));
    }
}
