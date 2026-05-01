#!/usr/bin/env node
// ============================================================================
// Headed Chrome diagnostic — captures HAR, console, network, and effect errors.
// Stays open for manual inspection. Ctrl+C to close.
// ============================================================================
import { chromium } from '@playwright/test';
import { writeFileSync, mkdirSync } from 'fs';
import { join } from 'path';

const BASE = process.env.BASE_URL || 'http://localhost:9739';
const HAR_DIR = join(import.meta.dirname, '..', 'test-results');
const HAR_PATH = join(HAR_DIR, 'diagnose.har');

mkdirSync(HAR_DIR, { recursive: true });

const consoleMsgs = [];
const networkLog = [];
const errors = [];
let effectLoopDetected = false;

console.log(`\n=== WEBSHELL DIAGNOSTIC ===`);
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

// --- Console capture ---
page.on('console', (msg) => {
  const entry = { type: msg.type(), text: msg.text(), ts: Date.now() };
  consoleMsgs.push(entry);

  const prefix = msg.type() === 'error' ? '  [ERR]' :
                 msg.type() === 'warning' ? '  [WRN]' : '  [LOG]';
  console.log(`${prefix} ${msg.text().slice(0, 200)}`);

  if (msg.text().includes('effect_update_depth_exceeded')) {
    effectLoopDetected = true;
    console.log('\n  !!! EFFECT LOOP DETECTED !!!\n');
  }
});

// --- Page errors (uncaught exceptions) ---
page.on('pageerror', (err) => {
  errors.push({ message: err.message, ts: Date.now() });
  console.log(`  [CRASH] ${err.message.slice(0, 300)}`);
  if (err.message.includes('effect_update_depth_exceeded')) {
    effectLoopDetected = true;
    console.log('\n  !!! EFFECT LOOP DETECTED VIA PAGE ERROR !!!\n');
  }
});

// --- Network response capture ---
page.on('response', (resp) => {
  const entry = { url: resp.url(), status: resp.status(), ts: Date.now() };
  networkLog.push(entry);
  if (resp.status() >= 400) {
    console.log(`  [NET ${resp.status()}] ${resp.url()}`);
  }
});

// --- Navigate ---
console.log('Navigating to webshell...');
try {
  await page.goto(BASE, { waitUntil: 'domcontentloaded', timeout: 15000 });
  console.log('  Page loaded (domcontentloaded).');
} catch (e) {
  console.log(`  Navigation partial: ${e.message.slice(0, 200)}`);
}

// --- Phase 1: Watch for 8 seconds (splash + auto-advance window) ---
console.log('\n--- Phase 1: Splash observation (8s) ---');
await page.waitForTimeout(8000);

// Check if splash is still showing
const splashVisible = await page.evaluate(() => {
  const setupFlow = document.querySelector('[class*="setup"]') ||
                    document.querySelector('canvas:only-child');
  const navBar = document.querySelector('nav');
  return { hasNav: !!navBar, hasSplash: !navBar };
}).catch(() => ({ hasNav: false, hasSplash: true }));

console.log(`  Splash state: nav=${splashVisible.hasNav}, splash=${splashVisible.hasSplash}`);

if (splashVisible.hasSplash) {
  console.log('  Splash still showing — trying click to advance...');
  await page.click('body').catch(() => {});
  await page.waitForTimeout(3000);
}

// --- Phase 2: Check for main layout ---
console.log('\n--- Phase 2: Main layout check ---');
const layoutCheck = await page.evaluate(() => {
  const nav = document.querySelector('nav');
  const buttons = nav ? Array.from(nav.querySelectorAll('button')).map(b => b.textContent) : [];
  const canvas = document.querySelector('canvas');
  return {
    hasNav: !!nav,
    navButtons: buttons,
    hasCanvas: !!canvas,
    bodyHTML: document.body.innerHTML.slice(0, 500),
  };
}).catch((e) => ({ error: e.message }));

console.log(`  Layout: ${JSON.stringify(layoutCheck, null, 2)}`);

// --- Phase 3: Try tab navigation ---
if (layoutCheck.hasNav) {
  console.log('\n--- Phase 3: Tab navigation ---');
  for (const label of ['Activity', 'Queue', 'Intake', 'Sitrep']) {
    try {
      await page.click(`nav button:has-text("${label}")`);
      await page.waitForTimeout(1500);
      const hash = await page.evaluate(() => window.location.hash);
      const content = await page.evaluate(() => {
        const main = document.querySelector('.flex-1.flex.flex-col');
        return main ? main.textContent.slice(0, 200).trim() : 'NO MAIN CONTENT';
      });
      console.log(`  [${label}] hash=${hash} content="${content.slice(0, 80)}..."`);
    } catch (e) {
      console.log(`  [${label}] FAILED: ${e.message.slice(0, 100)}`);
    }
  }
}

// --- Phase 4: Check copilot drawer ---
console.log('\n--- Phase 4: Copilot & Memory ---');
try {
  await page.keyboard.press('Control+Backquote');
  await page.waitForTimeout(1000);
  const copilotOpen = await page.evaluate(() => {
    return !!document.querySelector('[data-testid="copilot-drawer"]') ||
           !!document.querySelector('.copilot-drawer');
  });
  console.log(`  Copilot drawer: ${copilotOpen ? 'OPEN' : 'not found'}`);
} catch (e) {
  console.log(`  Copilot: ${e.message.slice(0, 100)}`);
}

// --- Summary ---
console.log('\n=== DIAGNOSTIC SUMMARY ===');
console.log(`Console messages: ${consoleMsgs.length}`);
console.log(`Console errors:   ${consoleMsgs.filter(m => m.type === 'error').length}`);
console.log(`Network requests: ${networkLog.length}`);
console.log(`Network errors:   ${networkLog.filter(n => n.status >= 400).length}`);
console.log(`Page crashes:     ${errors.length}`);
console.log(`Effect loop:      ${effectLoopDetected ? 'YES !!!' : 'No'}`);

// List all 4xx/5xx network responses
const failedReqs = networkLog.filter(n => n.status >= 400);
if (failedReqs.length > 0) {
  console.log('\nFailed network requests:');
  for (const r of failedReqs) {
    console.log(`  ${r.status} ${r.url}`);
  }
}

// List all page errors
if (errors.length > 0) {
  console.log('\nPage errors:');
  for (const e of errors) {
    console.log(`  ${e.message.slice(0, 300)}`);
  }
}

// Save diagnostic report
const report = {
  timestamp: new Date().toISOString(),
  base: BASE,
  effectLoopDetected,
  consoleMsgs,
  networkLog: networkLog.filter(n => n.status >= 400),
  pageErrors: errors,
  layoutCheck,
  splashState: splashVisible,
};
const reportPath = join(HAR_DIR, 'diagnose-report.json');
writeFileSync(reportPath, JSON.stringify(report, null, 2));
console.log(`\nReport saved: ${reportPath}`);
console.log(`HAR saved:    ${HAR_PATH}`);

// --- Keep browser open for manual inspection ---
console.log('\n>>> Browser staying open. Press Ctrl+C to close. <<<\n');

// Keep alive — wait for manual close
await new Promise((resolve) => {
  process.on('SIGINT', async () => {
    console.log('\nClosing browser and saving HAR...');
    await context.close(); // This finalizes the HAR file
    await browser.close();
    resolve();
  });
});
