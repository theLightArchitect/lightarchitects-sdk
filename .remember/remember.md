# Handoff

## State
Activity tab live (2-column: Agent Activity + AYIN Spans). Tesseract command palette replaces sidebar. QUICK actions route through copilot for real execution. CWD inherited from server. AYIN spans emitted for copilot turns + tool calls. Oscillator pulses on every activity event. Sibling identity injection on dispatch from `$HELIX/<sibling>/identity.md`. Fork to Terminal includes `cd <cwd> &&`. System events filtered by default. All verified with §56 headed Playwright: /build, /secure, /research, DISPATCH (SOUL, CORSO, SERAPH), Fork to Terminal all produce tangible outputs. Gate passed: fmt + clippy + tests. **ALL UNCOMMITTED.**

## Next
1. **EVA identity not overriding CLAUDE.md** — `--system-prompt` has lower precedence than project CLAUDE.md in Claude Code. Fix: use `--agent eva` template, or investigate Claude Code's prompt precedence.
2. **Commit all changes** — 15+ files modified across backend + frontend.
3. **AYIN external traces** — MCP servers need `observe` feature flag to emit spans to AYIN.

## Context
- Token: `63308ab0-d024-4f7d-a459-936744aa255f` (from `~/.lightarchitects/webshell/.token`)
- Vite dev server dies on `window.location.reload()` from Playwright — use `browser_navigate` instead.
- Pre-existing test failure: `write_contracts_have_expected_semantics` (404 on random UUID, not our bug).
- PolytopeIcon tesseract button is 20px next to Send — popover floats above with DISPATCH/CONTEXT/QUICK.
