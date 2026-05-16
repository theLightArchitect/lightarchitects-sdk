<!-- uuid: b501151a-b97a-4e79-96f9-e45c7573032f -->

---
id: "1036a04c-fc883949"
date: "2026-03-22"
sibling: user
type: reference
significance: 8.0
strands: [user]
resonance: [reference]
themes: [standards]
epoch: production
---
# Cookbook 01: Foundations - Hooks & Utilities

**Purpose**: Core utilities and hook system for MCP servers
**Pattern**: Chain of Responsibility / Interceptor
**Key Concept**: Hooks run before/after tool execution, enabling validation, caching, and formatting

---

## Hook Architecture

Hooks intercept tool execution at two points:
- **Pre-hooks**: Run BEFORE the orchestrator (validation, rate limiting, context injection)
- **Post-hooks**: Run AFTER the orchestrator (response formatting, logging, caching)

```
Request → [Pre-Hook Chain] → Orchestrator → [Post-Hook Chain] → Response
              │                                    │
              ├─ InputValidation (priority 5)      ├─ ResponseFormatting (priority 50)
              ├─ RateLimiter (priority 10)         └─ AuditLogging (priority 60)
              └─ ContextInjection (priority 20)
```

---

## Hook Traits

```rust
// src/hooks/traits.rs
use async_trait::async_trait;

/// Pre-tool execution hook
#[async_trait]
pub trait PreToolUse: Send + Sync {
    /// Unique hook name for logging
    fn name(&self) -> &'static str;

    /// Priority (lower = runs first). Default: 50
    fn priority(&self) -> i32 { 50 }

    /// Optional tool filter - None means all tools
    fn tool_filter(&self) -> Option<Vec<&'static str>> { None }

    /// Execute the hook
    async fn execute(&self, ctx: HookContext) -> HookResult;
}

/// Post-tool execution hook
#[async_trait]
pub trait PostToolUse: Send + Sync {
    fn name(&self) -> &'static str;
    fn priority(&self) -> i32 { 50 }
    fn tool_filter(&self) -> Option<Vec<&'static str>> { None }
    async fn execute(&self, ctx: HookContext) -> HookResult;
}
```

---

## HookContext

```rust
// src/hooks/context.rs
use std::collections::HashMap;
use std::sync::Arc;
use chrono::{DateTime, Utc};
use serde::{Serialize, Deserialize, de::DeserializeOwned};

#[derive(Clone)]
pub struct HookContext {
    /// Tool being invoked
    pub tool_name: String,

    /// Original request parameters
    pub params: serde_json::Value,

    /// Tool result (only populated in post-hooks)
    pub result: Option<serde_json::Value>,

    /// Inter-hook metadata (hooks can pass data to later hooks)
    pub metadata: HashMap<String, serde_json::Value>,

    /// Request timestamp
    pub timestamp: DateTime<Utc>,
}

impl HookContext {
    pub fn new(tool_name: &str, params: serde_json::Value) -> Self {
        Self {
            tool_name: tool_name.to_string(),
            params,
            result: None,
            metadata: HashMap::new(),
            timestamp: Utc::now(),
        }
    }

    /// Store metadata for downstream hooks
    pub fn set_metadata<T: Serialize>(&mut self, key: &str, value: T) {
        if let Ok(v) = serde_json::to_value(value) {
            self.metadata.insert(key.to_string(), v);
        }
    }

    /// Retrieve metadata from upstream hooks
    pub fn get_metadata<T: DeserializeOwned>(&self, key: &str) -> Option<T> {
        self.metadata.get(key)
            .and_then(|v| serde_json::from_value(v.clone()).ok())
    }
}
```

---

## HookResult

```rust
// src/hooks/result.rs

pub enum HookResult {
    /// Continue to next hook (passes potentially modified context)
    Continue(HookContext),

    /// Block execution entirely (tool will not run)
    Block {
        reason: String,
        remediation: Option<String>,
    },

    /// Skip remaining hooks, return cached/synthetic result
    Skip {
        context: HookContext,
        cached_result: serde_json::Value,
    },

    /// Pause for human review (for sensitive operations)
    RequireReview {
        context: HookContext,
        review_prompt: String,
        threshold_violated: String,
    },
}
```

---

## HookRegistry

```rust
// src/hooks/mod.rs
use crate::hooks::traits::{PreToolUse, PostToolUse};
use crate::hooks::result::HookResult;
use crate::hooks::context::HookContext;

pub struct HookRegistry {
    pre_hooks: Vec<Box<dyn PreToolUse>>,
    post_hooks: Vec<Box<dyn PostToolUse>>,
}

impl HookRegistry {
    pub fn new() -> Self {
        let mut registry = Self {
            pre_hooks: Vec::new(),
            post_hooks: Vec::new(),
        };
        registry.register_builtins();
        registry
    }

    fn register_builtins(&mut self) {
        // Register built-in hooks here
        // self.pre_hooks.push(Box::new(InputValidationHook));
        // self.post_hooks.push(Box::new(ResponseFormattingHook));

        // Sort by priority
        self.pre_hooks.sort_by_key(|h| h.priority());
        self.post_hooks.sort_by_key(|h| h.priority());
    }

    pub fn register_pre_hook(&mut self, hook: Box<dyn PreToolUse>) {
        self.pre_hooks.push(hook);
        self.pre_hooks.sort_by_key(|h| h.priority());
    }

    pub fn register_post_hook(&mut self, hook: Box<dyn PostToolUse>) {
        self.post_hooks.push(hook);
        self.post_hooks.sort_by_key(|h| h.priority());
    }

    /// Run pre-hooks before tool execution
    pub async fn run_pre_hooks(&self, ctx: HookContext) -> HookResult {
        let mut current_ctx = ctx;

        for hook in &self.pre_hooks {
            // Check tool filter
            if let Some(filter) = hook.tool_filter() {
                if !filter.contains(&current_ctx.tool_name.as_str()) {
                    continue;
                }
            }

            match hook.execute(current_ctx.clone()).await {
                HookResult::Continue(new_ctx) => current_ctx = new_ctx,
                result => return result, // Block, Skip, or RequireReview
            }
        }

        HookResult::Continue(current_ctx)
    }

    /// Run post-hooks after tool execution
    pub async fn run_post_hooks(&self, ctx: HookContext) -> HookResult {
        let mut current_ctx = ctx;

        for hook in &self.post_hooks {
            if let Some(filter) = hook.tool_filter() {
                if !filter.contains(&current_ctx.tool_name.as_str()) {
                    continue;
                }
            }

            match hook.execute(current_ctx.clone()).await {
                HookResult::Continue(new_ctx) => current_ctx = new_ctx,
                result => return result,
            }
        }

        HookResult::Continue(current_ctx)
    }
}
```

---

## Example: Input Validation Hook

```rust
// src/hooks/builtin/input_validation.rs
use crate::hooks::{PreToolUse, HookContext, HookResult};
use async_trait::async_trait;

pub struct InputValidationHook;

#[async_trait]
impl PreToolUse for InputValidationHook {
    fn name(&self) -> &'static str { "input_validation" }
    fn priority(&self) -> i32 { 5 } // Run first

    async fn execute(&self, mut ctx: HookContext) -> HookResult {
        // Validate params is an object
        if !ctx.params.is_object() {
            return HookResult::Block {
                reason: "Parameters must be a JSON object".to_string(),
                remediation: Some("Wrap parameters in {}".to_string()),
            };
        }

        // Sanitize string fields
        if let Some(obj) = ctx.params.as_object_mut() {
            for (_, value) in obj.iter_mut() {
                if let Some(s) = value.as_str() {
                    // Remove control characters
                    let sanitized: String = s.chars()
                        .filter(|c| !c.is_control() || *c == '\n' || *c == '\t')
                        .take(10000) // Max length
                        .collect();
                    *value = serde_json::Value::String(sanitized);
                }
            }
        }

        // Mark validation complete
        ctx.set_metadata("validated", true);

        HookResult::Continue(ctx)
    }
}
```

---

## Example: Rate Limiter Hook

```rust
// src/hooks/builtin/rate_limiter.rs
use crate::hooks::{PreToolUse, HookContext, HookResult};
use async_trait::async_trait;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

pub struct RateLimiterHook {
    requests_per_minute: u64,
    request_count: AtomicU64,
    window_start: std::sync::Mutex<Instant>,
}

impl RateLimiterHook {
    pub fn new(requests_per_minute: u64) -> Self {
        Self {
            requests_per_minute,
            request_count: AtomicU64::new(0),
            window_start: std::sync::Mutex::new(Instant::now()),
        }
    }
}

#[async_trait]
impl PreToolUse for RateLimiterHook {
    fn name(&self) -> &'static str { "rate_limiter" }
    fn priority(&self) -> i32 { 10 }

    async fn execute(&self, ctx: HookContext) -> HookResult {
        let mut window = self.window_start.lock().unwrap();

        // Reset window if minute has passed
        if window.elapsed() > Duration::from_secs(60) {
            *window = Instant::now();
            self.request_count.store(0, Ordering::SeqCst);
        }

        let count = self.request_count.fetch_add(1, Ordering::SeqCst);

        if count >= self.requests_per_minute {
            return HookResult::Block {
                reason: format!("Rate limit exceeded: {} requests/minute", self.requests_per_minute),
                remediation: Some("Wait before retrying".to_string()),
            };
        }

        HookResult::Continue(ctx)
    }
}
```

---

## Integration with MCP Server

```rust
// src/mcp.rs
use crate::hooks::{HookRegistry, HookContext, HookResult};

async fn handle_tool_call(
    name: &str,
    args: serde_json::Value,
    hook_registry: &HookRegistry,
) -> Result<serde_json::Value, Error> {
    // Build context
    let ctx = HookContext::new(name, args.clone());

    // Run pre-hooks
    let ctx = match hook_registry.run_pre_hooks(ctx).await {
        HookResult::Continue(ctx) => ctx,
        HookResult::Block { reason, .. } => return Err(Error::Blocked(reason)),
        HookResult::Skip { cached_result, .. } => return Ok(cached_result),
        HookResult::RequireReview { review_prompt, .. } => {
            return Err(Error::ReviewRequired(review_prompt));
        }
    };

    // Execute tool
    let result = match name {
        "your_tool" => your_tool::execute(ctx.params.clone()).await?,
        _ => return Err(Error::UnknownTool(name.to_string())),
    };

    // Run post-hooks
    let mut post_ctx = ctx;
    post_ctx.result = Some(result);

    match hook_registry.run_post_hooks(post_ctx).await {
        HookResult::Continue(ctx) => Ok(ctx.result.unwrap()),
        HookResult::Block { reason, .. } => Err(Error::Blocked(reason)),
        _ => unreachable!(),
    }
}
```

---

## Best Practices

1. **Priority Guidelines**:
   - 1-10: Security (validation, rate limiting)
   - 11-30: Context (injection, enrichment)
   - 31-50: Default
   - 51-70: Formatting
   - 71-100: Logging, cleanup

2. **Keep Hooks Focused**: One hook = one responsibility

3. **Use Metadata for Communication**: Hooks should pass data via `ctx.metadata`, not global state

4. **Tool Filters**: Use `tool_filter()` to limit hooks to specific tools

5. **Error Handling**: Return `HookResult::Block` for recoverable errors, panic for bugs

---

## Next Steps

- **[02-orchestrator.md](./02-orchestrator.md)** - Building MCP tools
- **[03-security.md](./03-security.md)** - Security patterns for hooks

---

*Platform-agnostic hook patterns for any Rust MCP server*

## Links

- [[user/standards/_index-standards|Standards Index]]
- [[user/identity|User Identity]]
- [[user/standards/canon/builders-cookbook|Builders Cookbook]]
