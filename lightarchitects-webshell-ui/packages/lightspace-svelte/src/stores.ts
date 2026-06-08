// Map-based Svelte stores for per-session Lightspace canvas state.
//
// WHY Map not Array: concurrent SSE-keyed mutations need O(1) get/set by card ID.
// writable<Map<...>> requires new Map(prev) before any mutation to trigger
// subscribers — in-place Map.set() does not signal Svelte's store contract.

import { writable } from 'svelte/store';
import type {
  CardData, DrawerFileData, MaterializePhase, UpdateMode, HitlItem,
  CanvasSnapshot,
} from './types';

export const canvasStore   = writable<Map<string, CardData>>(new Map());
export const drawerStore   = writable<Map<string, DrawerFileData>>(new Map());
export const hitlStore     = writable<Map<string, HitlItem>>(new Map());
export const materializePhase = writable<MaterializePhase>('idle');

// ── Canvas mutations ──────────────────────────────────────────────────────────

export function canvasAttachCard(card: CardData): void {
  canvasStore.update(m => { const n = new Map(m); n.set(card.id, card); return n; });
}

export function canvasDetachCard(cardId: string): void {
  canvasStore.update(m => { const n = new Map(m); n.delete(cardId); return n; });
}

export function canvasUpdateCard(
  cardId: string,
  seq: number,
  mode: UpdateMode,
  path: string | undefined,
  payload: unknown,
): void {
  canvasStore.update(m => {
    const card = m.get(cardId);
    if (!card) return m;
    const n = new Map(m);
    let content: unknown = card.content;
    if (mode === 'replace') {
      content = path ? applyReplace(content, path, payload) : payload;
    } else if (mode === 'append' && path) {
      content = applyAppend(content, path, payload);
    }
    n.set(cardId, { ...card, content });
    return n;
  });
}

export function canvasReset(snapshot: CanvasSnapshot): void {
  const cards = new Map<string, CardData>(Object.entries(snapshot.cards));
  const files = new Map<string, DrawerFileData>(Object.entries(snapshot.drawer_files));
  canvasStore.set(cards);
  drawerStore.set(files);
  hitlStore.set(new Map());
  const ph = snapshot.materialize_phase;
  if (ph !== null && ph !== undefined) {
    materializePhase.set(ph >= 255 ? 'complete' : 'canvas');
  }
}

// ── Drawer mutations ──────────────────────────────────────────────────────────

export function drawerAttachFile(file: DrawerFileData): void {
  drawerStore.update(m => { const n = new Map(m); n.set(file.id, file); return n; });
}

export function drawerDetachFile(fileId: string): void {
  drawerStore.update(m => { const n = new Map(m); n.delete(fileId); return n; });
}

// ── HITL mutations ────────────────────────────────────────────────────────────

export function hitlEnqueue(item: HitlItem): void {
  hitlStore.update(m => { const n = new Map(m); n.set(item.id, item); return n; });
}

export function hitlDequeue(id: string): void {
  hitlStore.update(m => { const n = new Map(m); n.delete(id); return n; });
}

// ── RFC 6901 path helpers (subset) ───────────────────────────────────────────

function applyReplace(content: unknown, path: string, value: unknown): unknown {
  const segments = path.replace(/^\//, '').split('/');
  return setNestedPath(content, segments, value);
}

function applyAppend(content: unknown, path: string, value: unknown): unknown {
  const segments = path.replace(/^\//, '').split('/');
  const arr = getNestedPath(content, segments);
  if (Array.isArray(arr)) {
    return setNestedPath(content, segments, [...arr, value]);
  }
  return content;
}

function getNestedPath(obj: unknown, segments: string[]): unknown {
  let cur: unknown = obj;
  for (const seg of segments) {
    if (!cur || typeof cur !== 'object') return undefined;
    cur = (cur as Record<string, unknown>)[seg];
  }
  return cur;
}

function setNestedPath(obj: unknown, segments: string[], value: unknown): unknown {
  if (segments.length === 0) return value;
  const [head, ...tail] = segments;
  const o = (obj && typeof obj === 'object' ? obj : {}) as Record<string, unknown>;
  return { ...o, [head]: setNestedPath(o[head], tail, value) };
}
