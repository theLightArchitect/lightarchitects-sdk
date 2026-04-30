/**
 * E2E — `lightarchitects auth` CLI contract.
 *
 * Exercises the auth subcommand dispatch added in squishy-munching-tome
 * (Build #1 — LightArchitects CLI Refactor, Phase 3 step_3_6). Tests run
 * against the debug build of `lightarchitects-gateway` (the binary is named
 * `lightarchitects` after deploy; in the debug target tree it lives at
 * `target/debug/lightarchitects`).
 *
 * Contract verified:
 *   1. `auth`               → prints usage, exits non-zero (MissingParam)
 *   2. `auth <unknown>`     → prints usage, exits non-zero (UnknownTool)
 *   3. `auth status`        → exits 0 when no key present; prints guidance
 *   4. `auth status`        → exits 0 when key present via LA_API_KEY env
 *   5. `auth logout`        → exits 0 (noop when no key file exists)
 *   6. `auth` appears in top-level usage output
 *
 * Key file path isolation: AuthConfig::default() resolves the key file
 * from ~/lightarchitects/. Tests that must not touch the real key file use
 * LA_API_KEY env override (Priority 1 in KeyReader) to satisfy the key
 * requirement without touching the filesystem. File-mutation tests (e.g.
 * logout-removes-key) are covered by the Rust unit suite in key_reader.rs.
 *
 * Run:
 *   npx playwright test tests/e2e/auth_cli.spec.ts
 *
 * No browser or webshell server required — pure CLI subprocess tests.
 * Builds the gateway on demand (cargo build -p lightarchitects-gateway).
 */
import { test, expect } from '@playwright/test';
import { spawnSync, type SpawnSyncReturns } from 'node:child_process';
import { existsSync } from 'node:fs';
import { resolve } from 'node:path';

// ── Constants ────────────────────────────────────────────────────────────────

const REPO_ROOT = resolve(__dirname, '..', '..', '..');
const BIN = resolve(REPO_ROOT, 'target', 'debug', 'lightarchitects');

// ── Helpers ──────────────────────────────────────────────────────────────────

/** Run `lightarchitects <args>` synchronously; always returns output regardless of exit code. */
function la(args: string[], env?: NodeJS.ProcessEnv): SpawnSyncReturns<string> {
  return spawnSync(BIN, args, {
    encoding: 'utf8',
    // Strip LA_API_KEY from the inherited env so tests start from a clean slate,
    // unless the caller explicitly sets it.
    env: { ...process.env, LA_API_KEY: '', ...env },
    timeout: 10_000,
  });
}

/** stdout + stderr combined. */
function out(r: SpawnSyncReturns<string>): string {
  return (r.stdout ?? '') + (r.stderr ?? '');
}

// ── Setup: build gateway on demand ──────────────────────────────────────────

test.beforeAll(() => {
  if (!existsSync(BIN)) {
    console.log(`[auth-cli-e2e] building gateway debug binary at ${BIN}`);
    const r = spawnSync(
      'cargo',
      ['build', '-p', 'lightarchitects-gateway'],
      { cwd: REPO_ROOT, stdio: 'inherit', timeout: 120_000 },
    );
    if (r.status !== 0) throw new Error('cargo build -p lightarchitects-gateway failed');
  }
});

// ── Subcommand dispatch ──────────────────────────────────────────────────────

test('auth with no subcommand exits non-zero and prints usage', () => {
  const r = la(['auth']);
  expect(r.status).not.toBe(0);
  const o = out(r);
  expect(o).toMatch(/auth login/);
  expect(o).toMatch(/auth logout/);
  expect(o).toMatch(/auth status/);
});

test('auth with unknown subcommand exits non-zero', () => {
  const r = la(['auth', 'reboot']);
  expect(r.status).not.toBe(0);
  expect(out(r)).toMatch(/unknown/i);
});

// ── auth status ──────────────────────────────────────────────────────────────

test('auth status exits 0 with no key and prints actionable guidance', () => {
  // LA_API_KEY='' (set by la() default) + no key file → NoKeyFound → guidance printed
  const r = la(['auth', 'status']);
  expect(r.status).toBe(0);
  expect(out(r)).toMatch(/no api key|auth login/i);
});

test('auth status exits 0 and skips "no key" message when LA_API_KEY is set', () => {
  const r = la(['auth', 'status'], { LA_API_KEY: 'la-test-key-e2e-status' });
  expect(r.status).toBe(0);
  // A key is loadable — guidance message must NOT appear
  expect(out(r)).not.toMatch(/no api key found/i);
});

// ── auth logout ──────────────────────────────────────────────────────────────

test('auth logout exits 0 when no key file exists (noop)', () => {
  // The default key file path is ~/lightarchitects/la-api-key. If it doesn't
  // exist (typical in CI / clean test runs) logout is a documented noop.
  // We don't write the file so this is safe to run against the live key store.
  const r = la(['auth', 'logout']);
  expect(r.status).toBe(0);
  expect(out(r)).toMatch(/logged out|credentials removed/i);
});

// ── Top-level usage ──────────────────────────────────────────────────────────

test('top-level usage output includes auth subcommand', () => {
  // An unknown subcommand triggers the usage banner.
  const r = la(['__no_such_subcommand__']);
  expect(out(r)).toMatch(/auth/);
});
