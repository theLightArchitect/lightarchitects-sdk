# C2 Container — Lightspace Conversation API in System Context

```mermaid
C4Container
  title Lightspace Conversation API — C2 Container

  Person(operator, "Lightspace Operator", "Uses lightspace to orchestrate builds via strategy loops")

  System_Boundary(lightspace, "Lightspace (lightarchitects-sdk)") {
    Container(webshell_ui, "lightarchitects-webshell-ui", "SvelteKit/Svelte 5", "Lightspace browser UI. Lobby.svelte: entry point. conversation.svelte.ts: typed API client + SSE subscriber.")
    Container(webshell, "lightarchitects-webshell", "Rust/Axum", "HTTP gateway. Hosts conversation/* routes (5 handlers), ConvSessionStore (DashMap), TTL eviction background task.")
    Container(lightarchitects_lib, "lightarchitects crate", "Rust lib", "ConversationSession<P> + InMemoryConversationMemory + StrategyRegistry (19 registered profiles: ReActStrategy, BuildStrategy, SecureStrategy, EnrichStrategy, etc.). Turn execution engine.")
  }

  System_Ext(litellm, "LiteLLM Proxy", "Provider-agnostic LLM gateway. Translates to Ollama / Anthropic / OpenAI.")

  Rel(operator, webshell_ui, "Opens lightspace, types intent, submits", "HTTPS")
  Rel(webshell_ui, webshell, "POST /api/conversation; GET /api/conversation/{id}/stream (SSE); POST /api/conversation/{id}", "HTTP/SSE")
  Rel(webshell, lightarchitects_lib, "dispatch_conversation_turn() — strategy or native turn execution", "Rust inprocess")
  Rel(lightarchitects_lib, litellm, "LLM token stream requests", "HTTP/SSE")
  Rel(lightarchitects_lib, strategy_runner, "StrategyRegistry::lookup(name) → dispatch", "Rust inprocess")
```

## Key design decisions

- **No buildId prerequisite** — `ConvSessionStore` is independent of `BuildSession` (`state.builds`). A conversation can start before any build exists.
- **SSE for streaming** — `broadcast::Sender<ConvSSEEvent>` fans out to multiple subscribers; keepalive ping every 15s.
- **LiteLLM as the LLM layer** — `lightarchitects_lib::LitellmConfig::build_provider()` constructs the provider; v1 uses global config (per-session override is NG2).
