# Surface Design — litellm-platform-integration

> Research + design artifact. No implementation code. Generated 2026-05-31.

---

## Task 1 — Codebase Analysis Answers

### 1. `Provider` type definition in `setup.ts`

`setup.ts` does **not** export a `Provider` type. The type is declared locally inside `ProviderStep.svelte`:

```
type Provider = 'anthropic' | 'openai' | 'openrouter' | 'mistral-vibe';
```

It is a local union literal — not exported from `setup.ts`. The authoritative backend identifier is `selectedBackend: writable<string | null>`, which accepts an untyped string. The `Provider` union is purely a UI guard inside the component.

`setup.ts` exports `SetupTier` (`'local' | 'byok' | 'la-platform'`) but no `Provider` type.

### 2. How `ProviderStep` communicates the selected provider

On `proceed()`:
```
selectedBackend.set(chosen);      // e.g. 'anthropic', 'openai', 'openrouter', 'mistral-vibe'
selectedAgent.set('lightarchitects');
step.set('auth');
```

Both `selectedBackend` and `selectedAgent` are `writable` stores exported from `setup.ts`. The wizard then advances to the `'auth'` step where `AuthStep` reads `selectedBackend` to know which credential form to render.

`ProviderStep` does NOT write to `providerStore.ts` — that store is consumed by the runtime `ProviderPill` / `ProviderSettings` components after setup is complete.

### 3. Does Ollama local go through `ProviderStep`?

No. `SourceStep` short-circuits for the `'local'` tier:

```ts
if (chosen === 'local') {
  selectedBackend.set('ollama-launch');
  selectedAgent.set('lightarchitects');
  step.set('auth');          // jumps directly to auth, bypassing provider
}
```

Only the `'byok'` tier routes through `ProviderStep`. Ollama Cloud is a BYOK provider (requires an API key) and must appear in `ProviderStep`. Ollama Local (`ollama-launch`) is tier-local and never enters `ProviderStep`.

### 4. `providerStore.ts` current shape

Two stores and two async actions:

```ts
// Stores
export const providerConfig = writable<ProviderConfig | null | undefined>(null);
// null = loading, undefined = fetch failure, ProviderConfig = loaded

export interface ProviderConfig {
  base_url: string;     // LiteLLM proxy URL
  model: string;        // active model slug
  has_key: boolean;     // whether a key is stored
  updated_at: string;   // ISO-8601 UTC
}

// Actions
loadProvider()   // GET /api/litellm/config → updates providerConfig
saveProvider({base_url, model, api_key})  // POST /api/litellm/config (204) → optimistic update
```

The store is **provider-agnostic** — it stores `base_url` + `model` for whatever LiteLLM endpoint is active, not a typed provider enum. It fires `la:litellm-config-saved` on save for legacy listeners.

---

## Task 2 — 7-Provider Card Layout Design

### Routing clarification

The P1 plan's 7 providers are:
- Anthropic, OpenAI, Ollama (local), Ollama Cloud, DeepSeek, Google Vertex, Mistral

Ollama (local) routes through `SourceStep` tier-local → `selectedBackend = 'ollama-launch'`, bypassing `ProviderStep` entirely. `ProviderStep` is only shown for the `byok` tier.

**ProviderStep therefore shows 6 BYOK provider cards** (Ollama Local is absent):

| # | Provider ID | Label | Subtitle | Auth type | Keychain service |
|---|-------------|-------|----------|-----------|-----------------|
| 1 | `anthropic` | Anthropic | Claude Sonnet · Opus · Haiku | API key | `la-anthropic-credential` |
| 2 | `openai` | OpenAI | GPT-4o · o3 · o4-mini | API key | `la-openai-credential` |
| 3 | `ollama-cloud` | Ollama Cloud | Llama 3.3 · Qwen3 · Phi-4 | API key (Bearer) | `la-ollama-cloud-credential` |
| 4 | `deepseek` | DeepSeek | DeepSeek-V3 · DeepSeek-R1 | API key | `la-deepseek-credential` |
| 5 | `google-vertex` | Google Vertex | Gemini 2.5 Pro · Flash | Service account JSON | `la-vertex-credential` (JSON) + `la-vertex-project` (project ID) |
| 6 | `mistral` | Mistral | Mistral Large · Codestral | API key | `la-mistral-credential` |

### OpenRouter status

**OpenRouter is replaced.** It is not in the P1 plan's 7-provider list. The current `'openrouter'` card and `orHasKey` derived store should be removed. OpenRouter may be added in P2+ as an optional hub provider, but it is out of scope for P1.

The `AuthStatus` interface in `setup.ts` has an `openrouter: OpenRouterAuthStatus` field — that field should be deprecated/removed from the Rust side in a follow-on cleanup (not in-scope for the surface change).

### Card layout notes

With 6 cards vs the current 4, the `max-width: 860px` container with `flex-wrap` and 190px card width will render as a 3+3 two-row grid at most viewport sizes. This is clean. No layout change required — `flex-wrap: wrap; justify-content: center` already handles it.

For Google Vertex (OAuth / service account), the `key-badge` text should read "Account configured ✓" rather than "Key stored ✓" to avoid confusion. The auth step for Vertex will show a file picker / paste area for the JSON credential, not a plain API key input — that detail belongs to `AuthStep.svelte` design.

### Data model change needed in `setup.ts`

The `Provider` type is local to `ProviderStep.svelte` and should be promoted to a named export in `setup.ts` (or a new `src/lib/providers.ts`) so `AuthStep` and `ProviderSettings` can reference it without duplication.

Proposed exported type:

```ts
// src/lib/setup.ts (or providers.ts)
export type Provider =
  | 'anthropic'
  | 'openai'
  | 'ollama-cloud'
  | 'deepseek'
  | 'google-vertex'
  | 'mistral';
```

Changes from current:
- Remove `'openrouter'`
- Remove `'mistral-vibe'` → replace with `'mistral'` (drop the `-vibe` suffix, which was a legacy naming artifact)
- Add `'ollama-cloud'`, `'deepseek'`, `'google-vertex'`

The `selectedBackend` store remains `writable<string | null>` — no type narrowing needed there since the Rust backend uses a plain string slug.

`AuthStatus` in `setup.ts` needs three new status interfaces:

```ts
export interface OllamaCloudAuthStatus {
  has_api_key: boolean;
  login_source?: string;
}

export interface DeepSeekAuthStatus {
  has_api_key: boolean;
  login_source?: string;
}

export interface GoogleVertexAuthStatus {
  has_service_account: boolean;   // service account JSON, not an API key
  project_id?: string;            // GCP project ID if configured
}
```

And `AuthStatus` gains three fields (removing `openrouter` is a breaking wire-format change; defer to a v2 migration or keep the field as `openrouter?: OpenRouterAuthStatus`):

```ts
export interface AuthStatus {
  claude: ClaudeAuthStatus;
  codex: CodexAuthStatus;
  ollama: OllamaAuthStatus;
  mistral: MistralAuthStatus;
  openrouter?: OpenRouterAuthStatus;   // keep as optional, deprecate in P2
  // NEW:
  ollama_cloud: OllamaCloudAuthStatus;
  deepseek: DeepSeekAuthStatus;
  google_vertex: GoogleVertexAuthStatus;
}
```

Note: `mistral-vibe` → `mistral` rename requires the Rust `/api/setup/info` endpoint and keychain lookup to be updated to match. The `authStatus?.mistral` derived store path stays the same since the field name is already `mistral`.

---

## Task 3 — Budget Guard Rust Type Design

### Context

P2 IronClaw budget enforcement needs to track per-session token spend and enforce a hard ceiling. The existing `AppState` holds no budget state. The design must:
- Track aggregate spend per active build session
- Enforce a ceiling (operator-configurable, default $X / N tokens)
- Broadcast an SSE event on exhaustion so the frontend can surface a hard block

### `BudgetState` struct

```rust
// lightarchitects-webshell/src/budget/mod.rs  (new module)

/// Per-session token and cost accounting for IronClaw budget enforcement.
///
/// All arithmetic uses integer token counts to avoid floating-point drift.
/// USD cost is derived at read time from `tokens_used * cost_per_token_usd`.
pub struct BudgetState {
    /// Hard ceiling in tokens. Derived from the operator's USD budget:
    ///   `ceiling_tokens = budget_usd / cost_per_token_usd`
    /// Stored as tokens so enforcement is a simple integer compare.
    pub ceiling_tokens: u64,

    /// Tokens consumed so far across all turns in this session.
    /// Atomically updated by the copilot runner on each `result` event.
    pub tokens_used: Arc<AtomicU64>,

    /// Input tokens from the most recently completed turn (informational).
    pub last_turn_input_tokens: Arc<AtomicU64>,

    /// Output tokens from the most recently completed turn (informational).
    pub last_turn_output_tokens: Arc<AtomicU64>,

    /// Whether the ceiling has been hit. Set to `true` by the enforcement
    /// gate; never reset within a session (operator must open a new session).
    pub exhausted: Arc<AtomicBool>,

    /// Session the budget tracks. Matches `BuildSession::id`.
    pub session_id: Uuid,

    /// UTC timestamp when the budget was initialised (for audit log).
    pub started_at: DateTime<Utc>,
}

impl BudgetState {
    /// Returns `true` and sets `exhausted = true` if `tokens_used` would
    /// exceed `ceiling_tokens` after adding `delta`. Returns `false` if
    /// the increment fits within budget.
    ///
    /// Compare-and-set is unnecessary — the copilot runner calls this on
    /// the per-turn `result` event, which is serialised within a single
    /// `BuildSession` task. No concurrent callers for the same session.
    pub fn record_turn(&self, input: u64, output: u64) -> BudgetCheckResult;
}

pub enum BudgetCheckResult {
    /// Turn fits within budget. Remaining tokens included for the SSE gauge.
    Ok { remaining_tokens: u64 },
    /// Turn exhausted the budget. Session must be halted.
    Exhausted { overage_tokens: u64 },
}
```

### Where on `AppState` does `BudgetState` live?

Budget state is per-session (per `BuildSession`), not global. It belongs on `BuildSession`, not `AppState`.

```rust
// lightarchitects-webshell/src/builds/session.rs (existing type, new field)
pub struct BuildSession {
    // ... existing fields ...

    /// IronClaw budget guard. `None` when no budget ceiling is configured
    /// (operator opted out or feature-flagged off). Populated at session
    /// creation from `AppState::ironclaw_config.budget_usd`.
    pub budget: Option<Arc<BudgetState>>,
}
```

`AppState` gains one field for the feature-level config (ceiling in USD, enabled flag):

```rust
// AppState addition
/// IronClaw budget enforcement config. Loaded from `~/.lightarchitects/ironclaw.toml`
/// at startup; `None` when the file is absent (budget enforcement disabled).
pub ironclaw_config: Option<Arc<IronclawConfig>>,

pub struct IronclawConfig {
    /// USD hard ceiling per session. `0.0` = disabled.
    pub budget_usd: f64,
    /// Model-specific cost per output token in USD (used to derive `ceiling_tokens`).
    pub cost_per_output_token_usd: f64,
    /// Whether to HITL-escalate at 80% consumption before hard cut-off.
    pub warn_at_pct: f32,
}
```

This avoids polluting `AppState` with per-session mutable state, which would require fine-grained locking and leak session lifetime into the global state struct.

### SSE event on budget exhaustion

Add a new variant to `WebEvent` in `lightarchitects-webshell/src/events/types.rs`:

```rust
/// IronClaw budget ceiling reached — copilot session halted.
///
/// Emitted by the budget enforcement gate in `BuildSession::record_turn`
/// when `BudgetCheckResult::Exhausted` is returned. The frontend must
/// surface a hard block UI and prevent further turn submission.
///
/// Wire tag: `"budget_exhausted"`.
BudgetExhausted(BudgetExhaustedEvent),
```

```rust
/// Payload for [`WebEvent::BudgetExhausted`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BudgetExhaustedEvent {
    /// Build session that hit the ceiling.
    pub build_id: Uuid,
    /// Total tokens consumed when the ceiling was reached.
    pub tokens_used: u64,
    /// The ceiling that was exceeded.
    pub ceiling_tokens: u64,
    /// Overage tokens from the turn that triggered exhaustion.
    pub overage_tokens: u64,
    /// Approximate USD cost at exhaustion (informational, derived server-side).
    /// Serialised as a string to avoid float precision issues in JSON.
    pub cost_usd_approx: String,
    /// UTC timestamp of exhaustion.
    pub exhausted_at: DateTime<Utc>,
}
```

A companion `BudgetWarning` event fires at the `warn_at_pct` threshold (default 80%):

```rust
/// IronClaw budget warning — approaching the ceiling.
///
/// Emitted when `tokens_used / ceiling_tokens >= warn_at_pct`.
/// The frontend shows a non-blocking warning banner.
/// Wire tag: `"budget_warning"`.
BudgetWarning(BudgetWarningEvent),

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BudgetWarningEvent {
    pub build_id: Uuid,
    pub tokens_used: u64,
    pub ceiling_tokens: u64,
    /// Fraction consumed, 0.0–1.0.
    pub pct_consumed: f32,
}
```

Both variants must be added to the `sse_contract_all_web_event_variants_have_known_type_tags` test
and to the `EventType` union in `lightarchitects-webshell-ui/src/lib/types.ts`.

### Broadcast path

```
BuildSession.run_turn()
  → copilot_runner emits `result` event with token counts
  → calls budget.record_turn(input_tokens, output_tokens)
    → BudgetCheckResult::Ok { remaining }  → emit BudgetWarning if pct >= warn_at_pct
    → BudgetCheckResult::Exhausted { .. }  → emit BudgetExhausted, halt session
  → event broadcast via BuildSession.event_tx (per-build channel)
  → SSE fan-out to browser (existing GET /api/events path)
```

No new HTTP endpoints required. The enforcement is fully SSE-push — the browser observes `budget_exhausted` and disables the send button; the server refuses further turns via `BuildSession::exhausted` check at the top of `run_turn`.

---

## Summary

| Item | Decision |
|------|----------|
| ProviderStep card count | 6 (Ollama local excluded — tier-local path) |
| OpenRouter | Removed from P1; `openrouter?` kept as optional deprecated field in `AuthStatus` |
| `mistral-vibe` → `mistral` | Renamed; both Rust keychain key and TS type updated |
| `Provider` type | Promoted to exported type in `setup.ts`; 6-value union |
| New BYOK providers | `ollama-cloud`, `deepseek`, `google-vertex` |
| Google Vertex auth | Service account JSON (not API key); separate `GoogleVertexAuthStatus` interface |
| `BudgetState` home | `BuildSession` field (not `AppState`) — per-session scope |
| `AppState` addition | `ironclaw_config: Option<Arc<IronclawConfig>>` only |
| New SSE variants | `BudgetExhausted` + `BudgetWarning` — both need SSE contract test + FE `EventType` |
