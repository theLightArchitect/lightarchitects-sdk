#!/usr/bin/env node
/**
 * End-to-end continuity proof for session-sync-snapshot-handoff.
 *
 * The Playwright e2e spec proves the PLUMBING: a UUID passed via
 * `--resume-session` lands in `/api/setup/info`, `createBuild` pre-seeds
 * the `CopilotProcess`, and `/api/session/fork` emits the correct
 * `claude --resume <uuid>` command.
 *
 * This script proves the SEMANTICS: that when the webshell's
 * `run_print_turn` handler actually invokes `claude --resume <uuid>`
 * with a session ID it did NOT originate, the spawned `claude` CLI
 * reads the prior conversation from disk and continues it.
 *
 * Tier-1.5 (task #7): parametrized over a cwd matrix so we verify the
 * cwd-pinning fix in `run_print_turn` works for real project paths and
 * paths with spaces, not just `/tmp`.
 *
 * Protocol per cwd (≈2 Claude calls against subscription, $0 marginal):
 *
 *   Step 1  Seed a claude CLI session directly with a unique code phrase.
 *   Step 2  Capture the session UUID + confirm the JSONL landed on disk.
 *   Step 3  Spawn the webshell with --resume-session <uuid>.
 *   Step 4  Call POST /api/builds with resume_session_id=<uuid>.
 *   Step 5  Call POST /api/builds/:id/copilot asking Claude to recall
 *           the code phrase.
 *   Step 6  Assert the response contains the code phrase.
 *   Step 7  Assert the SAME JSONL file now has both turns appended.
 *
 * Run:
 *   node tests/e2e/continuity_proof.mjs
 *
 *   # single cwd override (skips the matrix):
 *   PROOF_CWD=/some/path node tests/e2e/continuity_proof.mjs
 *
 * Exits 0 on full success, non-zero on any failed assertion.
 */
import { spawn, spawnSync } from 'node:child_process';
import { readFileSync, realpathSync, statSync, existsSync, mkdirSync, rmSync } from 'node:fs';
import { resolve, join } from 'node:path';
import { homedir, platform } from 'node:os';

// ── Config ───────────────────────────────────────────────────────────────────

const BASE_PORT = Number(process.env.PROOF_PORT ?? 8746);
const REPO_ROOT = resolve(import.meta.dirname, '..', '..', '..');
const WEBSHELL_BIN = join(REPO_ROOT, 'target', 'release', 'lightarchitects-webshell');
const WEBSHELL_BIN_DEBUG = join(REPO_ROOT, 'target', 'debug', 'lightarchitects-webshell');

// ── Helpers ──────────────────────────────────────────────────────────────────

/**
 * Return a copy of `process.env` with Anthropic-auth-confounding variables
 * removed so spawned `claude` subprocesses fall through to OAuth/keychain.
 */
function cleanEnvForClaude() {
  const env = { ...process.env };
  delete env.ANTHROPIC_API_KEY;
  delete env.ANTHROPIC_AUTH_TOKEN;
  delete env.CLAUDE_CODE_OAUTH_TOKEN;
  delete env.CLAUDECODE;
  delete env.CLAUDE_CODE_ENTRYPOINT;
  delete env.CLAUDE_CODE_EXECPATH;
  return env;
}

function log(step, msg) { console.log(`    [step ${step}] ${msg}`); }
function sub(msg)       { console.log(`      · ${msg}`); }

/**
 * A controlled per-cwd failure: throws a proof-row error, returns up to the
 * matrix driver so other rows can still run.
 */
class RowFailure extends Error {}
function rowFail(msg) { throw new RowFailure(msg); }

async function waitForHealth(base, timeoutMs = 8000) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    try {
      const r = await fetch(`${base}/api/health`);
      if (r.ok) return true;
    } catch { /* keep polling */ }
    await new Promise(r => setTimeout(r, 150));
  }
  return false;
}

// Claude Code canonicalizes symlinks (e.g. /tmp → /private/tmp on macOS)
// BEFORE hashing, then rewrites non-filename-safe characters to `-`.
// Empirically: `/`, `.`, and ` ` are all mapped to `-`. Confirmed via the
// shape of existing project dirs like `-Users-kft--claude-commands`
// (where `/.claude` → `--claude`: the `/` and the `.` each became `-`).
function projectDirName(cwd) {
  const canonical = realpathSync(cwd);
  return canonical.replace(/[/. ]/g, '-');
}

function jsonlPathFor(sessionId, cwd) {
  return join(homedir(), '.claude', 'projects', projectDirName(cwd), `${sessionId}.jsonl`);
}

// ── The per-cwd proof body (steps 1–7) ───────────────────────────────────────

/**
 * Run the full 7-step proof for a single cwd. Returns a result object.
 * Throws RowFailure on any assertion failure so the matrix driver can
 * record the failure and continue with the next row.
 */
async function runProofForCwd(workCwd, port) {
  const token = 'continuity-proof-' + Date.now() + '-' + Math.random().toString(36).slice(2, 7);
  const base = `http://localhost:${port}`;
  const codePhrase = 'ORCHID-' + Math.random().toString(36).slice(2, 8).toUpperCase();

  log(1, `Seeding at cwd=${workCwd} with phrase ${codePhrase}`);

  const claudeEnv = cleanEnvForClaude();
  const seedResult = spawnSync(
    'claude',
    [
      '--print',
      '--output-format', 'stream-json',
      '--verbose',
      `Remember this code phrase exactly and reply with exactly: ${codePhrase}`,
    ],
    { cwd: workCwd, encoding: 'utf-8', maxBuffer: 10 * 1024 * 1024, env: claudeEnv },
  );
  if (seedResult.status !== 0) {
    const combined = (seedResult.stderr || '') + (seedResult.stdout || '');
    rowFail(`claude --print exited ${seedResult.status}: ${combined.slice(0, 400)}`);
  }

  let sessionId = null;
  let seedReply = null;
  for (const line of seedResult.stdout.split('\n')) {
    if (!line.trim()) continue;
    try {
      const obj = JSON.parse(line);
      if (obj.session_id && !sessionId) sessionId = obj.session_id;
      if (obj.type === 'result' && obj.subtype === 'success') seedReply = obj.result;
    } catch { /* ignore non-JSON lines */ }
  }
  if (!sessionId) rowFail('no session_id found in claude stream-json output');
  sub(`session UUID: ${sessionId}`);
  sub(`seed reply: ${(seedReply ?? '').slice(0, 80)}`);

  log(2, `Verifying JSONL`);
  const jsonlPath = jsonlPathFor(sessionId, workCwd);
  if (!existsSync(jsonlPath)) rowFail(`expected JSONL missing: ${jsonlPath}`);
  const stat1 = statSync(jsonlPath);
  const content1 = readFileSync(jsonlPath, 'utf-8');
  const lines1 = content1.split('\n').filter(l => l.trim()).length;
  sub(`${jsonlPath}`);
  sub(`size=${stat1.size}B lines=${lines1}`);
  if (!content1.includes(codePhrase)) rowFail(`code phrase absent from JSONL`);

  log(3, `Spawning webshell on port ${port}`);
  try { spawnSync('sh', ['-c', `lsof -ti:${port} | xargs -r kill -9 2>/dev/null`]); } catch { /**/ }

  const binPath = existsSync(WEBSHELL_BIN) ? WEBSHELL_BIN
                : existsSync(WEBSHELL_BIN_DEBUG) ? WEBSHELL_BIN_DEBUG
                : rowFail(`no webshell binary at ${WEBSHELL_BIN} or ${WEBSHELL_BIN_DEBUG}`);

  const webshell = spawn(
    binPath,
    ['--port', String(port), '--cwd', workCwd, '--resume-session', sessionId],
    {
      env: {
        ...cleanEnvForClaude(),
        LIGHTARCHITECTS_WEBSHELL_TOKEN: token,
        RUST_LOG: 'warn',
      },
      stdio: ['ignore', 'pipe', 'pipe'],
    },
  );
  // Throwaway: keep stdout/stderr silent per-row to cut noise across matrix.
  webshell.stdout.on('data', () => {});
  webshell.stderr.on('data', () => {});

  const cleanup = () => {
    if (!webshell.killed) webshell.kill('SIGTERM');
  };

  try {
    const up = await waitForHealth(base);
    if (!up) rowFail(`webshell did not come up on ${base} within 8s`);

    const infoRes = await fetch(`${base}/api/setup/info`);
    const info = await infoRes.json();
    if (info.resume_session !== sessionId) {
      rowFail(`setup/info resume_session mismatch: got=${info.resume_session}`);
    }

    log(4, 'Creating build with resume_session_id');
    const buildRes = await fetch(`${base}/api/builds`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json', Authorization: `Bearer ${token}` },
      body: JSON.stringify({ cwd: workCwd, resume_session_id: sessionId }),
    });
    if (!buildRes.ok) rowFail(`POST /api/builds failed: ${buildRes.status}`);
    const { build_id } = await buildRes.json();

    log(5, 'Sending recall query via copilot');
    const question =
      'Reply with EXACTLY the code phrase you remembered in the previous turn. ' +
      'No other words. If you do not remember, reply with the literal word FORGOT.';
    const copilotRes = await fetch(`${base}/api/builds/${build_id}/copilot`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json', Authorization: `Bearer ${token}` },
      body: JSON.stringify({ message: question }),
    });
    if (!copilotRes.ok) rowFail(`copilot POST ${copilotRes.status}: ${await copilotRes.text()}`);
    const copilotBody = await copilotRes.json();
    const response = typeof copilotBody.response === 'string'
      ? copilotBody.response
      : JSON.stringify(copilotBody);
    sub(`response: ${response.slice(0, 100)}`);

    log(6, 'Asserting recall');
    if (!response.includes(codePhrase)) {
      rowFail(`Continuity broken — response lacks code phrase. Got: ${response.slice(0, 200)}`);
    }

    log(7, 'Inspecting JSONL after resumed turn');
    const stat2 = statSync(jsonlPath);
    const content2 = readFileSync(jsonlPath, 'utf-8');
    const lines2 = content2.split('\n').filter(l => l.trim()).length;
    if (stat2.size <= stat1.size) rowFail(`JSONL did not grow: ${stat1.size}→${stat2.size}`);
    if (lines2 <= lines1) rowFail(`JSONL line count did not grow: ${lines1}→${lines2}`);
    sub(`JSONL grew ${stat1.size}B → ${stat2.size}B (+${stat2.size - stat1.size})`);

    return {
      pass: true,
      cwd: workCwd,
      sessionId,
      codePhrase,
      jsonlPath,
      jsonlBefore: stat1.size,
      jsonlAfter: stat2.size,
      response: response.slice(0, 120),
    };
  } finally {
    cleanup();
    // Give the OS a tick to reap the port before the next row reuses it.
    await new Promise(r => setTimeout(r, 300));
  }
}

// ── Matrix driver ────────────────────────────────────────────────────────────

if (platform() !== 'darwin') {
  console.log(`Skipping proof on non-macOS (${platform()})`);
  process.exit(0);
}

const claudeCheck = spawnSync('claude', ['--version'], { encoding: 'utf-8' });
if (claudeCheck.status !== 0) {
  console.error(`claude CLI not available: ${claudeCheck.stderr}`);
  process.exit(1);
}
console.log(`claude binary: ${claudeCheck.stdout.trim()}`);

// Decide the cwd matrix:
//   - single-cwd mode (env override): just that one.
//   - matrix mode: three distinct filesystem locations exercising
//     (1) the baseline /tmp, (2) a real project path, (3) a path
//     containing spaces (file-system edge case for claude's project-hash
//     derivation).
const SPACE_CWD = '/tmp/la-cwd-proof with spaces';
let matrix;
if (process.env.PROOF_CWD) {
  matrix = [{ cwd: process.env.PROOF_CWD, label: 'override' }];
} else {
  matrix = [
    { cwd: '/tmp',                                   label: 'baseline (/tmp)' },
    { cwd: '/Users/kft/Projects/lightarchitects-sdk', label: 'real project path' },
    { cwd: SPACE_CWD,                                label: 'path with spaces' },
  ];
}

// Create the spaces-cwd dir if it's in the matrix.
if (matrix.some(m => m.cwd === SPACE_CWD)) {
  mkdirSync(SPACE_CWD, { recursive: true });
}

const results = [];
let rowIndex = 0;
for (const { cwd, label } of matrix) {
  rowIndex += 1;
  const port = BASE_PORT + rowIndex; // avoid collisions across rows
  console.log(`\n── Row ${rowIndex}/${matrix.length}: ${label} ─────────`);
  try {
    const r = await runProofForCwd(cwd, port);
    results.push({ ...r, label });
  } catch (e) {
    if (e instanceof RowFailure) {
      console.error(`      ✘ ${e.message}`);
      results.push({ pass: false, cwd, label, error: e.message });
    } else {
      console.error(`      ✘ unexpected: ${e.message}`);
      results.push({ pass: false, cwd, label, error: e.message });
    }
  }
}

// Best-effort cleanup of the spaces-cwd dir (don't rm -rf from an arbitrary
// override; only clean what we created).
try { rmSync(SPACE_CWD, { recursive: true, force: true }); } catch { /**/ }

// ── Summary ──────────────────────────────────────────────────────────────────

const passed = results.filter(r => r.pass).length;
const failed = results.length - passed;

console.log('\n════════════════════════════════════════════════════════════════');
console.log(`  CONTINUITY PROOF — MATRIX: ${passed}/${results.length} passed`);
console.log('════════════════════════════════════════════════════════════════');
for (const r of results) {
  const mark = r.pass ? '✓' : '✘';
  console.log(`  ${mark} ${r.label.padEnd(28)} cwd=${r.cwd}`);
  if (r.pass) {
    console.log(`      session=${r.sessionId}`);
    console.log(`      JSONL: ${r.jsonlBefore}B → ${r.jsonlAfter}B`);
    console.log(`      response: ${r.response}`);
  } else {
    console.log(`      error: ${r.error}`);
  }
}
console.log('');

process.exit(failed === 0 ? 0 : 1);
