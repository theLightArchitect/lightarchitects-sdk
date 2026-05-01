// ============================================================================
// Global hotkey registry (#102 HOTKEY)
// ============================================================================
//
// Centralises ALL keyboard shortcuts in one reactive store so:
//   1. KeymapLegend always shows the live, accurate list
//   2. Scope isolation (global vs screen-specific) is enforced in one place
//   3. Components register/deregister cleanly with Svelte lifecycle
//   4. Users can rebind any shortcut — overrides persist in localStorage
//
// Usage — in a Svelte component:
//
//   import { useHotkey } from '$lib/hotkeyRegistry';
//
//   // Svelte action (auto-deregisters on component destroy):
//   <div use:useHotkey={{
//     id: 'squad-reset',
//     keys: ['R'],
//     label: 'Reset dispatch',
//     group: 'Squad Dispatch',
//     scope: 'squad-dispatch',
//     matches: e => !e.metaKey && !e.ctrlKey && !e.altKey && e.key === 'r',
//     handler: () => reset(),
//   }}></div>
//
//   // Or imperative:
//   const unreg = registerHotkey({ ... });
//   onDestroy(unreg);

import { writable, get } from 'svelte/store';

// ── Types ──────────────────────────────────────────────────────────────────

export type HotkeyScope =
  | 'global'         // fires on any screen
  | 'squad-dispatch'; // fires only when /squad-dispatch is active

export interface HotkeyEntry {
  /** Unique stable ID — duplicate registrations with the same ID are silently de-duped. */
  id: string;
  /** Display key sequence for KeymapLegend, e.g. ['⌘', 'K'] or ['R']. */
  keys: string[];
  /** Human-readable description shown in legend. */
  label: string;
  /** Legend group heading. */
  group: string;
  /** Where this shortcut fires. */
  scope: HotkeyScope;
  /** Predicate — return true when the KeyboardEvent matches this binding. */
  matches: (e: KeyboardEvent) => boolean;
  /** Callback — called when matches() returns true and scope is active. */
  handler: (e: KeyboardEvent) => void;
}

// ── User-override types ────────────────────────────────────────────────────

/**
 * Serialisable description of a key chord — stored in localStorage so the
 * matches() predicate can be reconstructed after page reload.
 */
export interface KeyChord {
  /** e.key value — e.g. "r", "Enter", "k", "/" */
  key: string;
  /** Human-readable display sequence — e.g. ['⌘', 'K'] */
  keys: string[];
  metaKey:  boolean;
  ctrlKey:  boolean;
  altKey:   boolean;
  shiftKey: boolean;
}

const STORAGE_KEY = 'la_hotkey_overrides';

/** Load all user overrides from localStorage. */
function loadOverrides(): Map<string, KeyChord> {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return new Map();
    const parsed = JSON.parse(raw) as Record<string, KeyChord>;
    return new Map(Object.entries(parsed));
  } catch {
    return new Map();
  }
}

/** Persist overrides map back to localStorage. */
function saveOverrides(map: Map<string, KeyChord>): void {
  try {
    const obj: Record<string, KeyChord> = {};
    map.forEach((v, k) => { obj[k] = v; });
    localStorage.setItem(STORAGE_KEY, JSON.stringify(obj));
  } catch {
    // localStorage unavailable in SSR / private browsing — silently skip
  }
}

/** Build a matches() predicate from a serialised KeyChord. */
export function chordToMatches(chord: KeyChord): (e: KeyboardEvent) => boolean {
  return (e: KeyboardEvent) =>
    e.key === chord.key &&
    e.metaKey  === chord.metaKey &&
    e.ctrlKey  === chord.ctrlKey &&
    e.altKey   === chord.altKey &&
    e.shiftKey === chord.shiftKey;
}

/** Build a human-readable key sequence from a native KeyboardEvent. */
export function eventToChord(e: KeyboardEvent): KeyChord {
  const keys: string[] = [];
  if (e.metaKey)  keys.push('⌘');
  if (e.ctrlKey)  keys.push('⌃');
  if (e.altKey)   keys.push('⌥');
  if (e.shiftKey) keys.push('⇧');
  // Append the base key, using readable glyphs for special keys
  const readable: Record<string, string> = {
    Enter: '↵', Escape: 'Esc', Backspace: '⌫', Tab: '⇥', ' ': 'Space',
    ArrowUp: '↑', ArrowDown: '↓', ArrowLeft: '←', ArrowRight: '→',
  };
  keys.push(readable[e.key] ?? e.key.toUpperCase());
  return {
    key: e.key,
    keys,
    metaKey:  e.metaKey,
    ctrlKey:  e.ctrlKey,
    altKey:   e.altKey,
    shiftKey: e.shiftKey,
  };
}

// ── Stores ─────────────────────────────────────────────────────────────────

export const hotkeyRegistry = writable<HotkeyEntry[]>([]);

/**
 * User overrides store — maps entry.id → KeyChord.
 * Components read this to show the current (possibly overridden) key display.
 */
export const hotkeyOverrides = writable<Map<string, KeyChord>>(loadOverrides());

// Side map: id → original (pre-override) entry, so clearUserOverride can
// restore the default without requiring a component remount.
const originalEntries = new Map<string, HotkeyEntry>();

// ── Register / unregister ──────────────────────────────────────────────────

/** Register a hotkey. Returns an unregister function. */
export function registerHotkey(entry: HotkeyEntry): () => void {
  // Stash the original (pre-override) entry so clearUserOverride can restore.
  originalEntries.set(entry.id, entry);

  // Apply any stored user override to the matches predicate.
  const overrides = get(hotkeyOverrides);
  const override = overrides.get(entry.id);
  const resolved: HotkeyEntry = override
    ? { ...entry, keys: override.keys, matches: chordToMatches(override) }
    : entry;

  hotkeyRegistry.update(r => {
    // De-dupe: if the same id is re-registered (e.g. HMR), replace in place.
    const existing = r.findIndex(e => e.id === resolved.id);
    if (existing >= 0) {
      const copy = [...r];
      copy[existing] = resolved;
      return copy;
    }
    return [...r, resolved];
  });
  return () => {
    originalEntries.delete(entry.id);
    hotkeyRegistry.update(r => r.filter(e => e.id !== resolved.id));
  };
}

/** Svelte action — registers on node mount, deregisters on node destroy. */
export function useHotkey(
  _node: Element,
  entry: HotkeyEntry,
): { update: (e: HotkeyEntry) => void; destroy: () => void } {
  let unregister = registerHotkey(entry);
  return {
    update(newEntry: HotkeyEntry) {
      unregister();
      unregister = registerHotkey(newEntry);
    },
    destroy() {
      unregister();
    },
  };
}

// ── User rebind ────────────────────────────────────────────────────────────

/**
 * Persist a user-chosen key chord for a given hotkey id, then re-apply it
 * to any currently-registered entry with that id.
 */
export function setUserOverride(id: string, chord: KeyChord): void {
  const overrides = get(hotkeyOverrides);
  const next = new Map(overrides);
  next.set(id, chord);
  saveOverrides(next);
  hotkeyOverrides.set(next);

  // Patch the live registry entry if it is currently registered.
  hotkeyRegistry.update(r =>
    r.map(e =>
      e.id === id
        ? { ...e, keys: chord.keys, matches: chordToMatches(chord) }
        : e,
    ),
  );
}

/**
 * Remove a user override for an entry id, reverting to the default binding.
 * The entry must re-register itself for the revert to take full effect — this
 * handles the display side by clearing the override and removing from live registry,
 * prompting the component's next `registerHotkey` call to use the original.
 */
export function clearUserOverride(id: string): void {
  const overrides = get(hotkeyOverrides);
  const next = new Map(overrides);
  next.delete(id);
  saveOverrides(next);
  hotkeyOverrides.set(next);

  // Restore the original entry in the live registry so the hotkey remains
  // active without requiring a component remount.
  const original = originalEntries.get(id);
  if (original) {
    hotkeyRegistry.update(r =>
      r.map(e => e.id === id ? original : e),
    );
  } else {
    // Component already unmounted — entry is gone, nothing to restore.
    hotkeyRegistry.update(r => r.filter(e => e.id !== id));
  }
}

// ── Dispatch helper ────────────────────────────────────────────────────────

/**
 * Call from a single global `window.addEventListener('keydown', ...)`.
 *
 * Iterates registry entries whose scope is active, checks the predicate,
 * and calls the handler on the first match. Stops at the first match.
 *
 * @param e     The native KeyboardEvent.
 * @param route The current SPA hash route (e.g. '/squad-dispatch').
 */
export function dispatchHotkey(e: KeyboardEvent, route: string): boolean {
  const registry = get(hotkeyRegistry);
  const isDispatch = route === '/squad-dispatch';

  for (const entry of registry) {
    if (entry.scope === 'squad-dispatch' && !isDispatch) continue;
    if (entry.matches(e)) {
      e.preventDefault();
      entry.handler(e);
      return true;
    }
  }
  return false;
}

// ── Legend helpers ─────────────────────────────────────────────────────────

/** Group registry entries by their group label for KeymapLegend rendering. */
export function groupedEntries(
  entries: HotkeyEntry[],
): { title: string; rows: { keys: string[]; label: string; id: string }[] }[] {
  const map = new Map<string, { keys: string[]; label: string; id: string }[]>();
  for (const e of entries) {
    if (!map.has(e.group)) map.set(e.group, []);
    map.get(e.group)!.push({ keys: e.keys, label: e.label, id: e.id });
  }
  return Array.from(map.entries()).map(([title, rows]) => ({ title, rows }));
}
