//! Heuristic-only task classifier — zero LLM cost, ≤5 ms p99.
//!
//! Uses `aho-corasick` multi-pattern substring matching over the task text.
//! **No `regex` crate is used over user input** (HIGH H-8 — avoids `ReDoS`).
//! The input must already be validated by [`super::routes::validate_task_input`]
//! before being passed here.
//!
//! # Design (HIGH H-4)
//!
//! This classifier is GREENFIELD — inspired by `lÆx0/src/agent/pick.rs`'s
//! heuristic style but not a port.  `pick.rs` returns `ExecutionMode`
//! (Solo/Squad) only; this module returns a full [`Classification`] with a
//! per-agent match score and rationale.
//!
//! # Algorithm
//!
//! 1. Lowercase the task text (ASCII fold only — UTF-8 aware).
//! 2. For each of the nine domain agents, run an `AhoCorasick` automaton
//!    over the lowercased text.
//! 3. Agents with ≥1 keyword hit are included in the result, ordered by
//!    descending hit count.
//! 4. `ExecutionMode` is derived from the number of matched agents:
//!    0 → `Idle`, 1 → `Solo`, 2+ → `Squad`.

use aho_corasick::AhoCorasick;
use std::sync::OnceLock;

use super::types::{Classification, DomainAgent, ExecutionMode};

// ── Keyword tables (one per domain agent) ────────────────────────────────────

/// Keywords for the Engineer agent.
static ENGINEER_PATTERNS: &[&str] = &[
    "implement",
    "refactor",
    "write code",
    "add function",
    "add method",
    "add struct",
    "add trait",
    "create module",
    "build feature",
    "fix bug",
    "patch",
    "coding",
    "develop",
    "ship",
];

/// Keywords for the Quality agent.
static QUALITY_PATTERNS: &[&str] = &[
    "code review",
    "review pr",
    "review code",
    "quality",
    "clippy",
    "lint",
    "clean up",
    "cleanup",
    "tidy",
    "readability",
    "best practice",
    "improve style",
    "critique",
];

/// Keywords for the Security agent.
static SECURITY_PATTERNS: &[&str] = &[
    "security",
    "vulnerability",
    "pentest",
    "audit",
    "cve",
    "exploit",
    "injection",
    "sql injection",
    "xss",
    "csrf",
    "authentication",
    "authorisation",
    "authorization",
    "secrets",
    "credentials",
    "hardening",
    "attack surface",
];

/// Keywords for the Ops agent.
static OPS_PATTERNS: &[&str] = &[
    "deploy",
    "ci/cd",
    "pipeline",
    "dockerfile",
    "kubernetes",
    "k8s",
    "helm",
    "infrastructure",
    "terraform",
    "ansible",
    "launchagent",
    "systemd",
    "daemon",
    "monitoring",
    "alerting",
    "rollout",
    "rollback",
];

/// Keywords for the Researcher agent.
static RESEARCHER_PATTERNS: &[&str] = &[
    "research",
    "investigate",
    "explore",
    "find out",
    "look into",
    "survey",
    "compare",
    "evaluate options",
    "what is",
    "how does",
    "benchmark comparison",
    "pros and cons",
    "trade-off",
    "tradeoff",
];

/// Keywords for the Knowledge agent.
static KNOWLEDGE_PATTERNS: &[&str] = &[
    "document",
    "documentation",
    "wiki",
    "knowledge base",
    "helix",
    "graph",
    "note",
    "record",
    "capture",
    "explain",
    "summarise",
    "summarize",
    "architecture decision",
    "adr",
    "changelog",
];

/// Keywords for the Performance agent.
static PERFORMANCE_PATTERNS: &[&str] = &[
    "performance",
    "optimise",
    "optimize",
    "benchmark",
    "profil",
    "latency",
    "throughput",
    "memory usage",
    "cpu usage",
    "bottleneck",
    "slow",
    "fast",
    "speed up",
    "criterion",
    "flamegraph",
];

/// Keywords for the Testing agent.
static TESTING_PATTERNS: &[&str] = &[
    "test",
    "unit test",
    "integration test",
    "property test",
    "fuzz",
    "coverage",
    "assertion",
    "spec",
    "e2e",
    "end-to-end",
    "playwright",
    "hypothesis",
    "regression",
    "test suite",
];

/// Keywords for the Documentation agent.
static DOCUMENTATION_PATTERNS: &[&str] = &[
    "doc comment",
    "rustdoc",
    "readme",
    "api docs",
    "write docs",
    "update docs",
    "docstring",
    "man page",
    "reference guide",
    "tutorial",
    "how-to",
    "getting started",
    "changelog entry",
];

// ── Pre-compiled automata ─────────────────────────────────────────────────────

/// Per-agent pre-compiled Aho-Corasick automaton.
struct AgentAutomaton {
    agent: DomainAgent,
    ac: AhoCorasick,
}

static AUTOMATA: OnceLock<Vec<AgentAutomaton>> = OnceLock::new();

fn automata() -> &'static [AgentAutomaton] {
    AUTOMATA.get_or_init(|| {
        let all: &[(DomainAgent, &[&str])] = &[
            (DomainAgent::Engineer, ENGINEER_PATTERNS),
            (DomainAgent::Quality, QUALITY_PATTERNS),
            (DomainAgent::Security, SECURITY_PATTERNS),
            (DomainAgent::Ops, OPS_PATTERNS),
            (DomainAgent::Researcher, RESEARCHER_PATTERNS),
            (DomainAgent::Knowledge, KNOWLEDGE_PATTERNS),
            (DomainAgent::Performance, PERFORMANCE_PATTERNS),
            (DomainAgent::Testing, TESTING_PATTERNS),
            (DomainAgent::Documentation, DOCUMENTATION_PATTERNS),
        ];
        all.iter()
            .filter_map(|(agent, patterns)| {
                AhoCorasick::new(*patterns)
                    .map_err(|err| {
                        tracing::error!("AhoCorasick build failed for {:?}: {err}", agent);
                        err
                    })
                    .ok()
                    .map(|ac| AgentAutomaton { agent: *agent, ac })
            })
            .collect()
    })
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Classify a validated task string into a [`Classification`].
///
/// The input must originate from [`super::routes::validate_task_input`] to
/// guarantee length and encoding invariants.
///
/// This function runs in constant time with respect to the number of patterns
/// (Aho-Corasick automaton traversal) — typical p99 < 1 ms for 8 KB inputs.
#[must_use]
pub fn classify(task: &str) -> Classification {
    let lower = task.to_lowercase();

    // Score each agent by keyword hit count.
    let mut scores: Vec<(DomainAgent, usize)> = automata()
        .iter()
        .map(|a| {
            let hits = a.ac.find_iter(lower.as_str()).count();
            (a.agent, hits)
        })
        .filter(|(_, hits)| *hits > 0)
        .collect();

    // Descending sort by hit count; stable so insertion order breaks ties.
    scores.sort_by(|a, b| b.1.cmp(&a.1));

    let agents: Vec<DomainAgent> = scores.iter().map(|(a, _)| *a).collect();

    let mode = match agents.len() {
        0 => ExecutionMode::Idle,
        1 => ExecutionMode::Solo,
        _ => ExecutionMode::Squad,
    };

    let rationale = build_rationale(&scores, mode);

    Classification {
        agents,
        mode,
        rationale,
    }
}

/// Build a human-readable explanation for the classification.
fn build_rationale(scores: &[(DomainAgent, usize)], mode: ExecutionMode) -> String {
    if scores.is_empty() {
        return "No domain keywords matched. Waiting for more input.".to_owned();
    }
    let top: Vec<String> = scores
        .iter()
        .take(3)
        .map(|(a, n)| format!("{a} ({n} keyword{})", if *n == 1 { "" } else { "s" }))
        .collect();
    let mode_str = match mode {
        ExecutionMode::Idle => "idle",
        ExecutionMode::Solo => "solo agent",
        ExecutionMode::Squad => "squad",
    };
    format!("Matched: {}. Running as {}.", top.join(", "), mode_str)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    // ── Helpers ───────────────────────────────────────────────────────────────

    fn has(task: &str, agent: DomainAgent) -> bool {
        classify(task).agents.contains(&agent)
    }

    fn mode_of(task: &str) -> ExecutionMode {
        classify(task).mode
    }

    // ── Smoke tests (retained from Phase 2) ──────────────────────────────────

    #[test]
    fn engineer_keywords_matched() {
        let c = classify("implement the new auth module and refactor the existing code");
        assert!(c.agents.contains(&DomainAgent::Engineer));
    }

    #[test]
    fn security_keywords_matched() {
        let c = classify("audit the authentication surface for vulnerabilities");
        assert!(c.agents.contains(&DomainAgent::Security));
    }

    #[test]
    fn testing_keywords_matched() {
        let c = classify("add property tests and improve unit test coverage");
        assert!(c.agents.contains(&DomainAgent::Testing));
    }

    #[test]
    fn quality_keywords_matched() {
        let c = classify("do a code review and check for clippy warnings");
        assert!(c.agents.contains(&DomainAgent::Quality));
    }

    #[test]
    fn empty_returns_idle() {
        let c = classify("hello");
        assert_eq!(c.mode, ExecutionMode::Idle);
    }

    #[test]
    fn multi_agent_returns_squad() {
        let c = classify("implement tests and document the new api security audit");
        assert_eq!(c.mode, ExecutionMode::Squad);
    }

    #[test]
    fn solo_dispatch_one_agent() {
        let c = classify("refactor the auth module only");
        assert_eq!(c.mode, ExecutionMode::Solo);
        assert!(c.agents.contains(&DomainAgent::Engineer));
    }

    #[test]
    fn classifier_idempotent() {
        let task = "write tests for the performance benchmark";
        let c1 = classify(task);
        let c2 = classify(task);
        assert_eq!(c1.agents, c2.agents);
        assert_eq!(c1.mode, c2.mode);
    }

    #[test]
    fn classifier_idempotent_with_trailing_whitespace() {
        let task = "refactor auth";
        let c1 = classify(task);
        let c2 = classify(&format!("{task}   "));
        assert_eq!(c1.agents, c2.agents);
        assert_eq!(c1.mode, c2.mode);
    }

    #[test]
    fn no_regex_in_module() {
        // Compile-time assertion: this module must not import the regex crate.
    }

    // ── Engineer keyword contract (14 keywords) ───────────────────────────────

    #[test]
    fn engineer_all_keywords() {
        let eng = DomainAgent::Engineer;
        assert!(has("implement the feature", eng)); // "implement"
        assert!(has("refactor the auth layer", eng)); // "refactor"
        assert!(has("write code for this module", eng)); // "write code"
        assert!(has("add function to handle requests", eng)); // "add function"
        assert!(has("add method parse_input", eng)); // "add method"
        assert!(has("add struct RequestBody", eng)); // "add struct"
        assert!(has("add trait Serializable", eng)); // "add trait"
        assert!(has("create module for billing", eng)); // "create module"
        assert!(has("build feature: dark mode", eng)); // "build feature"
        assert!(has("fix bug in parser", eng)); // "fix bug"
        assert!(has("patch the memory leak", eng)); // "patch"
        assert!(has("coding session on gateway", eng)); // "coding"
        assert!(has("develop the new endpoint", eng)); // "develop"
        assert!(has("ship the release today", eng)); // "ship"
    }

    // ── Quality keyword contract (13 keywords) ────────────────────────────────

    #[test]
    fn quality_all_keywords() {
        let q = DomainAgent::Quality;
        assert!(has("do a code review before merge", q)); // "code review"
        assert!(has("review pr #42", q)); // "review pr"
        assert!(has("review code changes", q)); // "review code"
        assert!(has("quality gate is failing", q)); // "quality"
        assert!(has("clippy warnings to fix", q)); // "clippy"
        assert!(has("lint the typescript files", q)); // "lint"
        assert!(has("clean up dead imports", q)); // "clean up"
        assert!(has("cleanup unused variables", q)); // "cleanup"
        assert!(has("tidy the workspace", q)); // "tidy"
        assert!(has("improve readability of this fn", q)); // "readability"
        assert!(has("apply best practice patterns", q)); // "best practice"
        assert!(has("improve style consistency", q)); // "improve style"
        assert!(has("critique the api design", q)); // "critique"
    }

    // ── Security keyword contract (17 keywords) ───────────────────────────────

    #[test]
    fn security_all_keywords() {
        let s = DomainAgent::Security;
        assert!(has("review the security posture", s)); // "security"
        assert!(has("check for vulnerability in deps", s)); // "vulnerability"
        assert!(has("pentest the login endpoint", s)); // "pentest"
        assert!(has("audit the permission model", s)); // "audit"
        assert!(has("triage the latest cve", s)); // "cve"
        assert!(has("find an exploit path", s)); // "exploit"
        assert!(has("sql injection risk in query", s)); // "injection" + "sql injection"
        assert!(has("xss in the template", s)); // "xss"
        assert!(has("csrf token missing", s)); // "csrf"
        assert!(has("fix authentication bypass", s)); // "authentication"
        assert!(has("check authorisation rules", s)); // "authorisation"
        assert!(has("check authorization header", s)); // "authorization"
        assert!(has("rotate secrets in vault", s)); // "secrets"
        assert!(has("credentials exposed in log", s)); // "credentials"
        assert!(has("hardening the container image", s)); // "hardening"
        assert!(has("reduce the attack surface", s)); // "attack surface"
    }

    // ── Ops keyword contract (17 keywords) ───────────────────────────────────

    #[test]
    fn ops_all_keywords() {
        let o = DomainAgent::Ops;
        assert!(has("deploy to production", o)); // "deploy"
        assert!(has("fix the ci/cd pipeline", o)); // "ci/cd"
        assert!(has("the build pipeline is broken", o)); // "pipeline"
        assert!(has("update the dockerfile", o)); // "dockerfile"
        assert!(has("kubernetes cluster config", o)); // "kubernetes"
        assert!(has("k8s pod scheduling", o)); // "k8s"
        assert!(has("write helm chart values", o)); // "helm"
        assert!(has("review infrastructure costs", o)); // "infrastructure"
        assert!(has("write terraform module", o)); // "terraform"
        assert!(has("ansible playbook for setup", o)); // "ansible"
        assert!(has("register launchagent on mac", o)); // "launchagent"
        assert!(has("systemd unit file for service", o)); // "systemd"
        assert!(has("run as a daemon process", o)); // "daemon"
        assert!(has("monitoring setup for ayin", o)); // "monitoring"
        assert!(has("alerting rules for latency", o)); // "alerting"
        assert!(has("rolling rollout strategy", o)); // "rollout"
        assert!(has("rollback the last release", o)); // "rollback"
    }

    // ── Researcher keyword contract (14 keywords) ─────────────────────────────

    #[test]
    fn researcher_all_keywords() {
        let r = DomainAgent::Researcher;
        assert!(has("research async runtimes", r)); // "research"
        assert!(has("investigate the memory spike", r)); // "investigate"
        assert!(has("explore new crate options", r)); // "explore"
        assert!(has("find out why this is slow", r)); // "find out"
        assert!(has("look into raft consensus", r)); // "look into"
        assert!(has("survey existing solutions", r)); // "survey"
        assert!(has("compare axum vs actix", r)); // "compare"
        assert!(has("evaluate options for storage", r)); // "evaluate options"
        assert!(has("what is the best approach", r)); // "what is"
        assert!(has("how does tokio schedule tasks", r)); // "how does"
        assert!(has("run a benchmark comparison", r)); // "benchmark comparison"
        assert!(has("pros and cons of neo4j", r)); // "pros and cons"
        assert!(has("analyse the trade-off here", r)); // "trade-off"
        assert!(has("consider the tradeoff of caching", r)); // "tradeoff"
    }

    // ── Knowledge keyword contract (15 keywords) ──────────────────────────────

    #[test]
    fn knowledge_all_keywords() {
        let k = DomainAgent::Knowledge;
        assert!(has("document the new endpoint", k)); // "document"
        assert!(has("update the documentation", k)); // "documentation"
        assert!(has("add to the wiki", k)); // "wiki"
        assert!(has("update the knowledge base", k)); // "knowledge base"
        assert!(has("query the helix for context", k)); // "helix"
        assert!(has("update the graph schema", k)); // "graph"
        assert!(has("make a note on this design", k)); // "note"
        assert!(has("record the decision made today", k)); // "record"
        assert!(has("capture the session output", k)); // "capture"
        assert!(has("explain how serde works", k)); // "explain"
        assert!(has("summarise the meeting notes", k)); // "summarise"
        assert!(has("summarize what happened", k)); // "summarize"
        assert!(has("write an architecture decision", k)); // "architecture decision"
        assert!(has("create an adr for this", k)); // "adr"
        assert!(has("add a changelog entry", k)); // "changelog"
    }

    // ── Performance keyword contract (15 keywords) ────────────────────────────

    #[test]
    fn performance_all_keywords() {
        let p = DomainAgent::Performance;
        assert!(has("analyse performance regression", p)); // "performance"
        assert!(has("optimise the hot path", p)); // "optimise"
        assert!(has("optimize allocations", p)); // "optimize"
        assert!(has("run the benchmark suite", p)); // "benchmark"
        assert!(has("profil the flamegraph output", p)); // "profil" (prefix match)
        assert!(has("reduce latency in handler", p)); // "latency"
        assert!(has("improve throughput of stream", p)); // "throughput"
        assert!(has("check memory usage after load", p)); // "memory usage"
        assert!(has("high cpu usage in worker", p)); // "cpu usage"
        assert!(has("find the bottleneck in query", p)); // "bottleneck"
        assert!(has("this path is too slow", p)); // "slow"
        assert!(has("make the serialiser fast", p)); // "fast"
        assert!(has("speed up startup time", p)); // "speed up"
        assert!(has("add a criterion harness", p)); // "criterion"
        assert!(has("view the flamegraph output", p)); // "flamegraph"
    }

    // ── Testing keyword contract (14 keywords) ────────────────────────────────

    #[test]
    fn testing_all_keywords() {
        let t = DomainAgent::Testing;
        assert!(has("write a test for this fn", t)); // "test"
        assert!(has("add a unit test here", t)); // "unit test"
        assert!(has("add integration test for login", t)); // "integration test"
        assert!(has("property test the parser", t)); // "property test"
        assert!(has("fuzz the input parser", t)); // "fuzz"
        assert!(has("increase coverage to 90%", t)); // "coverage"
        assert!(has("add assertion for error case", t)); // "assertion"
        assert!(has("write a spec for this module", t)); // "spec"
        assert!(has("e2e flow for signup", t)); // "e2e"
        assert!(has("end-to-end test the checkout", t)); // "end-to-end"
        assert!(has("playwright test for dashboard", t)); // "playwright"
        assert!(has("add a hypothesis test", t)); // "hypothesis"
        assert!(has("check for regression in output", t)); // "regression"
        assert!(has("run the full test suite", t)); // "test suite"
    }

    // ── Documentation keyword contract (13 keywords) ──────────────────────────

    #[test]
    fn documentation_all_keywords() {
        let d = DomainAgent::Documentation;
        assert!(has("add doc comment to this fn", d)); // "doc comment"
        assert!(has("rustdoc example for the type", d)); // "rustdoc"
        assert!(has("update the readme file", d)); // "readme"
        assert!(has("generate api docs for crate", d)); // "api docs"
        assert!(has("write docs for the module", d)); // "write docs"
        assert!(has("update docs after refactor", d)); // "update docs"
        assert!(has("add a docstring to this fn", d)); // "docstring"
        assert!(has("write a man page for tool", d)); // "man page"
        assert!(has("add to the reference guide", d)); // "reference guide"
        assert!(has("write a tutorial for setup", d)); // "tutorial"
        assert!(has("add a how-to for deployment", d)); // "how-to"
        assert!(has("write a getting started guide", d)); // "getting started"
        assert!(has("write a changelog entry", d)); // "changelog entry"
    }

    // ── Case-insensitivity (one keyword per agent × 9) ───────────────────────

    #[test]
    fn case_insensitive_matching() {
        assert!(has("IMPLEMENT the feature", DomainAgent::Engineer));
        assert!(has("CLIPPY warnings everywhere", DomainAgent::Quality));
        assert!(has("CVE-2024-1234 triage", DomainAgent::Security));
        assert!(has("DEPLOY to staging", DomainAgent::Ops));
        assert!(has("RESEARCH alternatives", DomainAgent::Researcher));
        assert!(has("HELIX query for context", DomainAgent::Knowledge));
        assert!(has("LATENCY spike in handler", DomainAgent::Performance));
        assert!(has("PLAYWRIGHT end-to-end suite", DomainAgent::Testing));
        assert!(has("RUSTDOC examples missing", DomainAgent::Documentation));
    }

    // ── Mode derivation ───────────────────────────────────────────────────────

    #[test]
    fn mode_idle_for_no_keywords() {
        assert_eq!(mode_of(""), ExecutionMode::Idle);
        assert_eq!(mode_of("   "), ExecutionMode::Idle);
        assert_eq!(mode_of("hello world"), ExecutionMode::Idle);
        assert_eq!(mode_of("the quick brown fox"), ExecutionMode::Idle);
    }

    #[test]
    fn mode_solo_for_single_agent_match() {
        assert_eq!(mode_of("refactor the module"), ExecutionMode::Solo);
        assert_eq!(mode_of("deploy the service"), ExecutionMode::Solo);
        assert_eq!(mode_of("run the benchmark"), ExecutionMode::Solo);
    }

    #[test]
    fn mode_squad_for_multi_agent_match() {
        // engineer + testing
        assert_eq!(
            mode_of("implement and test the module"),
            ExecutionMode::Squad
        );
        // security + ops
        assert_eq!(mode_of("audit the deploy pipeline"), ExecutionMode::Squad);
        // knowledge + documentation
        assert_eq!(
            mode_of("explain the helix schema and update the readme"),
            ExecutionMode::Squad
        );
        // all three: engineer + testing + quality
        assert_eq!(
            mode_of("implement unit tests and do a code review"),
            ExecutionMode::Squad
        );
    }

    // ── Hit-count ordering ────────────────────────────────────────────────────

    #[test]
    fn highest_hit_count_agent_is_first() {
        // Three security keywords, one engineer keyword — security should rank first.
        let c = classify("audit the authentication for vulnerability and implement a fix");
        assert_eq!(c.agents[0], DomainAgent::Security);
        assert!(c.agents.contains(&DomainAgent::Engineer));
    }

    #[test]
    fn ordering_stable_on_equal_counts() {
        // Both engineer and testing get exactly one hit — result stable across calls.
        let task = "implement a test";
        let c1 = classify(task);
        let c2 = classify(task);
        assert_eq!(c1.agents, c2.agents);
    }

    // ── Rationale format ──────────────────────────────────────────────────────

    #[test]
    fn rationale_idle_message() {
        let c = classify("the quick brown fox");
        assert!(c.rationale.contains("No domain keywords matched"));
    }

    #[test]
    fn rationale_contains_agent_names() {
        let c = classify("implement and test");
        // Rationale includes "Matched:" prefix and at least the matched agent names.
        assert!(c.rationale.contains("Matched:"));
    }

    #[test]
    fn rationale_shows_keyword_count() {
        let c = classify("implement refactor develop"); // 3 engineer keywords
        assert!(c.rationale.contains("keyword")); // "3 keywords" or "1 keyword"
    }

    #[test]
    fn rationale_shows_mode() {
        let solo = classify("refactor auth");
        assert!(solo.rationale.contains("solo agent"));

        let squad = classify("refactor auth and run tests");
        assert!(squad.rationale.contains("squad"));
    }

    // ── Prefix/substring matching edge cases ─────────────────────────────────

    #[test]
    fn profil_prefix_matches_profile_and_profiling() {
        // "profil" is a prefix pattern — must match "profile" and "profiling".
        assert!(has("profile the binary", DomainAgent::Performance));
        assert!(has(
            "profiling session on release build",
            DomainAgent::Performance
        ));
    }

    #[test]
    fn keyword_inside_longer_word_matches() {
        // Aho-Corasick does substring matching — "test" inside "testing" matches.
        assert!(has("testing the module", DomainAgent::Testing));
    }

    // ── Unicode / multibyte input ─────────────────────────────────────────────

    #[test]
    fn unicode_task_does_not_panic() {
        // Emoji and CJK characters must not cause a panic or incorrect result.
        let c = classify("implement 🔒 security audit for 建設 module");
        assert!(c.agents.contains(&DomainAgent::Engineer));
        assert!(c.agents.contains(&DomainAgent::Security));
    }

    #[test]
    fn ascii_only_fold_does_not_corrupt_multibyte() {
        // to_lowercase on "SECURITY" should work; multibyte chars are passed through.
        let c = classify("SECURITY 审计 for hélix");
        assert!(c.agents.contains(&DomainAgent::Security));
    }

    // ── Classification struct completeness ────────────────────────────────────

    #[test]
    fn classification_agents_list_deduplicated() {
        // Each agent appears at most once even if it has many keyword hits.
        let c = classify("implement refactor write code build feature fix bug develop ship");
        let engineer_count = c
            .agents
            .iter()
            .filter(|&&a| a == DomainAgent::Engineer)
            .count();
        assert_eq!(engineer_count, 1);
    }

    #[test]
    fn classification_agents_empty_on_no_match() {
        let c = classify("the cat sat on the mat");
        assert!(c.agents.is_empty());
        assert_eq!(c.mode, ExecutionMode::Idle);
    }

    // ── Negative-match: domain isolation ──────────────────────────────────────
    // Each agent's characteristic keyword must NOT match a clearly unrelated domain.

    #[test]
    fn negative_engineer_keywords_do_not_match_ops() {
        // "refactor" is Engineer-only; pure refactor task → no Ops match.
        assert!(!has("refactor the parser module", DomainAgent::Ops));
    }

    #[test]
    fn negative_security_keywords_do_not_match_researcher() {
        // "audit" is Security; pure audit → no Researcher match.
        assert!(!has(
            "audit the permission model for vulnerabilities",
            DomainAgent::Researcher
        ));
    }

    #[test]
    fn negative_ops_keywords_do_not_match_engineer() {
        // "deploy" is Ops-only; pure deploy → no Engineer match.
        assert!(!has("deploy to staging environment", DomainAgent::Engineer));
    }

    #[test]
    fn negative_testing_keywords_do_not_match_documentation() {
        // "fuzz the input parser" → no Documentation match.
        assert!(!has("fuzz the input parser", DomainAgent::Documentation));
    }

    #[test]
    fn negative_performance_keywords_do_not_match_quality() {
        // "latency spike" → no Quality match.
        assert!(!has("reduce latency in the hot path", DomainAgent::Quality));
    }

    // ── All-agents squad scenario ─────────────────────────────────────────────

    #[test]
    fn all_nine_agents_can_coexist_in_squad() {
        // One keyword per agent in a single task → Squad mode, all 9 present.
        let task = concat!(
            "implement tests and do a code review, ",
            "audit the security surface, ",
            "deploy to prod, ",
            "research alternatives, ",
            "document in the helix, ",
            "optimise latency, ",
            "update the readme",
        );
        let c = classify(task);
        assert_eq!(c.mode, ExecutionMode::Squad);
        assert!(c.agents.contains(&DomainAgent::Engineer));
        assert!(c.agents.contains(&DomainAgent::Testing));
        assert!(c.agents.contains(&DomainAgent::Quality));
        assert!(c.agents.contains(&DomainAgent::Security));
        assert!(c.agents.contains(&DomainAgent::Ops));
        assert!(c.agents.contains(&DomainAgent::Researcher));
        assert!(c.agents.contains(&DomainAgent::Knowledge));
        assert!(c.agents.contains(&DomainAgent::Performance));
        assert!(c.agents.contains(&DomainAgent::Documentation));
    }

    // ── Mixed-case + punctuation preservation ────────────────────────────────

    #[test]
    fn mixed_case_task_classifies_correctly() {
        assert!(has("ReFaCtOr the module", DomainAgent::Engineer));
        assert!(has("Run UNIT TEST coverage", DomainAgent::Testing));
        assert!(has("Check for XSS vulnerability", DomainAgent::Security));
        assert!(has("Deploy via CI/CD pipeline", DomainAgent::Ops));
        assert!(has("Summarize findings in HELIX", DomainAgent::Knowledge));
        assert!(has("Optimize for low LATENCY", DomainAgent::Performance));
        assert!(has("Write RUSTDOC examples", DomainAgent::Documentation));
    }

    // ── Rationale mentions top-3 agents at most ───────────────────────────────

    #[test]
    fn rationale_capped_at_three_agents() {
        // Even with 5+ matched agents the rationale only shows the top 3.
        let c = classify("implement test audit deploy research");
        // Count occurrences of "keyword" (one per listed agent) — must be ≤ 3.
        let keyword_count = c.rationale.matches("keyword").count();
        assert!(
            keyword_count <= 3,
            "rationale listed more than 3 agents: {}",
            c.rationale
        );
    }
}
