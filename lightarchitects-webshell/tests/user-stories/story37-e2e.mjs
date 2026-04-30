/**
 * [37] strand_activation E2E — verifies AYIN-native spans with top-level
 * strand_activations propagate through AYIN's SSE → webshell's ayin_client
 * → webshell's /api/events as WebEvent::StrandActivation.
 *
 * Before the 2026-04-20 fix: webshell dropped the top-level field during
 * deserialize (TraceSpanSummary only captured `metadata`), so this path
 * NEVER fired in production. This test is the regression gate.
 */
import { chromium } from '/Users/kft/.npm/_npx/9833c18b2d85bc59/node_modules/playwright/index.mjs';
import { readFileSync, writeFileSync, mkdirSync, unlinkSync } from 'fs';
import { randomUUID } from 'node:crypto';

const TOKEN = readFileSync(`${process.env.HOME}/lightarchitects/webshell/.token`, 'utf8').trim();
const API = 'http://localhost:8733';
const BASE = `${API}/#token=${TOKEN}`;
const sleep = ms => new Promise(r => setTimeout(r, ms));

// AYIN's trace dir on this system per CLAUDE.md (3-tier resolution default)
const TODAY = new Date().toISOString().slice(0, 10);  // YYYY-MM-DD
const TRACE_DIR = `${process.env.HOME}/lightarchitects/soul/helix/ayin/traces/corso/${TODAY}`;
mkdirSync(TRACE_DIR, { recursive: true });

// Build a synthetic TraceSpan with strand_activations at TOP LEVEL
// (the real AYIN wire shape — pre-fix this was being silently dropped).
const spanId = randomUUID();
const SPAN_PATH = `${TRACE_DIR}/${Date.now()}-story37-${spanId.slice(0, 8)}.json`;
const STRAND_MARKER = `s37-${Date.now()}`;
const span = {
  id: spanId,
  parent_id: null,
  session_id: null,
  actor: 'corso',
  action: 'test.story37.harness',
  timestamp: new Date().toISOString(),
  duration_ms: 7,
  outcome: { type: 'Continue' },
  metadata: { input: { marker: STRAND_MARKER } },
  strand_activations: [
    { strand: 'precision', weight: 0.92 },
    { strand: 'analytical', weight: 0.67 },
  ],
  decision_points: [],
};

const browser = await chromium.launch({ headless: false, slowMo: 60 });
const page = await (await browser.newContext({ viewport: { width: 1600, height: 1000 } })).newPage();
await page.goto(BASE, { waitUntil: 'load' });
await page.waitForSelector('[data-testid="memory-toggle"]', { timeout: 10_000 });
console.log('[SETUP] webshell loaded');

// Start SSE listener before writing the span (avoid race)
const ssePromise = page.evaluate(async ({ api, token }) => {
  const ac = new AbortController();
  setTimeout(() => ac.abort(), 25_000);
  const tally = {}; const activations = [];
  try {
    const r = await fetch(`${api}/api/events`, {
      headers: { authorization: `Bearer ${token}`, accept: 'text/event-stream' },
      signal: ac.signal,
    });
    const reader = r.body.getReader(); const dec = new TextDecoder(); let buf = '';
    while (true) {
      const { value, done } = await reader.read(); if (done) break;
      buf += dec.decode(value, { stream: true });
      let idx;
      while ((idx = buf.indexOf('\n\n')) !== -1) {
        const chunk = buf.slice(0, idx); buf = buf.slice(idx + 2);
        const line = chunk.split('\n').find(l => l.startsWith('data: '));
        if (!line) continue;
        try {
          const o = JSON.parse(line.slice(6));
          if (o.type) tally[o.type] = (tally[o.type] ?? 0) + 1;
          if (o.type === 'strand_activation') activations.push(o);
        } catch {}
      }
    }
  } catch {}
  return { tally, activations };
}, { api: API, token: TOKEN });

await sleep(700);  // let listener attach

// Drop the span file — AYIN's fs watcher + webshell's ayin_client should ingest it
console.log(`[DROP] ${SPAN_PATH}`);
writeFileSync(SPAN_PATH, JSON.stringify(span, null, 2), 'utf8');

// Collect results
const { tally, activations } = await ssePromise;
console.log(`\nTally: ${JSON.stringify(tally)}`);
console.log(`Activations received: ${activations.length}`);
if (activations.length > 0) console.log(`First: ${JSON.stringify(activations[0])}`);

// Cleanup
try { unlinkSync(SPAN_PATH); console.log(`[CLEANUP] removed ${SPAN_PATH}`); } catch {}
await sleep(2_000);
await browser.close();

// Assert: webshell must emit at least one strand_activation event with
// sibling=corso and strand in {precision, analytical}.
const got = activations.filter(a => a.sibling === 'corso' && (a.strand === 'precision' || a.strand === 'analytical'));
if (got.length < 2) {
  console.error(`\n[37] FAIL: expected >=2 activations for corso/{precision,analytical}, got ${got.length}`);
  console.error(`All activations seen: ${JSON.stringify(activations)}`);
  process.exit(1);
}
console.log(`\n[37] PASS: ${got.length} strand_activation events from AYIN span — bug fix verified end-to-end`);
process.exit(0);
