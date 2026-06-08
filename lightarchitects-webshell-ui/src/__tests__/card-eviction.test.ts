/**
 * card-eviction.ts — Phase 2 gate [T]
 * Tests the pure-TS eviction engine: evictPriority(), selectEvictionVictim(),
 * and the canvasAddCard eviction-on-full behaviour.
 */

import { describe, it, expect } from 'vitest';
import {
  evictPriority,
  selectEvictionVictim,
  MAX_CANVAS_CARDS,
  KIND_EVICT_PRIORITY,
} from '$lib/card-eviction';
import type { BentoCardData } from '$lib/lightspace-types';

function makeCard(id: string, kind: BentoCardData['kind'], opts: Partial<BentoCardData> = {}): BentoCardData {
  return { id, kind, span: 'span-4', title: id, ts: Date.now(), data: {}, ...opts };
}

describe('card-eviction: evictPriority()', () => {
  it('returns -1 for pinned cards (never evict)', () => {
    expect(evictPriority(makeCard('p', 'monitor', { _pinned: true }))).toBe(-1);
  });

  it('returns 9 for completed agentspawn cards', () => {
    expect(evictPriority(makeCard('a', 'agentspawn', { _agentDone: true }))).toBe(9);
  });

  it('returns base priority for live agentspawn', () => {
    expect(evictPriority(makeCard('a', 'agentspawn'))).toBe(KIND_EVICT_PRIORITY.agentspawn);
  });

  it('trace has lowest base priority (0)', () => {
    expect(evictPriority(makeCard('t', 'trace'))).toBe(0);
  });

  it('diff has highest base priority (8)', () => {
    expect(evictPriority(makeCard('d', 'diff'))).toBe(8);
  });

  it('unknown kind falls back to 5', () => {
    const card: BentoCardData = { id: 'u', kind: 'unknown' as BentoCardData['kind'], span: 'span-4', title: 'u', ts: 0, data: {} };
    expect(evictPriority(card)).toBe(5);
  });
});

describe('card-eviction: selectEvictionVictim()', () => {
  it('returns null for empty array', () => {
    expect(selectEvictionVictim([])).toBeNull();
  });

  it('returns null when all cards are pinned', () => {
    const cards = [
      makeCard('a', 'monitor', { _pinned: true }),
      makeCard('b', 'trace',   { _pinned: true }),
    ];
    expect(selectEvictionVictim(cards)).toBeNull();
  });

  it('selects the highest-priority victim (diff > monitor)', () => {
    const cards = [makeCard('m', 'monitor'), makeCard('d', 'diff')];
    expect(selectEvictionVictim(cards)?.id).toBe('d');
  });

  it('tie-breaks by insertion order (later index evicted first)', () => {
    const cards = [makeCard('a', 'diff'), makeCard('b', 'diff')];
    // Both are diff (priority 8) — later index wins the eviction
    expect(selectEvictionVictim(cards)?.id).toBe('b');
  });

  it('skips pinned cards and evicts the next highest', () => {
    const cards = [
      makeCard('arch', 'archgallery'),           // priority 7
      makeCard('diff', 'diff', { _pinned: true }), // priority -1 (pinned)
    ];
    expect(selectEvictionVictim(cards)?.id).toBe('arch');
  });

  it('completed agentspawn (priority 9) evicted before diff (priority 8)', () => {
    const cards = [
      makeCard('d', 'diff'),
      makeCard('a', 'agentspawn', { _agentDone: true }),
    ];
    expect(selectEvictionVictim(cards)?.id).toBe('a');
  });
});

describe('card-eviction: MAX_CANVAS_CARDS', () => {
  it('is exactly 9', () => {
    expect(MAX_CANVAS_CARDS).toBe(9);
  });
});

describe('card-eviction: canvasAddCard via store — eviction on full canvas', () => {
  // Import store functions here to test the integration path
  it('canvasAddCard evicts lowest-priority when canvas is full', async () => {
    const { canvasAddCard, lightspaceCanvasStore, canvasClear } = await import('$lib/lightspace-stores');
    const { get } = await import('svelte/store');

    canvasClear();

    // Fill canvas with 9 trace cards (priority 0 — last to go)
    for (let i = 0; i < MAX_CANVAS_CARDS; i++) {
      canvasAddCard(makeCard(`t${i}`, 'trace'));
    }
    expect(get(lightspaceCanvasStore).cards).toHaveLength(9);

    // Add a 10th card — should evict one trace (the last inserted wins ties)
    canvasAddCard(makeCard('new', 'monitor'));
    const state = get(lightspaceCanvasStore);
    expect(state.cards).toHaveLength(9);
    expect(state.cards.find(c => c.id === 'new')).toBeDefined();
    expect(state.tombstones).toHaveLength(1);

    canvasClear();
  });

  it('canvasAddCard never evicts pinned cards', async () => {
    const { canvasAddCard, lightspaceCanvasStore, canvasClear } = await import('$lib/lightspace-stores');
    const { get } = await import('svelte/store');

    canvasClear();

    // Fill canvas with 8 traces + 1 pinned diff
    for (let i = 0; i < 8; i++) canvasAddCard(makeCard(`t${i}`, 'trace'));
    canvasAddCard(makeCard('pinned-diff', 'diff', { _pinned: true }));
    expect(get(lightspaceCanvasStore).cards).toHaveLength(9);

    // Adding a 10th should evict a trace, not the pinned diff
    canvasAddCard(makeCard('new', 'monitor'));
    const state = get(lightspaceCanvasStore);
    expect(state.cards.find(c => c.id === 'pinned-diff')).toBeDefined();
    expect(state.tombstones[0].id).toMatch(/^t/);

    canvasClear();
  });
});
