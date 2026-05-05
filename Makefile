# l-arc-sdk — Light Architects SDK workspace
# Standard Light Architects Makefile targets

.PHONY: help quality test test-features build deploy deploy-fast rollback doc fix push clean

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

build: ## Build all crates (release)
	cargo build --workspace --release

deploy: quality ## Quality gates + build + deploy gateway to ~/.lightarchitects/bin/
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

clean: ## Clean build artifacts
	cargo clean
