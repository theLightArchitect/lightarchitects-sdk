/**
 * Visual screenshot walk — Choose Backend wizard flows.
 * Captures before/after screenshots for each of the 8 backend cards.
 * Not a regression suite — run on demand to eyeball wizard UX.
 *
 * Run:
 *   PLAYWRIGHT_BASE_URL=http://localhost:5176 \
 *   pnpm exec playwright test e2e/wizard-screenshot-walk.spec.ts \
 *     --config=e2e/pw-provider.config.ts --reporter=line --headed --workers=1
 */

import { test, type Page } from '@playwright/test';
import path from 'path';
import fs from 'fs';

const BASE = process.env.PLAYWRIGHT_BASE_URL ?? 'http://localhost:5176';
const OUT  = '/tmp/wizard-screenshots';
fs.mkdirSync(OUT, { recursive: true });

async function suppressTutorials(page: Page) {
  await page.addInitScript(() => {
    for (const id of ['t1', 't2', 't3', 't4', 't5', 't6']) {
      localStorage.setItem(`la.tutorial.completed.${id}`, 'true');
    }
  });
}

async function setupWizardMocks(page: Page, modelsOverride?: object[]) {
  await suppressTutorials(page);
  const routes: [string | ((url: URL) => boolean), object][] = [
    ['**/api/health',               { status: 200, body: 'ok' }],
    ['**/api/auth-check',           { status: 200, contentType: 'application/json', body: JSON.stringify({ valid: true }) }],
    ['**/api/auth/status',          { status: 200, contentType: 'application/json', body: JSON.stringify({ ok: true }) }],
    ['**/api/auth/exchange',        { status: 200, contentType: 'application/json', body: JSON.stringify({ ok: true }) }],
    ['**/api/auth/nonce-exchange',  { status: 204 }],
    ['**/api/siblings',             { status: 200, contentType: 'application/json', body: JSON.stringify([]) }],
    ['**/api/sitrep',               { status: 200, contentType: 'application/json', body: JSON.stringify({ status: 'ok' }) }],
    ['**/api/builds',               { status: 200, contentType: 'application/json', body: JSON.stringify({ builds: [] }) }],
    ['**/api/conductor/status',     { status: 200, contentType: 'application/json', body: JSON.stringify({ nodes: [], edges: [], queue_depth: 0 }) }],
    ['**/api/conductor/hitl',       { status: 200, contentType: 'application/json', body: JSON.stringify([]) }],
    ['**/api/gitforest/hitl-search',{ status: 200, contentType: 'application/json', body: JSON.stringify([]) }],
    ['**/api/litellm/config',       { status: 200, contentType: 'application/json', body: JSON.stringify({ base_url: '', model: '', has_key: false, updated_at: '' }) }],
    ['**/api/workspaces',           { status: 200, contentType: 'application/json', body: JSON.stringify([]) }],
    ['**/api/memory/**',            { status: 200, contentType: 'application/json', body: JSON.stringify([]) }],
    ['**/api/soul/**',              { status: 200, contentType: 'application/json', body: JSON.stringify({ status: 'ok', counts: {}, tiers: {} }) }],
    ['**/api/coordination/**',      { status: 200, contentType: 'application/json', body: JSON.stringify({ ok: true }) }],
    ['**/api/setup/save',           { status: 200, contentType: 'application/json', body: JSON.stringify({ ok: true }) }],
  ];

  for (const [pattern, response] of routes) {
    await page.route(pattern as string, r => r.fulfill(response as Parameters<typeof r.fulfill>[0]));
  }

  await page.route((url: URL) => url.pathname.startsWith('/api/events'),
    r => r.fulfill({ status: 200, contentType: 'text/event-stream', body: '' }));
  await page.route((url: URL) => url.pathname.startsWith('/api/arena'),
    r => r.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ agents: [], tasks: [], nodes: [], edges: [], queue_depth: 0 }) }));

  await page.route('**/api/setup/info', r => r.fulfill({
    status: 200, contentType: 'application/json',
    body: JSON.stringify({
      setup_complete: false,
      auth_status: {
        claude:    { has_keychain_auth: false, has_api_key: false, login_method: 'none', login_source: 'none' },
        codex:     { has_keychain_auth: false, has_api_key: false, login_method: 'none', login_source: 'none' },
        ollama:    { base_url: 'http://localhost:11434', reachable: false },
        la_native: { has_api_key: false },
      },
      config: null, resume_session: null, cwd: '/tmp/webshell-e2e',
    }),
  }));

  const models = modelsOverride ?? [
    { id: 'claude-opus-4-7',   label: 'Claude Opus 4.7',    tier: 'flagship'  },
    { id: 'claude-sonnet-4-6', label: 'Claude Sonnet 4.6',  tier: 'balanced'  },
  ];
  await page.route('**/api/setup/models', r => r.fulfill({
    status: 200, contentType: 'application/json', body: JSON.stringify(models),
  }));
}

async function snap(page: Page, name: string) {
  const file = path.join(OUT, `${name}.png`);
  await page.screenshot({ path: file, fullPage: false });
  console.log(`  📸  ${file}`);
}

// ── 8-backend visual walk ──────────────────────────────────────────────────────

const BACKENDS = [
  { label: 'Claude Code',  slug: '1-claude-code',    models: [{ id: 'claude-sonnet-4-6', label: 'Claude Sonnet 4.6', tier: 'balanced' }] },
  { label: 'Codex',        slug: '2-codex',           models: [{ id: 'gpt-4o', label: 'GPT-4o', tier: 'balanced' }] },
  { label: 'Ollama Local', slug: '3-ollama-local',    models: [{ id: 'llama3.2:3b', label: 'Llama 3.2 3B', tier: 'local' }] },
  { label: 'Mistral Vibe', slug: '4-mistral-vibe',    models: [{ id: 'mistral-large-latest', label: 'Mistral Large', tier: 'flagship' }] },
  { label: 'Ollama Cloud', slug: '5-ollama-cloud',    models: [{ id: 'qwen2.5-coder:32b-cloud', label: 'Qwen2.5 Coder 32B', tier: 'flagship' }] },
  { label: 'LA Native',    slug: '6-la-native',       models: [{ id: 'nemotron-70b', label: 'Nemotron 70B', tier: 'flagship' }] },
  { label: 'OpenRouter',   slug: '7-openrouter',      models: [{ id: 'openai/gpt-4o', label: 'GPT-4o', tier: 'balanced' }] },
  { label: 'LA Cloud',     slug: '8-la-cloud',        comingSoon: true, models: [] },
] as const;

test.describe('Wizard screenshot walk', () => {

  for (const backend of BACKENDS) {
    test(`${backend.slug} — ${backend.label}`, async ({ page }) => {
      await setupWizardMocks(page, [...backend.models]);
      await page.goto(BASE, { waitUntil: 'commit' });

      // Wait for the Choose Backend screen
      await page.locator('text=Choose Backend').waitFor({ timeout: 8000 });
      await page.waitForTimeout(400); // let CSS transitions settle

      // Screenshot: the card grid (before any selection)
      await snap(page, `${backend.slug}-a-card-grid`);

      if ('comingSoon' in backend && backend.comingSoon) {
        // LA Cloud — just capture the disabled state, no click
        const card = page.locator('button.card').filter({ has: page.locator('.card-label', { hasText: /^LA Cloud$/ }) });
        await card.waitFor({ timeout: 3000 });
        await snap(page, `${backend.slug}-b-disabled-state`);
        return;
      }

      // Click the card
      await page.locator('button.card')
        .filter({ has: page.locator('.card-label', { hasText: new RegExp(`^${backend.label}$`) }) })
        .click();
      await page.waitForTimeout(300);

      // Screenshot: card selected (highlight state)
      await snap(page, `${backend.slug}-b-card-selected`);

      // Click Continue
      await page.locator('button.btn-continue').click();
      await page.waitForTimeout(600);

      // Screenshot: whatever appears next (URL config / API key / model picker / etc.)
      await snap(page, `${backend.slug}-c-next-step`);
    });
  }

});
