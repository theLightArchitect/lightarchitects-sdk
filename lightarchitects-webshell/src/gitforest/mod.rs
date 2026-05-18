//! GitForest — live operational branch-hierarchy map.
//!
//! Exposes the domain types used by the three backend endpoints
//! (`/api/gitforest/topology`, `/api/gitforest/live`, `/api/gitforest/node/:id`)
//! and by `WebEvent::GitForestUpdate`.

pub mod broadcaster;
pub mod routes;
pub mod types;

pub use types::{
    BranchKind, BranchLifecycle, BranchNode, BranchOverlayMeta, BuildProgress, CiStatus, HitlState,
    WorktreeAssignment, WorktreeState,
};
