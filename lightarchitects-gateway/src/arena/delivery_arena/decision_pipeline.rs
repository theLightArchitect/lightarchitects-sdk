//! 4-layer decision pipeline for autonomous HITL resolution.
//!
//! Phase 4 implementation:
//! - `DecisionPipeline { canon_cache, northstar, lightarchitect_router }`
//! - `resolve(msg: HitlRequest) -> DecisionVerdict`
//! - L1 Canon check: Sonnet with prompt-cached canon system prompt (§11.3a)
//! - L2 Northstar check: Pillar + component-Northstar alignment gate
//! - L3 LightArchitect routing: domain → sibling, invokes /CANON-DECIDE skill
//! - L4 User escalation: appends to `decisions.md`; blocks wave until resolved
//!
//! Target: 95% of HITL requests resolved at L1–L3 without L4 escalation.
