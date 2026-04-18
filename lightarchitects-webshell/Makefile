# lightarchitects-webshell Makefile
# "The eye of the LORD is upon them" — Psalm 33:18 (KJV)

.PHONY: help build test quality fix push deploy deploy-fast mockcli run

.DEFAULT_GOAL := help

BINARY         := lightarchitects-webshell
DEPLOY_DIR     := $(HOME)/lightarchitects/webshell/bin
MOCKCLI_DIR    := $(HOME)/Projects/Lightarchitectmockcli
SYMLINK        := $(HOME)/.local/bin/webshell
# Cargo workspace root is one level up; binary is placed there by cargo.
CARGO_TARGET   := ../target/release/$(BINARY)

help: ## Show available targets
	@echo ""
	@echo "lightarchitects-webshell"
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | awk 'BEGIN {FS = ":.*?## "}; {printf "  %-15s %s\n", $$1, $$2}'
	@echo ""

# ── Frontend ────────────────────────────────────────────────────────────────

mockcli: ## Build Svelte Mockcli frontend into Lightarchitectmockcli/dist/ (embedded by rust-embed)
	@echo "Building Mockcli frontend…"
	@cd $(MOCKCLI_DIR) && pnpm install --frozen-lockfile
	@cd $(MOCKCLI_DIR) && pnpm build
	@echo "Mockcli built: $(MOCKCLI_DIR)/dist/"

# ── Backend ─────────────────────────────────────────────────────────────────

build: ## Build release binary (requires Mockcli dist/ to be current)
	@echo "Building $(BINARY)…"
	@cargo build --release -p lightarchitects-webshell --quiet

test: ## Run Rust test suite (webshell crate only)
	@cargo test -p lightarchitects-webshell --quiet

quality: ## Check code quality: fmt + clippy + Rust tests + Mockcli tests
	@cargo fmt -- --check
	@cargo clippy -p lightarchitects-webshell --all-targets -- -D warnings
	@cargo test -p lightarchitects-webshell --quiet
	@cd $(MOCKCLI_DIR) && pnpm test:run

fix: ## Auto-fix fmt and clippy issues
	@cargo fmt
	@cargo clippy --fix --allow-dirty --allow-staged --workspace --all-targets

push: quality ## Quality gates + git push
	@git push

# ── Deploy ───────────────────────────────────────────────────────────────────

deploy: mockcli quality ## Build Mockcli + quality gates + deploy to ~/lightarchitects/webshell/bin/
	@echo "Building release binary…"
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

deploy-fast: ## Deploy without quality gates (assumes Mockcli dist/ is current)
	@echo "Building release binary (fast)…"
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
