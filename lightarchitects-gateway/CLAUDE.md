# CLAUDE.md — lightarchitects-gateway (ironclaw-spine worktree)

## Scope note (2026-05-18 — ironclaw-spine Phase 1)

The `arena/` module is the **research arena** — the existing autonomous multi-agent
research platform (HTTP + scheduler + heartbeat agents). The new `arena/delivery_arena/`
module is the **delivery arena** — the Pillar 2 autonomous build delivery engine.

### Module split

| Module | Status | Purpose |
|--------|--------|---------|
| `arena/` (research_arena) | Existing — do not rename yet | Research + heartbeat agents (HTTP API, Ollama) |
| `arena/delivery_arena/` | Phase 1 skeleton → Phase 3–5 impl | Autonomous build delivery engine |

The rename `arena/` → `research_arena/` is a **Phase 7 deliverable** (cross-cutting rename).
Do not rename `arena/` in earlier phases — it requires updating all import paths workspace-wide.

### delivery_arena feature flag

Off by default. Enable with:

```bash
cargo check -p lightarchitects-gateway --features delivery_arena
cargo test -p lightarchitects-gateway --features delivery_arena
cargo clippy -p lightarchitects-gateway --features delivery_arena -- -D warnings
```

### Build commands (standard)

Same as workspace root:

```bash
make quality    # fmt + clippy + tests (pre-commit gate)
make build      # compile release binary
make deploy     # quality + build + install to ~/lightarchitects/*/bin/
```
