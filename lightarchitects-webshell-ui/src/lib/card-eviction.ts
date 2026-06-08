/**
 * Lightspace card eviction engine — pure TypeScript, no Svelte dependencies.
 * Fully unit-testable in isolation.
 *
 * @integration src/lib/lightspace-types.ts — CardKind, BentoCardData
 * @integration src/lib/lightspace-stores.ts — canvasAddCard calls selectEvictionVictim
 */

import type { BentoCardData, CardKind } from './lightspace-types';

export const MAX_CANVAS_CARDS = 9;

/**
 * Eviction priority per card kind. Higher = evicted first. -1 = never evict (pinned).
 */
export const KIND_EVICT_PRIORITY: Record<CardKind, number> = {
  trace:       0,  // activity stream anchor
  monitor:     1,
  instrument:  1,
  artifact:    2,
  branchlane:  3,
  research:    4,
  thinking:    5,
  agentspawn:  5,  // bumped to 9 when _agentDone=true
  toolcall:    6,
  bash:        6,
  archgallery: 7,
  diff:        8,
};

export function evictPriority(card: BentoCardData): number {
  if (card._pinned) return -1;
  if (card.kind === 'agentspawn' && card._agentDone) return 9;
  return KIND_EVICT_PRIORITY[card.kind] ?? 5;
}

/** Returns the highest-priority eviction victim; null if all pinned or empty. */
export function selectEvictionVictim(cards: BentoCardData[]): BentoCardData | null {
  const candidates = cards
    .map((c, i) => ({ c, i, pri: evictPriority(c) }))
    .filter(x => x.pri >= 0)
    .sort((a, b) => b.pri - a.pri || b.i - a.i);
  return candidates[0]?.c ?? null;
}
