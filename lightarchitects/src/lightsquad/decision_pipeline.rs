//! 4-layer decision pipeline — Canon → Northstar → LightArchitect → User.
//!
//! Per canonical IRONCLAW PDF spec (Decision Pipeline §):
//!
//! 1. **Canon Check** — Does any canon doc directly address this decision?
//!    If yes, Supervisor decides and resumes without further escalation.
//! 2. **Northstar Check** — Does this move toward or away from the ultimate goal?
//!    Canon-compliant but Northstar-violating decisions are still blocked.
//! 3. **LightArchitect Consultation** — Supervisor identifies relevant domain,
//!    spawns appropriate LightArchitect (`crate::lightsquad::light_architects`).
//! 4. **User Escalation** — Genuinely novel decisions only. Target: 2-5 escalations
//!    across a full multi-build program. Security + irreversible migrations
//!    always escalate regardless of canon coverage.
//!
//! Phase 4 implementation — uses `crate::squad_registry` for LA routing,
//! `crate::turnlog` for decision-log HMAC chaining, `crate::platform::PlatformClient`
//! for canon resolution.
//!
//! Phase 1 stub — flow declared in Phase 4.
