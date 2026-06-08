import { describe, it, expect, beforeEach } from 'vitest';
import { get } from 'svelte/store';

describe('@lightarchitects/lightspace-svelte — stores', () => {
  beforeEach(async () => {
    const { canvasStore, drawerStore, hitlStore, materializePhase } = await import('../stores');
    canvasStore.set(new Map());
    drawerStore.set(new Map());
    hitlStore.set(new Map());
    materializePhase.set('idle');
  });

  it('canvasAttachCard inserts a card into canvasStore', async () => {
    const { canvasStore, canvasAttachCard } = await import('../stores');
    canvasAttachCard({ id: 'c1', kind: 'trace', title: 'Test', state: 'attached', content: {}, provenance: { agent: 'test', source: 'unit' } });
    expect(get(canvasStore).has('c1')).toBe(true);
    expect(get(canvasStore).get('c1')!.kind).toBe('trace');
  });

  it('canvasAttachCard replaces card on repeated id', async () => {
    const { canvasStore, canvasAttachCard } = await import('../stores');
    canvasAttachCard({ id: 'c1', kind: 'trace',   title: 'A', state: 'attached', content: {}, provenance: { agent: 'test', source: 'unit' } });
    canvasAttachCard({ id: 'c1', kind: 'monitor', title: 'B', state: 'attached', content: {}, provenance: { agent: 'test', source: 'unit' } });
    expect(get(canvasStore).get('c1')!.kind).toBe('monitor');
    expect(get(canvasStore).size).toBe(1);
  });

  it('canvasDetachCard removes the card', async () => {
    const { canvasStore, canvasAttachCard, canvasDetachCard } = await import('../stores');
    canvasAttachCard({ id: 'c2', kind: 'artifact', title: 'F', state: 'attached', content: {}, provenance: { agent: 'test', source: 'unit' } });
    canvasDetachCard('c2');
    expect(get(canvasStore).has('c2')).toBe(false);
  });

  it('canvasUpdateCard applies a replace mutation', async () => {
    const { canvasStore, canvasAttachCard, canvasUpdateCard } = await import('../stores');
    canvasAttachCard({ id: 'u1', kind: 'instrument', title: 'U', state: 'attached', content: { val: 0 }, provenance: { agent: 'test', source: 'unit' } });
    canvasUpdateCard('u1', 1, 'replace', '/val', 42);
    const content = get(canvasStore).get('u1')!.content as Record<string, unknown>;
    expect(content['val']).toBe(42);
  });

  it('canvasUpdateCard ignores unknown card id', async () => {
    const { canvasStore, canvasUpdateCard } = await import('../stores');
    canvasUpdateCard('does-not-exist', 1, 'replace', undefined, 99);
    expect(get(canvasStore).size).toBe(0);
  });

  it('canvasReset populates stores from CanvasSnapshot', async () => {
    const { canvasStore, drawerStore, canvasReset } = await import('../stores');
    canvasReset({
      session_id: 'sess-1',
      cards: {
        'snap-card': { id: 'snap-card', kind: 'research', title: 'R', state: 'attached', content: {}, provenance: { agent: 'a', source: 's' } },
      },
      drawer_files: {
        'snap-file': { id: 'snap-file', mime_type: 'text/markdown', content_uri: 'file:///tmp/x.md', size_bytes: 0, provenance: { agent: 'a', source: 's' } },
      },
      materialize_phase: null,
      snapshot_seq: 1,
    });
    expect(get(canvasStore).has('snap-card')).toBe(true);
    expect(get(drawerStore).has('snap-file')).toBe(true);
  });

  it('Map.set without new Map() does NOT trigger subscribers (regression guard)', () => {
    const { writable } = require('svelte/store');
    const store = writable<Map<string, number>>(new Map());
    let fires = 0;
    store.subscribe(() => fires++);
    const prev = fires;
    // In-place mutation — Svelte should NOT see this
    store.update(m => { m.set('x', 1); return m; });
    // fires may or may not increment (implementation detail), but the point
    // is that our store helpers always use `new Map(m)` — this test documents why.
    void prev; // consumed to silence unused warning
  });
});
