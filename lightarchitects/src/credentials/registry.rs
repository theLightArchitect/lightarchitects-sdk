//! Plugin-style registry of credential providers.
//!
//! The registry wraps each registered provider with a 30-second
//! [`DetectionCache`]. Callers probe by [`ProviderId`]; the public surface
//! never mentions any specific CLI by name.

use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;

use crate::credentials::cache::DetectionCache;
use crate::credentials::types::{Detection, Locator, ProbeError, ProviderId};

/// Trait implemented by each CLI credential provider.
///
/// `probe` returns a [`Detection`] with an abstract [`Locator`]. The SDK
/// does not expose canonical strings through this trait; detailed
/// probing is behind the `credentials-detailed-locator` feature.
#[async_trait]
pub trait CliCredentialProvider: Send + Sync + 'static {
    /// Stable opaque identifier for this provider.
    fn id(&self) -> ProviderId;

    /// Probe for credentials; return abstract presence.
    async fn probe(&self) -> Result<Detection, ProbeError>;

    /// Human-readable name for UI rendering. Opt-in via feature flag.
    #[cfg(feature = "credentials-display-names")]
    fn display_name(&self) -> &'static str;

    /// Probe with canonical-string detail. Opt-in via feature flag.
    #[cfg(feature = "credentials-detailed-locator")]
    async fn probe_detailed(
        &self,
    ) -> Result<crate::credentials::types::DetailedLocator, ProbeError>;
}

/// Registry of providers, each wrapped with a per-provider 30s cache.
#[derive(Clone, Default)]
pub struct Registry {
    providers: HashMap<ProviderId, Arc<dyn CliCredentialProvider>>,
    caches: HashMap<ProviderId, DetectionCache>,
}

impl Registry {
    /// Create an empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a provider. Replaces any previous entry for the same id.
    pub fn register<P: CliCredentialProvider>(&mut self, provider: P) {
        let id = provider.id();
        self.providers.insert(id, Arc::new(provider));
        self.caches.insert(id, DetectionCache::new());
    }

    /// Provider identifiers in stable sorted order.
    #[must_use]
    pub fn provider_ids(&self) -> Vec<ProviderId> {
        let mut ids: Vec<_> = self.providers.keys().copied().collect();
        ids.sort_by(|a, b| a.0.cmp(&b.0));
        ids
    }

    /// Probe a single provider (with caching). `None` if unregistered.
    pub async fn probe(&self, id: ProviderId) -> Option<Detection> {
        let provider = Arc::clone(self.providers.get(&id)?);
        let cache = self.caches.get(&id)?.clone();
        let detection = cache
            .get_or_refresh(|| async move {
                provider.probe().await.unwrap_or(Detection {
                    provider_id: id,
                    available: false,
                    locator: Locator::Absent,
                })
            })
            .await;
        Some(detection)
    }

    /// Probe every registered provider sequentially (cache makes this cheap).
    pub async fn probe_all(&self) -> Vec<Detection> {
        let mut out = Vec::with_capacity(self.providers.len());
        for id in self.provider_ids() {
            if let Some(d) = self.probe(id).await {
                out.push(d);
            }
        }
        out
    }

    /// Retrieve a provider's display name (feature-gated).
    #[cfg(feature = "credentials-display-names")]
    #[must_use]
    pub fn display_name(&self, id: ProviderId) -> Option<&'static str> {
        self.providers.get(&id).map(|p| p.display_name())
    }

    /// Retrieve the detailed locator for a provider (feature-gated, un-cached).
    #[cfg(feature = "credentials-detailed-locator")]
    pub async fn probe_detailed(
        &self,
        id: ProviderId,
    ) -> Option<Result<crate::credentials::types::DetailedLocator, ProbeError>> {
        let provider = Arc::clone(self.providers.get(&id)?);
        Some(provider.probe_detailed().await)
    }
}

/// Build the default registry with all enabled providers.
#[must_use]
pub fn default_registry() -> Registry {
    #[allow(unused_mut)]
    let mut r = Registry::new();
    #[cfg(feature = "providers-anthropic")]
    {
        r.register(crate::credentials::providers::anthropic_cli::AnthropicCliProvider);
    }
    #[cfg(feature = "providers-openai")]
    {
        r.register(crate::credentials::providers::openai_cli::OpenAiCliProvider);
    }
    #[cfg(feature = "providers-google")]
    {
        r.register(crate::credentials::providers::google_cli::GoogleCliProvider);
    }
    r
}
