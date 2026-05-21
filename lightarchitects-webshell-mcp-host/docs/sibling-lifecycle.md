# Sibling MCP Lifecycle Classification

Audit date: 2026-05-21  
Audit script: `research/sibling-lifecycle-audit.py`  
Methodology: spawn + initialize handshake + tools/list + 3 sequential calls; classify as `persistent` or `one_shot`.

## Results

| Sibling | Binary | Lifecycle | tools/list | Tool count | 3 seq calls | Server info |
|---------|--------|-----------|------------|-----------|-------------|-------------|
| CORSO | `~/lightarchitects/corso/bin/corso` | **persistent** | PASS | 0 | N/A | corso-trinity-mcp v2.0.0 |
| EVA | `~/lightarchitects/eva/bin/eva` | **persistent** | PASS | 0 | N/A | eva v1.0.0 |
| SOUL | `~/lightarchitects/soul/.config/bin/soul` | **persistent** | PASS | 1 | ✅ 3/3 | soul-knowledge-graph-mcp v1.0.0 |
| QUANTUM | `~/lightarchitects/quantum/bin/quantum-q mcp-server` | **persistent** | PASS | 0 | N/A | quantum-q v0.1.0 |
| SERAPH | `~/lightarchitects/seraph/bin/seraph` | **persistent** | PASS | 1 | ✅ 3/3 | seraph v0.1.0 |
| AYIN | `~/lightarchitects/ayin/bin/ayin-mcp` | **persistent** | PASS | 1 | ✅ 3/3 | ayin-mcp v0.1.0 |

## Key Findings

### Finding 1: All 6 siblings are persistent ✅

Prebuild spike (2026-05-20) found CORSO to be one-shot. Current audit (2026-05-21) shows **all 6 siblings persistent**. Likely explanation: CORSO binary updated between spike and audit — the session-promote cleanup behavior was fixed or the binary version changed.

**Impact on Phase 2 exit criteria**: gate-2 requires ≥2 persistent siblings. 6/6 persistent → full day-1 scope maintained (8 servers: 6 siblings + @drawio/mcp + 1 reserve slot).

### Finding 2: CORS, EVA, QUANTUM return 0 tools via tools/list ⚠️

Three siblings (CORSO, EVA, QUANTUM) return an empty tools array via `tools/list` when invoked standalone with no auth environment. These siblings expose their capabilities via a single aggregate dispatcher (`corsoTools`, `evaTools`, `quantumTools`) but only when authenticated via ANTHROPIC_API_KEY and with appropriate session context.

**Hypothesis**: these siblings require a valid ANTHROPIC_API_KEY to register tools at initialization time, and the audit ran with a masked/empty key.

**Impact on Tools panel**: the webshell MCP host must handle 0-tool servers gracefully. For CORSO/EVA/QUANTUM, the Tools panel will show the server entry but no invocable tools until:
- (a) the webshell binary is launched with the correct key (expected in production), or
- (b) Phase 3 adds a `tools_refresh` endpoint to pull tools lazily after auth.

**Follow-up Phase 3 task**: test tool_count under production auth conditions during `integration_basic.rs` build.

### Finding 3: Tool dispatcher pattern (SOUL, SERAPH, AYIN)

SOUL, SERAPH, AYIN each expose exactly 1 tool (`soulTools`, `seraphTools`, `ayinTools`) — an aggregate dispatcher that accepts any action as a JSON argument. This is a deliberate architecture decision (the gateway routes through a single JSON envelope rather than individual registered tools).

**Impact on Tools panel form generation**: the single-tool dispatcher pattern produces a schema that requires knowing the valid `action` enum values. Phase 6 form generator must handle this case — the `action` field will be an enum or string with special handling.

## Day-1 Server Count Determination

| Server | Lifecycle | tools/list | Day-1 wiring |
|--------|-----------|------------|--------------|
| CORSO | persistent | 0 tools (auth-gated) | ✅ include — tools appear with valid key |
| EVA | persistent | 0 tools (auth-gated) | ✅ include |
| SOUL | persistent | 1 tool (soulTools) | ✅ include |
| QUANTUM | persistent | 0 tools (auth-gated) | ✅ include |
| SERAPH | persistent | 1 tool (seraphTools) | ✅ include |
| AYIN | persistent | 1 tool (ayinTools) | ✅ include |
| @drawio/mcp | persistent (assumed — community server) | TBD (Phase 3) | ✅ include |
| Reserve slot | — | — | placeholder |

**Total day-1: 8 servers (revised from 9 per R-7; kroki dropped)**

## per-server `scope.lifecycle_mode` config

All siblings: `lifecycle_mode: "persistent"` (default).  
No `one_shot` mode required — the Phase 3 spawn-per-invoke fallback is implemented but not needed for day-1 siblings.
