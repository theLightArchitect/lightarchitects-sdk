/**
 * SvelteKit route structure tests.
 *
 * matchRoute() was removed in the SvelteKit migration (2026-06-08) — routing
 * is now handled by SvelteKit's file-system router in src/routes/.
 *
 * These tests verify:
 *   1. Key +page.svelte files exist for the expected URL paths
 *   2. Redirect +page.ts files exist for all legacy aliases
 *   3. Param directory names match what screen components read via page.params
 */

import { describe, it, expect } from 'vitest';
import { existsSync } from 'fs';
import { resolve } from 'path';

const R = (rel: string) => resolve(process.cwd(), 'src/routes', rel);
const exists = (rel: string) => existsSync(R(rel));

// ── Screen pages ────────────────────────────────────────────────────────────
describe('SvelteKit route structure — screen pages', () => {
  it('/ → Lightspace', () => expect(exists('+page.svelte')).toBe(true));
  it('/dashboard → Dashboard', () => expect(exists('dashboard/+page.svelte')).toBe(true));
  it('/dispatch → Dispatch', () => expect(exists('dispatch/+page.svelte')).toBe(true));
  it('/builds → Builds', () => expect(exists('builds/+page.svelte')).toBe(true));
  it('/intake → Intake', () => expect(exists('intake/+page.svelte')).toBe(true));
  it('/helix → Helix', () => expect(exists('helix/+page.svelte')).toBe(true));
  it('/activity → Comms', () => expect(exists('activity/+page.svelte')).toBe(true));
  it('/knowledge → Helix', () => expect(exists('knowledge/+page.svelte')).toBe(true));
  it('/git → Git', () => expect(exists('git/+page.svelte')).toBe(true));
  it('/observability → Observability', () => expect(exists('observability/+page.svelte')).toBe(true));
  it('/security → Security', () => expect(exists('security/+page.svelte')).toBe(true));
  it('/tools → Tools', () => expect(exists('tools/+page.svelte')).toBe(true));
  it('/chat → Chat', () => expect(exists('chat/+page.svelte')).toBe(true));
  it('/cockpit/platform → CockpitPlatform', () => expect(exists('cockpit/platform/+page.svelte')).toBe(true));
});

// ── Parameterized routes ─────────────────────────────────────────────────────
describe('SvelteKit route structure — parameterized routes', () => {
  it('/builds/[buildId] exists', () => expect(exists('builds/[buildId]/+page.svelte')).toBe(true));
  it('/builds/[buildId]/[view] exists', () => expect(exists('builds/[buildId]/[view]/+page.svelte')).toBe(true));
  it('/builds/[buildId]/phase/[phaseId] exists', () => expect(exists('builds/[buildId]/phase/[phaseId]/+page.svelte')).toBe(true));
  it('/builds/[buildId]/phase/[phaseId]/wave/[waveId] exists', () =>
    expect(exists('builds/[buildId]/phase/[phaseId]/wave/[waveId]/+page.svelte')).toBe(true));
  it('/builds/[buildId]/phase/[phaseId]/wave/[waveId]/agent/[agentKey] exists', () =>
    expect(exists('builds/[buildId]/phase/[phaseId]/wave/[waveId]/agent/[agentKey]/+page.svelte')).toBe(true));
  it('/builds/[buildId]/phase/[phaseId]/wave/[waveId]/agent/[agentKey]/task/[taskId] exists', () =>
    expect(exists('builds/[buildId]/phase/[phaseId]/wave/[waveId]/agent/[agentKey]/task/[taskId]/+page.svelte')).toBe(true));
  it('/dispatch/run/[runId] exists', () => expect(exists('dispatch/run/[runId]/+page.svelte')).toBe(true));
  it('/helix/strand/[siblingKey] exists', () => expect(exists('helix/strand/[siblingKey]/+page.svelte')).toBe(true));
  it('/helix/entry/[entryId] exists', () => expect(exists('helix/entry/[entryId]/+page.svelte')).toBe(true));
  it('/project/[projectId] exists', () => expect(exists('project/[projectId]/+page.svelte')).toBe(true));
  it('/pr/[number] exists', () => expect(exists('pr/[number]/+page.svelte')).toBe(true));
  it('/pr/new exists', () => expect(exists('pr/new/+page.svelte')).toBe(true));
  it('/cockpit/project/[projectId] exists', () => expect(exists('cockpit/project/[projectId]/+page.svelte')).toBe(true));
  it('/cockpit/build/[codename] exists', () => expect(exists('cockpit/build/[codename]/+page.svelte')).toBe(true));
  it('/cockpit/file/[codename]/[...filePath] exists', () =>
    expect(exists('cockpit/file/[codename]/[...filePath]/+page.svelte')).toBe(true));
  it('/editor/[...filepath] exists', () => expect(exists('editor/[...filepath]/+page.svelte')).toBe(true));
  it('/diagrams/[...project] exists', () => expect(exists('diagrams/[...project]/+page.svelte')).toBe(true));
});

// ── Legacy alias redirects ───────────────────────────────────────────────────
describe('SvelteKit route structure — redirect pages', () => {
  it('/ops → /dashboard redirect exists', () => expect(exists('ops/+page.ts')).toBe(true));
  it('/monitor → /dashboard redirect exists', () => expect(exists('monitor/+page.ts')).toBe(true));
  it('/workspace/[...path] → /builds redirect exists', () => expect(exists('workspace/[...path]/+page.ts')).toBe(true));
  it('/cockpit → /cockpit/platform redirect exists', () => expect(exists('cockpit/+page.ts')).toBe(true));
  it('/ayin → /observability redirect exists', () => expect(exists('ayin/+page.ts')).toBe(true));
  it('/manage → /builds redirect exists', () => expect(exists('manage/+page.ts')).toBe(true));
  it('/arch/[...project] → /diagrams redirect exists', () => expect(exists('arch/[...project]/+page.ts')).toBe(true));
  it('/squad-dispatch → /dispatch redirect exists', () => expect(exists('squad-dispatch/+page.ts')).toBe(true));
});

// ── Root layout and build config ──────────────────────────────────────────────
describe('SvelteKit root layout and build config', () => {
  it('+layout.svelte exists (shared chrome)', () => expect(exists('+layout.svelte')).toBe(true));

  it('src/app.html exists (SvelteKit entry)', () =>
    expect(existsSync(resolve(process.cwd(), 'src/app.html'))).toBe(true));

  it('svelte.config.js uses adapter-static outputting to dist/ with index.html fallback', () => {
    // eslint-disable-next-line @typescript-eslint/no-require-imports
    const fs = require('fs') as typeof import('fs');
    const config = fs.readFileSync(resolve(process.cwd(), 'svelte.config.js'), 'utf-8');
    expect(config).toContain('adapter-static');
    // Output to dist/ — required by lightarchitects-webshell rust-embed (#[folder="../lightarchitects-webshell-ui/dist/"])
    expect(config).toContain("pages: 'dist'");
    // SPA fallback matches static_assets.rs::serve() fallback call to Assets::get("index.html")
    expect(config).toContain("fallback: 'index.html'");
  });
});
