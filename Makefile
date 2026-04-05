# l-arc-sdk — Light Architects SDK workspace
# Standard Light Architects Makefile targets

.PHONY: help quality test test-features build deploy deploy-fast doc fix push clean

GATEWAY_BIN := $(HOME)/.lightarchitects/bin/lightarchitects

help: ## Show available targets
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | sort | \
		awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-15s\033[0m %s\n", $$1, $$2}'

quality: ## Run quality gates (fmt check + clippy + tests)
	cargo fmt --all -- --check
	cargo clippy --workspace --all-targets --all-features -- -D warnings
	cargo test --workspace --all-features

test: ## Run all tests
	cargo test --workspace --all-features

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
	mkdir -p "$(dir $(GATEWAY_BIN))"
	cp target/release/lightarchitects "$(GATEWAY_BIN)"
	codesign --force --sign - "$(GATEWAY_BIN)"
	@printf '{\n  "mcpServers": {\n    "lightarchitects": {\n      "command": "%s"\n    }\n  }\n}\n' \
		"$(HOME)/.lightarchitects/bin/lightarchitects" \
		> "$(HOME)/.lightarchitects/lightarchitects.mcp.json"
	@echo "Deployed → $(GATEWAY_BIN)"
	@echo "MCP config → $(HOME)/.lightarchitects/lightarchitects.mcp.json"

deploy-fast: ## Build + deploy gateway without quality gates
	cargo build --release -p lightarchitects-gateway
	mkdir -p "$(dir $(GATEWAY_BIN))"
	cp target/release/lightarchitects "$(GATEWAY_BIN)"
	codesign --force --sign - "$(GATEWAY_BIN)"
	@printf '{\n  "mcpServers": {\n    "lightarchitects": {\n      "command": "%s"\n    }\n  }\n}\n' \
		"$(HOME)/.lightarchitects/bin/lightarchitects" \
		> "$(HOME)/.lightarchitects/lightarchitects.mcp.json"
	@echo "Deployed → $(GATEWAY_BIN)"
	@echo "MCP config → $(HOME)/.lightarchitects/lightarchitects.mcp.json"

fix: ## Auto-fix formatting and clippy issues
	cargo fmt --all
	cargo clippy --workspace --fix --allow-dirty --allow-staged --all-targets -- -D warnings

push: quality ## Quality gates + git push
	git push

doc: ## Build and open documentation
	cargo doc --workspace --no-deps --open

clean: ## Clean build artifacts
	cargo clean
