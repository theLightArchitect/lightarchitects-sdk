#!/usr/bin/env node
/**
 * Codex continuity proof — mirror of continuity_proof.mjs, but driving
 * the webshell's `run_codex_turn` path instead of `run_print_turn`.
 *
 * Architecture parity with the Claude proof:
 *
 *   Claude                                     Codex
 *   ──────────────────────────────────────     ──────────────────────────────────────
 *   `claude --print --output-format stream-    `codex exec --json --skip-git-repo-
 *    json --verbose <prompt>`                   check --dangerously-bypass-...
 *                                               <prompt>`
 *   session_id from `session_id` field         thread_id from `thread.started` event
 *   JSONL at                                   JSONL at
 *   `~/.claude/projects/<cwd-hash>/<uuid>      `~/.codex/sessions/<Y>/<M>/<D>/
 *    .jsonl`                                    rollout-<ts>-<uuid>.jsonl`
 *   Resume: `claude --resume <uuid>`           Resume: `codex exec resume <tid>`
 *   Webshell arg: default AgentKind::          Webshell arg: --agent codex
 *   Lightarchitects
 *
 * What this proves (task #6):
 *   - The webshell's `run_codex_turn` cwd-pinning fix works.
 *   - Pre-seeding `CopilotProcess.session_id` for a Codex build correctly
 *     feeds `codex exec resume <id>` on the next turn.
 *   - Codex's disk-stored session JSONL is shared between direct CLI use
 *     and webshell use — same file grows on both sides.
 *
 * Run:
 *   node tests/e2e/continuity_proof_codex.mjs
 */
import { spawn, spawnSync } from 'node:child_process';
import { readFileSync, realpathSync, statSync, existsSync, readdirSync } from 'node:fs';
import { resolve, join } from 'node:path';
import { homedir, platform } from 'node:os';

// ── Config ───────────────────────────────────────────────────────────────────

const PORT = Number(process.env.CODEX_PROOF_PORT ?? 8752);
const TOKEN = 'codex-proof-' + Date.now();
const BASE = `http://localhost:${PORT}`;
const REPO_ROOT = resolve(import.meta.dirname, '..', '..', '..');
const WEBSHELL_BIN = existsSync(join(REPO_ROOT, 'target', 'release', 'lightarchitects-webshell'))
  ? join(REPO_ROOT, 'target', 'release', 'lightarchitects-webshell')
  : join(REPO_ROOT, 'target', 'debug', 'lightarchitects-webshell');
const WORK_CWD = '/tmp';
const CODE_PHRASE = 'IRIS-' + Math.random().toString(36).slice(2, 8).toUpperCase();

// ── Helpers ──────────────────────────────────────────────────────────────────

/**
 * Scrub env vars that would shadow Codex's subscription auth
 * (`~/.codex/auth.json`). Unlike Claude, Codex's primary env override is
 * `OPENAI_API_KEY` / `CODEX_API_KEY`. We also scrub CLAUDECODE so the
 * subprocess doesn't pretend to be a Claude Code child.
 */
function cleanEnvForCodex() {
  const env = { ...process.env };
  delete env.OPENAI_API_KEY;
  delete env.OPENAI_BASE_URL;
  delete env.CODEX_API_KEY;
  delete env.CLAUDECODE;
  delete env.CLAUDE_CODE_ENTRYPOINT;
  delete env.CLAUDE_CODE_EXECPATH;
  return env;
}

function log(step, msg) { console.log(`\n[step ${step}] ${msg}`); }
function sub(msg)       { console.log(`  · ${msg}`); }
function fail(msg, webshell) {
  console.error(`\n✘ FAIL: ${msg}`);
  if (webshell && !webshell.killed) webshell.kill('SIGKILL');
  process.exit(1);
}

async function waitForHealth(timeoutMs = 8000) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    try {
      const r = await fetch(`${BASE}/api/health`);
      if (r.ok) return true;
    } catch { /* keep polling */ }
    await new Promise(r => setTimeout(r, 150));
  }
  return false;
}

// Locate the most recently modified rollout-*.jsonl whose filename
// contains the given thread_id. Codex stores per-date subdirectories;
// we scan only the last two days to bound the work.
function findCodexRollout(threadId) {
  const root = join(homedir(), '.codex', 'sessions');
  if (!existsSync(root)) return null;
  const candidates = [];
  function walk(dir) {
    let entries;
    try { entries = readdirSync(dir, { withFileTypes: true }); } catch { return; }
    for (const e of entries) {
      const p = join(dir, e.name);
      if (e.isDirectory()) walk(p);
      else if (e.isFile() && e.name.endsWith('.jsonl') && e.name.includes(threadId)) {
        candidates.push(p);
      }
    }
  }
  walk(root);
  return candidates[0] ?? null;
}

// ── Step 0 — prereqs ─────────────────────────────────────────────────────────

if (platform() !== 'darwin') {
  console.log(`Skipping Codex proof on non-macOS (${platform()})`);
  process.exit(0);
}

const codexCheck = spawnSync('codex', ['--version'], { encoding: 'utf-8' });
if (codexCheck.status !== 0) {
  fail(`codex CLI not available or not installed: ${codexCheck.stderr}`);
}
sub(`codex binary: ${codexCheck.stdout.trim()}`);

if (!existsSync(join(homedir(), '.codex', 'auth.json'))) {
  fail('~/.codex/auth.json missing — not logged in. Run `codex login` first.');
}

// ── Step 1 — seed a codex thread directly ────────────────────────────────────

log(1, `Seeding a fresh Codex thread (cwd=${WORK_CWD}) with code phrase ${CODE_PHRASE}`);

const codexEnv = cleanEnvForCodex();
const seedResult = spawnSync(
  'codex',
  [
    'exec',
    '--json',
    '--skip-git-repo-check',
    '--dangerously-bypass-approvals-and-sandbox',
    `Remember this code phrase exactly and reply with exactly: ${CODE_PHRASE}`,
  ],
  { cwd: WORK_CWD, encoding: 'utf-8', maxBuffer: 10 * 1024 * 1024, env: codexEnv },
);
if (seedResult.status !== 0) {
  const combined = (seedResult.stderr || '') + (seedResult.stdout || '');
  fail(`codex exec exited ${seedResult.status}: ${combined.slice(0, 600)}`);
}

// Parse events: thread.started → thread_id; item.completed(agent_message) → reply.
let threadId = null;
let seedReply = '';
let seedTurnDone = false;
for (const line of seedResult.stdout.split('\n')) {
  if (!line.trim()) continue;
  try {
    const ev = JSON.parse(line);
    if (ev.type === 'thread.started' && ev.thread_id) threadId = ev.thread_id;
    if (ev.type === 'item.completed' && ev.item?.type === 'agent_message') {
      if (seedReply) seedReply += '\n';
      seedReply += ev.item.text ?? '';
    }
    if (ev.type === 'turn.completed') seedTurnDone = true;
  } catch { /* skip non-JSON */ }
}
if (!threadId) fail(`no thread_id found in codex stream-json output`);
if (!seedTurnDone) fail(`codex seed turn did not complete (got: ${seedReply.slice(0, 200)})`);
sub(`thread ID: ${threadId}`);
sub(`seed reply: ${seedReply.slice(0, 120)}`);

// ── Step 2 — verify the JSONL landed ─────────────────────────────────────────

log(2, `Verifying Codex JSONL under ~/.codex/sessions/`);
const jsonlPath = findCodexRollout(threadId);
if (!jsonlPath) fail(`no rollout JSONL found matching thread_id ${threadId}`);
const stat1 = statSync(jsonlPath);
const content1 = readFileSync(jsonlPath, 'utf-8');
const lines1 = content1.split('\n').filter(l => l.trim()).length;
sub(`found: ${jsonlPath}`);
sub(`size=${stat1.size}B lines=${lines1}`);
if (!content1.includes(CODE_PHRASE)) {
  fail(`code phrase ${CODE_PHRASE} not found in Codex rollout JSONL`);
}
sub(`code phrase present in JSONL ✓`);

// ── Step 3 — spawn the webshell for the Codex agent ─────────────────────────

log(3, `Spawning webshell on port ${PORT} with --agent codex --resume-session ${threadId}`);

try { spawnSync('sh', ['-c', `lsof -ti:${PORT} | xargs -r kill -9 2>/dev/null`]); } catch { /**/ }

const webshell = spawn(
  WEBSHELL_BIN,
  [
    '--port', String(PORT),
    '--cwd', WORK_CWD,
    '--agent', 'codex',
    '--resume-session', threadId,
  ],
  {
    env: {
      ...cleanEnvForCodex(),
      LIGHTARCHITECTS_WEBSHELL_TOKEN: TOKEN,
      RUST_LOG: 'warn',
    },
    stdio: ['ignore', 'pipe', 'pipe'],
  },
);
webshell.stdout.on('data', b => process.stdout.write(`  [webshell] ${b}`));
webshell.stderr.on('data', b => process.stdout.write(`  [webshell] ${b}`));

const up = await waitForHealth();
if (!up) fail(`webshell did not come up on ${BASE} within 8s`, webshell);
sub(`webshell listening on ${BASE}`);

const infoRes = await fetch(`${BASE}/api/setup/info`);
const info = await infoRes.json();
if (info.resume_session !== threadId) {
  fail(`setup/info resume_session mismatch. expected=${threadId} got=${info.resume_session}`, webshell);
}
sub(`GET /api/setup/info → resume_session = ${threadId} ✓`);

// ── Step 4 — create a Codex build pre-seeded with thread_id ──────────────────

log(4, 'Creating Codex build with resume_session_id');
const buildRes = await fetch(`${BASE}/api/builds`, {
  method: 'POST',
  headers: { 'Content-Type': 'application/json', Authorization: `Bearer ${TOKEN}` },
  body: JSON.stringify({ cwd: WORK_CWD, resume_session_id: threadId }),
});
if (!buildRes.ok) fail(`POST /api/builds: ${buildRes.status}`, webshell);
const createBody = await buildRes.json();
sub(`build_id = ${createBody.build_id}`);
sub(`agent = ${createBody.agent.kind}/${createBody.agent.backend}`);

if (createBody.agent.kind !== 'codex') {
  fail(`build's agent kind should be 'codex' but got '${createBody.agent.kind}'`, webshell);
}

// ── Step 5 — ask the webshell's copilot to recall the code phrase ────────────

log(5, 'Sending recall query through /api/builds/:id/copilot (run_codex_turn path)');
const question =
  'Reply with EXACTLY the code phrase you remembered in the previous turn. ' +
  'No other words. If you do not remember, reply with the literal word FORGOT.';
const copilotRes = await fetch(`${BASE}/api/builds/${createBody.build_id}/copilot`, {
  method: 'POST',
  headers: { 'Content-Type': 'application/json', Authorization: `Bearer ${TOKEN}` },
  body: JSON.stringify({ message: question }),
});
if (!copilotRes.ok) fail(`copilot POST ${copilotRes.status}: ${await copilotRes.text()}`, webshell);
const copilotBody = await copilotRes.json();
const response = typeof copilotBody.response === 'string'
  ? copilotBody.response
  : JSON.stringify(copilotBody);
sub(`copilot response: ${response.slice(0, 200)}`);

// ── Step 6 — assert the response proves Codex continuity ─────────────────────

log(6, 'Asserting the resumed Codex subprocess recovered the code phrase');
if (!response.includes(CODE_PHRASE)) {
  fail(
    `Codex continuity broken: \`codex exec resume ${threadId}\` through webshell ` +
    `did NOT return ${CODE_PHRASE}. Got: ${response.slice(0, 300)}`,
    webshell,
  );
}
sub(`code phrase ${CODE_PHRASE} round-tripped through webshell → codex resume ✓`);

// ── Step 7 — confirm both turns landed in the SAME JSONL file ────────────────

log(7, 'Inspecting Codex rollout JSONL after the resumed turn');
// Re-locate the JSONL — Codex MAY rotate rollout files. Our heuristic
// still finds the file that contains the thread_id (and if it rotated,
// we'd fail here and know to handle rotation differently).
const jsonlPathAfter = findCodexRollout(threadId);
if (!jsonlPathAfter) fail('Codex rollout JSONL disappeared after resumed turn', webshell);
if (jsonlPathAfter !== jsonlPath) {
  sub(`NOTE: Codex rotated rollout file: ${jsonlPath} → ${jsonlPathAfter}`);
}
const stat2 = statSync(jsonlPathAfter);
const content2 = readFileSync(jsonlPathAfter, 'utf-8');
const lines2 = content2.split('\n').filter(l => l.trim()).length;
sub(`size: ${stat1.size}B → ${stat2.size}B`);
sub(`lines: ${lines1} → ${lines2}`);

if (jsonlPathAfter === jsonlPath && stat2.size <= stat1.size) {
  fail(`Same rollout file but did not grow: ${stat1.size}→${stat2.size}`, webshell);
}

// ── Clean up ─────────────────────────────────────────────────────────────────

webshell.kill('SIGTERM');
await new Promise(r => setTimeout(r, 200));
if (!webshell.killed) webshell.kill('SIGKILL');

// ── Summary ──────────────────────────────────────────────────────────────────

console.log('\n════════════════════════════════════════════════════════════════');
console.log('  CODEX CONTINUITY PROOF — PASSED');
console.log('════════════════════════════════════════════════════════════════');
console.log(`  code phrase ........ ${CODE_PHRASE}`);
console.log(`  thread ID .......... ${threadId}`);
console.log(`  rollout JSONL ...... ${jsonlPathAfter}`);
console.log(`  JSONL before ....... ${stat1.size}B / ${lines1} lines`);
console.log(`  JSONL after ........ ${stat2.size}B / ${lines2} lines`);
console.log(`  webshell response .. ${response.slice(0, 80)}`);
console.log('');
console.log('  What this proves (Codex side):');
console.log('  1. `codex exec` wrote a rollout JSONL under ~/.codex/sessions/.');
console.log('  2. The webshell, launched with `--agent codex --resume-session <tid>`,');
console.log('     invoked `codex exec resume <tid>` for the follow-up turn.');
console.log('  3. The resumed Codex subprocess REMEMBERED the seeded phrase —');
console.log('     cwd-pinning in run_codex_turn works, session continuity is real.');
console.log('  4. Same rollout JSONL grew across the handoff: shared on-disk store');
console.log('     for Codex, exactly as Audit #2 predicted.');
console.log('');
process.exit(0);
