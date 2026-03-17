# la-crypto — Scripture-forged cryptographic foundation
# Standard Light Architects Makefile targets

.PHONY: help quality test build deploy deploy-fast fix push clean

help: ## Show available targets
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | sort | \
		awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-15s\033[0m %s\n", $$1, $$2}'

quality: ## Run quality gates (fmt check + clippy + tests)
	cargo fmt --check
	cargo clippy --all-targets --all-features -- -D warnings
	cargo test --all-features

test: ## Run all tests
	cargo test --all-features

build: ## Build release binary
	cargo build --release

fix: ## Auto-fix formatting and clippy issues
	cargo fmt
	cargo clippy --fix --allow-dirty --allow-staged --all-targets -- -D warnings

push: quality ## Quality gates + git push
	git push

clean: ## Clean build artifacts
	cargo clean
