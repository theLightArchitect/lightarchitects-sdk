# Operator Handoff — ironclaw-spine

Cold-context reference for operating the autonomous build pipeline.
No prior knowledge of ironclaw-spine assumed.

---

## Prerequisites

| Requirement | Check |
|-------------|-------|
| `lightarchitects` binary | `~/.lightarchitects/bin/lightarchitects --version` |
| Neo4j | `curl -s bolt://localhost:7687` returns a connection (used by SOUL knowledge graph) |
| Webshell binary | `~/.lightarchitects/bin/webshell --version` |
| Anthropic API key | `echo $ANTHROPIC_API_KEY` (or Keychain — see below) |

The binary is at `~/.lightarchitects/bin/lightarchitects`. Add it to PATH if not already present:

```bash
export PATH="$HOME/.lightarchitects/bin:$PATH"
```

If you launched from a Claude Code session, strip the session key so Keychain OAuth credentials
are used instead:

```bash
env -u ANTHROPIC_API_KEY lightarchitects webshell start
```

---

## Step 1 — Start the webshell server

The webshell serves the build UI at `http://localhost:8733` and exposes the REST/SSE API the
conductor writes to.

```bash
lightarchitects webshell start
```

Default port: **8733**. Override with `--port <N>`.

Verify it is up:

```bash
lightarchitects webshell status
# Expected: HTTP 200 from http://localhost:8733/api/health
```

Leave this process running in a dedicated terminal or tmux pane. All subsequent commands in
other panes assume webshell is live.

---

## Step 2 — Start the autonomous build supervisor

The conductor is a background daemon that owns the task queue and drives wave execution.

```bash
lightarchitects conductor start
```

What this does:
- Forks `lightarchitects conductor run` as a detached child
- Writes PID to `~/.lightarchitects/conductor.pid`
- Logs to `~/.lightarchitects/logs/conductor-daemon.log`

Check it is running:

```bash
lightarchitects conductor status
```

Expected output:

```
Conductor Status
  Pending:     0
  In Progress: 0
  Completed:   N
  Failed:      0
  Heartbeat:   Xs ago (healthy)    ← must be < 120s
  Daemon:      running (PID XXXXX)
```

If heartbeat shows STALE or "not running", restart:

```bash
lightarchitects conductor stop
lightarchitects conductor start
```

Tail the daemon log:

```bash
lightarchitects conductor logs
```

---

## Step 3 — Trigger an autonomous build

An autonomous build is submitted via `POST /api/builds` with `mode: "autonomous"`. The CLI
wrapper accepts a plan file:

```bash
lightarchitects build --autonomous --plan ~/.claude/plans/<plan>.md
```

Replace `<plan>.md` with your target plan file, for example:

```bash
lightarchitects build --autonomous --plan ~/.claude/plans/ironclaw-spine.md
```

The command prints the assigned build UUID:

```
Build created: 3f8a1c2d-4b5e-6f7a-8c9d-0e1f2a3b4c5d
  Mode: autonomous
  URL:  http://localhost:8733/#/builds/3f8a1c2d-4b5e-6f7a-8c9d-0e1f2a3b4c5d
```

Note the UUID. You will need it to inspect decisions and watch wave progress.

### Mode field

Every build record carries a `mode` field:

| Value | Meaning |
|-------|---------|
| `"interactive"` | Default. Claude Code PTY session; operator drives each step manually. |
| `"autonomous"` | Conductor-driven. Waves execute automatically; operator confirms gate transitions. |

The `mode` is echoed in the `POST /api/builds` response and is visible in the BuildDetail
screen header.

---

## Step 4 — APPROVE gate (autonomous mode only)

When `mode = "autonomous"`, the webshell requires operator confirmation before wave 1 begins.

Open the build URL in a browser:

```
http://localhost:8733/#/builds/<build_id>
```

The **PreflightPanel** appears in the Intake screen after you select "Autonomous" mode. It
runs environment checks (Core / Important / Optional categories) and shows pass/warn/fail
status for each. A preflight must reach **Ready** or **Degraded** status before the
conductor proceeds.

For tool-call permissions during execution, an **APPROVE / DENY** card appears in the
AgentConsole panel whenever the agent requests a medium-risk or high-risk operation. The card
shows:

- The tool name being requested
- A truncated input summary (up to 240 chars)
- A countdown timer (seconds remaining)
- Risk tier: Low / Medium / High / Critical

Click **APPROVE** to allow the tool call or **DENY** to reject it. Denied calls emit an L4
escalation entry in the decision log. If the timer expires without operator action, the
request is treated as denied.

API equivalent (for scripted approval):

```bash
curl -s -X POST http://localhost:8733/api/builds/<build_id>/copilot/approve \
  -H "Content-Type: application/json" \
  -H "X-LA-Notify-Token: <notify_token>" \
  -d '{"call_id": "<call_id>", "comment": "approved by operator"}'
```

The notify token is written to `~/.lightarchitects/webshell/.token` at startup.

---

## Step 5 — Watch wave progression

Navigate to the build detail screen:

```
http://localhost:8733/#/builds/<build_id>
```

The BuildDetail screen shows real-time wave state via SSE. Each wave progresses through these
states in order:

```
QUEUED → IN_PROGRESS → GATE_PENDING → GATE_PASS
                                    → GATE_FAIL
                              (on GATE_PASS) → COMPLETE
```

| State | Meaning |
|-------|---------|
| `QUEUED` | Wave is scheduled; conductor has not started it yet |
| `IN_PROGRESS` | Workers are executing; FixAgents may be running |
| `GATE_PENDING` | All workers finished; waiting for gate evaluation |
| `GATE_PASS` | Gate dimensions passed; wave advances |
| `GATE_FAIL` | Gate failed after FixAgent retries exhausted; escalated to operator |
| `COMPLETE` | All waves finished; build is done |

The seven CORSO pillars (arch / sec / qual / perf / test / doc / ops) each run as parallel
workers within a wave. Gate evaluation fires after all pillar workers report.

### FixAgent retries

When a gate dimension fails, a FixAgent is spawned to attempt automatic remediation. The
ReviewGate cap is **3 iterations per gate failure**. The BuildDetail screen shows a
`fix_agent_iteration` event before each pass, including the 1-based iteration counter and
a short summary of the failing dimension.

After 3 failed FixAgent iterations, the gate transitions to `GATE_FAIL` and the conductor
emits an L4 escalation entry. At that point operator intervention is required.

To retry a failed agent manually:

```bash
curl -s -X POST http://localhost:8733/api/dispatch/retry/<dispatch_id>/<agent_slot> \
  -H "X-LA-Notify-Token: <notify_token>"
```

---

## Step 6 — Inspect the decision log

After (or during) a build, the decision log records every architectural, implementation, and
quality-gate choice the autonomous system made.

### File location

```
~/.lightarchitects/builds/decisions/<build_id>.ndjson
```

One NDJSON file per build. The directory is created automatically on first write.

### Read the log

```bash
cat ~/.lightarchitects/builds/decisions/<build_id>.ndjson
```

Pretty-print with `jq`:

```bash
cat ~/.lightarchitects/builds/decisions/<build_id>.ndjson | jq .
```

Filter to escalations only:

```bash
cat ~/.lightarchitects/builds/decisions/<build_id>.ndjson | jq 'select(.level == "L4")'
```

### Field reference

Each line is a JSON object with these fields:

| Field | Type | Description |
|-------|------|-------------|
| `line_n` | integer | Zero-based line index. Stable key across reads. |
| `timestamp` | string | ISO-8601 UTC timestamp of when the decision was recorded. |
| `level` | string | Decision level (see taxonomy below). |
| `decision` | string | Human-readable description of the decision made. |
| `canon_ref` | string or absent | Canon URI that governs this decision, e.g. `"canon://builders-cookbook#§64"`. Absent on L4 escalations. |
| `hmac` | string or absent | Hex-encoded HMAC-SHA256 chain tag for this entry. |
| `hmac_ok` | boolean or absent | `true` = chain intact. `false` = tampering or corruption detected. Absent on live L4 entries appended by the UI before server-side verification. |

### Decision level taxonomy

| Level | Name | When emitted |
|-------|------|-------------|
| `L1` | ARCHITECTURAL | High-impact structural choices (crate layout, API surface, dependency selection). |
| `L2` | IMPLEMENTATION | Implementation-level choices (algorithm selection, error handling strategy). |
| `L3` | QUALITY GATE | Gate evaluation outcomes (pass/fail per CORSO pillar dimension). |
| `L4` | ESCALATION | FixAgent retries exhausted; operator action required. Also emitted when a tool-call permission is denied. |

### API endpoint

The webshell exposes a REST endpoint that the DecisionLog UI component uses:

```bash
curl -s http://localhost:8733/api/builds/<build_id>/decisions \
  -H "X-LA-Notify-Token: <notify_token>" | jq .
```

Returns a JSON array of `DecisionEntry` objects in line-number order.

---

## Step 7 — Interpret the HMAC chain

The decision log is an append-only HMAC-SHA256 chain. Each entry's HMAC covers the previous
entry's HMAC plus the current entry's content, preventing silent log tampering.

### Chain construction

```
entry[0].hmac = HMAC-SHA256(pepper, "0||<decision>||<canon_ref>")
entry[N].hmac = HMAC-SHA256(pepper, "<entry[N-1].hmac>||<decision>||<canon_ref>")
```

The pepper is a server-side secret generated at webshell startup (never exposed in the API).

### Interpreting chain status in the UI

The DecisionLog panel in BuildDetail shows each entry with a coloured left border:

| Border colour | Level |
|---------------|-------|
| Blue (`--la-focus-ring`) | L1 ARCHITECTURAL |
| Steel blue | L2 IMPLEMENTATION |
| Amber | L3 QUALITY GATE |
| Red | L4 ESCALATION |

**Normal operation: entries display no badge.** The absence of a badge means `hmac_ok === true` — the chain is intact. You only see UI indicators when something is wrong.

When `hmac_ok === false` the entry displays a **"⚠ HMAC"** badge in amber. This means either:

- The log file was edited after the fact, or
- A byte was corrupted during write (rare; check disk health)

An entry without `hmac_ok` (absent field) is a live L4 escalation appended by the UI from an
SSE event before the server has written and verified it. This is expected and not an error.

### Verifying the chain offline

The `verify_chain()` function in `lightarchitects-webshell/src/events/decisions.rs` is the
authoritative verifier. To verify manually, re-compute each HMAC using the chain rule above.
You cannot verify without the pepper — the pepper lives only in the running webshell process.

If you need audit evidence for a completed build, export via the API while the server is
still running:

```bash
curl -s http://localhost:8733/api/builds/<build_id>/decisions \
  -H "X-LA-Notify-Token: <notify_token>" > decisions-export-<build_id>.json
```

The exported objects include `hmac_ok: true` when the server has verified them.

---

## Quick-reference command table

```bash
# Startup sequence
lightarchitects webshell start
lightarchitects conductor start
lightarchitects conductor status

# Trigger a build
lightarchitects build --autonomous --plan ~/.claude/plans/<plan>.md

# Watch status
lightarchitects conductor status
open http://localhost:8733/#/builds/<build_id>

# Inspect decisions
cat ~/.lightarchitects/builds/decisions/<build_id>.ndjson | jq .
cat ~/.lightarchitects/builds/decisions/<build_id>.ndjson | jq 'select(.level == "L4")'

# Fetch decisions via API (includes server-verified hmac_ok)
curl -s http://localhost:8733/api/builds/<build_id>/decisions \
  -H "X-LA-Notify-Token: $(cat ~/.lightarchitects/webshell/.token)"

# Logs
lightarchitects conductor logs
tail -f ~/.lightarchitects/logs/conductor-daemon.log

# Stop
lightarchitects conductor stop
```

---

## Troubleshooting

### Conductor heartbeat is STALE

```bash
lightarchitects conductor stop
lightarchitects conductor start
lightarchitects conductor status   # heartbeat should reset within 30s
```

### Build stuck in GATE_PENDING

The gate is waiting for all pillar workers to report. Check:

```bash
curl -s http://localhost:8733/api/builds/<build_id> \
  -H "X-LA-Notify-Token: $(cat ~/.lightarchitects/webshell/.token)" | jq .status
```

If status has not changed in >5 minutes, a worker may have stalled. Retry via the dispatch
endpoint or restart the build.

### "already running" on conductor start

A stale PID file exists. Safe to remove:

```bash
lightarchitects conductor stop   # cleans up PID file even if process is dead
lightarchitects conductor start
```

### Webshell port in use

Use `--port` to select an alternate port:

```bash
lightarchitects webshell start --port 8734
```

Update any hardcoded `http://localhost:8733` references in your workflow accordingly.

### Neo4j not reachable

The SOUL knowledge graph requires Neo4j on `bolt://localhost:7687`. If SOUL tools fail:

```bash
# Check Neo4j service (macOS with homebrew)
brew services list | grep neo4j
brew services start neo4j

# Verify bolt port
nc -z localhost 7687 && echo "OK" || echo "UNREACHABLE"
```

---

## File locations summary

| Path | Purpose |
|------|---------|
| `~/.lightarchitects/bin/lightarchitects` | Gateway binary |
| `~/.lightarchitects/conductor.pid` | Conductor daemon PID |
| `~/.lightarchitects/logs/conductor-daemon.log` | Conductor log |
| `~/.lightarchitects/builds/decisions/<build_id>.ndjson` | Per-build decision log (HMAC-chained) |
| `~/.lightarchitects/webshell/.token` | X-LA-Notify-Token for API auth |
| `~/.lightarchitects/conductor-queue.json` | Task queue (conductor state) |
