// ============================================================================
// roadmap.ts — Roadmap panel store
//
// Writable state for /api/roadmap fetch lifecycle. The component controls
// init/teardown via onMount; the store is the source of truth for state.
// SSE auto-refresh is wired via the 'la:build-update' DOM event dispatched
// by sse.ts on every build_update event.
//
// Security: raw HTML is NOT stored here — caller must DOMPurify.sanitize()
// before injecting into the DOM. This store holds the raw response text only
// to avoid coupling sanitization logic to the store layer.
// ============================================================================

import { writable } from 'svelte/store';

export type RoadmapStatus = 'idle' | 'loading' | 'success' | 'error' | 'empty';

export interface RoadmapState {
  status: RoadmapStatus;
  /** Raw HTML text from /api/roadmap — DOMPurify.sanitize() before use. */
  rawHtml: string;
  error: string;
  lastUpdated: number | null;
}

function createRoadmapStore() {
  const { subscribe, update } = writable<RoadmapState>({
    status: 'idle',
    rawHtml: '',
    error: '',
    lastUpdated: null,
  });

  async function fetch_roadmap(): Promise<void> {
    update(s => ({ ...s, status: 'loading' }));
    try {
      const res = await fetch('/api/roadmap', { credentials: 'same-origin' });
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
      const text = await res.text();
      if (!text.trim()) {
        update(s => ({ ...s, status: 'empty', rawHtml: '', error: '' }));
      } else {
        update(s => ({
          ...s,
          status: 'success',
          rawHtml: text,
          error: '',
          lastUpdated: Date.now(),
        }));
      }
    } catch (e) {
      update(s => ({
        ...s,
        status: 'error',
        error: e instanceof Error ? e.message : 'Unknown error',
      }));
    }
  }

  return { subscribe, fetch: fetch_roadmap };
}

export const roadmapStore = createRoadmapStore();
