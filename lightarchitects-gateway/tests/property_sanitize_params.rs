//! Property tests for `sanitize_params` — security-critical two-plane sanitization.
//!
//! Covers Canon XXVII Suite 3 (property) for the iter-8 `sanitize_params` surface.
use lightarchitects::agent::sanitize_params;
use proptest::prelude::*;

/// ASCII printable characters that contain none of the 10 forbidden control-plane tokens.
/// Used as the "safe" input strategy for the identity plane.
const SAFE_IDENTITY_CHARS: &str = r"[a-zA-Z0-9 .,;:!?@#%^&*()\[\]{}\-+=/|~`']{0,200}";

/// Arbitrary bytes for the prompt plane (no length restriction — we test that).
const ARBITRARY_PROMPT: &str = r".{0,300}";

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    /// Safe identity + safe prompt must always pass sanitization.
    #[test]
    fn safe_inputs_always_pass(
        identity in SAFE_IDENTITY_CHARS,
        prompt in SAFE_IDENTITY_CHARS,
    ) {
        // Ensure none of the forbidden tokens snuck in via the regex engine.
        prop_assume!(
            !identity.contains("<system>") &&
            !identity.contains("</system>") &&
            !identity.contains('\u{202E}') &&
            !identity.contains('\x00') &&
            !identity.contains('\u{200B}') &&
            !identity.contains('\u{200C}') &&
            !identity.contains('\u{200D}') &&
            !identity.contains('\u{200E}') &&
            !identity.contains('\u{200F}') &&
            !identity.contains('\u{FEFF}')
        );
        prop_assert!(
            sanitize_params(&identity, &prompt).is_ok(),
            "safe inputs must pass: identity={identity:?} prompt={prompt:?}"
        );
    }

    /// The sanitized prompt output must never contain raw `<` or `>`.
    #[test]
    fn prompt_output_never_contains_raw_angle_brackets(prompt in ARBITRARY_PROMPT) {
        // identity plane uses a known-safe string so failures isolate to the prompt.
        match sanitize_params("safe-identity", &prompt) {
            Ok((_, safe_prompt)) => {
                prop_assert!(
                    !safe_prompt.contains('<') && !safe_prompt.contains('>'),
                    "prompt output must not contain raw angle brackets; got {safe_prompt:?}"
                );
            }
            Err(_) => {
                // Oversized or forbidden-token errors are also acceptable — they mean
                // the input was rejected, not that unsafe content passed through.
            }
        }
    }

    /// Strings exceeding MAX_PARAM_BYTES in the identity position must always be rejected.
    #[test]
    fn oversized_identity_always_rejects(len in 8193usize..9000) {
        let big = "a".repeat(len);
        prop_assert!(
            sanitize_params(&big, "ok").is_err(),
            "identity exceeding MAX_PARAM_BYTES must be rejected"
        );
    }

    /// Strings exceeding MAX_PARAM_BYTES in the prompt position must always be rejected.
    #[test]
    fn oversized_prompt_always_rejects(len in 8193usize..9000) {
        let big = "a".repeat(len);
        prop_assert!(
            sanitize_params("safe-identity", &big).is_err(),
            "prompt exceeding MAX_PARAM_BYTES must be rejected"
        );
    }

    /// Content-plane sanitization must be idempotent: applying it twice gives the same result.
    #[test]
    fn prompt_sanitization_is_idempotent(prompt in SAFE_IDENTITY_CHARS) {
        if let Ok((_, first)) = sanitize_params("safe-identity", &prompt) {
            if let Ok((_, second)) = sanitize_params("safe-identity", &first) {
                prop_assert_eq!(
                    first, second,
                    "content-plane sanitization must be idempotent"
                );
            }
        }
    }

    /// R7 — Action-name allowlist: arbitrary bytes containing forbidden control-plane
    /// characters must never produce output that re-embeds those characters verbatim.
    ///
    /// When the identity position contains a byte outside the safe set (e.g. null,
    /// invisible Unicode, RTL overrides), the sanitizer must either:
    ///   (a) reject the input entirely (`Err`), OR
    ///   (b) return a sanitized form that contains no control characters.
    ///
    /// This property guards the wire-visible `HandlerError` message surface
    /// (Security Guardrails §2.4 — sibling/action name injection).
    #[test]
    fn r7_control_chars_in_identity_never_pass_through_unsanitized(
        // Generate identities that include at least one ASCII control character
        // (range 0x00–0x1F) to stress the control-plane gate.
        control_prefix in r"[\x00-\x1f]{1,4}",
        safe_suffix in r"[a-zA-Z0-9]{0,20}",
    ) {
        let identity = format!("{control_prefix}{safe_suffix}");
        match sanitize_params(&identity, "safe-prompt") {
            Err(_) => {
                // Rejection is the correct outcome — the input was refused.
            }
            Ok((sanitized_identity, _)) => {
                // If the sanitizer allowed it, the output must contain no raw control chars.
                prop_assert!(
                    !sanitized_identity.chars().any(|c| c.is_control()),
                    "sanitized identity must not contain raw control chars; got: {sanitized_identity:?}"
                );
            }
        }
    }
}
