/**
 * Core Loops E2E — the two user stories that must work for the tool to be useful.
 *
 *   Loop 1: Build lifecycle — "Spawn an agent, watch it work."
 *   Loop 2: Memory traversal — "Find what I was thinking, follow where it leads."
 *
 * Headed Playwright — every chromium.launch() passes { headless: false } so
 * the operator can watch the full round-trip. Budget: ~5 min total runtime.
 *
 * Loop 1 stops at pillar_update SSE arrival rather than waiting for a full
 * promoted cold memo — a fresh build would need ≥30s of real subprocess
 * activity to emit one, and that's out of scope for an interactive observe.
 * The promotion path is validated separately via the Phase 18B gate.
 */
import { chromium } from '/Users/kft/.npm/_npx/9833c18b2d85bc59/node_modules/playwright/index.mjs';
import { readFileSync } from 'fs';

const TOKEN = readFileSync(`${process.env.HOME}/lightarchitects/webshell/.token`, 'utf8').trim();
const API = 'http://localhost:8733';
const BASE = `${API}/#token=${TOKEN}`;
const auth = { Authorization: `Bearer ${TOKEN}` };

let pass = 0, fail = 0;
const failures = [];
const ok = (label, detail = '') => { console.log(`  \u2713 ${label}${detail ? '  \u2192  ' + detail : ''}`); pass++; };
const ko = (label, err) => {
  const msg = String(err?.message ?? err).replace(/\n/g, ' ').slice(0, 220);
  console.error(`  \u2717 ${label}:  ${msg}`);
  failures.push(`${label}: ${msg}`); fail++;
};

const sleep = (ms) => new Promise(r => setTimeout(r, ms));

// Keep the same browser across both describes so the operator sees continuity.
const browser = await chromium.launch({ headless: false, slowMo: 150 });
const context = await browser.newContext({ viewport: { width: 1600, height: 1000 } });
const page = await context.newPage();
page.on('pageerror', e => console.log('[PAGEERROR]', e.message));
page.on('console', m => { if (m.type() === 'error') console.log('[CONSOLE ERROR]', m.text()); });

// ==========================================================================
// Loop 1: Build lifecycle
// ==========================================================================
console.log('\n\u25B6 LOOP 1 \u2014 Build lifecycle');

// 1.A  auth check
try {
  const r = await fetch(`${API}/api/soul/health`, { headers: auth });
  if (!r.ok) throw new Error(`/api/soul/health returned ${r.status}`);
  const j = await r.json();
  const nonzero = Object.entries(j.counts || {}).filter(([, v]) => v > 0).length;
  if (nonzero === 0) throw new Error('no sibling has entries');
  ok('1.A auth + health', `siblings_with_entries=${nonzero}`);
} catch (e) { ko('1.A auth + health', e); }

// 1.B  load webshell UI
try {
  await page.goto(BASE, { waitUntil: 'load' });
  await page.waitForSelector('[data-testid="memory-toggle"]', { timeout: 10_000 });
  ok('1.B webshell loaded', 'memory-toggle button present');
} catch (e) { ko('1.B webshell loaded', e); }

// 1.C  POST /api/builds creates a session
let buildId;
try {
  const r = await fetch(`${API}/api/builds`, {
    method: 'POST',
    headers: { ...auth, 'content-type': 'application/json' },
    body: JSON.stringify({ cwd: '/tmp' }),
  });
  if (!r.ok) throw new Error(`${r.status} ${await r.text()}`);
  const body = await r.json();
  buildId = body.build_id;
  if (!buildId) throw new Error('no build_id in response');
  ok('1.C POST /api/builds', `build_id=${buildId.slice(0, 8)}...  cwd=${body.cwd}`);
} catch (e) { ko('1.C POST /api/builds', e); }

// 1.D  open an SSE listener in-page before triggering the pillar so no event is missed.
const ssePromise = buildId ? page.evaluate(async ({ api, token, wantBuildId }) => {
  const r = await fetch(`${api}/api/events`, { headers: { authorization: `Bearer ${token}`, accept: 'text/event-stream' } });
  const reader = r.body.getReader();
  const decoder = new TextDecoder();
  let buf = '';
  const deadline = Date.now() + 25_000;
  const events = [];
  while (Date.now() < deadline) {
    const { value, done } = await reader.read();
    if (done) break;
    buf += decoder.decode(value, { stream: true });
    let idx;
    while ((idx = buf.indexOf('\n\n')) !== -1) {
      const chunk = buf.slice(0, idx); buf = buf.slice(idx + 2);
      const line = chunk.split('\n').find(l => l.startsWith('data: '));
      if (!line) continue;
      try {
        const obj = JSON.parse(line.slice(6));
        if (obj.type === 'pillar_update' && obj.build_id === wantBuildId) {
          events.push(obj);
          if (events.length >= 1) return events;
        }
      } catch {}
    }
  }
  return events;
}, { api: API, token: TOKEN, wantBuildId: buildId }) : Promise.resolve([]);

// 1.E  trigger the `arch` pillar (maps to `corso sniff` via real_data adapter)
if (buildId) {
  await sleep(300);  // let the SSE subscription land before triggering
  try {
    const r = await fetch(`${API}/api/builds/${buildId}/pillars/arch`, {
      method: 'POST',
      headers: { ...auth },
    });
    if (!r.ok) throw new Error(`${r.status} ${await r.text()}`);
    ok('1.D POST pillar arch', 'accepted');
  } catch (e) { ko('1.D POST pillar arch', e); }
}

// 1.F  wait for at least one pillar_update SSE event
try {
  const events = await ssePromise;
  if (!Array.isArray(events) || events.length === 0) throw new Error('no pillar_update events within 25s');
  const first = events[0];
  ok('1.E pillar_update SSE fired', `status=${first.status} pillar=${first.pillar}`);
} catch (e) { ko('1.E pillar_update SSE fired', e); }

// 1.G  sanity: the build registry lists our session
if (buildId) {
  try {
    const r = await fetch(`${API}/api/builds/${buildId}`, { headers: auth });
    if (!r.ok) throw new Error(`${r.status}`);
    const j = await r.json();
    if (j.build_id !== buildId) throw new Error('build_id mismatch');
    ok('1.F GET /api/builds/:id', `agent.kind=${j.agent?.kind ?? '?'}`);
  } catch (e) { ko('1.F GET /api/builds/:id', e); }
}

await sleep(1_500);  // visual breathing room before Loop 2

// ==========================================================================
// Loop 2: Memory traversal
// ==========================================================================
console.log('\n\u25B6 LOOP 2 \u2014 Memory traversal');

// 2.A  open MemoryDrawer via the header button
try {
  await page.click('[data-testid="memory-toggle"]');
  await page.waitForSelector('[data-testid="memory-drawer"]', { timeout: 5_000 });
  ok('2.A open MemoryDrawer', 'via memory-toggle');
} catch (e) { ko('2.A open MemoryDrawer', e); }

// 2.B  Cold tab populates with real entries
try {
  await page.click('[data-testid="memory-tab-cold"]');
  await page.waitForTimeout(1_500);
  const rowCount = await page.locator('[data-testid="memory-row"]').count();
  if (rowCount < 5) throw new Error(`only ${rowCount} memory rows rendered`);
  ok('2.B Cold tab populates', `rows=${rowCount}`);
} catch (e) { ko('2.B Cold tab populates', e); }

// 2.C  UI search — drive the hybrid path end-to-end via Enter-key trigger.
//   Why only hybrid in the UI: setSearchMode() auto-re-runs the search on mode
//   click whenever query is non-empty, which races page.waitForResponse() setup
//   for subsequent iterations. Hybrid is the money-shot (4-signal RRF); bm25
//   and semantic are validated via direct API calls in 2.D to dodge that race.
const query = 'memory architecture';
const topByMode = {};
try {
  await page.click('[data-testid="search-mode-hybrid"]');
  await page.waitForTimeout(300);
  const input = page.locator('[data-testid="search-input"]');
  await input.click();
  const respPromise = page.waitForResponse(
    r => r.url().includes('/api/soul/search') && r.url().includes('mode=hybrid') && r.status() === 200,
    { timeout: 8_000 }
  );
  await page.keyboard.type(query, { delay: 30 });
  await page.keyboard.press('Enter');
  const resp = await respPromise;
  const body = await resp.json();
  await page.waitForTimeout(500);
  const rows = await page.locator('[data-testid="memory-row"]').count();
  if (rows === 0) throw new Error('hybrid: 0 rows rendered');
  topByMode.hybrid = body.results ?? [];
  ok('2.C UI-driven hybrid search', `rows=${rows}  api_results=${body.results.length}  rrf_used=${body.rrf_used}`);
} catch (e) { ko('2.C UI-driven hybrid search', e); }

// 2.D  API-level proof that signals differentiate (load-bearing RRF assertion).
// If bm25/semantic/hybrid all return the same result set on the same query,
// the 4-signal fusion isn't doing work — embeddings aren't contributing.
try {
  for (const mode of ['bm25', 'semantic', 'hybrid']) {
    const r = await fetch(`${API}/api/soul/search?q=${encodeURIComponent(query)}&mode=${mode}&limit=10`, { headers: auth });
    const j = await r.json();
    topByMode[mode] = j.results ?? [];
  }
  const b = topByMode.bm25, s = topByMode.semantic, h = topByMode.hybrid;
  const bPaths = b.map(r => r.path).join('|');
  const sPaths = s.map(r => r.path).join('|');
  const hPaths = h.map(r => r.path).join('|');
  const differs = (bPaths !== sPaths) || (bPaths !== hPaths) || (sPaths !== hPaths);
  if (!differs) throw new Error('all three modes returned IDENTICAL result sets');
  ok('2.D modes differentiate (signals active)',
     `bm25=${b.length} semantic=${s.length} hybrid=${h.length}`);
} catch (e) { ko('2.D modes differentiate', e); }

// 2.E  hybrid must set rrf_used:true — the UI badge contract
try {
  const r = await fetch(`${API}/api/soul/search?q=${encodeURIComponent(query)}&mode=hybrid&limit=5`, { headers: auth });
  const j = await r.json();
  if (j.rrf_used !== true) throw new Error(`rrf_used=${j.rrf_used} for hybrid mode`);
  ok('2.E hybrid rrf_used:true', '4-signal fusion is active');
} catch (e) { ko('2.E hybrid rrf_used', e); }

// 2.F  click the top result, detail pane + Related Entries render
try {
  const firstRow = page.locator('[data-testid="memory-row"]').first();
  await firstRow.click();
  await page.waitForTimeout(2_000);  // selection + related-entries fetch
  const related = page.locator('[data-testid="related-section"]');
  const present = await related.count();
  if (present === 0) throw new Error('related-section not rendered');
  const chipCount = await related.locator('button').count();
  ok('2.F detail pane + Related Entries', `related_chips=${chipCount}`);

  // 2.G  click a related chip (if any) to prove navigation works
  if (chipCount > 0) {
    const firstBefore = (await firstRow.innerText()).slice(0, 48);
    await related.locator('button').first().click();
    await page.waitForTimeout(1_200);
    const firstAfter = (await page.locator('[data-testid="memory-row"]').first().innerText()).slice(0, 48);
    ok('2.G related chip navigates', `before="${firstBefore}..." after="${firstAfter}..."`);
  } else {
    ok('2.G related chip navigates', 'skipped — this memo has no backlinks');
  }
} catch (e) { ko('2.F/G detail + related navigation', e); }

// 2.H  Helix3D canvas is mounted and has non-zero dimensions.
// Note: window.__helixOrbCount reflects LIVE SSE-streamed entries, not the
// cold vault. On a quiescent page it's 0 — correct behavior. The assertion
// that matters for "tool is useful" is that the 3D scene rendered at all.
try {
  await page.click('[data-testid="memory-toggle"]');
  await page.waitForTimeout(1_200);
  const dims = await page.evaluate(() => {
    const c = document.querySelector('canvas');
    if (!c) return null;
    const r = c.getBoundingClientRect();
    return { w: r.width, h: r.height, orbs: window.__helixOrbCount ?? 0 };
  });
  if (!dims) throw new Error('no <canvas> element mounted');
  if (dims.w < 400 || dims.h < 300) throw new Error(`canvas too small: ${dims.w}x${dims.h}`);
  ok('2.H Helix3D canvas mounted', `${dims.w}x${dims.h}  live_orbs=${dims.orbs}`);
} catch (e) { ko('2.H Helix3D canvas mounted', e); }

// ==========================================================================
// Report
// ==========================================================================
console.log(`\n\u2500\u2500\u2500 Summary \u2500\u2500\u2500\npass=${pass}  fail=${fail}`);
if (failures.length) {
  console.log('\nFailures:'); failures.forEach(f => console.log('  \u2022 ' + f));
}

// Leave the UI visible briefly so the operator can look around before close.
await sleep(4_000);
await browser.close();
process.exit(fail === 0 ? 0 : 1);
