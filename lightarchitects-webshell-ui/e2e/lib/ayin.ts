/**
 * Emit test lifecycle spans to AYIN (§57.9a).
 *
 * Test spans use actor="e2e" and appear in the webshell OPS Live Trace
 * tab alongside production spans. This makes test runs first-class
 * observable events in the platform.
 *
 * Fails silently — AYIN being down must never fail a test.
 */

const AYIN_BASE = process.env.AYIN_BASE_URL ?? 'http://127.0.0.1:3742';
const WEBSHELL_TOKEN = process.env.WEBSHELL_TOKEN ?? '63308ab0-d024-4f7d-a459-936744aa255f';

interface TestSpan {
  actor: 'e2e';
  action: string;
  label: string;
  outcome?: unknown;
  duration_ms?: number;
}

export async function emitAyinSpan(span: TestSpan): Promise<void> {
  try {
    await fetch(`${AYIN_BASE}/api/spans`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        Authorization: `Bearer ${WEBSHELL_TOKEN}`,
      },
      body: JSON.stringify({
        span_id: crypto.randomUUID(),
        actor: span.actor,
        action: span.action,
        label: span.label,
        outcome: span.outcome ?? 'success',
        duration_ms: span.duration_ms ?? 0,
        start_time: new Date().toISOString(),
        end_time: new Date().toISOString(),
        metadata: { source: 'e2e-test-suite' },
      }),
      signal: AbortSignal.timeout(2_000),
    });
  } catch {
    // AYIN unavailable — tests continue unaffected
  }
}

export async function emitRunStart(specFile: string): Promise<void> {
  await emitAyinSpan({ actor: 'e2e', action: 'run_start', label: specFile });
}

export async function emitTestResult(
  testName: string,
  passed: boolean,
  durationMs: number,
): Promise<void> {
  await emitAyinSpan({
    actor: 'e2e',
    action: passed ? 'test_passed' : 'test_failed',
    label: testName,
    outcome: passed ? 'success' : 'failure',
    duration_ms: durationMs,
  });
}
