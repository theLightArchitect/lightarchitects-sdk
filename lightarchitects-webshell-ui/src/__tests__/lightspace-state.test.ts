import { describe, it, expect, beforeEach } from 'vitest';
import { LightspaceState } from '$lib/lightspace/state.svelte';
import type { Card, LightspaceFile } from '$lib/lightspace/types';

// LightspaceState is a class — we instantiate a fresh copy per test
// to avoid cross-test state contamination from the module singleton.
function makeState() { return new LightspaceState(); }

const CARD_A: Card = { id: 'a', kind: 'monitor', title: 'A', body: '' };
const CARD_B: Card = { id: 'b', kind: 'trace',   title: 'B', body: '' };
const FILE_1: LightspaceFile = {
  id: 'f1', name: 'plan.md', mime: 'md',
  meta: 'copilot', prov: { agent: 'copilot' },
};

describe('LightspaceState', () => {

  describe('lobby → workspace transition', () => {
    it('starts in lobby', () => {
      const ls = makeState();
      expect(ls.inLobby).toBe(true);
      expect(ls.materializing).toBe(false);
    });

    it('exitLobby() leaves lobby and starts materializing', () => {
      const ls = makeState();
      ls.exitLobby();
      expect(ls.inLobby).toBe(false);
      expect(ls.materializing).toBe(true);
      expect(ls.wsState).toBe('materialising');
    });

    it('setMatPhase(complete) ends materializing', () => {
      const ls = makeState();
      ls.exitLobby();
      ls.setMatPhase('complete');
      expect(ls.materializing).toBe(false);
      expect(ls.wsState).toBe('materialised');
    });

    it('setMatPhase accumulates seen phases without duplicates', () => {
      const ls = makeState();
      ls.setMatPhase('begin');
      ls.setMatPhase('begin');  // duplicate
      ls.setMatPhase('grid_revealed');
      expect(ls.matPhasesSeen.size).toBe(2);
      expect(ls.matPhasesSeen.has('begin')).toBe(true);
      expect(ls.matPhasesSeen.has('grid_revealed')).toBe(true);
    });
  });

  describe('card management', () => {
    it('addCard appends a card', () => {
      const ls = makeState();
      ls.addCard(CARD_A);
      expect(ls.cards.length).toBe(1);
      expect(ls.cards[0].id).toBe('a');
    });

    it('updateCard mutates the card in place', () => {
      const ls = makeState();
      ls.addCard(CARD_A);
      ls.updateCard('a', { title: 'Updated' });
      expect(ls.cards[0].title).toBe('Updated');
    });

    it('updateCard on unknown id is a no-op', () => {
      const ls = makeState();
      ls.addCard(CARD_A);
      ls.updateCard('z', { title: 'Ghost' });
      expect(ls.cards[0].title).toBe('A');
    });

    it('removeCard moves the card to cachedCards', () => {
      const ls = makeState();
      ls.addCard(CARD_A);
      ls.removeCard('a');
      expect(ls.cards.length).toBe(0);
      expect(ls.cachedCards.length).toBe(1);
      expect(ls.cachedCards[0].id).toBe('a');
    });

    it('removeCard on unknown id is a no-op', () => {
      const ls = makeState();
      ls.addCard(CARD_A);
      ls.removeCard('z');
      expect(ls.cards.length).toBe(1);
      expect(ls.cachedCards.length).toBe(0);
    });

    it('evicts lowest-priority card when canvas is full (9 cards)', () => {
      const ls = makeState();
      // Fill canvas with trace cards (evict priority = 0 = never evict)
      // plus one diff card (priority = 8 = first to go)
      for (let i = 0; i < 8; i++) {
        ls.addCard({ id: `t${i}`, kind: 'trace', title: `T${i}`, body: '' });
      }
      ls.addCard({ id: 'diff-1', kind: 'diff', title: 'Diff', body: '' });
      expect(ls.cards.length).toBe(9);

      // Adding a 10th triggers eviction of the diff card
      ls.addCard({ id: 'new', kind: 'monitor', title: 'New', body: '' });
      expect(ls.cards.length).toBe(9);
      expect(ls.cards.find(c => c.id === 'diff-1')).toBeUndefined();
      expect(ls.cachedCards.find(c => c.id === 'diff-1')).toBeDefined();
    });

    it('pinned cards are never auto-evicted', () => {
      const ls = makeState();
      ls.addCard({ id: 'pinned', kind: 'diff', title: 'Pinned', body: '', pinned: true });
      for (let i = 0; i < 9; i++) {
        ls.addCard({ id: `x${i}`, kind: 'trace', title: `X${i}`, body: '' });
      }
      expect(ls.cards.find(c => c.id === 'pinned')).toBeDefined();
    });
  });

  describe('conversation', () => {
    it('addConv appends messages', () => {
      const ls = makeState();
      ls.addConv({ who: 'operator', text: 'Hello' });
      ls.addConv({ who: 'copilot',  text: 'World' });
      expect(ls.conv.length).toBe(2);
      expect(ls.conv[1].who).toBe('copilot');
    });
  });

  describe('files', () => {
    it('addFile adds a file and opens the drawer', () => {
      const ls = makeState();
      ls.addFile(FILE_1);
      expect(ls.files.length).toBe(1);
      expect(ls.filesOpen).toBe(true);
    });

    it('addFile is idempotent by id', () => {
      const ls = makeState();
      ls.addFile(FILE_1);
      ls.addFile(FILE_1);
      expect(ls.files.length).toBe(1);
    });
  });

  describe('tickSpan', () => {
    it('increments span counter and records last event', () => {
      const ls = makeState();
      ls.tickSpan('quantum.spawn');
      expect(ls.spans).toBe(1);
      expect(ls.lastEvent).toBe('quantum.spawn');
    });

    it('caps throughput history at 12 samples', () => {
      const ls = makeState();
      for (let i = 0; i < 20; i++) ls.tickSpan('evt');
      expect(ls.throughputHistory.length).toBeLessThanOrEqual(12);
    });
  });

  describe('reset', () => {
    it('clears all workspace state', () => {
      const ls = makeState();
      ls.addCard(CARD_A);
      ls.addConv({ who: 'operator', text: 'Hi' });
      ls.addFile(FILE_1);
      ls.tickSpan('test');
      ls.reset();
      expect(ls.cards.length).toBe(0);
      expect(ls.conv.length).toBe(0);
      expect(ls.files.length).toBe(0);
      expect(ls.spans).toBe(0);
    });
  });

  describe('rootClass', () => {
    it('includes in-lobby when inLobby is true', () => {
      const ls = makeState();
      expect(ls.rootClass).toContain('in-lobby');
    });

    it('removes in-lobby after exitLobby', () => {
      const ls = makeState();
      ls.exitLobby();
      expect(ls.rootClass).not.toContain('in-lobby');
      expect(ls.rootClass).toContain('materializing');
    });

    it('includes rail-collapsed when railCollapsed', () => {
      const ls = makeState();
      ls.railCollapsed = true;
      expect(ls.rootClass).toContain('rail-collapsed');
    });
  });
});
