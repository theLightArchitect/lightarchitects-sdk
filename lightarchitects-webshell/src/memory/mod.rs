//! Hybrid memory module — hot (Neo4j `:HotMemo` tier) + cold (helix entries on disk).
//!
//! Phase 9 SOUL integration. Hot and cold are the two tiers of the SOUL vault
//! memory model: hot lives in the Neo4j `:HotMemo` graph tier (Phase 18c),
//! cold lives in the helix filesystem at `~/lightarchitects/soul/helix/{sibling}/entries/`.
//!
//! The HTTP surface in [`crate::events::soul_routes`] (Phase 9.5) serves the
//! Svelte webshell's `MemoryDrawer` component. As of Phase 18c Step 3 the hot
//! path reads exclusively from Neo4j; [`hot::snapshot_hot`] remains for tests
//! and NDJSON archive inspection but is no longer on the hot serving path.
//!
//! # Design boundary
//!
//! Hot memos are projections of [`lightarchitects::helix::types::HotMemo`] —
//! HMAC chain fields (`hmac_prev`, `hmac_self`) are present on the Neo4j node
//! but dropped at the `ContextMemo` projection layer to keep the wire payload
//! small. Full chain verification uses the `:NEXT` graph walk (Phase 18c Step 2).

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
