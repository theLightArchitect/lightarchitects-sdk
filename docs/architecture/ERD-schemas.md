# ERD — Data Schemas: ironclaw-autonomous-e2e

> Canon XLI: Architect-authored design input. Phase 1 deliverable.

```mermaid
erDiagram
  ProgramConfig {
    String codename
    PathBuf repo_root
    PathBuf worktree_root
    String feat_branch
    Vec_Wave waves
    PathBuf plan_path
    Option_Url ollama_base_url
    String ollama_model
    Option_String ollama_api_key
  }

  Wave {
    String id
    Vec_Task tasks
    WaveParallelism parallelism
  }

  Task {
    String id
    String slug
    String description
    Vec_String file_ownership
    AgentTier tier
    TaskStatus status
  }

  IronclawHitlEscalationEvent {
    String build_codename
    String task_id
    String decision_context
    Vec_String options
    Bytes_32 nonce
    String w3c_traceparent
    String timestamp_iso
    u32 layer_reached
  }

  HitlResolution {
    String build_codename
    String task_id
    Bytes_32 nonce_echo
    String verdict
    String operator_note
    String w3c_traceparent
  }

  DecisionEntry {
    String id
    String build_codename
    String task_id
    DecisionKind kind
    String layer_resolved
    String verdict
    String rationale
    String prev_hash_hex
    String hash_hex
    String timestamp_iso
  }

  OllamaWorkerRequest {
    String model
    Vec_Message messages
    Boolean stream
    Option_Float temperature
  }

  ValidationResult {
    Boolean pass
    Vec_Violation violations
    usize diff_bytes
  }

  Violation {
    ViolationKind kind
    String detail
  }

  ProgramConfig ||--o{ Wave : "contains"
  Wave ||--o{ Task : "contains"
  Task ||--o| IronclawHitlEscalationEvent : "may escalate"
  IronclawHitlEscalationEvent ||--|| HitlResolution : "resolved by"
  Task ||--o{ DecisionEntry : "logged in"
  DecisionEntry }o--|| DecisionEntry : "prev_hash chain"
  Task ||--o| OllamaWorkerRequest : "processed by"
  OllamaWorkerRequest ||--|| ValidationResult : "validated"
  ValidationResult ||--o{ Violation : "contains"
```

## Key schema invariants

- `DecisionEntry.prev_hash_hex` — SHA-256 of the raw previous NDJSON line (genesis = 64 zeros). HMAC via HKDF wave subkey. Tamper-evident chain identical to container-hitl-audit pattern.
- `IronclawHitlEscalationEvent.nonce` — 32 random bytes from ChaCha20Rng; echoed in `HitlResolution.nonce_echo` to prevent replay attacks.
- `IronclawHitlEscalationEvent.w3c_traceparent` — propagated from supervisor's AYIN span context through SSE payload and echoed back in POST response headers for continuous trace.
- `ValidationResult.diff_bytes` — must be ≤ `DIFF_BYTES_MAX = 524_288` (512KB). Oversized diffs → `Violation::DiffCeiling`.
