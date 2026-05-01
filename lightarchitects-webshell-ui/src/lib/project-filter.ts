import { writable } from 'svelte/store';

const KEY = 'la.currentProject';

function readStorage(): string | null {
  try { return localStorage.getItem(KEY); } catch { return null; }
}

function writeStorage(val: string | null): void {
  try {
    if (val) localStorage.setItem(KEY, val);
    else localStorage.removeItem(KEY);
  } catch { /* storage unavailable */ }
}

function createProjectFilter() {
  const { subscribe, set } = writable<string | null>(readStorage());
  return {
    subscribe,
    select(path: string | null) {
      writeStorage(path);
      set(path);
    },
  };
}

export const selectedProject = createProjectFilter();
