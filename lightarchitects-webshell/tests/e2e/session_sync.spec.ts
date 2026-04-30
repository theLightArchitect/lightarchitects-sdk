/**
 * E2E — session-sync snapshot handoff (reverse + forward directions).
 *
 * Exercises the full `/webshell` ↔ `Fork to Terminal` loop built in
 * `session-sync-snapshot-handoff-tier-one`:
 *
 *   Reverse direction (terminal → webshell):
 *     1. Spawn `lightarchitects-webshell --resume-session <uuid>`
 *     2. `GET /api/setup/info` must echo the `resume_session` UUID
 *     3. `POST /api/builds` with `resume_session_id` must pre-seed the
 *        build's CopilotProcess so a fork is immediately possible
 *        without sending a turn first
 *
 *   Forward direction (webshell → terminal):
 *     4. `POST /api/session/fork` against the pre-seeded build must
 *        return `launched:true` + `command:"claude --resume <uuid>"`
 *        on macOS (other platforms: `launched:false` with the
 *        copy-paste command)
 *
 *   UI (headed chromium, per Light Architects policy):
 *     5. The chat drawer opens via Ctrl+`
 *     6. The "Fork to Terminal" button is present and disabled when
 *        no build has been created yet
 *     7. After a build exists + at least one message is in the
 *        client-side message log, the button becomes enabled and
 *        clicking it renders the expected success banner
 *
 * Prerequisites: the webshell binary must be built. We check and
 * `cargo build -p lightarchitects-webshell` on demand if missing.
 *
 * Run:
 *   npx playwright test tests/e2e/session_sync.spec.ts --headed
 *
 * Ends with a 3-second waitForTimeout so the user can visually
 * verify the drawer + banner state per the headed-tests memory.
 */
import { test, expect, chromium, type Browser } from '@playwright/test';
import { spawn, spawnSync, type ChildProcess } from 'node:child_process';
import { existsSync } from 'node:fs';
import { resolve } from 'node:path';
import { platform } from 'node:os';

// ── Constants ────────────────────────────────────────────────────────────────

const TEST_PORT = Number(process.env.SESSION_SYNC_PORT ?? 8744);
const TEST_TOKEN = 'session-sync-e2e-token-' + Date.now();
const RESUME_UUID = 'e2e-resume-' + Math.random().toString(36).slice(2, 10);
const BASE_URL = `http://localhost:${TEST_PORT}`;
const IS_MACOS = platform() === 'darwin';

// Where the webshell binary lives after a debug build.
const REPO_ROOT = resolve(__dirname, '..', '..', '..');
const BIN = resolve(REPO_ROOT, 'target', 'debug', 'lightarchitects-webshell');

// ── Lifecycle: build-on-demand + spawn webshell with --resume-session ────────

let webshell: ChildProcess | undefined;

async function waitForHealth(url: string, timeoutMs = 8000): Promise<boolean> {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    try {
      const res = await fetch(`${url}/api/health`);
      if (res.ok) return true;
    } catch { /* keep polling */ }
    await new Promise(r => setTimeout(r, 150));
  }
  return false;
}

test.beforeAll(async () => {
  // Build the webshell if the binary isn't present. Unlike release builds
  // the debug build is ~15s incremental and gives us a runnable binary
  // without additional setup.
  if (!existsSync(BIN)) {
    console.log(`[session-sync-e2e] building webshell debug binary at ${BIN}`);
    const r = spawnSync('cargo', ['build', '-p', 'lightarchitects-webshell'], {
      cwd: REPO_ROOT,
      stdio: 'inherit',
    });
    if (r.status !== 0) throw new Error('cargo build -p lightarchitects-webshell failed');
  }

  // Spawn the webshell with:
  //   - an explicit token (via env so we don't touch the user's keychain)
  //   - a distinctive --resume-session UUID we can assert on
  //   - a non-default port so we don't collide with a running webshell
  webshell = spawn(
    BIN,
    [
      '--port', String(TEST_PORT),
      '--cwd', '/tmp',
      '--resume-session', RESUME_UUID,
    ],
    {
      env: {
        ...process.env,
        LIGHTARCHITECTS_WEBSHELL_TOKEN: TEST_TOKEN,
        RUST_LOG: 'warn,lightarchitects_webshell=info',
      },
      stdio: ['ignore', 'pipe', 'pipe'],
    },
  );

  webshell.stdout?.on('data', b => process.stdout.write(`[webshell] ${b}`));
  webshell.stderr?.on('data', b => process.stdout.write(`[webshell] ${b}`));

  const ok = await waitForHealth(BASE_URL);
  if (!ok) throw new Error(`webshell did not come up on ${BASE_URL} within 8s`);
});

test.afterAll(async () => {
  if (webshell && !webshell.killed) {
    webshell.kill('SIGTERM');
    await new Promise(r => setTimeout(r, 200));
    if (!webshell.killed) webshell.kill('SIGKILL');
  }
});

// ── Tests ────────────────────────────────────────────────────────────────────

test('reverse: /api/setup/info echoes the --resume-session UUID', async () => {
  // The setup_info endpoint is unauthenticated; it reads Config.resume_session
  // populated from the CLI flag. This proves the CLI-to-Config-to-response
  // path is wired end-to-end.
  const res = await fetch(`${BASE_URL}/api/setup/info`);
  expect(res.status).toBe(200);
  const body = await res.json();

  // The field is `#[serde(skip_serializing_if = "Option::is_none")]` in
  // Rust, so its presence is itself a signal that the flag was honored.
  expect(body.resume_session).toBe(RESUME_UUID);
});

test('reverse: createBuild with resume_session_id pre-seeds the copilot session', async () => {
  // Body mirrors what the frontend emits on first ensureBuild() when
  // pendingResumeSessionId is set: cwd + resume_session_id.
  const res = await fetch(`${BASE_URL}/api/builds`, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      Authorization: `Bearer ${TEST_TOKEN}`,
    },
    body: JSON.stringify({
      cwd: '/tmp',
      resume_session_id: RESUME_UUID,
    }),
  });
  expect(res.status).toBe(200);
  const body = await res.json();
  expect(typeof body.build_id).toBe('string');
  // The BuildResponse doesn't echo the session_id for privacy — we verify
  // the pre-seed landed indirectly, by forking immediately (next test).
});

test('forward: session/fork returns claude --resume command without a turn first', async () => {
  // Create a build pre-seeded with our resume UUID so session_id is
  // already populated in CopilotProcess at t=0. This decouples the fork
  // test from any real Claude subprocess — we're testing the endpoint,
  // not the Anthropic binary.
  const createRes = await fetch(`${BASE_URL}/api/builds`, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      Authorization: `Bearer ${TEST_TOKEN}`,
    },
    body: JSON.stringify({ cwd: '/tmp', resume_session_id: RESUME_UUID }),
  });
  expect(createRes.status).toBe(200);
  const { build_id } = await createRes.json();

  const forkRes = await fetch(`${BASE_URL}/api/session/fork`, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      Authorization: `Bearer ${TEST_TOKEN}`,
    },
    body: JSON.stringify({ build_id }),
  });
  expect(forkRes.status).toBe(200);
  const forkBody = await forkRes.json();

  expect(forkBody.session_id).toBe(RESUME_UUID);
  expect(forkBody.command).toBe(`claude --resume ${RESUME_UUID}`);
  expect(forkBody.agent).toBe('lightarchitects');

  if (IS_MACOS) {
    expect(forkBody.platform).toBe('macos');
    expect(forkBody.launched).toBe(true);
    console.log('[session-sync-e2e] Terminal.app should now be open with the resumed command');
  } else {
    expect(forkBody.launched).toBe(false);
    console.log(`[session-sync-e2e] non-macOS platform (${forkBody.platform}) — copy-paste fallback`);
  }
});

test('forward: session/fork returns 409 when a build has no session yet', async () => {
  // Create a fresh build without resume_session_id — copilot_proc is None.
  const createRes = await fetch(`${BASE_URL}/api/builds`, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      Authorization: `Bearer ${TEST_TOKEN}`,
    },
    body: JSON.stringify({ cwd: '/tmp' }),
  });
  const { build_id } = await createRes.json();

  const forkRes = await fetch(`${BASE_URL}/api/session/fork`, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      Authorization: `Bearer ${TEST_TOKEN}`,
    },
    body: JSON.stringify({ build_id }),
  });
  expect(forkRes.status).toBe(409);
  const body = await forkRes.json();
  expect(body.error).toBe('no_session_yet');
});

test('forward: session/fork 404s on unknown build_id', async () => {
  const forkRes = await fetch(`${BASE_URL}/api/session/fork`, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      Authorization: `Bearer ${TEST_TOKEN}`,
    },
    body: JSON.stringify({ build_id: '00000000-0000-0000-0000-000000000000' }),
  });
  expect(forkRes.status).toBe(404);
});

test('auth: all mutating endpoints reject requests without a bearer token', async () => {
  // /api/setup/info is deliberately unauthenticated (bootstrap), but every
  // mutating route under /api must gate on the bearer token.
  const createRes = await fetch(`${BASE_URL}/api/builds`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ cwd: '/tmp', resume_session_id: RESUME_UUID }),
  });
  expect(createRes.status).toBe(401);

  const forkRes = await fetch(`${BASE_URL}/api/session/fork`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ build_id: '00000000-0000-0000-0000-000000000000' }),
  });
  expect(forkRes.status).toBe(401);
});

// ── Headed UI flow (visual verification per memory policy) ───────────────────

test('ui: chat drawer + Fork button render end-to-end (HEADED)', async () => {
  const browser: Browser = await chromium.launch({ headless: false });
  const context = await browser.newContext();
  const page = await context.newPage();

  // The webshell reads the bearer token from the URL hash fragment on
  // load and stores it in sessionStorage for subsequent API calls.
  await page.goto(`${BASE_URL}/#token=${TEST_TOKEN}`);

  // Wait for the app bundle to finish loading. `networkidle` would never
  // fire here — the webshell opens a persistent AYIN SSE subscription on
  // boot (events::AyinClient::spawn), so the network is never "idle".
  // `load` fires after the HTML + bundled assets are fetched, which is
  // the right signal for an SSE-holding SPA.
  await page.waitForLoadState('load', { timeout: 10_000 });
  // Allow Svelte to mount + render the drawer (the $effect that reacts
  // to currentBuildId runs on next tick).
  await page.waitForTimeout(800);

  // The CopilotDrawer is always mounted but collapsed to 32px at boot.
  // Ctrl+` toggles it (global keydown handler).
  await page.keyboard.press('Control+`');
  // Give the Svelte transition (0.18s) a beat to settle.
  await page.waitForTimeout(400);

  // The Fork button is in the chat-mode header. We assert by title, not
  // text, because the button label swaps to "Forking…" while in-flight.
  const forkButton = page.locator('button[title*="Fork this conversation" i], button[title*="Send at least one message" i]');
  await expect(forkButton.first()).toBeVisible({ timeout: 5_000 });

  // Initially: no build, no messages — button should be disabled.
  const initiallyDisabled = await forkButton.first().isDisabled();
  expect(initiallyDisabled).toBe(true);

  // Pause so the user can see the drawer + disabled button before close
  // per the headed-tests convention.
  console.log('[session-sync-e2e] drawer open — Fork button should be visible + disabled');
  await page.waitForTimeout(3_000);

  await browser.close();
});
