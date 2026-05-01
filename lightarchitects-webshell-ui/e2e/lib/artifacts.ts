/**
 * EvidenceCollector — §57.2 artifact bundle implementation.
 *
 * Instantiate once per test in beforeEach; call flush() in afterEach.
 * Always writes artifacts — not just on failure (§57.2a).
 *
 * Usage:
 *   let ec: EvidenceCollector;
 *   test.beforeEach(async () => { ec = new EvidenceCollector(page, test.info()); });
 *   test.afterEach(async ({}, testInfo) => { await ec.flush(testInfo.status === 'passed'); });
 */
import type { Page, TestInfo } from '@playwright/test';
import * as fs from 'node:fs';
import * as path from 'node:path';
import { captureStoreSnapshot } from './store-snapshot';
import { emitAyinSpan } from './ayin';

export interface ConsoleEntry {
  type: string;
  text: string;
  ts: number;
}

export interface FailedRequest {
  url: string;
  method: string;
  status: number;
  ts: number;
}

export interface SseFrame {
  ts: number;
  raw: string;
  parsed: unknown;
}

export interface AyinSpanSummary {
  actor: string;
  action: string;
  outcome: string;
  ts: number;
}

export interface EvidenceBundle {
  version: '1';
  testName: string;
  specFile: string;
  passed: boolean;
  timestamp: string;
  durationMs: number;
  failedAssertion?: string;
  storeSnapshot: Record<string, unknown>;
  consoleLogs: ConsoleEntry[];
  failedRequests: FailedRequest[];
  lastSseFrames: SseFrame[];
  artifactPaths: {
    screenshot: string;
    video?: string;
    har: string;
    playwrightTrace: string;
    serverLog?: string;
  };
  ayinSpans: AyinSpanSummary[];
}

export class EvidenceCollector {
  private consoleLogs: ConsoleEntry[] = [];
  private failedRequests: FailedRequest[] = [];
  private sseFrames: SseFrame[] = [];
  private ayinSpans: AyinSpanSummary[] = [];
  private startMs = Date.now();
  private outDir: string;

  constructor(
    private page: Page,
    private testInfo: TestInfo,
  ) {
    this.outDir = path.join(
      'test-results', 'runs',
      `${new Date().toISOString().replace(/[:.]/g, '-')}_${slugify(testInfo.titlePath.join('_'))}`,
    );
    fs.mkdirSync(path.join(this.outDir, 'network'), { recursive: true });
    fs.mkdirSync(path.join(this.outDir, 'visual'), { recursive: true });
    fs.mkdirSync(path.join(this.outDir, 'runtime'), { recursive: true });
    fs.mkdirSync(path.join(this.outDir, 'trace'), { recursive: true });
    fs.mkdirSync(path.join(this.outDir, 'report'), { recursive: true });

    page.on('console', (msg) => {
      this.consoleLogs.push({ type: msg.type(), text: msg.text(), ts: Date.now() });
    });

    page.on('response', (res) => {
      if (res.status() >= 400 && !res.url().includes('/events')) {
        this.failedRequests.push({
          url: res.url(),
          method: res.request().method(),
          status: res.status(),
          ts: Date.now(),
        });
      }
    });
  }

  /** Record an SSE frame for the transcript (call from sse-capture.ts). */
  recordSseFrame(frame: SseFrame): void {
    this.sseFrames.push(frame);
    const file = path.join(this.outDir, 'network', 'sse-transcript.ndjson');
    fs.appendFileSync(file, JSON.stringify(frame) + '\n');
  }

  /** Record an AYIN span observed during the test. */
  recordAyinSpan(span: AyinSpanSummary): void {
    this.ayinSpans.push(span);
    const file = path.join(this.outDir, 'trace', 'ayin-spans.ndjson');
    fs.appendFileSync(file, JSON.stringify(span) + '\n');
  }

  /** Write all artifacts and the evidence-bundle.json. Call in afterEach. */
  async flush(passed: boolean, failedAssertion?: string): Promise<void> {
    const screenshotName = passed ? 'pass.png' : 'fail.png';
    const screenshotPath = path.join(this.outDir, 'visual', screenshotName);

    const [, storeSnapshot] = await Promise.all([
      this.page.screenshot({ fullPage: true, path: screenshotPath }).catch(() => null),
      captureStoreSnapshot(this.page),
    ]);

    // Write console log
    const consoleFile = path.join(this.outDir, 'runtime', 'console.ndjson');
    for (const entry of this.consoleLogs) {
      fs.appendFileSync(consoleFile, JSON.stringify(entry) + '\n');
    }

    // Write failed requests
    if (this.failedRequests.length > 0) {
      fs.writeFileSync(
        path.join(this.outDir, 'runtime', 'failed-requests.json'),
        JSON.stringify(this.failedRequests, null, 2),
      );
    }

    // Write store snapshot
    fs.writeFileSync(
      path.join(this.outDir, 'runtime', 'stores-snapshot.json'),
      JSON.stringify(storeSnapshot, null, 2),
    );

    const bundle: EvidenceBundle = {
      version: '1',
      testName: this.testInfo.title,
      specFile: this.testInfo.file,
      passed,
      timestamp: new Date().toISOString(),
      durationMs: Date.now() - this.startMs,
      failedAssertion,
      storeSnapshot,
      consoleLogs: this.consoleLogs.filter(e => e.type === 'error' || e.type === 'warning'),
      failedRequests: this.failedRequests,
      lastSseFrames: this.sseFrames.slice(-10),
      artifactPaths: {
        screenshot: screenshotPath,
        har: path.join(this.outDir, '..', '..', 'test-results', 'webshell-e2e.har'),
        playwrightTrace: path.join(this.outDir, 'trace', 'playwright.zip'),
      },
      ayinSpans: this.ayinSpans,
    };

    fs.writeFileSync(
      path.join(this.outDir, 'report', 'evidence-bundle.json'),
      JSON.stringify(bundle, null, 2),
    );

    if (!passed) {
      await emitAyinSpan({
        actor: 'e2e',
        action: 'test_failed',
        label: this.testInfo.title,
        outcome: { passed: false, failedAssertion, bundlePath: path.join(this.outDir, 'report', 'evidence-bundle.json') },
      }).catch(() => {});
    }
  }
}

function slugify(s: string): string {
  return s.replace(/[^a-z0-9]+/gi, '-').slice(0, 80).toLowerCase();
}
