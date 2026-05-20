import { describe, it, expect, vi, afterEach } from 'vitest';
import { get } from 'svelte/store';

afterEach(() => { vi.restoreAllMocks(); });

describe('roadmapStore', () => {
  it('exports roadmapStore with idle initial status', async () => {
    const { roadmapStore } = await import('$lib/roadmap');
    const state = get(roadmapStore);
    expect(state.status).toBe('idle');
    expect(state.rawHtml).toBe('');
    expect(state.error).toBe('');
    expect(state.lastUpdated).toBeNull();
  });

  it('transitions to success on 200 with HTML content', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue({
      ok: true,
      text: () => Promise.resolve('<html><body>kanban</body></html>'),
    }));
    const { roadmapStore } = await import('$lib/roadmap');
    await roadmapStore.fetch();
    const state = get(roadmapStore);
    expect(state.status).toBe('success');
    expect(state.rawHtml).toContain('kanban');
    expect(state.lastUpdated).not.toBeNull();
  });

  it('transitions to empty on 200 with blank body', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue({
      ok: true,
      text: () => Promise.resolve('   '),
    }));
    const { roadmapStore } = await import('$lib/roadmap');
    await roadmapStore.fetch();
    const state = get(roadmapStore);
    expect(state.status).toBe('empty');
    expect(state.rawHtml).toBe('');
  });

  it('transitions to error on non-ok response', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue({
      ok: false,
      status: 500,
      text: () => Promise.resolve('error'),
    }));
    const { roadmapStore } = await import('$lib/roadmap');
    await roadmapStore.fetch();
    const state = get(roadmapStore);
    expect(state.status).toBe('error');
    expect(state.error).toMatch(/HTTP 500/);
  });

  it('transitions to error on network failure', async () => {
    vi.stubGlobal('fetch', vi.fn().mockRejectedValue(new Error('network down')));
    const { roadmapStore } = await import('$lib/roadmap');
    await roadmapStore.fetch();
    const state = get(roadmapStore);
    expect(state.status).toBe('error');
    expect(state.error).toBe('network down');
  });
});

describe('RoadmapPanel', () => {
  it('module imports successfully', async () => {
    const mod = await import('$lib/components/RoadmapPanel.svelte');
    expect(mod.default).toBeDefined();
  });
});
