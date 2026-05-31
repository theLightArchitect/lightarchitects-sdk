//! Phase-2 feature disclosure constants.

/// Disclosure message surfaced to callers when a Phase-2 feature is requested
/// before it has been implemented.
///
/// Returned inside [`crate::container_spawn::policy::SpawnError::NotYetImplemented`]
/// for `NetworkPolicy::Balanced` and `CredentialStrategy::Proxy`.
pub const PHASE_2_DISCLOSURE: &str = "\
NetworkPolicy::Balanced and CredentialStrategy::Proxy are scheduled for Phase 2 \
of the container-spawn-policy build. They are declared now so that call-sites can \
be updated without a breaking API change, but the docker-args plumbing and gateway \
credential-proxy routes are not yet wired. Use NetworkPolicy::Bridge (default) or \
NetworkPolicy::None (Airgapped) until Phase 2 ships.";
