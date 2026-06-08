/**
 * Lightspace domain stores — 6 writable<T> stores.
 * Follows the existing stores.ts pattern (writable from svelte/store).
 *
 * @integration src/lib/stores.ts — writable pattern
 * @integration src/lib/lightspace-types.ts — all interface types
 * @integration src/lib/card-eviction.ts — MAX_CANVAS_CARDS, selectEvictionVictim
 */

import { writable, derived } from 'svelte/store';
import type {
  LightspaceSessionState,
  LightspaceCanvasState,
  LightspaceFilesState,
  LightspaceUiState,
  LightspaceLasdlcState,
  LightspaceMetricsState,
  BentoCardData,
  TombstoneData,
  SkeletonData,
  ConvMessage,
  MaterializePhase,
} from './lightspace-types';
import { MAX_CANVAS_CARDS, selectEvictionVictim } from './card-eviction';

// ── 1. Session ───────────────────────────────────────────────────────────
export const lightspaceSessionStore = writable<LightspaceSessionState>({
  buildId: null, runStatus: 'idle', intent: '', lobbyInput: '',
  conv: [], mode: 'demo', northstarState: null, materializePhase: 'idle',
});

// ── 2. Canvas ────────────────────────────────────────────────────────────
export const lightspaceCanvasStore = writable<LightspaceCanvasState>({
  cards: [], skeletons: [], tombstones: [], expandedCardId: null, highlightCardId: null,
});

// ── 3. Files ─────────────────────────────────────────────────────────────
export const lightspaceFilesStore = writable<LightspaceFilesState>({
  files: [], activeFileId: null, heroFileId: null, heroTombId: null,
});

// ── 4. UI ────────────────────────────────────────────────────────────────
export const lightspaceUiStore = writable<LightspaceUiState>({
  tombFlash: false, sidebarOpen: true, schematicOpen: true,
  viewPreset: 'default', filesDrawerOpen: false, cacheDrawerOpen: false,
});

// ── 5. LASDLC ────────────────────────────────────────────────────────────
export const lightspaceLasdlcStore = writable<LightspaceLasdlcState>({
  lasdlc: { phases: [], currentPhaseId: null, codename: null },
  gateMatrix: [], branchLanes: [],
});

// ── 6. Metrics ───────────────────────────────────────────────────────────
export const lightspaceMetricsStore = writable<LightspaceMetricsState>({
  loopBudget: { turns: 0, maxTurns: 8, steps: 0, costUsd: 0, status: 'pending' },
  diffFeed: [],
  pubsub: { seq: 0, folded: 0, lag: 0, producerPhase: 'init', loopStatus: 'pending', lastTopic: null, topicHistory: [] },
  react: { currentPhase: -1, observation: '', thought: '', action: '', stepCount: 0, turnCount: 0 },
  citation: { sources: 0, verified: 0, multi: 0, contras: 0 },
  mermaidNodes: 0, fleet: null,
});

// ── Derived ──────────────────────────────────────────────────────────────
export const canvasCardCount = derived(lightspaceCanvasStore, $s => $s.cards.length);
export const tombstoneCount  = derived(lightspaceCanvasStore, $s => $s.tombstones.length);
export const filesCount      = derived(lightspaceFilesStore,  $s => $s.files.length);

// ── Canvas mutations ─────────────────────────────────────────────────────

/** Add card; evict lowest-priority card first if canvas is full. */
export function canvasAddCard(card: BentoCardData): void {
  lightspaceCanvasStore.update(s => {
    let { cards, tombstones, skeletons } = s;
    while (cards.length >= MAX_CANVAS_CARDS) {
      const victim = selectEvictionVictim(cards);
      if (!victim) break;
      cards = cards.filter(c => c.id !== victim.id);
      const tomb: TombstoneData = {
        id: victim.id, kind: victim.kind, title: victim.title,
        evictedAt: Date.now(), cardSnapshot: victim,
      };
      tombstones = [tomb, ...tombstones].slice(0, 50);
    }
    // Replace first matching skeleton of same kind if present
    const skelIdx = skeletons.findIndex(sk => sk.kind === card.kind);
    if (skelIdx >= 0) skeletons = skeletons.filter((_, i) => i !== skelIdx);
    return { ...s, cards: [...cards, card], tombstones, skeletons };
  });
}

/** Upsert a streaming card (update in place; no-op if not on canvas). */
export function canvasUpsertCard(patch: Partial<BentoCardData> & { id: string }): void {
  lightspaceCanvasStore.update(s => {
    if (!s.cards.find(c => c.id === patch.id)) return s;
    return { ...s, cards: s.cards.map(c => c.id === patch.id ? { ...c, ...patch } : c) };
  });
}

export function canvasAddSkeleton(skel: SkeletonData): void {
  lightspaceCanvasStore.update(s => ({ ...s, skeletons: [...s.skeletons, skel] }));
}

export function canvasRestoreFromTomb(tombId: string): void {
  lightspaceCanvasStore.update(s => {
    const tomb = s.tombstones.find(t => t.id === tombId);
    if (!tomb) return s;
    return {
      ...s,
      cards: [...s.cards, tomb.cardSnapshot],
      tombstones: s.tombstones.filter(t => t.id !== tombId),
    };
  });
}

export function canvasClear(): void {
  lightspaceCanvasStore.set({
    cards: [], skeletons: [], tombstones: [], expandedCardId: null, highlightCardId: null,
  });
}

// ── LASDLC mutations ─────────────────────────────────────────────────────

export function lasdlcUpdateGates(gatesPassed: string[], gatesSkipped: string[]): void {
  lightspaceLasdlcStore.update(s => ({
    ...s,
    gateMatrix: s.gateMatrix.map(g => {
      if (gatesPassed.includes(g.id)) return { ...g, status: 'pass' as const };
      if (gatesSkipped.includes(g.id)) return { ...g, status: 'skip' as const };
      return g;
    }),
  }));
}

// ── Session mutations ────────────────────────────────────────────────────

export function sessionAddConvMessage(msg: ConvMessage): void {
  lightspaceSessionStore.update(s => ({ ...s, conv: [...s.conv, msg] }));
}

export function sessionSetMaterializePhase(phase: MaterializePhase): void {
  lightspaceSessionStore.update(s => ({ ...s, materializePhase: phase }));
}
