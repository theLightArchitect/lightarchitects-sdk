//! Hardcoded registry of Ollama Cloud model metadata.
//!
//! Provides the 17-entry [`CLOUD_MODEL_REGISTRY`] with capability flags,
//! context window sizes, and cost tiers for the model picker UI and
//! cost-gate enforcement.

use std::collections::BTreeMap;

/// Cost tier for a cloud-hosted model.
///
/// Maps to operator-visible badges in the Copilot Drawer model picker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CostTier {
    /// Lowest cost — budget batch workloads.
    Low,
    /// Mid-range cost — interactive coding sessions.
    Medium,
    /// Higher cost — large-context or flagship models.
    High,
    /// Highest cost — frontier-scale models (>500B params).
    Premium,
}

impl CostTier {
    /// Lowercase string tag for JSON serialization.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Premium => "premium",
        }
    }
}

/// Capability metadata for a single Ollama Cloud model.
#[derive(Debug, Clone)]
pub struct CloudModel {
    /// Ollama model slug (e.g. `"glm-5.1:cloud"`).
    pub slug: &'static str,
    /// Human-readable display name (e.g. `"GLM 5.1"`).
    pub display_name: &'static str,
    /// Model family grouping (e.g. `"GLM"`).
    pub family: &'static str,
    /// Organization that created this model.
    pub provider_org: &'static str,
    /// Context window in tokens.
    pub context_length: u32,
    /// Whether the model supports tool/function calling.
    pub tool_use: bool,
    /// Whether the model supports vision/image inputs.
    pub vision: bool,
    /// Parameter count in billions, if publicly disclosed.
    pub params_billion: Option<u32>,
    /// Operator-visible cost tier.
    pub cost_tier: CostTier,
}

/// 18-entry registry of Ollama Cloud models with capability metadata.
///
/// All slugs end with `:cloud`. Used by the model picker UI and cost-gate
/// enforcement. Families: `GLM`, `Kimi`, `DeepSeek`, `Qwen`, `Gemma`, `MiniMax`,
/// `Nemotron`, `Cogito`, `Mistral`.
pub const CLOUD_MODEL_REGISTRY: &[CloudModel] = &[
    // GLM — Zhipu AI
    CloudModel {
        slug: "glm-5.1:cloud",
        display_name: "GLM 5.1",
        family: "GLM",
        provider_org: "Zhipu AI",
        context_length: 128_000,
        tool_use: true,
        vision: false,
        params_billion: None,
        cost_tier: CostTier::Medium,
    },
    CloudModel {
        slug: "glm-5:cloud",
        display_name: "GLM 5",
        family: "GLM",
        provider_org: "Zhipu AI",
        context_length: 128_000,
        tool_use: true,
        vision: false,
        params_billion: None,
        cost_tier: CostTier::Medium,
    },
    // Kimi — Moonshot AI
    CloudModel {
        slug: "kimi-k2.6:cloud",
        display_name: "Kimi K2.6",
        family: "Kimi",
        provider_org: "Moonshot AI",
        context_length: 200_000,
        tool_use: true,
        vision: false,
        params_billion: None,
        cost_tier: CostTier::Medium,
    },
    CloudModel {
        slug: "kimi-k2.5:cloud",
        display_name: "Kimi K2.5",
        family: "Kimi",
        provider_org: "Moonshot AI",
        context_length: 200_000,
        tool_use: true,
        vision: false,
        params_billion: None,
        cost_tier: CostTier::Medium,
    },
    // DeepSeek
    CloudModel {
        slug: "deepseek-v4-pro:cloud",
        display_name: "DeepSeek V4 Pro",
        family: "DeepSeek",
        provider_org: "DeepSeek",
        context_length: 128_000,
        tool_use: true,
        vision: false,
        params_billion: None,
        cost_tier: CostTier::High,
    },
    CloudModel {
        slug: "deepseek-v4-flash:cloud",
        display_name: "DeepSeek V4 Flash",
        family: "DeepSeek",
        provider_org: "DeepSeek",
        context_length: 128_000,
        tool_use: true,
        vision: false,
        params_billion: None,
        cost_tier: CostTier::Low,
    },
    // Qwen — Alibaba
    CloudModel {
        slug: "qwen3.5:cloud",
        display_name: "Qwen 3.5",
        family: "Qwen",
        provider_org: "Alibaba",
        context_length: 128_000,
        tool_use: true,
        vision: false,
        params_billion: None,
        cost_tier: CostTier::Medium,
    },
    CloudModel {
        slug: "qwen3.5:397b-cloud",
        display_name: "Qwen 3.5 (397B)",
        family: "Qwen",
        provider_org: "Alibaba",
        context_length: 128_000,
        tool_use: true,
        vision: false,
        params_billion: Some(397),
        cost_tier: CostTier::Premium,
    },
    // ironclaw Phase 3 default coding model (SLOT 1-3 workers)
    CloudModel {
        slug: "qwen3-coder:480b-cloud",
        display_name: "Qwen 3 Coder (480B)",
        family: "Qwen",
        provider_org: "Alibaba",
        context_length: 1_000_000,
        tool_use: false,
        vision: false,
        params_billion: Some(480),
        cost_tier: CostTier::Premium,
    },
    CloudModel {
        slug: "qwen3-vl:235b-cloud",
        display_name: "Qwen 3 VL (235B)",
        family: "Qwen",
        provider_org: "Alibaba",
        context_length: 128_000,
        tool_use: true,
        vision: true,
        params_billion: Some(235),
        cost_tier: CostTier::High,
    },
    // Gemma — Google
    CloudModel {
        slug: "gemma4:31b-cloud",
        display_name: "Gemma 4 (31B)",
        family: "Gemma",
        provider_org: "Google",
        context_length: 128_000,
        tool_use: true,
        vision: false,
        params_billion: Some(31),
        cost_tier: CostTier::Medium,
    },
    CloudModel {
        slug: "gemma3:27b-cloud",
        display_name: "Gemma 3 (27B)",
        family: "Gemma",
        provider_org: "Google",
        context_length: 128_000,
        tool_use: true,
        vision: false,
        params_billion: Some(27),
        cost_tier: CostTier::Low,
    },
    // MiniMax
    CloudModel {
        slug: "minimax-m2.7:cloud",
        display_name: "MiniMax M2.7",
        family: "MiniMax",
        provider_org: "MiniMax",
        context_length: 256_000,
        tool_use: true,
        vision: false,
        params_billion: None,
        cost_tier: CostTier::Medium,
    },
    // Nemotron — NVIDIA
    CloudModel {
        slug: "nemotron-3-super:cloud",
        display_name: "Nemotron 3 Super",
        family: "Nemotron",
        provider_org: "NVIDIA",
        context_length: 128_000,
        tool_use: true,
        vision: false,
        params_billion: None,
        cost_tier: CostTier::High,
    },
    // Cogito
    CloudModel {
        slug: "cogito-2.1:671b-cloud",
        display_name: "Cogito 2.1 (671B)",
        family: "Cogito",
        provider_org: "Cogito",
        context_length: 128_000,
        tool_use: true,
        vision: false,
        params_billion: Some(671),
        cost_tier: CostTier::Premium,
    },
    // Mistral
    CloudModel {
        slug: "mistral-large-3:675b-cloud",
        display_name: "Mistral Large 3 (675B)",
        family: "Mistral",
        provider_org: "Mistral AI",
        context_length: 128_000,
        tool_use: true,
        vision: false,
        params_billion: Some(675),
        cost_tier: CostTier::Premium,
    },
    CloudModel {
        slug: "ministral-3:14b-cloud",
        display_name: "Ministral 3 (14B)",
        family: "Mistral",
        provider_org: "Mistral AI",
        context_length: 128_000,
        tool_use: true,
        vision: false,
        params_billion: Some(14),
        cost_tier: CostTier::Low,
    },
    CloudModel {
        slug: "ministral-3:8b-cloud",
        display_name: "Ministral 3 (8B)",
        family: "Mistral",
        provider_org: "Mistral AI",
        context_length: 128_000,
        tool_use: true,
        vision: false,
        params_billion: Some(8),
        cost_tier: CostTier::Low,
    },
];

/// Look up a model by its slug.
///
/// Returns `None` if the slug is not in [`CLOUD_MODEL_REGISTRY`].
#[must_use]
pub fn lookup(slug: &str) -> Option<&'static CloudModel> {
    CLOUD_MODEL_REGISTRY.iter().find(|m| m.slug == slug)
}

/// Group all models by family, returning families in alphabetical order.
#[must_use]
pub fn list_by_family() -> BTreeMap<&'static str, Vec<&'static CloudModel>> {
    let mut map: BTreeMap<&'static str, Vec<&'static CloudModel>> = BTreeMap::new();
    for m in CLOUD_MODEL_REGISTRY {
        map.entry(m.family).or_default().push(m);
    }
    map
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn registry_has_exactly_18_entries() {
        assert_eq!(
            CLOUD_MODEL_REGISTRY.len(),
            18,
            "CLOUD_MODEL_REGISTRY must have exactly 18 entries (entry 18: qwen3-coder:480b-cloud)"
        );
    }

    #[test]
    fn all_slugs_end_with_cloud() {
        for m in CLOUD_MODEL_REGISTRY {
            assert!(
                m.slug.ends_with("cloud"),
                "slug '{}' must end with 'cloud' (via ':cloud' or '<size>-cloud')",
                m.slug
            );
        }
    }

    #[test]
    fn all_families_non_empty() {
        for m in CLOUD_MODEL_REGISTRY {
            assert!(!m.family.is_empty(), "model '{}' has empty family", m.slug);
        }
    }

    #[test]
    fn lookup_every_registered_slug_returns_some() {
        for m in CLOUD_MODEL_REGISTRY {
            assert!(lookup(m.slug).is_some(), "lookup({}) returned None", m.slug);
        }
    }

    #[test]
    fn lookup_unknown_slug_returns_none() {
        assert!(lookup("not-a-real-model:cloud").is_none());
        assert!(lookup("glm-5.1").is_none()); // missing :cloud suffix
        assert!(lookup("").is_none());
    }

    #[test]
    fn list_by_family_groups_all_entries() {
        let by_family = list_by_family();
        let total: usize = by_family.values().map(Vec::len).sum();
        assert_eq!(
            total,
            CLOUD_MODEL_REGISTRY.len(),
            "list_by_family must include every registry entry"
        );
    }

    #[test]
    fn list_by_family_keys_are_sorted() {
        let by_family = list_by_family();
        let keys: Vec<&&str> = by_family.keys().collect();
        let mut sorted = keys.clone();
        sorted.sort_unstable();
        assert_eq!(keys, sorted, "BTreeMap keys must be in sorted order");
    }

    #[test]
    fn cost_tier_as_str_round_trips() {
        assert_eq!(CostTier::Low.as_str(), "low");
        assert_eq!(CostTier::Medium.as_str(), "medium");
        assert_eq!(CostTier::High.as_str(), "high");
        assert_eq!(CostTier::Premium.as_str(), "premium");
    }
}
