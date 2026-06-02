//! Vertex AI Search integration — enterprise document search over the LA security corpus.
//!
//! Enable with: `features = ["vertex-search"]`
//!
//! Auth: Application Default Credentials via `gcloud auth print-access-token`.
//! No additional Cargo deps required beyond `reqwest` (already in the SDK).

pub mod client;
pub(super) mod types;

pub use client::{VertexSearchClient, VertexSearchConfig};
pub use types::{VertexSearchOutput, VertexSearchResult};
