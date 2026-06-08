# ERD — Persistence schema (Lightspace runtime data)

All data stored under `~/.lightarchitects/lightspace/<session_id>/` as flat files.

```mermaid
erDiagram
  SESSION ||--o{ EVENT_LOG : "records"
  SESSION ||--o{ SNAPSHOT  : "checkpoints"
  SESSION ||--o{ DRAWER_FILE : "owns"
  SESSION ||--|| HMAC_SEED  : "secures"
  EVENT_LOG ||--o{ CARD      : "produces"
  EVENT_LOG ||--o{ LIFECYCLE_TXN : "transitions"
  EVENT_LOG ||--o{ GATING_EVAL   : "evaluates"
  EVENT_LOG ||--o{ CONTRADICTION  : "resolves"
  CARD ||--o{ PROVENANCE   : "carries"
  CARD ||--o| CONFIDENCE   : "scores"
  DRAWER_FILE ||--|| PROVENANCE : "carries"
  CONTRADICTION ||--o{ RESOLUTION : "yields"

  SESSION {
    uuid   session_id  PK  "UUIDv7 — time-ordered, unguessable"
    string operator_id     "bearer token hash (never raw token)"
    ts     started_at
  }
  EVENT_LOG {
    uuid    id         PK
    uuid    session_id FK
    int     seq            "monotonic; gap triggers snapshot fallback"
    ts      at
    bytes   payload        "CanvasEvent serde_json"
    bytes   hmac_chain     "HMAC(prev_hmac || payload)"
  }
  SNAPSHOT {
    uuid    id         PK
    uuid    session_id FK
    int     at_seq         "EVENT_LOG.seq at snapshot time"
    ts      taken_at
    bytes   state_payload  "CanvasState serde_json"
    bytes   integrity_hmac "HMAC(state_payload)"
  }
  DRAWER_FILE {
    string  file_id    PK
    uuid    session_id FK
    string  mime_type
    string  content_uri    "allowlisted scheme only (CWE-22)"
    int     size_bytes
    ts      attached_at
  }
  CARD {
    string  card_id         PK
    uuid    session_id      FK
    string  kind
    string  title
    jsonb   content_json        "kind-discriminated payload"
    jsonb   provenance_inline   "Provenance struct serialised inline"
  }
  PROVENANCE {
    string  agent
    string  source_uri     "pattern: ^(file|helix|https|ayin|memory)://"
    string  span_id
    ts      ts
  }
  CONFIDENCE {
    float   value          "0.0–1.0"
    string  basis
    string  evidence_tier
  }
  HMAC_SEED {
    uuid    session_id PK FK
    bytes   seed_bytes     "32B from getrandom; persisted to macOS Keychain"
    ts      rotated_at
  }
  CONTRADICTION {
    uuid    id         PK
    uuid    session_id FK
    string  winner_target_id
    int     depth_reached
    bool    cycle_yielded
  }
  RESOLUTION {
    uuid    contradiction_id FK
    string  chosen_target_id
    string  reason
    ts      resolved_at
  }
```

## File layout on disk

```
~/.lightarchitects/lightspace/
└── <session_id>/
    ├── events.jsonl       NDJSON event log (HMAC-chained)
    ├── snapshots/
    │   └── <seq>.json     periodic CanvasState snapshots
    └── drawer/
        └── <file_id>.*    graduated files (mime-typed)
```

## Security constraints (from plan R5)

- `session_id` is UUIDv7 — time-ordered but unguessable (no oracle risk)  
- HMAC seed: `getrandom` at session creation, persisted to macOS Keychain service `la-lightspace-hmac`  
- `content_uri` allowlist: `file://~/.lightarchitects/lightspace/`, `helix://`, project-rooted paths — never `file:///etc/`, never arbitrary `http://` hosts (CWE-22 + LLM07)  
- Path traversal: `safe_lightspace_path()` ancestor-walk canonicalization (Cookbook §63.P5)
