//! LASDLC — Light Architects Software Development Lifecycle types.
//!
//! Three orthogonal axes:
//! - Execution Phases (sequential): Plan -> Research -> Implement -> Harden -> Verify -> Ship -> Learn
//! - Quality Dimensions (parallel): Architecture, Security, Quality, Performance, Testing, Documentation, Operations
//! - Agent Topology (concurrent): file-ownership partitioned agents within each phase
//!
//! Spec: `helix/user/standards/canon/lasdlc-spec.md`
//! Template: `helix/corso/builds/LASDLC-TEMPLATE-v1.yaml`

use serde::{Deserialize, Serialize};

/// LASDLC execution phase — sequential work order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum ExecutionPhase {
    /// Strategic planning — scope, constraints, file ownership map.
    Plan,
    /// Investigation — prior art, library docs, codebase orientation.
    Research,
    /// Code authoring — file-partitioned parallel agents.
    Implement,
    /// Security hardening, error-path coverage, fuzzing.
    Harden,
    /// Integration tests, quality gates, regression checks.
    Verify,
    /// Release packaging, deployment, changelog.
    Ship,
    /// Retrospective — lessons learned, helix enrichment.
    Learn,
}

/// Build complexity tier — determines active phase set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum BuildTier {
    /// Quick fix or single-file change — 4 phases.
    Small,
    /// Feature or multi-file change — 6 phases.
    Medium,
    /// Major feature, cross-crate refactor, or new subsystem — all 7 phases.
    Large,
}

impl BuildTier {
    /// Return the execution phases active for this tier.
    #[must_use]
    pub fn phases(self) -> &'static [ExecutionPhase] {
        match self {
            Self::Small => &[
                ExecutionPhase::Plan,
                ExecutionPhase::Implement,
                ExecutionPhase::Verify,
                ExecutionPhase::Ship,
            ],
            Self::Medium => &[
                ExecutionPhase::Plan,
                ExecutionPhase::Research,
                ExecutionPhase::Implement,
                ExecutionPhase::Verify,
                ExecutionPhase::Ship,
                ExecutionPhase::Learn,
            ],
            Self::Large => &[
                ExecutionPhase::Plan,
                ExecutionPhase::Research,
                ExecutionPhase::Implement,
                ExecutionPhase::Harden,
                ExecutionPhase::Verify,
                ExecutionPhase::Ship,
                ExecutionPhase::Learn,
            ],
        }
    }
}

/// Quality dimension — checked in parallel at every phase boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum QualityDimension {
    /// Structural integrity — module boundaries, dependency direction, API surface.
    Architecture,
    /// Threat model, input validation, secret management.
    Security,
    /// Code style, complexity limits, linting, formatting.
    Quality,
    /// Latency budgets, allocation profiles, benchmark regression.
    Performance,
    /// Coverage targets, property tests, integration harness.
    Testing,
    /// Doc comments, changelogs, onboarding guides.
    Documentation,
    /// Deploy scripts, monitoring, rollback procedures.
    Operations,
}

impl QualityDimension {
    /// Single-character abbreviation for compact display.
    #[must_use]
    pub fn abbrev(self) -> char {
        match self {
            Self::Architecture => 'A',
            Self::Security => 'S',
            Self::Quality => 'Q',
            Self::Performance => 'P',
            Self::Testing => 'T',
            Self::Documentation => 'D',
            Self::Operations => 'O',
        }
    }

    /// Whether this dimension blocks phase transitions.
    ///
    /// All dimensions are blocking except `Documentation`.
    #[must_use]
    pub fn is_blocking(self) -> bool {
        !matches!(self, Self::Documentation)
    }
}

/// All 7 quality dimensions.
pub const ALL_DIMENSIONS: [QualityDimension; 7] = [
    QualityDimension::Architecture,
    QualityDimension::Security,
    QualityDimension::Quality,
    QualityDimension::Performance,
    QualityDimension::Testing,
    QualityDimension::Documentation,
    QualityDimension::Operations,
];
