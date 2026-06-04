# l-arc-sdk — Light Architects SDK workspace
# Standard Light Architects Makefile targets

.PHONY: help quality test test-gateway-smoke test-features test-claude-fixture-refresh build deploy deploy-fast rollback doc fix push clean lint-ask

GATEWAY_BIN      := $(HOME)/.lightarchitects/bin/lightarchitects
GATEWAY_PREV_BIN := $(HOME)/.lightarchitects/bin/lightarchitects.prev
GATEWAY_MIGRATIONS_SRC := lightarchitects-gateway/migrations/platform
GATEWAY_MIGRATIONS_DST := $(HOME)/.lightarchitects/migrations/platform

help: ## Show available targets
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | sort | \
		awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-15s\033[0m %s\n", $$1, $$2}'

quality: ## Run quality gates (fmt check + clippy + unit/integration tests)
	cargo fmt --all -- --check
	cargo clippy --workspace --all-targets --all-features -- -D warnings
	cargo test --workspace --all-features --lib --tests

test: ## Run unit and integration tests
	cargo test --workspace --all-features --lib --tests

test-gateway-smoke: ## Canon XXVII Suite 6 smoke tests — gateway G1 + chain-depth + action-allowlist (no subprocess)
	cd lightarchitects-gateway && cargo test --test smoke --features inline-all

doctest: ## Run doc-examples (separate gate — pre-existing crate:: path migration debt)
	cargo test --workspace --all-features --doc

## Isolated feature-gate tests — catches cross-feature contamination in the
## lightarchitects umbrella crate (the only crate with named sibling features).
##
## Security gates:
##   cargo tree --no-default-features -p lightarchitects | grep reqwest  → empty
##   cargo tree --features ayin-http -p lightarchitects  | grep reqwest  → exactly 1
##
## Baseline: ~102s on M-series Mac (update when adding crates)
test-features: ## Isolated feature-gate tests (catches cross-feature contamination)
	@echo "=== Feature: none (core only) ==="
	cargo test --workspace --no-default-features
	@echo "=== Feature: soul ==="
	cargo test --no-default-features --features soul -p lightarchitects
	@echo "=== Feature: corso ==="
	cargo test --no-default-features --features corso -p lightarchitects
	@echo "=== Feature: eva ==="
	cargo test --no-default-features --features eva -p lightarchitects
	@echo "=== Feature: quantum ==="
	cargo test --no-default-features --features quantum -p lightarchitects
	@echo "=== Feature: seraph ==="
	cargo test --no-default-features --features seraph -p lightarchitects
	@echo "=== Feature: ayin ==="
	cargo test --no-default-features --features ayin -p lightarchitects
	@echo "=== Feature: ayin-http ==="
	cargo test --no-default-features --features ayin-http -p lightarchitects
	@echo "=== Feature: full ==="
	cargo test --features full -p lightarchitects
	@echo "=== Feature: all ==="
	cargo test --workspace --all-features
	@echo "All feature combinations pass."

lint-ask: ## Count AskUserQuestion prose vs ```ask markers across installed SKILL.md files
	@echo "=== lint-ask: scanning SKILL.md files for HITL syntax ==="
	@SKILLS_DIR="$${HOME}/.claude/plugins/cache/light-architects/lightarchitects/1.0.0/skills"; \
	 if [ ! -d "$$SKILLS_DIR" ]; then \
	   echo "  SKIP: plugin cache not found at $$SKILLS_DIR"; exit 0; \
	 fi; \
	 ask_count=$$(grep -rl '^\`\`\`ask' "$$SKILLS_DIR" 2>/dev/null | wc -l | tr -d ' '); \
	 prose_count=$$(grep -rl 'AskUserQuestion' "$$SKILLS_DIR" 2>/dev/null | wc -l | tr -d ' '); \
	 echo "  \`\`\`ask markers : $$ask_count skill file(s)"; \
	 echo "  AskUserQuestion prose : $$prose_count skill file(s)"; \
	 echo "  app.skill.text_question_inline_total=$$ask_count"

build: ## Build all crates (release)
	cargo build --workspace --release

deploy: quality ## Quality gates + build + deploy gateway to ~/.lightarchitects/bin/
	# lightarchitects-gateway is excluded from the workspace (Cargo.toml exclude) due to
	# worktree lockfile collisions. If this fails with "did not match any packages", temporarily
	# add "lightarchitects-gateway" to workspace members, remove from exclude, build, then revert.
	cargo build --release -p lightarchitects-gateway
	mkdir -p "$(dir $(GATEWAY_BIN))" "$(GATEWAY_MIGRATIONS_DST)"
	@[ -f "$(GATEWAY_BIN)" ] && cp "$(GATEWAY_BIN)" "$(GATEWAY_PREV_BIN)" || true
	cp target/release/lightarchitects "$(GATEWAY_BIN)"
	cp -r "$(GATEWAY_MIGRATIONS_SRC)/." "$(GATEWAY_MIGRATIONS_DST)/"
	codesign --force --sign - "$(GATEWAY_BIN)"
	@printf '{\n  "mcpServers": {\n    "lightarchitects": {\n      "command": "%s"\n    }\n  }\n}\n' \
		"$(HOME)/.lightarchitects/bin/lightarchitects" \
		> "$(HOME)/.lightarchitects/lightarchitects.mcp.json"
	@sha="$$(git rev-parse HEAD 2>/dev/null || echo unknown)" && \
	 ts="$$(date -u '+%Y-%m-%dT%H:%M:%SZ')" && \
	 printf '{"version":"0.3.0","sha":"%s","deployed_at":"%s"}\n' "$$sha" "$$ts" \
	     > "$(HOME)/.lightarchitects/deploy-manifest.json"
	@echo "Deployed → $(GATEWAY_BIN)"
	@echo "Migrations → $(GATEWAY_MIGRATIONS_DST)"
	@echo "MCP config → $(HOME)/.lightarchitects/lightarchitects.mcp.json"
	@echo "Manifest  → $(HOME)/.lightarchitects/deploy-manifest.json"
	@echo ""
	@echo "⚠  Running gateway processes have the OLD code mapped in memory."
	@echo "   In Claude Code: /mcp → select 'lightarchitects' → Reconnect"
	@echo "   (Unix doesn't auto-reload binaries on file change — the running"
	@echo "    subprocess must be restarted for the new code to take effect.)"

deploy-fast: ## Build + deploy gateway without quality gates
	cargo build --release -p lightarchitects-gateway
	mkdir -p "$(dir $(GATEWAY_BIN))" "$(GATEWAY_MIGRATIONS_DST)"
	@[ -f "$(GATEWAY_BIN)" ] && cp "$(GATEWAY_BIN)" "$(GATEWAY_PREV_BIN)" || true
	cp target/release/lightarchitects "$(GATEWAY_BIN)"
	cp -r "$(GATEWAY_MIGRATIONS_SRC)/." "$(GATEWAY_MIGRATIONS_DST)/"
	codesign --force --sign - "$(GATEWAY_BIN)"
	@printf '{\n  "mcpServers": {\n    "lightarchitects": {\n      "command": "%s"\n    }\n  }\n}\n' \
		"$(HOME)/.lightarchitects/bin/lightarchitects" \
		> "$(HOME)/.lightarchitects/lightarchitects.mcp.json"
	@sha="$$(git rev-parse HEAD 2>/dev/null || echo unknown)" && \
	 ts="$$(date -u '+%Y-%m-%dT%H:%M:%SZ')" && \
	 printf '{"version":"0.3.0","sha":"%s","deployed_at":"%s"}\n' "$$sha" "$$ts" \
	     > "$(HOME)/.lightarchitects/deploy-manifest.json"
	@echo "Deployed → $(GATEWAY_BIN)"
	@echo "Migrations → $(GATEWAY_MIGRATIONS_DST)"
	@echo "MCP config → $(HOME)/.lightarchitects/lightarchitects.mcp.json"
	@echo "Manifest  → $(HOME)/.lightarchitects/deploy-manifest.json"
	@echo ""
	@echo "⚠  Running gateway processes have the OLD code mapped in memory."
	@echo "   In Claude Code: /mcp → select 'lightarchitects' → Reconnect"
	@echo "   (Unix doesn't auto-reload binaries on file change — the running"
	@echo "    subprocess must be restarted for the new code to take effect.)"

rollback: ## Restore the previous gateway binary (lightarchitects.prev → lightarchitects)
	@test -f "$(GATEWAY_PREV_BIN)" || \
	    (echo "ERROR: No previous binary at $(GATEWAY_PREV_BIN). Nothing to roll back."; exit 1)
	cp "$(GATEWAY_PREV_BIN)" "$(GATEWAY_BIN)"
	codesign --force --sign - "$(GATEWAY_BIN)"
	@echo "Rolled back → $(GATEWAY_BIN)"
	@echo "Run '/mcp' in Claude Code to reconnect."

fix: ## Auto-fix formatting and clippy issues
	cargo fmt --all
	cargo clippy --workspace --fix --allow-dirty --allow-staged --all-targets -- -D warnings

push: quality ## Quality gates + git push
	git push

doc: ## Build and open documentation
	cargo doc --workspace --no-deps --open

## Claude CLI fixture refresh — run when `claude --version` differs from
## lightarchitects/tests/fixtures/mock-claude.version, or when the [Q] gate
## detects event-shape divergence between fixture-replay and a live CLI run.
##
## Requires: real `claude` CLI accessible in PATH, network connectivity.
## Records: mock-claude.sh (canned NDJSON stream) + mock-claude.version (pinned version).
FIXTURE_DIR := lightarchitects/tests/fixtures
MOCK_SCRIPT := $(FIXTURE_DIR)/mock-claude.sh
VERSION_FILE := $(FIXTURE_DIR)/mock-claude.version

test-claude-fixture-refresh: ## Refresh Claude CLI mock fixture against current CLI version
	@command -v claude >/dev/null 2>&1 || \
	    (echo "ERROR: 'claude' CLI not found in PATH. Install Claude Code first."; exit 1)
	@echo "=== Refreshing Claude CLI fixture ==="
	@pinned="$$(cat $(VERSION_FILE) 2>/dev/null || echo none)" && \
	 live="$$(claude --version 2>/dev/null | head -1)" && \
	 echo "  Pinned: $$pinned" && \
	 echo "  Live:   $$live" && \
	 if [ "$$pinned" = "$$live" ]; then \
	     echo "  Versions match — fixture is current. Force-refresh with: make test-claude-fixture-refresh FORCE=1"; \
	     [ "$${FORCE:-0}" = "1" ] || exit 0; \
	 fi
	@echo "  Recording new NDJSON stream..."
	@printf '{"type":"system","subtype":"init","session_id":"mock-session-01","tools":[],"mcp_servers":[]}\n' > $(MOCK_SCRIPT).new
	@claude -p "Say exactly: Hello, world!" \
	    --output-format stream-json \
	    --verbose \
	    --max-turns 1 \
	    2>/dev/null >> $(MOCK_SCRIPT).new || true
	@echo "  Updating mock-claude.version..."
	@claude --version 2>/dev/null | head -1 > $(VERSION_FILE)
	@echo "  Updating mock-claude.sh..."
	@echo '#!/usr/bin/env bash' > $(MOCK_SCRIPT)
	@echo '# mock-claude.sh — refreshed by: make test-claude-fixture-refresh' >> $(MOCK_SCRIPT)
	@echo '# Pinned version: '"$$(cat $(VERSION_FILE))" >> $(MOCK_SCRIPT)
	@echo 'set -euo pipefail' >> $(MOCK_SCRIPT)
	@echo 'FORMAT="json"; VERSION_FLAG=0' >> $(MOCK_SCRIPT)
	@printf '%s\n' \
	    'while [[ $$# -gt 0 ]]; do case "$$1" in --output-format) FORMAT="$${2:-json}"; shift 2;; --verbose) shift;; --version) VERSION_FLAG=1; shift;; *) shift;; esac; done' >> $(MOCK_SCRIPT)
	@printf '%s\n' \
	    'if [[ "$$VERSION_FLAG" -eq 1 ]]; then cat "$$(dirname "$$0")/mock-claude.version"; exit 0; fi' >> $(MOCK_SCRIPT)
	@printf '%s\n' 'if [[ "$$FORMAT" == "stream-json" ]]; then' >> $(MOCK_SCRIPT)
	@cat $(MOCK_SCRIPT).new | while IFS= read -r line; do printf "    printf '%%s\\\\n' '%s'\n" "$$line"; done >> $(MOCK_SCRIPT)
	@printf '%s\n' '    exit 0; fi' >> $(MOCK_SCRIPT)
	@printf '%s\n' 'printf '"'"'%s\n'"'"' '"'"'{"type":"result","subtype":"success","result":"Hello, world!"}'"'"'' >> $(MOCK_SCRIPT)
	@chmod +x $(MOCK_SCRIPT)
	@rm -f $(MOCK_SCRIPT).new
	@echo "  Done. Run: cargo test -p lightarchitects --features agent-cli -- --test-threads=1"

clean: ## Clean build artifacts
	cargo clean
