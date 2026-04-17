//! Canon Enforcement Gate — compiled gates for Light Architects canon.
//!
//! Provides the [`CanonGate`] trait for defining individual canon checks,
//! [`CanonVerdict`] for pass/fail results with canon references, and
//! [`canon_check`] for running all registered gates against a context.
//!
//! # Architecture
//!
//! ```text
//! CanonContext (files, claims, security findings)
//!        |
//!        v
//! canon_check(&gates, &context)
//!        |
//!        +-- NoUnwrapGate::check()        → Cookbook §1 / Canon VIII
//!        +-- NoFalseWitnessGate::check()  → Covenant §2 / Canon V
//!        +-- SecurityBlockingGate::check() → Protocol §SEC / Canon XVII
//!        |
//!        v
//! Vec<CanonVerdict::Fail { .. }> (empty = all gates passed)
//! ```
//!
//! # Integration
//!
//! CORSO's manifest phase transitions call [`canon_check`] before
//! advancing the build state machine. A non-empty failure list halts
//! the transition with specific canon references.
//!
//! # Example
//!
//! ```rust
//! use crate::helix::canon::{
//!     canon_check, default_gates, CanonContext, CanonVerdict,
//! };
//!
//! let gates = default_gates();
//! let context = CanonContext::new()
//!     .with_source_snippet("fn main() { let x = foo.unwrap(); }");
//!
//! let failures = canon_check(&gates, &context);
//! assert!(!failures.is_empty()); // unwrap() detected
//!
//! if let CanonVerdict::Fail { canon_ref, reason } = &failures[0] {
//!     assert!(canon_ref.contains("Cookbook"));
//! }
//! ```

// ============================================================================
// Core types
// ============================================================================

/// Context passed to canon gates for evaluation.
///
/// Each field is optional — gates that need a particular field skip
/// gracefully (return `Pass`) when the field is absent.
#[derive(Debug, Clone, Default)]
pub struct CanonContext {
    /// Source code snippets to check (e.g., changed file contents).
    /// Each entry is the full text of a source file.
    pub source_snippets: Vec<String>,

    /// Claims or assertions made in output text (e.g., "this should work").
    /// Used by advisory gates that detect overconfidence language.
    pub claims: Vec<String>,

    /// Security scan findings. Each entry is a structured finding string.
    /// Format: "SEVERITY: description" (e.g., "HIGH: SQL injection in query.rs:42").
    pub security_findings: Vec<String>,

    /// Whether the source snippets include test code.
    /// When `true`, gates that allow patterns in tests (e.g., `.unwrap()`)
    /// will filter test sections before checking.
    pub includes_tests: bool,
}

impl CanonContext {
    /// Create an empty context.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a source code snippet.
    #[must_use]
    pub fn with_source_snippet(mut self, snippet: &str) -> Self {
        self.source_snippets.push(snippet.to_owned());
        self
    }

    /// Add a claim or assertion text.
    #[must_use]
    pub fn with_claim(mut self, claim: &str) -> Self {
        self.claims.push(claim.to_owned());
        self
    }

    /// Add a security finding.
    #[must_use]
    pub fn with_security_finding(mut self, finding: &str) -> Self {
        self.security_findings.push(finding.to_owned());
        self
    }

    /// Mark whether source snippets include test code.
    #[must_use]
    pub fn with_includes_tests(mut self, includes: bool) -> Self {
        self.includes_tests = includes;
        self
    }
}

/// Result of a canon gate evaluation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CanonVerdict {
    /// The gate passed — no canon violation detected.
    Pass,
    /// The gate failed — a specific canon violation was found.
    Fail {
        /// Which canon was violated (e.g., "Cookbook §1 / Canon VIII").
        canon_ref: String,
        /// Human-readable explanation of the violation.
        reason: String,
    },
}

impl CanonVerdict {
    /// Returns `true` if this verdict is a pass.
    #[must_use]
    pub fn is_pass(&self) -> bool {
        matches!(self, Self::Pass)
    }

    /// Returns `true` if this verdict is a failure.
    #[must_use]
    pub fn is_fail(&self) -> bool {
        matches!(self, Self::Fail { .. })
    }
}

/// A compiled canon enforcement gate.
///
/// Each gate checks one specific canon rule against the provided context.
/// Gates are stateless — all state lives in [`CanonContext`].
pub trait CanonGate: Send + Sync {
    /// Evaluate this gate against the given context.
    ///
    /// Returns [`CanonVerdict::Pass`] if the canon is satisfied,
    /// or [`CanonVerdict::Fail`] with the specific canon reference
    /// and reason if violated.
    fn check(&self, context: &CanonContext) -> CanonVerdict;

    /// The canonical reference string for the rule this gate enforces.
    ///
    /// Examples: "Cookbook §1 / Canon VIII", "Covenant §2 / Canon V".
    fn canon_ref(&self) -> &'static str;

    /// Human-readable name for this gate (used in logs and reports).
    fn name(&self) -> &'static str;
}

// ============================================================================
// Gate 1: NoUnwrapGate — Cookbook §1 / Canon VIII
// ============================================================================

/// Detects `.unwrap()` and `.expect()` in non-test production code.
///
/// **Canon**: Builders Cookbook §1 — "NO `.unwrap()`/`.expect()` in production."
/// **Canon VIII**: "Validate at the Boundary, Trust Within" — `.unwrap()` is a
/// trust assertion on unvalidated data.
///
/// The gate strips `#[cfg(test)]` blocks and `#[test]` functions before scanning,
/// since `.unwrap()` and `.expect()` are acceptable in test code.
pub struct NoUnwrapGate;

impl CanonGate for NoUnwrapGate {
    fn check(&self, context: &CanonContext) -> CanonVerdict {
        for snippet in &context.source_snippets {
            let production_code = if context.includes_tests {
                strip_test_sections(snippet)
            } else {
                snippet.clone()
            };

            let violations = find_unwrap_violations(&production_code);
            if !violations.is_empty() {
                return CanonVerdict::Fail {
                    canon_ref: self.canon_ref().to_owned(),
                    reason: format!(
                        "Found {} unwrap/expect call(s) in production code: {}",
                        violations.len(),
                        violations.join("; ")
                    ),
                };
            }
        }
        CanonVerdict::Pass
    }

    fn canon_ref(&self) -> &'static str {
        "Cookbook \u{00a7}1 / Canon VIII"
    }

    fn name(&self) -> &'static str {
        "NoUnwrapGate"
    }
}

/// Find `.unwrap()` and `.expect()` calls in source code, returning
/// a list of violation descriptions with approximate line numbers.
fn find_unwrap_violations(source: &str) -> Vec<String> {
    let mut violations = Vec::new();

    for (line_num, line) in source.lines().enumerate() {
        let trimmed = line.trim();

        // Skip comments
        if trimmed.starts_with("//") || trimmed.starts_with("/*") || trimmed.starts_with('*') {
            continue;
        }

        // Check for .unwrap() — but not in comments on the same line
        let code_part = line.split("//").next().unwrap_or(line);

        if code_part.contains(".unwrap()") {
            violations.push(format!("line {}: .unwrap()", line_num.saturating_add(1)));
        }
        if code_part.contains(".expect(") {
            violations.push(format!("line {}: .expect()", line_num.saturating_add(1)));
        }
    }

    violations
}

/// Strip `#[cfg(test)]` module blocks and `#[test]` function bodies
/// from source code, returning only the production portion.
///
/// Uses a simple line-based heuristic: once `#[cfg(test)]` is seen,
/// everything until the end of the file is considered test code
/// (standard Rust convention: test modules go at the bottom).
fn strip_test_sections(source: &str) -> String {
    let mut result = String::with_capacity(source.len());
    let mut in_test_section = false;

    for line in source.lines() {
        let trimmed = line.trim();

        // Detect #[cfg(test)] module — everything after is test code
        if trimmed == "#[cfg(test)]" {
            in_test_section = true;
            continue;
        }

        if !in_test_section {
            result.push_str(line);
            result.push('\n');
        }
    }

    result
}

// ============================================================================
// Gate 2: NoFalseWitnessGate — Covenant §2 / Canon V
// ============================================================================

/// Detects overconfidence language and unsubstantiated claims.
///
/// **Canon**: Communication Covenant §2 — "Thou shalt not bear false witness."
/// **Canon V**: "Arithmetic Before Assertions" — never claim something will work
/// without showing the math.
///
/// This is an advisory gate. It scans claims/text for anti-patterns:
/// - "should work" / "should be fine"
/// - "I'm confident" / "I'm pretty sure" / "I'm sure"
/// - "seems fine" / "looks good" (without evidence)
/// - "almost there" (without remaining steps)
///
/// These phrases are forbidden per the Communication Covenant because they
/// disguise uncertainty as certainty.
pub struct NoFalseWitnessGate;

/// Anti-patterns that indicate unsubstantiated confidence.
/// Each tuple: (pattern to search for, description of the violation).
const FALSE_WITNESS_PATTERNS: &[(&str, &str)] = &[
    (
        "should work",
        "\"should work\" — show the math or say \"I haven't verified\"",
    ),
    (
        "should be fine",
        "\"should be fine\" — state actual verification status",
    ),
    (
        "i'm confident",
        "\"I'm confident\" — confidence without calculation is recklessness",
    ),
    (
        "i'm pretty sure",
        "\"I'm pretty sure\" — state the actual probability",
    ),
    ("i'm sure", "\"I'm sure\" — certainty requires evidence"),
    (
        "seems fine",
        "\"seems fine\" — state what was actually checked",
    ),
    (
        "almost there",
        "\"almost there\" — state actual remaining steps and risks",
    ),
];

impl CanonGate for NoFalseWitnessGate {
    fn check(&self, context: &CanonContext) -> CanonVerdict {
        let mut violations = Vec::new();

        for claim in &context.claims {
            let lower = claim.to_lowercase();
            for &(pattern, description) in FALSE_WITNESS_PATTERNS {
                if lower.contains(pattern) {
                    violations.push(description.to_owned());
                }
            }
        }

        if violations.is_empty() {
            CanonVerdict::Pass
        } else {
            CanonVerdict::Fail {
                canon_ref: self.canon_ref().to_owned(),
                reason: format!(
                    "Found {} false-witness anti-pattern(s): {}",
                    violations.len(),
                    violations.join("; ")
                ),
            }
        }
    }

    fn canon_ref(&self) -> &'static str {
        "Covenant \u{00a7}2 / Canon V"
    }

    fn name(&self) -> &'static str {
        "NoFalseWitnessGate"
    }
}

// ============================================================================
// Gate 3: SecurityBlockingGate — Protocol §SEC / Canon XVII
// ============================================================================

/// Blocks phase transitions when HIGH or CRITICAL security findings exist.
///
/// **Canon**: CORSO Protocol §SEC — "Security & Privacy (7 rules)" with
/// `blocking: true`.
/// **Canon XVII**: "Hard Constraints — The Absolute Floor" — never deploy
/// code that knowingly contains critical security vulnerabilities.
///
/// Scans `security_findings` for severity prefixes: "HIGH:" and "CRITICAL:".
/// Any match halts the gate.
pub struct SecurityBlockingGate;

/// Severity prefixes that trigger a blocking failure.
const BLOCKING_SEVERITIES: &[&str] = &["HIGH:", "CRITICAL:"];

impl CanonGate for SecurityBlockingGate {
    fn check(&self, context: &CanonContext) -> CanonVerdict {
        let mut blockers = Vec::new();

        for finding in &context.security_findings {
            let upper = finding.to_uppercase();
            for &severity in BLOCKING_SEVERITIES {
                if upper.starts_with(severity) {
                    blockers.push(finding.clone());
                }
            }
        }

        if blockers.is_empty() {
            CanonVerdict::Pass
        } else {
            CanonVerdict::Fail {
                canon_ref: self.canon_ref().to_owned(),
                reason: format!(
                    "{} blocking security finding(s): {}",
                    blockers.len(),
                    blockers.join("; ")
                ),
            }
        }
    }

    fn canon_ref(&self) -> &'static str {
        "Protocol \u{00a7}SEC / Canon XVII"
    }

    fn name(&self) -> &'static str {
        "SecurityBlockingGate"
    }
}

// ============================================================================
// Gate runner
// ============================================================================

/// Build the default set of canon enforcement gates.
///
/// Returns the three top-impact compiled gates:
/// 1. [`NoUnwrapGate`] — Cookbook §1 / Canon VIII
/// 2. [`NoFalseWitnessGate`] — Covenant §2 / Canon V
/// 3. [`SecurityBlockingGate`] — Protocol §SEC / Canon XVII
#[must_use]
pub fn default_gates() -> Vec<Box<dyn CanonGate>> {
    vec![
        Box::new(NoUnwrapGate),
        Box::new(NoFalseWitnessGate),
        Box::new(SecurityBlockingGate),
    ]
}

/// Run all canon gates against the given context.
///
/// Returns a list of [`CanonVerdict::Fail`] entries. An empty list
/// means all gates passed. This is the primary programmatic entry
/// point for canon enforcement.
///
/// # Integration with CORSO manifest
///
/// Call this function before phase transitions:
///
/// ```rust
/// use crate::helix::canon::{canon_check, default_gates, CanonContext};
///
/// let gates = default_gates();
/// let ctx = CanonContext::new()
///     .with_source_snippet("let x = result?;") // clean code
///     .with_claim("Based on 3 benchmark runs, latency is 2ms.");
///
/// let failures = canon_check(&gates, &ctx);
/// assert!(failures.is_empty()); // all gates pass
/// ```
#[must_use]
pub fn canon_check(gates: &[Box<dyn CanonGate>], context: &CanonContext) -> Vec<CanonVerdict> {
    gates
        .iter()
        .map(|gate| gate.check(context))
        .filter(CanonVerdict::is_fail)
        .collect()
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    // ── NoUnwrapGate ────────────────────────────────────────────────────

    #[test]
    fn test_no_unwrap_gate_passes_clean_code() {
        let gate = NoUnwrapGate;
        let ctx = CanonContext::new()
            .with_source_snippet("fn main() {\n    let x = foo()?;\n    Ok(())\n}");
        assert!(gate.check(&ctx).is_pass());
    }

    #[test]
    fn test_no_unwrap_gate_fails_on_unwrap() {
        let gate = NoUnwrapGate;
        let ctx =
            CanonContext::new().with_source_snippet("fn main() {\n    let x = foo().unwrap();\n}");
        let verdict = gate.check(&ctx);
        assert!(verdict.is_fail());
        if let CanonVerdict::Fail { canon_ref, reason } = &verdict {
            assert!(canon_ref.contains("Cookbook"));
            assert!(reason.contains("unwrap"));
        }
    }

    #[test]
    fn test_no_unwrap_gate_fails_on_expect() {
        let gate = NoUnwrapGate;
        let ctx =
            CanonContext::new().with_source_snippet("let val = opt.expect(\"missing value\");");
        let verdict = gate.check(&ctx);
        assert!(verdict.is_fail());
        if let CanonVerdict::Fail { reason, .. } = &verdict {
            assert!(reason.contains("expect"));
        }
    }

    #[test]
    fn test_no_unwrap_gate_allows_unwrap_in_test_code() {
        let gate = NoUnwrapGate;
        let source = concat!(
            "fn production_code() -> Result<(), Error> {\n",
            "    let x = foo()?;\n",
            "    Ok(())\n",
            "}\n",
            "\n",
            "#[cfg(test)]\n",
            "mod tests {\n",
            "    #[test]\n",
            "    fn test_it() {\n",
            "        let x = foo().unwrap();\n",
            "    }\n",
            "}\n",
        );
        let ctx = CanonContext::new()
            .with_source_snippet(source)
            .with_includes_tests(true);
        assert!(gate.check(&ctx).is_pass());
    }

    #[test]
    fn test_no_unwrap_gate_ignores_comments() {
        let gate = NoUnwrapGate;
        let ctx = CanonContext::new().with_source_snippet(
            "// This comment mentions .unwrap() but it's fine\nlet x = foo()?;",
        );
        assert!(gate.check(&ctx).is_pass());
    }

    #[test]
    fn test_no_unwrap_gate_catches_inline_unwrap() {
        let gate = NoUnwrapGate;
        // .unwrap() before a comment on the same line
        let ctx = CanonContext::new().with_source_snippet("let x = foo().unwrap(); // bad");
        assert!(gate.check(&ctx).is_fail());
    }

    #[test]
    fn test_no_unwrap_gate_empty_context_passes() {
        let gate = NoUnwrapGate;
        let ctx = CanonContext::new();
        assert!(gate.check(&ctx).is_pass());
    }

    #[test]
    fn test_no_unwrap_gate_multiple_snippets() {
        let gate = NoUnwrapGate;
        let ctx = CanonContext::new()
            .with_source_snippet("let a = ok()?;")
            .with_source_snippet("let b = bad().unwrap();");
        assert!(gate.check(&ctx).is_fail());
    }

    // ── NoFalseWitnessGate ──────────────────────────────────────────────

    #[test]
    fn test_false_witness_gate_passes_evidenced_claim() {
        let gate = NoFalseWitnessGate;
        let ctx =
            CanonContext::new().with_claim("Based on 3 benchmark runs, P99 latency is 2.1ms.");
        assert!(gate.check(&ctx).is_pass());
    }

    #[test]
    fn test_false_witness_gate_fails_should_work() {
        let gate = NoFalseWitnessGate;
        let ctx = CanonContext::new().with_claim("This should work after the fix.");
        let verdict = gate.check(&ctx);
        assert!(verdict.is_fail());
        if let CanonVerdict::Fail { canon_ref, reason } = &verdict {
            assert!(canon_ref.contains("Covenant"));
            assert!(reason.contains("should work"));
        }
    }

    #[test]
    fn test_false_witness_gate_fails_confident_without_evidence() {
        let gate = NoFalseWitnessGate;
        let ctx = CanonContext::new().with_claim("I'm confident this will deploy cleanly.");
        assert!(gate.check(&ctx).is_fail());
    }

    #[test]
    fn test_false_witness_gate_fails_pretty_sure() {
        let gate = NoFalseWitnessGate;
        let ctx = CanonContext::new().with_claim("I'm pretty sure the tests cover this case.");
        assert!(gate.check(&ctx).is_fail());
    }

    #[test]
    fn test_false_witness_gate_fails_seems_fine() {
        let gate = NoFalseWitnessGate;
        let ctx = CanonContext::new().with_claim("The output seems fine to me.");
        assert!(gate.check(&ctx).is_fail());
    }

    #[test]
    fn test_false_witness_gate_fails_almost_there() {
        let gate = NoFalseWitnessGate;
        let ctx = CanonContext::new().with_claim("We're almost there, just a few more tweaks.");
        assert!(gate.check(&ctx).is_fail());
    }

    #[test]
    fn test_false_witness_gate_case_insensitive() {
        let gate = NoFalseWitnessGate;
        let ctx = CanonContext::new().with_claim("This SHOULD WORK fine.");
        assert!(gate.check(&ctx).is_fail());
    }

    #[test]
    fn test_false_witness_gate_empty_context_passes() {
        let gate = NoFalseWitnessGate;
        let ctx = CanonContext::new();
        assert!(gate.check(&ctx).is_pass());
    }

    #[test]
    fn test_false_witness_gate_multiple_violations() {
        let gate = NoFalseWitnessGate;
        let ctx = CanonContext::new().with_claim("I'm confident this should work and seems fine.");
        let verdict = gate.check(&ctx);
        assert!(verdict.is_fail());
        if let CanonVerdict::Fail { reason, .. } = &verdict {
            // Should detect multiple patterns
            assert!(reason.contains('3') || reason.contains('2'));
        }
    }

    // ── SecurityBlockingGate ────────────────────────────────────────────

    #[test]
    fn test_security_gate_passes_no_findings() {
        let gate = SecurityBlockingGate;
        let ctx = CanonContext::new();
        assert!(gate.check(&ctx).is_pass());
    }

    #[test]
    fn test_security_gate_passes_low_severity() {
        let gate = SecurityBlockingGate;
        let ctx = CanonContext::new()
            .with_security_finding("LOW: unused variable in config.rs")
            .with_security_finding("MEDIUM: missing rate limiting on /api/health");
        assert!(gate.check(&ctx).is_pass());
    }

    #[test]
    fn test_security_gate_fails_high_severity() {
        let gate = SecurityBlockingGate;
        let ctx = CanonContext::new().with_security_finding("HIGH: SQL injection in query.rs:42");
        let verdict = gate.check(&ctx);
        assert!(verdict.is_fail());
        if let CanonVerdict::Fail { canon_ref, reason } = &verdict {
            assert!(canon_ref.contains("Protocol"));
            assert!(canon_ref.contains("SEC"));
            assert!(reason.contains("SQL injection"));
        }
    }

    #[test]
    fn test_security_gate_fails_critical_severity() {
        let gate = SecurityBlockingGate;
        let ctx =
            CanonContext::new().with_security_finding("CRITICAL: hardcoded API key in main.rs:10");
        let verdict = gate.check(&ctx);
        assert!(verdict.is_fail());
        if let CanonVerdict::Fail { reason, .. } = &verdict {
            assert!(reason.contains("hardcoded API key"));
        }
    }

    #[test]
    fn test_security_gate_case_insensitive_severity() {
        let gate = SecurityBlockingGate;
        let ctx =
            CanonContext::new().with_security_finding("high: path traversal in upload handler");
        assert!(gate.check(&ctx).is_fail());
    }

    #[test]
    fn test_security_gate_mixed_severities() {
        let gate = SecurityBlockingGate;
        let ctx = CanonContext::new()
            .with_security_finding("LOW: informational")
            .with_security_finding("HIGH: XSS in template renderer")
            .with_security_finding("MEDIUM: weak cipher suite");
        let verdict = gate.check(&ctx);
        assert!(verdict.is_fail());
        if let CanonVerdict::Fail { reason, .. } = &verdict {
            assert!(reason.contains("1 blocking"));
        }
    }

    // ── canon_check integration ─────────────────────────────────────────

    #[test]
    fn test_canon_check_all_pass() {
        let gates = default_gates();
        let ctx = CanonContext::new()
            .with_source_snippet("let x = foo()?;")
            .with_claim("Verified: 3 tests pass, coverage 94%.")
            .with_security_finding("LOW: informational only");
        let failures = canon_check(&gates, &ctx);
        assert!(
            failures.is_empty(),
            "Expected all gates to pass: {failures:?}"
        );
    }

    #[test]
    fn test_canon_check_single_failure() {
        let gates = default_gates();
        let ctx = CanonContext::new().with_source_snippet("let x = foo().unwrap();");
        let failures = canon_check(&gates, &ctx);
        assert_eq!(failures.len(), 1);
        assert!(failures[0].is_fail());
    }

    #[test]
    fn test_canon_check_multiple_failures() {
        let gates = default_gates();
        let ctx = CanonContext::new()
            .with_source_snippet("let x = bad().unwrap();")
            .with_claim("This should work fine.")
            .with_security_finding("CRITICAL: RCE in deserializer");
        let failures = canon_check(&gates, &ctx);
        assert_eq!(failures.len(), 3, "Expected 3 failures: {failures:?}");
    }

    #[test]
    fn test_canon_check_empty_context() {
        let gates = default_gates();
        let ctx = CanonContext::new();
        let failures = canon_check(&gates, &ctx);
        assert!(failures.is_empty());
    }

    #[test]
    fn test_default_gates_returns_three() {
        let gates = default_gates();
        assert_eq!(gates.len(), 3);
        assert_eq!(gates[0].name(), "NoUnwrapGate");
        assert_eq!(gates[1].name(), "NoFalseWitnessGate");
        assert_eq!(gates[2].name(), "SecurityBlockingGate");
    }

    #[test]
    fn test_canon_verdict_is_pass_is_fail() {
        let pass = CanonVerdict::Pass;
        assert!(pass.is_pass());
        assert!(!pass.is_fail());

        let fail = CanonVerdict::Fail {
            canon_ref: "test".into(),
            reason: "test".into(),
        };
        assert!(!fail.is_pass());
        assert!(fail.is_fail());
    }

    // ── Helper function tests ───────────────────────────────────────────

    #[test]
    fn test_strip_test_sections() {
        let source = concat!(
            "fn prod() -> i32 { 42 }\n",
            "\n",
            "#[cfg(test)]\n",
            "mod tests {\n",
            "    fn test_it() { assert!(true); }\n",
            "}\n",
        );
        let stripped = strip_test_sections(source);
        assert!(stripped.contains("fn prod()"));
        assert!(!stripped.contains("test_it"));
        assert!(!stripped.contains("#[cfg(test)]"));
    }

    #[test]
    fn test_strip_test_sections_no_tests() {
        let source = "fn main() {\n    println!(\"hello\");\n}\n";
        let stripped = strip_test_sections(source);
        assert!(stripped.contains("fn main()"));
    }

    #[test]
    fn test_find_unwrap_violations_none() {
        let violations = find_unwrap_violations("let x = foo()?;\nlet y = bar.ok_or(err)?;");
        assert!(violations.is_empty());
    }

    #[test]
    fn test_find_unwrap_violations_found() {
        let violations =
            find_unwrap_violations("let x = foo().unwrap();\nlet y = bar.expect(\"msg\");");
        assert_eq!(violations.len(), 2);
        assert!(violations[0].contains("line 1"));
        assert!(violations[1].contains("line 2"));
    }

    #[test]
    fn test_find_unwrap_violations_in_comment_ignored() {
        let violations = find_unwrap_violations("// foo().unwrap() is bad\nlet x = foo()?;");
        assert!(violations.is_empty());
    }

    #[test]
    fn test_gate_blocks_non_compliant_decisions() {
        // Simulates a CORSO phase transition with non-compliant code
        let gates = default_gates();

        // The code being submitted for phase transition
        let changed_code = concat!(
            "pub fn handle_request(input: &str) -> String {\n",
            "    let parsed = serde_json::from_str(input).unwrap();\n",
            "    process(parsed)\n",
            "}\n",
        );

        let ctx = CanonContext::new()
            .with_source_snippet(changed_code)
            .with_claim("This should work for all valid JSON input.");

        let failures = canon_check(&gates, &ctx);

        // Must block: both unwrap in code AND "should work" in claim
        assert_eq!(failures.len(), 2);

        // Verify specific canon references
        let refs: Vec<&str> = failures
            .iter()
            .filter_map(|v| {
                if let CanonVerdict::Fail { canon_ref, .. } = v {
                    Some(canon_ref.as_str())
                } else {
                    None
                }
            })
            .collect();

        assert!(refs.iter().any(|r| r.contains("Cookbook")));
        assert!(refs.iter().any(|r| r.contains("Covenant")));
    }
}
