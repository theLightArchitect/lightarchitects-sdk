# l-arc-sdk — Light Architects SDK workspace
# Standard Light Architects Makefile targets

.PHONY: help quality test build deploy deploy-fast doc fix push clean

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

build: ## Build all crates (release)
	cargo build --workspace --release

deploy: quality ## Quality gates + build + deploy gateway to ~/.lightarchitects/bin/
	cargo build --release -p lightarchitects-gateway
	mkdir -p "$(dir $(GATEWAY_BIN))"
	cp target/release/lightarchitects "$(GATEWAY_BIN)"
	@printf '{\n  "mcpServers": {\n    "lightarchitects": {\n      "command": "%s"\n    }\n  }\n}\n' \
		"$(HOME)/.lightarchitects/bin/lightarchitects" \
		> "$(HOME)/.lightarchitects/lightarchitects.mcp.json"
	@echo "Deployed → $(GATEWAY_BIN)"
	@echo "MCP config → $(HOME)/.lightarchitects/lightarchitects.mcp.json"

deploy-fast: ## Build + deploy gateway without quality gates
	cargo build --release -p lightarchitects-gateway
	mkdir -p "$(dir $(GATEWAY_BIN))"
	cp target/release/lightarchitects "$(GATEWAY_BIN)"
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
