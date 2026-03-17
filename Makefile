# la-sdk — Light Architects SDK workspace
# Standard Light Architects Makefile targets

.PHONY: help quality test build fix push clean

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

fix: ## Auto-fix formatting and clippy issues
	cargo fmt --all
	cargo clippy --workspace --fix --allow-dirty --allow-staged --all-targets -- -D warnings

push: quality ## Quality gates + git push
	git push

clean: ## Clean build artifacts
	cargo clean
