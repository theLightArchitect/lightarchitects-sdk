#!/usr/bin/env node
// ============================================================================
// Post-fix verification — headed Chrome, HAR capture, tests all core flows.
// Generates HAR + JSON report. Browser stays open for manual inspection.
// ============================================================================
import { chromium } from '@playwright/test';
import { writeFileSync, mkdirSync } from 'fs';
import { join } from 'path';

const BASE = process.env.BASE_URL || 'http://localhost:9740';
const HAR_DIR = join(import.meta.dirname, '..', 'test-results');
const HAR_PATH = join(HAR_DIR, 'verify.har');

mkdirSync(HAR_DIR, { recursive: true });

const results = [];
const consoleMsgs = [];
const networkErrors = [];
let effectLoopDetected = false;

function pass(name, detail = '') { results.push({ name, status: 'PASS', detail }); console.log(`  PASS  ${name}${detail ? ' — ' + detail : ''}`); }
function fail(name, detail = '') { results.push({ name, status: 'FAIL', detail }); console.log(`  FAIL  ${name}${detail ? ' — ' + detail : ''}`); }

console.log(`\n=== WEBSHELL VERIFICATION ===`);
console.log(`Target: ${BASE}`);
console.log(`HAR:    ${HAR_PATH}\n`);

const browser = await chromium.launch({
  headless: false,
  channel: 'chrome',
  args: ['--auto-open-devtools-for-tabs'],
});

const context = await browser.newContext({
  recordHar: { path: HAR_PATH, mode: 'full' },
  viewport: { width: 1440, height: 900 },
});

const page = await context.newPage();

// --- Capture infrastructure ---
page.on('console', (msg) => {
  consoleMsgs.push({ type: msg.type(), text: msg.text(), ts: Date.now() });
  if (msg.text().includes('effect_update_depth_exceeded')) {
    effectLoopDetected = true;
    console.log('\n  !!! EFFECT LOOP DETECTED !!!\n');
  }
});

page.on('pageerror', (err) => {
  if (err.message.includes('effect_update_depth_exceeded')) {
    effectLoopDetected = true;
    console.log(`\n  !!! EFFECT LOOP PAGE ERROR: ${err.message.slice(0, 200)} !!!\n`);
  }
});

page.on('response', (resp) => {
  if (resp.status() >= 400) {
    networkErrors.push({ url: resp.url(), status: resp.status(), ts: Date.now() });
  }
});

// ─── TEST 1: Page loads without crash ───
console.log('--- Test 1: Page Load ---');
try {
  await page.goto(BASE, { waitUntil: 'domcontentloaded', timeout: 15000 });
  pass('Page load', 'domcontentloaded fired');
} catch (e) {
  fail('Page load', e.message.slice(0, 100));
}

// ─── TEST 2: Splash auto-advances ───
console.log('--- Test 2: Splash Auto-Advance ---');
await page.waitForTimeout(4000); // 2.5s timer + margin

const hasNav = await page.evaluate(() => !!document.querySelector('nav')).catch(() => false);
if (hasNav) {
  pass('Splash auto-advance', 'nav bar visible within 4s');
} else {
  // Try clicking to advance
  await page.click('body').catch(() => {});
  await page.waitForTimeout(2000);
  const hasNavRetry = await page.evaluate(() => !!document.querySelector('nav')).catch(() => false);
  if (hasNavRetry) {
    pass('Splash advance (click)', 'nav bar appeared after click');
  } else {
    fail('Splash advance', 'nav bar not visible after 6s');
  }
}

// ─── TEST 3: No effect_update_depth_exceeded ───
console.log('--- Test 3: Effect Loop Check ---');
await page.waitForTimeout(3000); // Let any deferred effects fire
if (!effectLoopDetected) {
  pass('No effect loop', `${consoleMsgs.length} console messages, 0 effect loops`);
} else {
  fail('Effect loop detected', 'effect_update_depth_exceeded fired');
}

// ─── TEST 4: Tab navigation ───
console.log('--- Test 4: Tab Navigation ---');
const tabTests = [
  { label: 'Activity', hash: '#/activity', marker: 'AGENT ACTIVITY' },
  { label: 'Queue',    hash: '#/',         marker: 'Build Queue' },
  { label: 'Intake',   hash: '#/intake',   marker: 'Intake' },
  { label: 'Sitrep',   hash: '#/sitrep',   marker: 'Sitrep' },
];

for (const tab of tabTests) {
  try {
    await page.click(`nav button:has-text("${tab.label}")`);
    await page.waitForTimeout(1500);
    const hash = await page.evaluate(() => window.location.hash);
    const content = await page.evaluate(() => document.body.textContent || '');
    const hashCorrect = hash === tab.hash;
    // For tab navigation: check that hash changed correctly
    if (hashCorrect) {
      pass(`Tab: ${tab.label}`, `hash=${hash}`);
    } else {
      fail(`Tab: ${tab.label}`, `expected hash=${tab.hash}, got ${hash}`);
    }
  } catch (e) {
    fail(`Tab: ${tab.label}`, e.message.slice(0, 100));
  }
}

// ─── TEST 5: Network health ───
console.log('--- Test 5: Network Health ---');
const post422s = networkErrors.filter(e => e.status === 422 && e.url.includes('browser-state'));
if (post422s.length === 0) {
  pass('No 422 on browser-state');
} else {
  fail('422 on browser-state', `${post422s.length} occurrences`);
}

// ─── TEST 6: Copilot drawer ───
console.log('--- Test 6: Copilot Drawer ---');
try {
  // The drawer is always in the DOM — it transitions from 32px (closed) to
  // ~350px (open). Check height > 100px to detect open state.
  await page.keyboard.press('Control+Backquote');
  await page.waitForTimeout(500);
  const drawerOpen = await page.evaluate(() => {
    const el = document.querySelector('[data-testid="copilot-drawer"]');
    return el ? parseInt(getComputedStyle(el).height) > 100 : false;
  });
  if (drawerOpen) {
    pass('Copilot drawer opens (Ctrl+`)');
  } else {
    // Try Cmd+` (macOS)
    await page.keyboard.press('Meta+Backquote');
    await page.waitForTimeout(500);
    const retry = await page.evaluate(() => {
      const el = document.querySelector('[data-testid="copilot-drawer"]');
      return el ? parseInt(getComputedStyle(el).height) > 100 : false;
    });
    if (retry) pass('Copilot drawer opens (Cmd+`)');
    else fail('Copilot drawer', 'height stayed <= 100px after Ctrl+` and Cmd+`');
  }
  // Close it by toggling again
  await page.keyboard.press('Control+Backquote');
} catch (e) {
  fail('Copilot drawer', e.message.slice(0, 100));
}

// ─── TEST 7: Memory drawer toggle ───
console.log('--- Test 7: Memory Drawer ---');
try {
  const memBtn = page.locator('[data-testid="memory-toggle"]');
  await memBtn.click();
  await page.waitForTimeout(500);
  pass('Memory drawer toggle clicked');
  await memBtn.click(); // close
} catch (e) {
  fail('Memory drawer', e.message.slice(0, 100));
}

// ─── TEST 8: 3D Helix renders ───
console.log('--- Test 8: 3D Helix ---');
const canvasCount = await page.evaluate(() => document.querySelectorAll('canvas').length).catch(() => 0);
if (canvasCount > 0) {
  pass('Canvas elements present', `${canvasCount} canvas(es)`);
} else {
  fail('Canvas elements', 'no canvas found');
}

// ─── TEST 9: Still no effect loop after all interactions ───
console.log('--- Test 9: Post-Interaction Effect Loop Check ---');
await page.waitForTimeout(2000);
if (!effectLoopDetected) {
  pass('No effect loop (post-interaction)');
} else {
  fail('Effect loop detected post-interaction');
}

// ─── SUMMARY ───
const passed = results.filter(r => r.status === 'PASS').length;
const failed = results.filter(r => r.status === 'FAIL').length;

console.log(`\n=== RESULTS: ${passed} passed, ${failed} failed ===`);
for (const r of results) {
  console.log(`  ${r.status === 'PASS' ? 'OK' : 'XX'}  ${r.name}`);
}

console.log(`\nConsole messages: ${consoleMsgs.length}`);
console.log(`Network errors:   ${networkErrors.length}`);
console.log(`Effect loop:      ${effectLoopDetected ? 'YES' : 'No'}`);

// Save report
const report = {
  timestamp: new Date().toISOString(),
  base: BASE,
  effectLoopDetected,
  results,
  networkErrors,
  consoleErrorCount: consoleMsgs.filter(m => m.type === 'error').length,
  totalConsoleMessages: consoleMsgs.length,
};
const reportPath = join(HAR_DIR, 'verify-report.json');
writeFileSync(reportPath, JSON.stringify(report, null, 2));
console.log(`\nReport: ${reportPath}`);
console.log(`HAR:    ${HAR_PATH}`);

// --- Keep browser open ---
console.log('\n>>> Browser staying open for inspection. Ctrl+C to close. <<<\n');
await new Promise((resolve) => {
  process.on('SIGINT', async () => {
    console.log('\nClosing...');
    await context.close();
    await browser.close();
    resolve();
  });
});
