//! Output grounding — verifies file references and strips hallucinations.
//!
//! Before posting any sibling output to Discord or Telegram, this module
//! scans for file path references and verifies they exist on disk.
//! Hallucinated file references are stripped and flagged.

use std::path::Path;

/// Scan output text for file-like references and verify they exist.
///
/// Returns `(cleaned_text, hallucination_count)`.
/// Hallucinated references are replaced with `[reference removed — file not found]`.
pub fn verify_and_clean(text: &str, data_dir: &Path) -> (String, u32) {
    let mut cleaned = String::with_capacity(text.len());
    let mut hallucinations: u32 = 0;

    for line in text.lines() {
        let cleaned_line = verify_line(line, data_dir, &mut hallucinations);
        cleaned.push_str(&cleaned_line);
        cleaned.push('\n');
    }

    // Trim trailing newline
    if cleaned.ends_with('\n') {
        cleaned.pop();
    }

    (cleaned, hallucinations)
}

/// Check a single line for file references and verify them.
fn verify_line(line: &str, data_dir: &Path, hallucinations: &mut u32) -> String {
    // Pattern 1: explicit file paths (helix/..., shared/..., workspace-*/**)
    // Pattern 2: markdown-style references like `file.md` or [file](path)
    let mut result = line.to_owned();

    // Check for helix/ references
    for reference in extract_path_references(line) {
        let resolved = resolve_reference(&reference, data_dir);
        if !resolved {
            tracing::debug!(reference = %reference, "Hallucinated file reference removed");
            result = result.replace(&reference, "[reference removed — file not found]");
            *hallucinations = hallucinations.saturating_add(1);
        }
    }

    result
}

/// Extract potential file path references from a line of text.
fn extract_path_references(line: &str) -> Vec<String> {
    let mut refs = Vec::new();
    let patterns = ["helix/", "shared/", "workspace-", ".soul/", ".arena/"];

    // Find path-like substrings
    for pattern in &patterns {
        let mut search_from = 0;
        while let Some(start) = line[search_from..].find(pattern) {
            let abs_start = search_from + start;
            // Walk backwards to find the start of the path
            let path_start = line[..abs_start]
                .rfind(|c: char| c.is_whitespace() || c == '(' || c == '[' || c == '`')
                .map_or(abs_start, |p| p + 1);

            // Walk forward to find the end of the path
            let path_end = line[abs_start..]
                .find(|c: char| c.is_whitespace() || c == ')' || c == ']' || c == '`')
                .map_or(line.len(), |p| abs_start + p);

            let reference = line[path_start..path_end].trim().to_owned();
            if reference.len() > 5 && reference.contains('/') {
                refs.push(reference);
            }
            search_from = path_end;
        }
    }

    refs
}

/// Resolve a file reference against known base paths.
fn resolve_reference(reference: &str, data_dir: &Path) -> bool {
    // Try direct path
    if Path::new(reference).exists() {
        return true;
    }

    // Try relative to data_dir (~/.arena/)
    if data_dir.join(reference).exists() {
        return true;
    }

    // Try relative to helix vault
    let home = dirs_next::home_dir().unwrap_or_default();
    let helix = home.join(".soul/helix");
    if helix.join(reference).exists() {
        return true;
    }

    // Try stripping common prefixes
    for prefix in &["helix/", "shared/", "~/.soul/helix/", "~/.soul/"] {
        if let Some(stripped) = reference.strip_prefix(prefix) {
            if helix.join(stripped).exists() || data_dir.join(stripped).exists() {
                return true;
            }
        }
    }

    false
}

/// Check if output contains fabricated data (invented statistics, fake experiments).
///
/// Returns `true` if the text likely contains fabricated measurements.
/// Heuristic: looks for percentage claims, p-values, and correlation coefficients
/// that appear in an "original research" context (not quoting a real paper).
pub fn detect_fabrication(text: &str) -> bool {
    let lower = text.to_lowercase();

    // Count suspicious statistical claims
    let mut suspicious = 0u32;

    // Percentage claims like "37.2% reduction" or "63.2% improvement"
    let pct_count = lower.matches('%').count();
    if pct_count >= 2 {
        suspicious = suspicious.saturating_add(1);
    }

    // P-values like "p<0.001" or "p < 0.01"
    if lower.contains("p<0.") || lower.contains("p < 0.") || lower.contains("p=0.") {
        suspicious = suspicious.saturating_add(1);
    }

    // Correlation coefficients like "r=0.68" or "r = 0.31"
    if lower.contains("r=0.") || lower.contains("r = 0.") {
        suspicious = suspicious.saturating_add(1);
    }

    // "Our results show" / "We measured" / "We found" — first-person experimental claims
    let experimental_claims = [
        "our results show",
        "we measured",
        "we found that",
        "we observed",
        "our analysis reveals",
        "our experiments",
        "we demonstrate",
        "we validated",
        "results show a",
    ];
    for claim in &experimental_claims {
        if lower.contains(claim) {
            suspicious = suspicious.saturating_add(1);
        }
    }

    // Threshold: 2+ suspicious indicators = likely fabrication
    suspicious >= 2
}

/// Check if output contains any proposal-like content (CORSO build plan format).
pub fn detect_proposal(text: &str) -> bool {
    let lower = text.to_lowercase();
    let markers = [
        "plan_id:",
        "## phase",
        "acceptance criteria",
        "depends_on",
        "risk_level:",
    ];
    markers.iter().filter(|m| lower.contains(*m)).count() >= 2
}

// ── Confidence classification ─────────────────────────────────────────

/// Confidence level for sibling output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Confidence {
    /// Cites a real paper (arXiv ID) or helix entry path that was verified to exist.
    Grounded,
    /// Sibling interpretation or analysis — no fabricated data, but not directly citing a source.
    Analysis,
    /// Neither grounded nor analysis — treat with caution.
    Unverified,
}

impl Confidence {
    /// Tag string prepended to Discord posts.
    pub fn tag(self) -> &'static str {
        match self {
            Self::Grounded => "[grounded]",
            Self::Analysis => "[analysis]",
            Self::Unverified => "[unverified]",
        }
    }
}

impl std::fmt::Display for Confidence {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.tag())
    }
}

/// Classify the confidence level of sibling output text.
///
/// - `Grounded`: contains a verified arXiv ID or helix path that exists on disk.
/// - `Analysis`: contains analytical language without fabrication markers.
/// - `Unverified`: everything else, or if fabrication is detected.
pub fn classify_confidence(text: &str, data_dir: &Path) -> Confidence {
    // Fabrication always downgrades to Unverified
    if detect_fabrication(text) {
        tracing::debug!("Fabrication detected — classifying as Unverified");
        return Confidence::Unverified;
    }

    // Check for grounded references (arXiv, CVE, or verified helix path)
    if has_verified_arxiv(text) || has_verified_cve(text) || has_verified_helix_path(text, data_dir)
    {
        return Confidence::Grounded;
    }

    // Check for analytical markers
    if has_analysis_markers(text) {
        return Confidence::Analysis;
    }

    Confidence::Unverified
}

/// Check if text contains an arXiv ID that passes basic format validation.
///
/// Matches patterns like `arXiv:2503.04302` or bare `2503.04302` (YYMM.NNNNN).
fn has_verified_arxiv(text: &str) -> bool {
    // Pattern: optional "arXiv:" prefix + YYMM.NNNNN (4-digit year-month, 4-5 digit ID)
    let lower = text.to_lowercase();
    let mut search = lower.as_str();

    while let Some(pos) = search.find("arxiv:") {
        let after = &search[pos + 6..];
        if is_arxiv_id_at(after) {
            return true;
        }
        search = &search[pos + 6..];
    }

    // Also check for bare YYMM.NNNNN patterns (at word boundaries)
    has_bare_arxiv_id(text)
}

/// Check if the string starts with a valid arXiv ID pattern (YYMM.NNNNN).
fn is_arxiv_id_at(s: &str) -> bool {
    let s = s.trim_start();
    if s.len() < 9 {
        return false;
    }
    let bytes = s.as_bytes();
    // First 4 chars: digits (YYMM)
    if !bytes[..4].iter().all(u8::is_ascii_digit) {
        return false;
    }
    // Then a dot
    if bytes[4] != b'.' {
        return false;
    }
    // Then 4-5 digits
    let tail = &bytes[5..];
    let digit_count = tail.iter().take_while(|b| b.is_ascii_digit()).count();
    (4..=5).contains(&digit_count)
}

/// Check for bare arXiv ID patterns (YYMM.NNNNN at word boundaries).
fn has_bare_arxiv_id(text: &str) -> bool {
    for (i, _) in text.char_indices() {
        // Must be at start or preceded by whitespace / punctuation
        if i > 0 {
            let prev = text.as_bytes()[i - 1];
            if prev.is_ascii_alphanumeric() {
                continue;
            }
        }
        if is_arxiv_id_at(&text[i..]) {
            return true;
        }
    }
    false
}

/// Check if text contains a valid CVE ID (CVE-YYYY-NNNNN format).
///
/// CVE IDs follow the pattern `CVE-YYYY-NNNN+` where YYYY is 1999-2099
/// and the numeric suffix is 4+ digits. We validate format only — we don't
/// hit the NVD API to confirm existence (the conductor already fetched
/// real CVEs into the feed, so the model is citing from that feed).
fn has_verified_cve(text: &str) -> bool {
    let upper = text.to_uppercase();
    let mut search = upper.as_str();

    while let Some(pos) = search.find("CVE-") {
        let after = &search[pos + 4..];
        if is_cve_id_at(after) {
            return true;
        }
        search = &search[pos + 4..];
    }
    false
}

/// Check if the string starts with a valid CVE ID suffix (YYYY-NNNN+).
fn is_cve_id_at(s: &str) -> bool {
    if s.len() < 9 {
        return false;
    }
    let bytes = s.as_bytes();
    // First 4 chars: year digits (1999-2099)
    if !bytes[..4].iter().all(u8::is_ascii_digit) {
        return false;
    }
    // Then a hyphen
    if bytes[4] != b'-' {
        return false;
    }
    // Then 4+ digits
    let digit_count = bytes[5..].iter().take_while(|b| b.is_ascii_digit()).count();
    digit_count >= 4
}

/// Check if text contains helix path references that exist on disk.
fn has_verified_helix_path(text: &str, data_dir: &Path) -> bool {
    let refs = extract_path_references(text);
    for reference in &refs {
        if resolve_reference(reference, data_dir) {
            return true;
        }
    }
    false
}

/// Check if text contains analytical language markers.
///
/// Recognizes both informal analysis language and structured intelligence
/// brief markers from the Think Tank format.
fn has_analysis_markers(text: &str) -> bool {
    let lower = text.to_lowercase();
    let markers = [
        // Analytical language
        "i think",
        "my analysis",
        "in my view",
        "i believe",
        "this suggests",
        "based on",
        "implications",
        "it appears",
        "likely that",
        "in my assessment",
        // Intelligence brief markers
        "**classification**:",
        "**confidence**:",
        "**source**:",
        "[intel]",
        "implications for light architects",
        "recommended actions",
        "executive summary",
        "technical analysis",
        "osint correlation",
    ];
    markers.iter().any(|m| lower.contains(m))
}

/// Check if output contains research paper content.
pub fn detect_research_paper(text: &str) -> bool {
    let lower = text.to_lowercase();
    let markers = [
        "abstract",
        "## findings",
        "## methodology",
        "## conclusion",
        "references",
    ];
    markers.iter().filter(|m| lower.contains(*m)).count() >= 2
}
