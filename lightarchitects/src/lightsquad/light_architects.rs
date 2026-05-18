//! 10 LightArchitects — gate-dimension domain specialists.
//!
//! Operator override (Canon XV) of canonical IRONCLAW PDF spec which declares 8 LightArchitects
//! (auth/database/api/security/testing/orchestrator/context/supervisor).
//!
//! Replacement: 10 specialists, one per Gatekeeper Registry gate dimension
//! `[A+S+Q+C+O+P+K+D+T+R]`. 1:1 routing to existing siblings via
//! `crate::squad_registry`, with fallback rules for the 3 dimensions without
//! dedicated sibling owners (architecture/operations/documentation).
//!
//! | Gate | Dimension       | LightArchitect | Sibling target          |
//! |------|-----------------|----------------|-------------------------|
//! | [A]  | Architecture    | architect      | CORSO (primary), SOUL   |
//! | [S]  | Security        | security       | SERAPH                  |
//! | [Q]  | Quality         | quality        | CORSO                   |
//! | [C]  | Canon           | canon          | LÆX                     |
//! | [O]  | Operations      | operations     | EVA (primary), AYIN     |
//! | [P]  | Performance     | performance    | EVA + AYIN              |
//! | [K]  | Knowledge       | knowledge      | SOUL                    |
//! | [D]  | Documentation   | documentation  | SOUL (primary), EVA     |
//! | [T]  | Testing         | testing        | CORSO                   |
//! | [R]  | Research + Risk | research       | QUANTUM                 |
//!
//! Operating modes (per canonical PDF):
//! - Pre-execution sign-off (before /BUILD, after Opus generates plan)
//! - On-demand consultation (during execution, on HITL escalation)
//! - Phase transition review (before each new phase starts)
//!
//! Phase 4 implementation — routing table + sibling dispatch via `crate::squad_registry`.
//!
//! Phase 1 stub — registry declared in Phase 4.
