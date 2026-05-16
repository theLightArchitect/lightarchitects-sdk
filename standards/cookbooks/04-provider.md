<!-- uuid: 8115682c-f317-4d57-b22e-aad174d0d8b8 -->

---
id: "e1c481f4-a5a5fa5b"
date: "2026-03-22"
sibling: user
type: reference
significance: 8.0
strands: [user]
resonance: [reference]
themes: [standards]
epoch: production
---
# Cookbook 04: AI Provider Integration

**Purpose**: Patterns for integrating AI model backends in MCP servers
**Key Concept**: Tier routing - local (free) → cloud (paid) with automatic fallback
**Providers**: Ollama (local), Claude, Gemini, OpenAI

---

## Tier Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                      AI Request                              │
├─────────────────────────────────────────────────────────────┤
│                    Tier Router                               │
│  ┌──────────┐    ┌──────────┐    ┌──────────┐              │
│  │  Tier 0  │ →  │  Tier 1  │ →  │  Tier 2  │              │
│  │  Ollama  │    │  Haiku   │    │  Sonnet  │              │
│  │  (local) │    │  (cloud) │    │ (premium)│              │
│  │   $0     │    │   $0.80  │    │   $3.00  │              │
│  └──────────┘    └──────────┘    └──────────┘              │
│       ↓ fallback      ↓ fallback                            │
└─────────────────────────────────────────────────────────────┘
```

**Routing Strategy**:
1. Try Tier 0 (local) first - free, fast, private
2. Fallback to Tier 1 if local unavailable - good enough for 95% of tasks
3. Escalate to Tier 2 only for customer-facing content

---

## Provider Trait

```rust
// src/providers/mod.rs

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub struct ProviderRequest {
    pub prompt: String,
    pub system: Option<String>,
    pub max_tokens: usize,
    pub temperature: f32,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProviderResponse {
    pub content: String,
    pub model: String,
    pub tokens_used: usize,
    pub provider: String,
}

#[async_trait]
pub trait AIProvider: Send + Sync {
    fn name(&self) -> &'static str;
    fn tier(&self) -> u8;
    async fn is_available(&self) -> bool;
    async fn generate(&self, request: ProviderRequest) -> Result<ProviderResponse, ProviderError>;
}

#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    #[error("Provider unavailable: {0}")]
    Unavailable(String),
    #[error("Request failed: {0}")]
    RequestFailed(String),
    #[error("Rate limited")]
    RateLimited,
    #[error("Invalid response: {0}")]
    InvalidResponse(String),
}
```

---

## Ollama Provider (Tier 0)

```rust
// src/providers/ollama.rs

use super::{AIProvider, ProviderRequest, ProviderResponse, ProviderError};
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};

pub struct OllamaProvider {
    client: Client,
    base_url: String,
    model: String,
}

impl OllamaProvider {
    pub fn new() -> Self {
        Self {
            client: Client::new(),
            base_url: std::env::var("OLLAMA_BASE_URL")
                .unwrap_or_else(|_| "http://localhost:11434".to_string()),
            model: std::env::var("OLLAMA_MODEL")
                .unwrap_or_else(|_| "mistral:7b".to_string()),
        }
    }
}

#[derive(Serialize)]
struct OllamaRequest {
    model: String,
    prompt: String,
    system: Option<String>,
    stream: bool,
}

#[derive(Deserialize)]
struct OllamaResponse {
    response: String,
    #[serde(default)]
    eval_count: usize,
}

#[async_trait]
impl AIProvider for OllamaProvider {
    fn name(&self) -> &'static str { "ollama" }
    fn tier(&self) -> u8 { 0 }

    async fn is_available(&self) -> bool {
        self.client
            .get(format!("{}/api/tags", self.base_url))
            .timeout(std::time::Duration::from_secs(2))
            .send()
            .await
            .is_ok()
    }

    async fn generate(&self, request: ProviderRequest) -> Result<ProviderResponse, ProviderError> {
        let ollama_req = OllamaRequest {
            model: self.model.clone(),
            prompt: request.prompt,
            system: request.system,
            stream: false,
        };

        let response = self.client
            .post(format!("{}/api/generate", self.base_url))
            .json(&ollama_req)
            .timeout(std::time::Duration::from_secs(120))
            .send()
            .await
            .map_err(|e| ProviderError::RequestFailed(e.to_string()))?;

        if !response.status().is_success() {
            return Err(ProviderError::RequestFailed(
                format!("HTTP {}", response.status())
            ));
        }

        let ollama_resp: OllamaResponse = response.json().await
            .map_err(|e| ProviderError::InvalidResponse(e.to_string()))?;

        Ok(ProviderResponse {
            content: ollama_resp.response,
            model: self.model.clone(),
            tokens_used: ollama_resp.eval_count,
            provider: "ollama".to_string(),
        })
    }
}
```

---

## Claude Provider (Tier 1/2)

```rust
// src/providers/claude.rs

use super::{AIProvider, ProviderRequest, ProviderResponse, ProviderError};
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};

pub struct ClaudeProvider {
    client: Client,
    api_key: String,
    model: String,
    tier: u8,
}

impl ClaudeProvider {
    pub fn haiku() -> Result<Self, ProviderError> {
        Ok(Self {
            client: Client::new(),
            api_key: std::env::var("ANTHROPIC_API_KEY")
                .map_err(|_| ProviderError::Unavailable("ANTHROPIC_API_KEY not set".to_string()))?,
            model: "claude-3-5-haiku-20241022".to_string(),
            tier: 1,
        })
    }

    pub fn sonnet() -> Result<Self, ProviderError> {
        Ok(Self {
            client: Client::new(),
            api_key: std::env::var("ANTHROPIC_API_KEY")
                .map_err(|_| ProviderError::Unavailable("ANTHROPIC_API_KEY not set".to_string()))?,
            model: "claude-sonnet-4-20250514".to_string(),
            tier: 2,
        })
    }
}

#[derive(Serialize)]
struct ClaudeRequest {
    model: String,
    max_tokens: usize,
    messages: Vec<ClaudeMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
}

#[derive(Serialize)]
struct ClaudeMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct ClaudeResponse {
    content: Vec<ClaudeContent>,
    usage: ClaudeUsage,
}

#[derive(Deserialize)]
struct ClaudeContent {
    text: String,
}

#[derive(Deserialize)]
struct ClaudeUsage {
    input_tokens: usize,
    output_tokens: usize,
}

#[async_trait]
impl AIProvider for ClaudeProvider {
    fn name(&self) -> &'static str {
        match self.tier {
            1 => "claude-haiku",
            2 => "claude-sonnet",
            _ => "claude",
        }
    }

    fn tier(&self) -> u8 { self.tier }

    async fn is_available(&self) -> bool {
        !self.api_key.is_empty()
    }

    async fn generate(&self, request: ProviderRequest) -> Result<ProviderResponse, ProviderError> {
        let claude_req = ClaudeRequest {
            model: self.model.clone(),
            max_tokens: request.max_tokens,
            messages: vec![ClaudeMessage {
                role: "user".to_string(),
                content: request.prompt,
            }],
            system: request.system,
        };

        let response = self.client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&claude_req)
            .timeout(std::time::Duration::from_secs(60))
            .send()
            .await
            .map_err(|e| ProviderError::RequestFailed(e.to_string()))?;

        if response.status() == 429 {
            return Err(ProviderError::RateLimited);
        }

        if !response.status().is_success() {
            return Err(ProviderError::RequestFailed(
                format!("HTTP {}", response.status())
            ));
        }

        let claude_resp: ClaudeResponse = response.json().await
            .map_err(|e| ProviderError::InvalidResponse(e.to_string()))?;

        let content = claude_resp.content
            .first()
            .map(|c| c.text.clone())
            .unwrap_or_default();

        Ok(ProviderResponse {
            content,
            model: self.model.clone(),
            tokens_used: claude_resp.usage.input_tokens + claude_resp.usage.output_tokens,
            provider: self.name().to_string(),
        })
    }
}
```

---

## Tier Router

```rust
// src/providers/router.rs

use super::{AIProvider, ProviderRequest, ProviderResponse, ProviderError};
use super::ollama::OllamaProvider;
use super::claude::ClaudeProvider;

pub struct TierRouter {
    providers: Vec<Box<dyn AIProvider>>,
}

impl TierRouter {
    pub fn new() -> Self {
        let mut providers: Vec<Box<dyn AIProvider>> = Vec::new();

        // Tier 0: Ollama (always available to try)
        providers.push(Box::new(OllamaProvider::new()));

        // Tier 1: Haiku
        if let Ok(haiku) = ClaudeProvider::haiku() {
            providers.push(Box::new(haiku));
        }

        // Tier 2: Sonnet
        if let Ok(sonnet) = ClaudeProvider::sonnet() {
            providers.push(Box::new(sonnet));
        }

        // Sort by tier (lowest first)
        providers.sort_by_key(|p| p.tier());

        Self { providers }
    }

    /// Generate with automatic fallback through tiers
    pub async fn generate(&self, request: ProviderRequest) -> Result<ProviderResponse, ProviderError> {
        self.generate_with_tier(request, None).await
    }

    /// Generate with minimum tier requirement
    pub async fn generate_with_tier(
        &self,
        request: ProviderRequest,
        min_tier: Option<u8>,
    ) -> Result<ProviderResponse, ProviderError> {
        let min_tier = min_tier.unwrap_or(0);
        let mut last_error = None;

        for provider in &self.providers {
            // Skip providers below minimum tier
            if provider.tier() < min_tier {
                continue;
            }

            // Check availability
            if !provider.is_available().await {
                tracing::debug!(provider = provider.name(), "Provider unavailable, trying next");
                continue;
            }

            // Try to generate
            match provider.generate(request.clone()).await {
                Ok(response) => {
                    tracing::info!(
                        provider = provider.name(),
                        tier = provider.tier(),
                        tokens = response.tokens_used,
                        "Generation successful"
                    );
                    return Ok(response);
                }
                Err(e) => {
                    tracing::warn!(
                        provider = provider.name(),
                        error = %e,
                        "Provider failed, trying next"
                    );
                    last_error = Some(e);
                }
            }
        }

        Err(last_error.unwrap_or(ProviderError::Unavailable("No providers available".to_string())))
    }

    /// Force specific tier (no fallback)
    pub async fn generate_tier(
        &self,
        request: ProviderRequest,
        tier: u8,
    ) -> Result<ProviderResponse, ProviderError> {
        for provider in &self.providers {
            if provider.tier() == tier && provider.is_available().await {
                return provider.generate(request).await;
            }
        }
        Err(ProviderError::Unavailable(format!("Tier {} not available", tier)))
    }
}
```

---

## Usage in Orchestrators

```rust
// src/orchestrators/synthesis.rs

use crate::providers::{TierRouter, ProviderRequest};

pub async fn execute(params: SynthesisParams) -> Result<SynthesisResult, Error> {
    let router = TierRouter::new();

    let request = ProviderRequest {
        prompt: format!("Analyze this data: {}", params.data),
        system: Some("You are a helpful analyst.".to_string()),
        max_tokens: 2000,
        temperature: 0.7,
    };

    // For internal analysis, use any tier (starts with local)
    let response = router.generate(request).await?;

    // For customer-facing, require at least Tier 1
    // let response = router.generate_with_tier(request, Some(1)).await?;

    Ok(SynthesisResult {
        analysis: response.content,
        model_used: response.model,
        provider: response.provider,
    })
}
```

---

## Prompt Templates

```rust
// src/providers/templates.rs

pub struct PromptTemplate {
    pub system: String,
    pub user_template: String,
}

impl PromptTemplate {
    pub fn render(&self, context: &serde_json::Value) -> ProviderRequest {
        let mut prompt = self.user_template.clone();

        if let Some(obj) = context.as_object() {
            for (key, value) in obj {
                let placeholder = format!("{{{{{}}}}}", key);
                let replacement = match value {
                    serde_json::Value::String(s) => s.clone(),
                    other => other.to_string(),
                };
                prompt = prompt.replace(&placeholder, &replacement);
            }
        }

        ProviderRequest {
            prompt,
            system: Some(self.system.clone()),
            max_tokens: 2000,
            temperature: 0.7,
        }
    }
}

// Example templates
pub fn analysis_template() -> PromptTemplate {
    PromptTemplate {
        system: "You are an expert analyst. Provide concise, actionable insights.".to_string(),
        user_template: "Analyze the following:\n\n{{content}}\n\nFocus on: {{focus}}".to_string(),
    }
}
```

---

## Cost Tracking

```rust
// src/providers/cost.rs

pub struct CostTracker {
    tier0_tokens: std::sync::atomic::AtomicU64,
    tier1_tokens: std::sync::atomic::AtomicU64,
    tier2_tokens: std::sync::atomic::AtomicU64,
}

impl CostTracker {
    pub fn new() -> Self {
        Self {
            tier0_tokens: std::sync::atomic::AtomicU64::new(0),
            tier1_tokens: std::sync::atomic::AtomicU64::new(0),
            tier2_tokens: std::sync::atomic::AtomicU64::new(0),
        }
    }

    pub fn record(&self, tier: u8, tokens: u64) {
        match tier {
            0 => self.tier0_tokens.fetch_add(tokens, std::sync::atomic::Ordering::Relaxed),
            1 => self.tier1_tokens.fetch_add(tokens, std::sync::atomic::Ordering::Relaxed),
            2 => self.tier2_tokens.fetch_add(tokens, std::sync::atomic::Ordering::Relaxed),
            _ => 0,
        };
    }

    pub fn estimated_cost(&self) -> f64 {
        let tier1 = self.tier1_tokens.load(std::sync::atomic::Ordering::Relaxed) as f64;
        let tier2 = self.tier2_tokens.load(std::sync::atomic::Ordering::Relaxed) as f64;

        // Cost per million tokens (approximate)
        (tier1 * 0.80 / 1_000_000.0) + (tier2 * 3.00 / 1_000_000.0)
    }
}
```

---

## Best Practices

1. **Default to Tier 0**: Always try local first for privacy and cost savings

2. **Require Tier for Customer-Facing**: Use `generate_with_tier(request, Some(1))` for quality-critical content

3. **Handle Rate Limits**: Implement exponential backoff for `RateLimited` errors

4. **Track Costs**: Monitor token usage per tier

5. **Timeout Appropriately**: Local models may be slower; adjust timeouts

6. **Cache Where Possible**: Cache identical requests to reduce provider calls

---

## Next Steps

- **[03-security.md](./03-security.md)** - Securing provider credentials
- **[05-mcp.md](./05-mcp.md)** - Integrating providers with MCP tools

---

*Platform-agnostic AI provider patterns for any Rust MCP server*

## Links

- [[user/standards/_index-standards|Standards Index]]
- [[user/identity|User Identity]]
- [[user/standards/canon/builders-cookbook|Builders Cookbook]]
