//! `events::soul` — split-out home for `soul_routes.rs` handlers (task #51).
//!
//! Aegis Wave 1 partial split. `soul_routes.rs` was 1604 LOC — over the
//! Cookbook §11 600-LOC soft cap and well over §11 60-LOC function limit.
//! Engineer F-1 + quality MR-4 + ops Q1 all flagged it as a god-file.
//!
//! Migration strategy (file-by-file, shim-preserving): each split keeps
//! `events::soul_routes::*` resolving via re-exports, so `server::mod.rs`
//! (mantis-locked) does NOT need to change. New imports can use the
//! split paths directly (`events::soul::convergences::convergences_handler`)
//! while old paths keep working.
//!
//! Status (2026-04-30):
//!   ✅ convergences  — split into events/soul/convergences.rs
//!   ⏳ search        — pending (largest, 6 helper fns + handler ~470 LOC)
//!   ⏳ entry         — pending (~25 LOC)
//!   ⏳ memory        — pending (hot+cold ~95 LOC)
//!   ⏳ health        — pending (health + parity ~180 LOC)
//!   ⏳ relationships — pending (~50 LOC)
//!   ⏳ edges         — pending (~90 LOC)
//!   ⏳ compaction    — pending (preview + apply ~125 LOC)
//!
//! Remaining splits deferred to a future session per manifest's "high
//! regression risk; needs dedicated session" classification of #51.

pub mod convergences;
