//! Hybrid memory module — hot (turnlog active sessions) + cold (helix entries on disk).
//!
//! Phase 9 SOUL integration. Hot and cold are the two tiers of the SOUL vault
//! memory model: hot lives in the ephemeral transactional log ([`lightarchitects::turnlog`]),
//! cold lives in the helix filesystem at `~/lightarchitects/soul/helix/{sibling}/entries/`.
//!
//! The HTTP surface in [`crate::events::soul_routes`] (Phase 9.5) consumes
//! [`hot::snapshot_hot`] and the cold reader to serve the Svelte webshell's
//! `MemoryDrawer` component.
//!
//! # Design boundary
//!
//! Hot memos are projections of [`lightarchitects::turnlog::entry::TurnEntry`] —
//! they drop the HMAC chain fields to keep the wire payload small. The full
//! `TurnEntry` is still readable from the NDJSON file when chain verification
//! is needed (not needed for UI display).

pub mod backfill;
pub mod cold;
pub mod compaction;
pub mod convergence;
pub mod embedder;
pub mod enrich;
pub mod frontmatter;
pub mod hot;
pub mod persistence;
pub mod promoter_bridge;
pub mod types;

pub use enrich::{EnrichError, enrich, enrich_async};
pub use frontmatter::{FrontMatterFields, enrich_file, parse};
pub use promoter_bridge::BroadcastingPromoter;
pub use types::{ContextMemo, EnrichedEntry, MemoryTier, PromotionEvent};
