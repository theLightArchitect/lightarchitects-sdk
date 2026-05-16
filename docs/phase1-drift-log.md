# harvesting-converging-quasar — Phase 1 Drift Log
**Date**: 2026-05-16
**Operator**: kft + claude

## Inventory summary

| Repo | Branch | Clean? | feat/* count | Archive |
|------|--------|--------|-------------|---------|
| lightarchitects-cli | main | DIRTY (44 untracked) | 13 | cli-wip-archive-20260513 |
| CORSO-DEV | main | DIRTY (Cargo.lock mod) | 4 | corso-wip-archive-20260513 |
| EVA-DEV/eva | main | CLEAN | 4 | eva-wip-archive-20260513 |
| AYIN-DEV | main | DIRTY (Cargo.lock mod + .superpowers/) | 5+1=6 | ayin-wip-archive-20260513 |
| lightarchitects-next | main | DIRTY (.superpowers/ untracked) | 6 | next-wip-archive-20260513 |

## Drift from triage report
- CLI main: 44 untracked files (.cargo/, .github/, HOMEBREW.md, docs/architecture/* — dev spillover)
- AYIN: new branch `feat/weaving-grafting-canon` (not in original triage)
- CLI branches `feat/claude-oauth` and `feat/lens-system`: 0 commits ahead of main → already merged
- NEXT: `feat/marketing-integration` is a worktree-linked integration branch (+7 commits)

## Final KEEP/REWORK/DROP decisions (Phase 1-4 collapsed)

### KEEP (20 branches)
#### CLI
- feat/pipeline-extensions — 12 new pipeline modules + workflows; P2 execution substrate
- feat/execution-spine — typed ExecutionPolicy/RuntimeState; P2 orchestration
- feat/sdk-native-siblings — StdioTransport sibling clients; P2 multi-agent
- feat/turnlog-integration — TurnLogAdapter + HMAC + helix; P2 knowledge
- feat/phase13-context-wiring — context wiring, actor rename; P2 substrate
- feat/hook-guards — cargo/test/dep quality gate hooks; P2 quality
- feat/mcp-extensions — MCP tool extensions; P2 parity
- feat/rubric-grading — C1-C8 output grading; P2 quality tooling
- feat/retrieval-distillation — semantic context distillation; P2 context management parity

#### CORSO
- feat/corso-license-proprietary
- feat/corso-path-rebrand — critical path migration
- feat/corso-skills-update
- feat/corso-task-manager-router — P2 task management

#### EVA
- feat/eva-license-proprietary
- feat/eva-path-rebrand — critical path migration
- feat/eva-skills-update

#### AYIN
- feat/ayin-license-proprietary
- feat/ayin-plugin-restructure — P2 plugin infrastructure
- feat/ayin-semconv — P2 observability semantic conventions
- feat/ayin-trace-propagation — P2 W3C trace propagation

### REWORK → KEEP (3 branches)
- feat/distribution — adds install.sh + Homebrew; name migration (laex0→lightarchitects-cli) incomplete
- feat/eva-phase-b-tools — EVA tools expansion; pre-flagged .unwrap() at mod.rs:275,287,293
- feat/ayin-http-dashboard — HTTP dashboard; supply-chain audit required (Cargo.lock +4244, vendored d3/three.js)

### DROP (10 branches)
- feat/claude-oauth — already in main (0 commits ahead)
- feat/lens-system — already in main (0 commits ahead)
- feat/reference-material — docs-only; belongs in helix not a branch
- feat/weaving-grafting-canon — single docs commit; belongs in PR on main
- feat/next-* (all 6) — not Northstar critical (P1/P2 = CLI/webshell/orchestration, not marketing site)
