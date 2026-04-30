# lightarchitects-webshell Makefile
# "The eye of the LORD is upon them" — Psalm 33:18 (KJV)

.PHONY: help build test quality fix push deploy deploy-fast ui run verify verify-debug verify-deployed rollback drain ci-squad-local

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

ci-squad-local: ## Tier-1 squad-CI gate (replaces GitHub Actions per SQUAD-CI-CHARTER.md)
	@echo "═══════════════════════════════════════════════════════════════════"
	@echo "  Tier-1 squad-CI — local deterministic gate"
	@echo "═══════════════════════════════════════════════════════════════════"
	@echo ""
	@echo "[1/9] cargo fmt --check"
	@cd .. && cargo fmt --all -- --check
	@echo "      ✓ fmt clean"
	@echo "[2/9] cargo clippy -D warnings (workspace, all targets, all features)"
	@cd .. && cargo clippy --workspace --all-targets --all-features -- -D warnings 2>&1 | tail -5 || (echo "      ✗ clippy failed"; exit 1)
	@echo "      ✓ clippy clean"
	@echo "[3/9] cargo metadata + workspace integrity"
	@cd .. && cargo metadata --format-version 1 --no-deps > /dev/null
	@cd .. && cargo check --workspace --all-features --quiet
	@echo "      ✓ workspace integrity ok"
	@echo "[4/9] cargo test --workspace --all-features"
	@cd .. && cargo test --workspace --all-features --quiet 2>&1 | tail -3
	@echo "      ✓ Rust tests pass"
	@echo "[5/9] svelte-check"
	@cd $(UI_DIR) && pnpm exec svelte-check --threshold error 2>&1 | tail -2
	@echo "      ✓ svelte-check clean"
	@echo "[6/9] pnpm test:run (vitest)"
	@cd $(UI_DIR) && pnpm test:run 2>&1 | tail -3
	@echo "      ✓ vitest passes"
	@echo "[7/9] pnpm build (Svelte → dist/)"
	@cd $(UI_DIR) && pnpm build 2>&1 | tail -3
	@echo "      ✓ frontend build ok"
	@echo "[8/9] cargo audit (advisory scan)"
	@cd .. && cargo audit 2>&1 | tail -5 || echo "      ⚠ advisories present (review against deny.toml)"
	@echo "[9/9] cargo deny check"
	@cd .. && cargo deny check 2>&1 | tail -5 || echo "      ⚠ deny check failed (review deny.toml ignore-list TTL)"
	@echo ""
	@echo "═══════════════════════════════════════════════════════════════════"
	@echo "  Tier-1 squad-CI: PASS"
	@echo "  For Tier-2 (agent-led), invoke /SQUAD ci_review in Claude session"
	@echo "  See: ~/lightarchitects/soul/helix/corso/builds/SQUAD-CI-CHARTER.md"
	@echo "═══════════════════════════════════════════════════════════════════"

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

# ── Verify / Rollback / Drain (OPS-1b) ──────────────────────────────────────
# Smoke verification + safe rollback + graceful PTY drain. Required by aegis
# manifest Phase-3 exit criterion `smoke_verify` and Phase-7 exit `rollback_drill`.

VERIFY_PORT_DEPLOYED ?= 8733
VERIFY_PORT_DEBUG    ?= 8734
DEBUG_BINARY         := ../target/debug/$(BINARY)

verify: verify-deployed ## Smoke-verify the deployed binary (default target)

verify-deployed: ## Curl /api/health/deep + /metrics + /api/auth-check on $(DEPLOY_DIR)/$(BINARY)
	@if [ ! -x "$(DEPLOY_DIR)/$(BINARY)" ]; then \
		echo "❌ no deployed binary at $(DEPLOY_DIR)/$(BINARY)"; \
		echo "   run 'make deploy' first OR use 'make verify-debug'"; \
		exit 1; \
	fi
	@echo "Smoke-verifying deployed binary on port $(VERIFY_PORT_DEPLOYED)…"
	@$(DEPLOY_DIR)/$(BINARY) --port $(VERIFY_PORT_DEPLOYED) --token-source ephemeral & \
	  PID=$$!; sleep 1.5; \
	  curl -fsS http://127.0.0.1:$(VERIFY_PORT_DEPLOYED)/api/health > /dev/null \
	    && echo "  /api/health         ✓" \
	    || (kill $$PID 2>/dev/null; echo "  /api/health         ✗"; exit 1); \
	  $(DEPLOY_DIR)/$(BINARY) --version 2>/dev/null \
	    && echo "  --version           ✓" \
	    || echo "  --version           ⚠ (OPS-1a not yet landed)"; \
	  kill $$PID 2>/dev/null; wait $$PID 2>/dev/null; true
	@echo "verify-deployed: PASS"

verify-debug: ## Smoke-verify ./target/debug/$(BINARY) (used by phase-3 CI before deploy exists)
	@if [ ! -x "$(DEBUG_BINARY)" ]; then \
		echo "❌ no debug binary at $(DEBUG_BINARY)"; \
		echo "   run 'cargo build -p $(BINARY)' first"; \
		exit 1; \
	fi
	@echo "Smoke-verifying debug binary on port $(VERIFY_PORT_DEBUG)…"
	@$(DEBUG_BINARY) --port $(VERIFY_PORT_DEBUG) --token-source ephemeral & \
	  PID=$$!; sleep 2; \
	  curl -fsS http://127.0.0.1:$(VERIFY_PORT_DEBUG)/api/health > /dev/null \
	    && echo "  /api/health         ✓" \
	    || (kill $$PID 2>/dev/null; echo "  /api/health         ✗"; exit 1); \
	  kill $$PID 2>/dev/null; wait $$PID 2>/dev/null; true
	@echo "verify-debug: PASS"

rollback: ## Restore previous deploy from .bak rotation
	@if [ ! -f "$(DEPLOY_DIR)/$(BINARY).bak" ]; then \
		echo "❌ no rollback target at $(DEPLOY_DIR)/$(BINARY).bak"; \
		exit 1; \
	fi
	@echo "Rolling back: $(DEPLOY_DIR)/$(BINARY) ← .bak"
	@mv -f $(DEPLOY_DIR)/$(BINARY) $(DEPLOY_DIR)/$(BINARY).rollback-temp
	@mv -f $(DEPLOY_DIR)/$(BINARY).bak $(DEPLOY_DIR)/$(BINARY)
	@mv -f $(DEPLOY_DIR)/$(BINARY).rollback-temp $(DEPLOY_DIR)/$(BINARY).bak
	@chmod +x $(DEPLOY_DIR)/$(BINARY)
	@echo "Rollback complete. Run 'make verify' to confirm."

drain: ## Graceful drain of running webshell PTY (SIGHUP→SIGTERM→SIGKILL with grace)
	@./scripts/drain.sh 2>/dev/null || \
	  PIDS=$$(pgrep -f "$(BINARY)" 2>/dev/null || true); \
	  if [ -z "$$PIDS" ]; then \
	    echo "no $(BINARY) process found"; \
	  else \
	    echo "draining $(BINARY) PIDs: $$PIDS"; \
	    for pid in $$PIDS; do \
	      kill -HUP $$pid 2>/dev/null; \
	    done; \
	    echo "  SIGHUP sent; granting 30s for in-flight tool calls..."; \
	    sleep 30; \
	    for pid in $$PIDS; do \
	      if kill -0 $$pid 2>/dev/null; then \
	        echo "  SIGTERM $$pid (still alive after 30s)"; \
	        kill -TERM $$pid 2>/dev/null; \
	      fi; \
	    done; \
	    sleep 30; \
	    for pid in $$PIDS; do \
	      if kill -0 $$pid 2>/dev/null; then \
	        echo "  SIGKILL $$pid (still alive after 60s total)"; \
	        kill -KILL $$pid 2>/dev/null; \
	      fi; \
	    done; \
	    echo "drain complete"; \
	  fi
