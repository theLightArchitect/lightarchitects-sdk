# webshell-dev-mode — Phase 2 Research Findings

## T2.1: chromiumoxide (Rust CDP client) — Context7 + cargo verify
- **Version**: 0.9.1 (latest stable)
- **Key types**: `Browser`, `BrowserConfig`, `Page`, `Element`
- **API**: `Page::screenshot(ScreenshotParams)`, `Page::evaluate(expression)`, `BrowserConfig::builder().no_sandbox().window_size()`
- **Connect modes**: `Browser::launch(config)` (spawn Chrome), `Browser::connect(url)` (existing Chrome)
- **Tokio compat**: chromiumoxide 0.9.1 → tokio v1.52.x (workspace: v1.52.1) ✓
- **Exact pin**: Use `"=0.9.1"` in Cargo.toml (not caret range)

## T2.2: playwright (Node.js) — Context7
- **@playwright/test ^1.59.1** already in package.json as devDependency
- **connectOverCDP(endpointURL)** — attaches to existing Chrome via CDP websocket
- **Production frontend does NOT use @playwright/test** — only E2E tests
- **Frontend calls Rust API endpoints** → Rust uses chromiumoxide for CDP

## T2.3: tokio::process — Context7
- **kill_on_drop(true)** — kills child when `Child` handle dropped (default: false)
- **Stdio::null()** — redirect stderr to /dev/null for `run_codex_turn`
- **output()** — collects both stdout+stderr (safe for `run_vibe_turn`)
- **No built-in timeout** — must wrap with `tokio::time::timeout(Duration, future)`

## T2.4: sonatype-guide
- chromiumoxide not in sonatype database (Rust crates coverage limited)
- cargo audit shows 1 vulnerability in rustls-native-certs (RUSTSEC-2025-0134) — pre-existing, not from chromiumoxide

## T2.5: CDP protocol compatibility
- Chrome ≥120 required for stable CDP features used by chromiumoxide 0.9.1
- CDP websocket binding: 127.0.0.1 only per Security Guardrails §5.4

## T2.7+T2.7b: chromiumoxide compiles with workspace tokio ✓
- chromiumoxide resolves tokio 1.52.3 in isolation; workspace uses 1.52.1
- Both 1.52.x — compatible under `version = "1"` workspace constraint

## T2.11: lightarchitects-cli NOT in this workspace ✓
- Confirmed: lightarchitects-cli source does not exist in lightarchitects-sdk
- TUI/CLI tasks correctly removed from plan in iter-3 gap analysis
