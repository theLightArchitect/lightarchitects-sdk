import { writable } from 'svelte/store';
import type { A2aEnvelopeEvent } from './types';

const MAX_PER_CODENAME = 500;

function createA2aFeedStore() {
  const { subscribe, update } = writable<Map<string, A2aEnvelopeEvent[]>>(new Map());
  return {
    subscribe,
    addEvent(event: A2aEnvelopeEvent) {
      update(m => {
        const next = new Map(m);
        const bucket = next.get(event.codename) ?? [];
        const trimmed =
          bucket.length >= MAX_PER_CODENAME
            ? bucket.slice(bucket.length - MAX_PER_CODENAME + 1)
            : [...bucket];
        trimmed.push(event);
        next.set(event.codename, trimmed);
        return next;
      });
    },
    clearCodename(codename: string) {
      update(m => {
        const next = new Map(m);
        next.delete(codename);
        return next;
      });
    },
  };
}

export const a2aFeedStore = createA2aFeedStore();
