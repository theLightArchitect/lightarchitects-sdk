# lightarchitects-webshell Makefile
# "The eye of the LORD is upon them" — Psalm 33:18 (KJV)

.PHONY: help build test quality fix push deploy deploy-fast ui run

.DEFAULT_GOAL := help

BINARY         := lightarchitects-webshell
DEPLOY_DIR     := $(HOME)/.lightarchitects/bin
UI_DIR         := ../lightarchitects-webshell-ui
SYMLINK        := $(HOME)/.local/bin/webshell
# Cargo workspace root is one level up; binary is placed there by cargo.
CARGO_TARGET   := ../target/release/$(BINARY)

help: ## Show available targets
	@echo ""
	@echo "lightarchitects-webshell"
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | awk 'BEGIN {FS = ":.*?## "}; {printf "  %-15s %s\n", $$1, $$2}'
	@echo ""

# ── Frontend ────────────────────────────────────────────────────────────────

ui: ## Build Svelte frontend into lightarchitects-webshell-ui/dist/ (embedded by rust-embed)
	@echo "Building webshell UI…"
	@cd $(UI_DIR) && pnpm install --frozen-lockfile
	@cd $(UI_DIR) && pnpm build
	@echo "UI built: $(UI_DIR)/dist/"

# ── Backend ─────────────────────────────────────────────────────────────────

build: ## Build release binary (requires webshell UI dist/ to be current)
	@echo "Building $(BINARY)…"
	@cargo build --release -p lightarchitects-webshell --quiet

test: ## Run Rust test suite (webshell crate only)
	@cargo test -p lightarchitects-webshell --quiet

quality: ## Check code quality: fmt + clippy + Rust tests + webshell UI tests
	@cargo fmt -- --check
	@cargo clippy -p lightarchitects-webshell --all-targets -- -D warnings
	@cargo test -p lightarchitects-webshell --quiet
	@cd $(UI_DIR) && pnpm test:run

fix: ## Auto-fix fmt and clippy issues
	@cargo fmt
	@cargo clippy --fix --allow-dirty --allow-staged --workspace --all-targets

push: quality ## Quality gates + git push
	@git push

# ── Deploy ───────────────────────────────────────────────────────────────────

deploy: ui quality ## Build UI + quality gates + deploy to ~/.lightarchitects/bin/
	@echo "Building release binary…"
	@touch src/static_assets.rs  # force cargo to re-embed dist/ via rust-embed
	@cargo build --release -p lightarchitects-webshell --quiet
	@mkdir -p $(DEPLOY_DIR)
	@if [ -f "$(DEPLOY_DIR)/$(BINARY)" ]; then \
		cp -f "$(DEPLOY_DIR)/$(BINARY)" "$(DEPLOY_DIR)/$(BINARY).bak"; \
	fi
	@cp $(CARGO_TARGET) $(DEPLOY_DIR)/$(BINARY).tmp
	@mv -f $(DEPLOY_DIR)/$(BINARY).tmp $(DEPLOY_DIR)/$(BINARY)
	@chmod +x $(DEPLOY_DIR)/$(BINARY)
	@codesign -fs - $(DEPLOY_DIR)/$(BINARY) 2>/dev/null || echo "WARNING: codesign failed (non-blocking)"
	@mkdir -p $(dir $(SYMLINK))
	@ln -sf $(DEPLOY_DIR)/$(BINARY) $(SYMLINK)
	@echo ""
	@echo "Deployed: $(DEPLOY_DIR)/$(BINARY)"
	@echo "Symlink:  $(SYMLINK) → $(DEPLOY_DIR)/$(BINARY)"
	@echo "Launch:   webshell"

deploy-fast: ## Deploy without quality gates (assumes webshell UI dist/ is current)
	@echo "Building release binary (fast)…"
	@touch src/static_assets.rs  # force cargo to re-embed dist/
	@cargo build --release -p lightarchitects-webshell --quiet
	@mkdir -p $(DEPLOY_DIR)
	@if [ -f "$(DEPLOY_DIR)/$(BINARY)" ]; then \
		cp -f "$(DEPLOY_DIR)/$(BINARY)" "$(DEPLOY_DIR)/$(BINARY).bak"; \
	fi
	@cp $(CARGO_TARGET) $(DEPLOY_DIR)/$(BINARY).tmp
	@mv -f $(DEPLOY_DIR)/$(BINARY).tmp $(DEPLOY_DIR)/$(BINARY)
	@chmod +x $(DEPLOY_DIR)/$(BINARY)
	@codesign -fs - $(DEPLOY_DIR)/$(BINARY) 2>/dev/null || echo "WARNING: codesign failed (non-blocking)"
	@mkdir -p $(dir $(SYMLINK))
	@ln -sf $(DEPLOY_DIR)/$(BINARY) $(SYMLINK)
	@echo "Deployed: $(DEPLOY_DIR)/$(BINARY)"

run: ## Deploy (fast) and launch the webshell server
	@$(MAKE) deploy-fast
	@echo ""
	@echo "Launching webshell…"
	@$(SYMLINK)
