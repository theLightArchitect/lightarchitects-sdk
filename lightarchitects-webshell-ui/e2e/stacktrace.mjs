#!/usr/bin/env node
// Headed Chrome — captures full stack trace of effect_update_depth_exceeded.
// Browser stays open.
import { chromium } from '@playwright/test';
import { writeFileSync, mkdirSync } from 'fs';
import { join } from 'path';

const BASE = process.env.BASE_URL || 'http://localhost:9740';
const HAR_DIR = join(import.meta.dirname, '..', 'test-results');
mkdirSync(HAR_DIR, { recursive: true });

const browser = await chromium.launch({
  headless: false,
  channel: 'chrome',
  args: ['--auto-open-devtools-for-tabs'],
});

const context = await browser.newContext({
  recordHar: { path: join(HAR_DIR, 'stacktrace.har'), mode: 'full' },
  viewport: { width: 1440, height: 900 },
});

const page = await context.newPage();

// Inject error handler BEFORE navigation to capture stack traces
await page.addInitScript(() => {
  const origError = window.onerror;
  window.onerror = function(msg, src, line, col, err) {
    if (String(msg).includes('effect_update_depth_exceeded') || (err && String(err.message).includes('effect_update_depth_exceeded'))) {
      console.error('__EFFECT_LOOP_STACK__', err ? err.stack : new Error(msg).stack);
    }
    if (origError) return origError.apply(this, arguments);
  };

  // Also catch unhandled rejections
  window.addEventListener('unhandledrejection', (e) => {
    const msg = e.reason?.message || String(e.reason);
    if (msg.includes('effect_update_depth_exceeded')) {
      console.error('__EFFECT_LOOP_REJECTION__', e.reason?.stack || msg);
    }
  });
});

page.on('pageerror', err => {
  if (err.message.includes('effect_update_depth_exceeded')) {
    console.log('\n=== EFFECT LOOP — PAGE ERROR STACK ===');
    console.log(err.stack || err.message);
    console.log('======================================\n');
  }
});

page.on('console', msg => {
  const text = msg.text();
  if (text.includes('__EFFECT_LOOP_STACK__') || text.includes('__EFFECT_LOOP_REJECTION__')) {
    console.log('\n=== EFFECT LOOP — INJECTED STACK ===');
    console.log(text);
    console.log('====================================\n');
  }
});

console.log(`Navigating to ${BASE}...`);
try {
  await page.goto(BASE, { waitUntil: 'domcontentloaded', timeout: 15000 });
} catch (e) {
  console.log('Nav:', e.message.slice(0, 100));
}

console.log('Waiting 10s for effect loop to fire...');
await page.waitForTimeout(10000);

console.log('\n>>> Browser open. Ctrl+C to close. <<<');
await new Promise(r => process.on('SIGINT', async () => {
  await context.close();
  await browser.close();
  r();
}));
