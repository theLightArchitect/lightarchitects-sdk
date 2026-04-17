# lightarchitects-webshell Makefile
# "The eye of the LORD is upon them" — Psalm 33:18 (KJV)
# Build: luminous-weaving-nautilus

.PHONY: help build test quality fix push deploy deploy-fast web web-test run

.DEFAULT_GOAL := help

BINARY     := lightarchitects-webshell
DEPLOY_DIR := $(HOME)/lightarchitects/webshell/bin
WEB_DIR    := web
SYMLINK    := $(HOME)/.local/bin/webshell

help: ## Show available targets
	@echo ""
	@echo "lightarchitects-webshell"
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | awk 'BEGIN {FS = ":.*?## "}; {printf "  %-15s %s\n", $$1, $$2}'
	@echo ""

# ── Frontend ────────────────────────────────────────────────────────────────

web: ## Build frontend bundle into web/dist/
	@echo "Building frontend…"
	@cd $(WEB_DIR) && pnpm install --frozen-lockfile
	@cd $(WEB_DIR) && pnpm build
	@echo "Frontend built: $(WEB_DIR)/dist/"

web-test: ## Run TypeScript tests (vitest)
	@cd $(WEB_DIR) && pnpm test

# ── Backend ─────────────────────────────────────────────────────────────────

build: ## Build release binary (requires web/dist/ to be current)
	@echo "Building $(BINARY)…"
	@cargo build --release -p lightarchitects-webshell --quiet

test: ## Run Rust test suite
	@cargo test --workspace --quiet

quality: ## Check code quality: fmt + clippy + Rust tests + TS tests
	@cargo fmt -- --check
	@cargo clippy --workspace --all-targets -- -D warnings
	@cargo test --workspace --quiet
	@cd $(WEB_DIR) && pnpm test

fix: ## Auto-fix fmt and clippy issues
	@cargo fmt
	@cargo clippy --fix --allow-dirty --allow-staged --workspace --all-targets

push: quality ## Quality gates + git push
	@git push

# ── Deploy ───────────────────────────────────────────────────────────────────

deploy: web quality ## Build frontend + quality gates + deploy to ~/lightarchitects/webshell/bin/
	@echo "Building release binary…"
	@cargo build --release -p lightarchitects-webshell --quiet
	@mkdir -p $(DEPLOY_DIR)
	@if [ -f "$(DEPLOY_DIR)/$(BINARY)" ]; then \
		cp -f "$(DEPLOY_DIR)/$(BINARY)" "$(DEPLOY_DIR)/$(BINARY).bak"; \
	fi
	@cp target/release/$(BINARY) $(DEPLOY_DIR)/$(BINARY).tmp
	@mv -f $(DEPLOY_DIR)/$(BINARY).tmp $(DEPLOY_DIR)/$(BINARY)
	@chmod +x $(DEPLOY_DIR)/$(BINARY)
	@codesign -fs - $(DEPLOY_DIR)/$(BINARY) 2>/dev/null || echo "WARNING: codesign failed (non-blocking)"
	@mkdir -p $(dir $(SYMLINK))
	@ln -sf $(DEPLOY_DIR)/$(BINARY) $(SYMLINK)
	@echo ""
	@echo "Deployed: $(DEPLOY_DIR)/$(BINARY)"
	@echo "Symlink:  $(SYMLINK) → $(DEPLOY_DIR)/$(BINARY)"
	@echo "Launch:   webshell"

deploy-fast: ## Deploy without quality gates (assumes web/dist/ is current)
	@echo "Building release binary (fast)…"
	@cargo build --release -p lightarchitects-webshell --quiet
	@mkdir -p $(DEPLOY_DIR)
	@if [ -f "$(DEPLOY_DIR)/$(BINARY)" ]; then \
		cp -f "$(DEPLOY_DIR)/$(BINARY)" "$(DEPLOY_DIR)/$(BINARY).bak"; \
	fi
	@cp target/release/$(BINARY) $(DEPLOY_DIR)/$(BINARY).tmp
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
