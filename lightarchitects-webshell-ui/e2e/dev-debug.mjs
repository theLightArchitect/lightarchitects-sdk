#!/usr/bin/env node
// Headed Chrome pointed at Vite dev server — full source-mapped stack traces.
import { chromium } from '@playwright/test';
import { mkdirSync } from 'fs';
import { join } from 'path';

const BASE = process.env.BASE_URL || 'http://localhost:5173';
const HAR_DIR = join(import.meta.dirname, '..', 'test-results');
mkdirSync(HAR_DIR, { recursive: true });

const browser = await chromium.launch({
  headless: false,
  channel: 'chrome',
  args: ['--auto-open-devtools-for-tabs'],
});

const context = await browser.newContext({
  recordHar: { path: join(HAR_DIR, 'dev-debug.har'), mode: 'full' },
  viewport: { width: 1440, height: 900 },
});

const page = await context.newPage();

page.on('pageerror', err => {
  console.log('\n=== PAGE ERROR ===');
  console.log(err.stack || err.message);
  console.log('==================\n');
});

page.on('console', msg => {
  if (msg.type() === 'error') {
    console.log(`[ERR] ${msg.text().substring(0, 500)}`);
  }
});

console.log(`Navigating to ${BASE} (Vite dev server)...`);
try {
  await page.goto(BASE, { waitUntil: 'domcontentloaded', timeout: 15000 });
  console.log('Page loaded.');
} catch (e) {
  console.log('Nav:', e.message.slice(0, 200));
}

console.log('Waiting 10s...');
await page.waitForTimeout(10000);

console.log('\n>>> Browser open. Ctrl+C to close. <<<');
await new Promise(r => process.on('SIGINT', async () => {
  await context.close();
  await browser.close();
  r();
}));
