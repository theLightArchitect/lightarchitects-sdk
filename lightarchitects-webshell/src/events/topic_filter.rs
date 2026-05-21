//! Server-side topic pattern matching for SSE streams.
//!
//! ## Pattern syntax
//!
//! Topics are dot-separated identifiers following the `v1.<domain>.<entity>.<event>`
//! taxonomy (see `docs/research/topic-taxonomy.md`).
//!
//! | Token | Meaning |
//! |-------|---------|
//! | `*`   | Matches exactly one segment (e.g. `v1.copilot.*`) |
//! | `>`   | Matches one or more trailing segments — must be the last token |
//! | other | Literal match — segment must equal this string exactly |
//!
//! ## Examples
//!
//! ```
//! use lightarchitects_webshell::events::TopicFilter;
//!
//! let f = TopicFilter::parse("v1.copilot.*").unwrap();
//! assert!(f.matches("v1.copilot.activity"));
//! assert!(f.matches("v1.copilot.response"));
//! assert!(!f.matches("v1.conductor.task"));
//!
//! let f = TopicFilter::parse("v1.>").unwrap();
//! assert!(f.matches("v1.copilot.activity"));
//! assert!(f.matches("v1.conductor.task.started"));
//! ```

/// A compiled topic-filter pattern.
///
/// Constructed via [`TopicFilter::parse`]; immutable after construction.
/// Cheap to clone — the internal representation is a small `Vec<Segment>`.
#[derive(Debug, Clone)]
pub struct TopicFilter {
    segments: Vec<Segment>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Segment {
    Literal(String),
    /// `*` — matches exactly one segment, any value.
    Single,
    /// `>` — matches one or more trailing segments. Only valid at the end.
    Multi,
}

impl TopicFilter {
    /// Parse a topic pattern into a [`TopicFilter`].
    ///
    /// Returns `Err` when the pattern is empty or `>` appears in a
    /// non-terminal position.
    ///
    /// # Errors
    ///
    /// Returns an error string describing the parse failure.
    pub fn parse(pattern: &str) -> Result<Self, String> {
        if pattern.is_empty() {
            return Err("topic pattern must not be empty".to_owned());
        }
        let mut segments: Vec<Segment> = Vec::new();
        let mut saw_multi = false;
        for part in pattern.split('.') {
            if saw_multi {
                return Err("`>` wildcard must be the last segment".to_owned());
            }
            match part {
                "*" => segments.push(Segment::Single),
                ">" => {
                    segments.push(Segment::Multi);
                    saw_multi = true;
                }
                s => segments.push(Segment::Literal(s.to_owned())),
            }
        }
        Ok(Self { segments })
    }

    /// Returns `true` when `topic` matches this filter pattern.
    pub fn matches(&self, topic: &str) -> bool {
        let parts: Vec<&str> = topic.split('.').collect();
        self.match_from(&parts, 0, 0)
    }

    fn match_from(&self, parts: &[&str], pi: usize, si: usize) -> bool {
        let segs = &self.segments;
        if si == segs.len() {
            return pi == parts.len();
        }
        match &segs[si] {
            Segment::Multi => pi < parts.len(),
            Segment::Single => pi < parts.len() && self.match_from(parts, pi + 1, si + 1),
            Segment::Literal(lit) => {
                pi < parts.len()
                    && parts[pi] == lit.as_str()
                    && self.match_from(parts, pi + 1, si + 1)
            }
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    // ── parse errors ──────────────────────────────────────────────────────────

    #[test]
    fn parse_empty_is_error() {
        assert!(TopicFilter::parse("").is_err());
    }

    #[test]
    fn parse_mid_multi_is_error() {
        assert!(TopicFilter::parse("v1.>.copilot").is_err());
    }

    // ── literal matching ──────────────────────────────────────────────────────

    #[test]
    fn literal_exact_match() {
        let f = TopicFilter::parse("v1.copilot.activity").unwrap();
        assert!(f.matches("v1.copilot.activity"));
    }

    #[test]
    fn literal_no_partial_match() {
        let f = TopicFilter::parse("v1.copilot.activity").unwrap();
        assert!(!f.matches("v1.copilot"));
        assert!(!f.matches("v1.copilot.activity.extra"));
    }

    // ── single-segment wildcard (*) ───────────────────────────────────────────

    #[test]
    fn star_matches_one_segment() {
        let f = TopicFilter::parse("v1.copilot.*").unwrap();
        assert!(f.matches("v1.copilot.activity"));
        assert!(f.matches("v1.copilot.response"));
        assert!(!f.matches("v1.copilot.activity.extra"));
        assert!(!f.matches("v1.copilot"));
        assert!(!f.matches("v1.conductor.task"));
    }

    #[test]
    fn star_mid_pattern() {
        let f = TopicFilter::parse("v1.*.activity").unwrap();
        assert!(f.matches("v1.copilot.activity"));
        assert!(f.matches("v1.conductor.activity"));
        assert!(!f.matches("v1.copilot.response"));
    }

    // ── multi-segment wildcard (>) ────────────────────────────────────────────

    #[test]
    fn multi_matches_tail() {
        let f = TopicFilter::parse("v1.>").unwrap();
        assert!(f.matches("v1.copilot.activity"));
        assert!(f.matches("v1.conductor.task.started"));
        assert!(f.matches("v1.a.b.c.d.e"));
        assert!(!f.matches("v2.copilot.activity")); // wrong version
    }

    #[test]
    fn multi_requires_at_least_one_segment() {
        let f = TopicFilter::parse("v1.copilot.>").unwrap();
        assert!(f.matches("v1.copilot.activity"));
        assert!(!f.matches("v1.copilot")); // > needs ≥1 trailing segment
    }

    #[test]
    fn multi_terminal_position() {
        let f = TopicFilter::parse("v1.copilot.>").unwrap();
        assert!(f.matches("v1.copilot.a.b.c"));
    }

    // ── parity matrix — algorithm coverage ───────────────────────────────────
    // Tests the wildcard algorithm with a variety of pattern + topic strings.
    // NOTE: these topic strings are *arbitrary algorithm-coverage examples*, NOT
    // the actual production taxonomy from `topic_for()` in envelope.rs. For real
    // taxonomy, see `actual_taxonomy_correctness` below.

    #[test]
    #[allow(clippy::panic)]
    fn parity_matrix_algorithm_coverage() {
        struct Case {
            pattern: &'static str,
            should_match: &'static [&'static str],
            should_not: &'static [&'static str],
        }

        let cases = [
            Case {
                pattern: "v1.copilot.*",
                should_match: &["v1.copilot.activity", "v1.copilot.response"],
                should_not: &["v1.conductor.task", "v1.memory.helix.entry"],
            },
            Case {
                pattern: "v1.conductor.*",
                should_match: &[
                    "v1.conductor.task",
                    "v1.conductor.tick",
                    "v1.conductor.worker_slot",
                ],
                should_not: &["v1.copilot.activity", "v1.conductor.task.extra"],
            },
            Case {
                pattern: "v1.conductor.>",
                should_match: &[
                    "v1.conductor.task",
                    "v1.conductor.tick",
                    "v1.conductor.worker_slot",
                ],
                should_not: &["v1.copilot.activity", "v1.memory.helix.entry"],
            },
            Case {
                pattern: "v1.memory.>",
                should_match: &[
                    "v1.memory.helix.entry",
                    "v1.memory.soul.promotion",
                    "v1.memory.strand.convergence",
                ],
                should_not: &["v1.copilot.activity", "v1.agent.status"],
            },
            Case {
                pattern: "v1.supervisor.*",
                should_match: &["v1.supervisor.update", "v1.supervisor.escalation"],
                should_not: &["v1.conductor.task", "v1.copilot.activity"],
            },
            Case {
                pattern: "v1.observability.>",
                should_match: &["v1.observability.ayin.span", "v1.observability.ayin.status"],
                should_not: &["v1.copilot.activity", "v1.agent.status"],
            },
            Case {
                pattern: "v1.>",
                should_match: &[
                    "v1.copilot.activity",
                    "v1.conductor.task",
                    "v1.conductor.tick",
                    "v1.conductor.worker_slot",
                    "v1.git.worktree.update",
                    "v1.memory.helix.entry",
                    "v1.memory.soul.promotion",
                    "v1.supervisor.update",
                    "v1.observability.ayin.span",
                    "v1.platform.control",
                ],
                should_not: &["v2.copilot.activity", "v3.x.y"],
            },
        ];

        for case in &cases {
            let f = TopicFilter::parse(case.pattern)
                .unwrap_or_else(|e| panic!("parse failed for {:?}: {e}", case.pattern));
            for &topic in case.should_match {
                assert!(
                    f.matches(topic),
                    "pattern {:?} should match {:?}",
                    case.pattern,
                    topic,
                );
            }
            for &topic in case.should_not {
                assert!(
                    !f.matches(topic),
                    "pattern {:?} should NOT match {:?}",
                    case.pattern,
                    topic,
                );
            }
        }
    }

    // ── actual production taxonomy (from topic_for() in envelope.rs) ──────────
    // These are the real topic strings the gateway emits. Any change to topic_for()
    // must be reflected here — these tests anchor the TS subscribeByTopic() callers.

    #[test]
    fn actual_taxonomy_correctness() {
        // AYIN status variants
        assert!(
            TopicFilter::parse("v1.agent.ayin.*")
                .unwrap()
                .matches("v1.agent.ayin.connected")
        );
        assert!(
            TopicFilter::parse("v1.agent.ayin.*")
                .unwrap()
                .matches("v1.agent.ayin.disconnected")
        );
        assert!(
            TopicFilter::parse("v1.agent.ayin.*")
                .unwrap()
                .matches("v1.agent.ayin.reconnecting")
        );

        // Worktree / gitforest
        assert!(
            TopicFilter::parse("v1.worktree.update")
                .unwrap()
                .matches("v1.worktree.update")
        );
        assert!(
            !TopicFilter::parse("v1.worktree.update")
                .unwrap()
                .matches("v1.worktree.create")
        );

        // Conductor
        assert!(
            TopicFilter::parse("v1.conductor.escalation")
                .unwrap()
                .matches("v1.conductor.escalation")
        );
        assert!(
            TopicFilter::parse("v1.conductor.*")
                .unwrap()
                .matches("v1.conductor.escalation")
        );
        assert!(
            TopicFilter::parse("v1.conductor.*")
                .unwrap()
                .matches("v1.conductor.tick")
        );
        assert!(
            !TopicFilter::parse("v1.conductor.*")
                .unwrap()
                .matches("v1.conductor.slot.gauge")
        );

        // Helix / SOUL
        assert!(
            TopicFilter::parse("v1.helix.>")
                .unwrap()
                .matches("v1.helix.entry.changed")
        );
        assert!(
            TopicFilter::parse("v1.helix.>")
                .unwrap()
                .matches("v1.helix.entry.promoted")
        );

        // Copilot (actual prefix is v1.agent.claude, not v1.copilot)
        assert!(
            TopicFilter::parse("v1.agent.claude.*")
                .unwrap()
                .matches("v1.agent.claude.activity")
        );
        assert!(
            TopicFilter::parse("v1.agent.claude.*")
                .unwrap()
                .matches("v1.agent.claude.response")
        );
        assert!(
            !TopicFilter::parse("v1.agent.claude.*")
                .unwrap()
                .matches("v1.agent.ayin.connected")
        );

        // Build
        assert!(
            TopicFilter::parse("v1.build.>")
                .unwrap()
                .matches("v1.build.update")
        );
        assert!(
            TopicFilter::parse("v1.build.>")
                .unwrap()
                .matches("v1.build.supervisor.update")
        );
        assert!(
            TopicFilter::parse("v1.build.>")
                .unwrap()
                .matches("v1.build.pillar.update")
        );

        // Catch-all v1
        let all = TopicFilter::parse("v1.>").unwrap();
        for topic in &[
            "v1.agent.ayin.connected",
            "v1.worktree.update",
            "v1.conductor.escalation",
            "v1.helix.entry.changed",
            "v1.agent.claude.activity",
            "v1.build.update",
        ] {
            assert!(all.matches(topic), "v1.> should match {topic}");
        }
    }
}
