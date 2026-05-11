//! Platform HTTP API client — typed REST client for `lightarchitects-gateway`.
//!
//! The platform API is a private HTTP server (default: `localhost:8080`) backed
//! by local Neo4j. It exposes canonical content, agent identities, skills, and
//! standards with optional per-org override resolution.
//!
//! # Quick start
//!
//! ```no_run
//! use lightarchitects::platform::PlatformClient;
//!
//! # async fn run() -> Result<(), Box<dyn std::error::Error>> {
//! let client = PlatformClient::builder().build()?;
//! let ok = client.health().await?;
//! assert_eq!(ok.status, "healthy");
//! # Ok(())
//! # }
//! ```
//!
//! # Org overrides
//!
//! ```no_run
//! use lightarchitects::platform::PlatformClient;
//!
//! # async fn run() -> Result<(), Box<dyn std::error::Error>> {
//! let client = PlatformClient::builder()
//!     .org_id("acme-corp")
//!     .build()?;
//! // Canon entries and agent identities will reflect acme-corp overrides.
//! let entry = client.canon("canon/builders-cookbook").await?;
//! # Ok(())
//! # }
//! ```

pub mod client;
pub mod error;
pub mod types;

pub use client::{DEFAULT_PLATFORM_BASE_URL, PlatformClient, PlatformClientBuilder};
pub use error::PlatformError;
pub use types::{
    AgentEntry, AgentStrands, CanonEntry, HealthStatus, HelixEntry, HelixPage, HelixQueryParams,
    NodeCount, SkillEntry, SkillSummary, SkillsPage, StandardEntry, UploadCanonRequest,
    UploadCanonResponse, VaultInfo,
};
