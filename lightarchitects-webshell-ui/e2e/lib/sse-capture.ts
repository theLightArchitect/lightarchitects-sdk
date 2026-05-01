/**
 * SSE transcript capture — intercepts and records every SSE frame (§57.2a).
 *
 * Registers a Playwright route interceptor that passes through the real
 * response while writing each `data:` frame to an NDJSON transcript.
 * The interceptor is transparent — the app receives the real stream.
 */
import type { Page } from '@playwright/test';
import type { SseFrame } from './artifacts';
import * as fs from 'node:fs';

/**
 * Attach SSE capture for a specific build's event stream.
 *
 * @param page     Playwright page
 * @param buildId  Build ID to capture (`*` to capture all builds)
 * @param outPath  Path to write NDJSON transcript
 * @returns        Array that accumulates frames as they arrive
 */
export function captureSseTranscript(
  page: Page,
  buildId: string,
  outPath: string,
): SseFrame[] {
  const frames: SseFrame[] = [];
  fs.mkdirSync(require('node:path').dirname(outPath), { recursive: true });

  page.route(`**/api/builds/${buildId}/events`, async (route) => {
    const response = await route.fetch();
    const body = await response.text();

    for (const line of body.split('\n')) {
      const trimmed = line.trim();
      if (!trimmed.startsWith('data:')) continue;
      const frame: SseFrame = {
        ts: Date.now(),
        raw: trimmed,
        parsed: tryParse(trimmed.slice(5).trim()),
      };
      frames.push(frame);
      fs.appendFileSync(outPath, JSON.stringify(frame) + '\n');
    }

    await route.fulfill({ response });
  }).catch(() => {});

  return frames;
}

function tryParse(s: string): unknown {
  try { return JSON.parse(s); } catch { return s; }
}
