/**
 * Final three-story sweep:
 *   [34] soul_promotion via real turnlog path (new /api/test/promote endpoint)
 *   [57] /SQUAD skill structural validity
 *   [58] /BUILD skill structural validity
 *   [59] /ENRICH skill structural validity
 *
 * All free + local. [34] drives the real promotion code path; the other three
 * validate skill markdown files have a deployable shape (YAML frontmatter,
 * required fields, body substance). They won't catch broken skill BEHAVIOR
 * (that needs Claude Code execution) but they catch broken skill FILES —
 * which silently misload and are the actual regression vector.
 */
import { chromium } from '/Users/kft/.npm/_npx/9833c18b2d85bc59/node_modules/playwright/index.mjs';
import { readFileSync, readdirSync, existsSync } from 'fs';
import { glob } from 'node:fs/promises';

const TOKEN = readFileSync(`${process.env.HOME}/lightarchitects/webshell/.token`, 'utf8').trim();
const API = 'http://localhost:8733';
const BASE = `${API}/#token=${TOKEN}`;
const auth = { Authorization: `Bearer ${TOKEN}` };
const sleep = ms => new Promise(r => setTimeout(r, ms));

let pass = 0, fail = 0;
const ok = (id, l, d='') => { console.log(`  \u2713 [${id}] ${l}${d?'  \u2192  '+d:''}`); pass++; };
const ko = (id, l, e) => { console.error(`  \u2717 [${id}] ${l}:  ${String(e?.message??e).replace(/\s+/g,' ').slice(0,200)}`); fail++; };

// ============================================================================
// [57] [58] [59] — structural skill validation
// ============================================================================
console.log('\n\u2500\u2500\u2500 [57][58][59] Skill file structural validity');

const SKILL_ROOT_GLOB = `${process.env.HOME}/.claude/plugins/cache/light-architects/lightarchitects`;
// Find the versioned dir (e.g. .../lightarchitects/1.0.0/skills/)
const versions = readdirSync(SKILL_ROOT_GLOB);
if (versions.length === 0) throw new Error(`no plugin versions under ${SKILL_ROOT_GLOB}`);
// pick first version (usually only one)
const SKILLS_DIR = `${SKILL_ROOT_GLOB}/${versions[0]}/skills`;
console.log(`  Skill root: ${SKILLS_DIR}`);

function validateSkillFile(skillName) {
  const path = `${SKILLS_DIR}/${skillName}/SKILL.md`;
  if (!existsSync(path)) throw new Error(`SKILL.md missing at ${path}`);
  const raw = readFileSync(path, 'utf8');
  if (raw.length < 500) throw new Error(`skill file too small (${raw.length} bytes); likely stub`);

  // Frontmatter: must start with --- and close with ---
  if (!raw.startsWith('---')) throw new Error('missing YAML frontmatter fence');
  const fmEnd = raw.indexOf('\n---', 4);
  if (fmEnd === -1) throw new Error('unclosed YAML frontmatter');
  const fmBlock = raw.slice(4, fmEnd).trim();
  const body = raw.slice(fmEnd + 4).trim();

  // Parse minimal YAML — name, description
  const nameMatch = fmBlock.match(/^name:\s*(.+)$/m);
  const descMatch = fmBlock.match(/^description:\s*([\s\S]+?)(?=^\w+:|\Z)/m);
  if (!nameMatch) throw new Error('frontmatter missing `name:` field');
  if (!descMatch) throw new Error('frontmatter missing `description:` field');
  const name = nameMatch[1].trim();
  const description = descMatch[1].replace(/\n\s+/g, ' ').trim();
  if (name !== skillName) throw new Error(`frontmatter name="${name}" mismatch against dir="${skillName}"`);
  if (description.length < 30) throw new Error(`description too short (${description.length} chars) — likely stub`);

  // Body: at least one h1
  const h1 = body.match(/^#\s+.+$/m);
  if (!h1) throw new Error('body missing h1 heading');
  if (body.length < 200) throw new Error(`body too short (${body.length} chars)`);

  return { name, body_chars: body.length, desc_chars: description.length, h1: h1[0].trim().slice(0, 60) };
}

for (const [id, skillName] of [[57, 'SQUAD'], [58, 'BUILD'], [59, 'ENRICH']]) {
  try {
    const r = validateSkillFile(skillName);
    ok(id, `/${skillName} skill deployable`, `h1="${r.h1}"  body=${r.body_chars}c  desc=${r.desc_chars}c`);
  } catch (e) { ko(id, `/${skillName} skill file`, e); }
}

// ============================================================================
// [34] soul_promotion via /api/test/promote — real turnlog path
// ============================================================================
console.log('\n\u2500\u2500\u2500 [34] soul_promotion via real turnlog');

const browser = await chromium.launch({ headless: false, slowMo: 60 });
const page = await (await browser.newContext({ viewport: { width: 1600, height: 1000 } })).newPage();
await page.goto(BASE, { waitUntil: 'load' });
await page.waitForSelector('[data-testid="memory-toggle"]', { timeout: 10_000 });
console.log('  webshell loaded');

// Start SSE listener BEFORE POSTing
const ssePromise = page.evaluate(async ({ api, token }) => {
  const ac = new AbortController();
  setTimeout(() => ac.abort(), 20_000);
  const events = [];
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
        try { const o = JSON.parse(line.slice(6)); if (o.type === 'soul_promotion') events.push(o); } catch {}
      }
    }
  } catch {}
  return events;
}, { api: API, token: TOKEN });
await sleep(500);

// Trigger the real promotion path
const marker = `upgrade-v3-${Date.now()}`;
const triggerBody = `Reflective memo from the /api/test/promote endpoint. marker=${marker}. ` +
  `This turnlog session_paused entry is created with weight=0.95, crossing the default 0.7 ` +
  `promotion threshold so the post-close promote_session_with_policy task fires and emits ` +
  `WebEvent::SoulPromotion via the BroadcastingPromoter. End-to-end regression gate for story [34].`;
try {
  const r = await fetch(`${API}/api/test/promote`, {
    method: 'POST',
    headers: { ...auth, 'content-type': 'application/json' },
    body: JSON.stringify({ body: triggerBody, weight: 0.95 }),
  });
  if (r.status !== 202) throw new Error(`test/promote returned ${r.status}`);
  const j = await r.json();
  console.log(`  [SEED] session_id=${j.session_id.slice(0, 8)}...  weight=0.95`);
} catch (e) { ko(34, '[34] trigger test/promote', e); }

const promoEvents = await ssePromise;
await sleep(1_500);
await browser.close();

try {
  if (promoEvents.length === 0) throw new Error('no soul_promotion SSE event received within 20s');
  const first = promoEvents[0];
  ok(34, 'soul_promotion fires from real turnlog path',
     `events=${promoEvents.length}  sibling=${first.sibling}  sig=${first.significance}  path=${(first.path||'?').slice(0,50)}`);
} catch (e) { ko(34, 'soul_promotion SSE', e); }

// ============================================================================
// Report
// ============================================================================
console.log(`\n${'='.repeat(60)}`);
console.log(`  Final 4-story sweep: \u2713 ${pass}  \u2717 ${fail}`);
console.log(`${'='.repeat(60)}`);
process.exit(fail === 0 ? 0 : 1);
