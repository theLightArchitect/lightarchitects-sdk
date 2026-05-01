import { describe, it, expect, beforeEach, vi } from 'vitest';
import { get } from 'svelte/store';

// Stub localStorage before importing the module so loadOverrides() sees stubs.
const localStorageStore: Record<string, string> = {};
vi.stubGlobal('localStorage', {
  getItem:   (k: string) => localStorageStore[k] ?? null,
  setItem:   (k: string, v: string) => { localStorageStore[k] = v; },
  removeItem:(k: string) => { delete localStorageStore[k]; },
  clear:     () => { Object.keys(localStorageStore).forEach(k => delete localStorageStore[k]); },
});

import {
  hotkeyRegistry,
  hotkeyOverrides,
  registerHotkey,
  setUserOverride,
  clearUserOverride,
  dispatchHotkey,
  groupedEntries,
  chordToMatches,
  eventToChord,
  type HotkeyEntry,
  type KeyChord,
} from '$lib/hotkeyRegistry';

// ── Helpers ────────────────────────────────────────────────────────────────

function makeEntry(overrides: Partial<HotkeyEntry> = {}): HotkeyEntry {
  return {
    id: 'test-hotkey',
    keys: ['⌘', 'K'],
    label: 'Test action',
    group: 'Test',
    scope: 'global',
    matches: (e) => e.metaKey && e.key === 'k',
    handler: vi.fn(),
    ...overrides,
  };
}

function fakeKeyEvent(overrides: Partial<KeyboardEvent> = {}): KeyboardEvent {
  return {
    key: 'k',
    metaKey: false,
    ctrlKey: false,
    altKey: false,
    shiftKey: false,
    preventDefault: vi.fn(),
    ...overrides,
  } as unknown as KeyboardEvent;
}

// ── Reset registry between tests ───────────────────────────────────────────

beforeEach(() => {
  hotkeyRegistry.set([]);
  hotkeyOverrides.set(new Map());
  localStorageStore['la_hotkey_overrides'] && delete localStorageStore['la_hotkey_overrides'];
});

// ── registerHotkey ─────────────────────────────────────────────────────────

describe('registerHotkey', () => {
  it('adds an entry to the registry', () => {
    const entry = makeEntry();
    registerHotkey(entry);
    expect(get(hotkeyRegistry)).toHaveLength(1);
    expect(get(hotkeyRegistry)[0].id).toBe('test-hotkey');
  });

  it('returns an unregister function that removes the entry', () => {
    const entry = makeEntry();
    const unreg = registerHotkey(entry);
    expect(get(hotkeyRegistry)).toHaveLength(1);
    unreg();
    expect(get(hotkeyRegistry)).toHaveLength(0);
  });

  it('de-dupes: re-registering same id replaces in place', () => {
    const entry = makeEntry({ label: 'Original' });
    registerHotkey(entry);
    registerHotkey({ ...entry, label: 'Updated' });
    const reg = get(hotkeyRegistry);
    expect(reg).toHaveLength(1);
    expect(reg[0].label).toBe('Updated');
  });

  it('applies a stored user override at registration time', () => {
    const chord: KeyChord = {
      key: 'p', keys: ['⌘', 'P'],
      metaKey: true, ctrlKey: false, altKey: false, shiftKey: false,
    };
    hotkeyOverrides.set(new Map([['test-hotkey', chord]]));

    const entry = makeEntry({ id: 'test-hotkey', keys: ['⌘', 'K'] });
    registerHotkey(entry);

    const reg = get(hotkeyRegistry);
    expect(reg[0].keys).toEqual(['⌘', 'P']);
  });

  it('preserves original entry so clearUserOverride can restore it', () => {
    const entry = makeEntry({ id: 'revert-test', keys: ['⌘', 'K'] });
    const chord: KeyChord = {
      key: 'p', keys: ['⌘', 'P'],
      metaKey: true, ctrlKey: false, altKey: false, shiftKey: false,
    };
    registerHotkey(entry);
    setUserOverride('revert-test', chord);
    expect(get(hotkeyRegistry)[0].keys).toEqual(['⌘', 'P']);

    clearUserOverride('revert-test');
    expect(get(hotkeyRegistry)[0].keys).toEqual(['⌘', 'K']);
  });
});

// ── setUserOverride ────────────────────────────────────────────────────────

describe('setUserOverride', () => {
  it('updates the live registry entry keys and matches predicate', () => {
    registerHotkey(makeEntry({ id: 'override-target', keys: ['⌘', 'K'] }));

    const chord: KeyChord = {
      key: 'g', keys: ['G'],
      metaKey: false, ctrlKey: false, altKey: false, shiftKey: false,
    };
    setUserOverride('override-target', chord);

    const reg = get(hotkeyRegistry);
    expect(reg[0].keys).toEqual(['G']);
    const e = fakeKeyEvent({ key: 'g' });
    expect(reg[0].matches(e)).toBe(true);
  });

  it('persists the override to localStorage', () => {
    registerHotkey(makeEntry({ id: 'persist-test' }));
    const chord: KeyChord = {
      key: 'z', keys: ['Z'],
      metaKey: false, ctrlKey: false, altKey: false, shiftKey: false,
    };
    setUserOverride('persist-test', chord);

    const stored = JSON.parse(localStorageStore['la_hotkey_overrides'] ?? '{}');
    expect(stored['persist-test']?.key).toBe('z');
  });

  it('updates hotkeyOverrides store', () => {
    registerHotkey(makeEntry({ id: 'store-test' }));
    const chord: KeyChord = {
      key: 'x', keys: ['X'],
      metaKey: false, ctrlKey: false, altKey: false, shiftKey: false,
    };
    setUserOverride('store-test', chord);

    const overrides = get(hotkeyOverrides);
    expect(overrides.get('store-test')?.key).toBe('x');
  });

  it('is a no-op on the registry if the entry is not currently registered', () => {
    const chord: KeyChord = {
      key: 'q', keys: ['Q'],
      metaKey: false, ctrlKey: false, altKey: false, shiftKey: false,
    };
    // Entry not registered — should not throw, registry stays empty
    setUserOverride('ghost-id', chord);
    expect(get(hotkeyRegistry)).toHaveLength(0);
    // Override should still be persisted for when the entry registers later
    expect(get(hotkeyOverrides).get('ghost-id')?.key).toBe('q');
  });
});

// ── clearUserOverride ──────────────────────────────────────────────────────

describe('clearUserOverride', () => {
  it('restores the original keys and matches in place (no remount needed)', () => {
    const entry = makeEntry({ id: 'restore-test', keys: ['⌘', 'K'] });
    registerHotkey(entry);

    const chord: KeyChord = {
      key: 'n', keys: ['N'],
      metaKey: false, ctrlKey: false, altKey: false, shiftKey: false,
    };
    setUserOverride('restore-test', chord);
    expect(get(hotkeyRegistry)[0].keys).toEqual(['N']);

    clearUserOverride('restore-test');
    expect(get(hotkeyRegistry)[0].keys).toEqual(['⌘', 'K']);
  });

  it('does not kill a live hotkey when override is cleared (no missing entry)', () => {
    const entry = makeEntry({ id: 'live-test' });
    registerHotkey(entry);

    const chord: KeyChord = {
      key: 'm', keys: ['M'],
      metaKey: false, ctrlKey: false, altKey: false, shiftKey: false,
    };
    setUserOverride('live-test', chord);
    clearUserOverride('live-test');

    // Entry still present — not removed, not duplicated
    expect(get(hotkeyRegistry)).toHaveLength(1);
    expect(get(hotkeyRegistry)[0].id).toBe('live-test');
  });

  it('removes the override from localStorage', () => {
    registerHotkey(makeEntry({ id: 'ls-clear-test' }));
    const chord: KeyChord = {
      key: 'b', keys: ['B'],
      metaKey: false, ctrlKey: false, altKey: false, shiftKey: false,
    };
    setUserOverride('ls-clear-test', chord);
    expect(localStorageStore['la_hotkey_overrides']).toContain('ls-clear-test');

    clearUserOverride('ls-clear-test');
    const stored = JSON.parse(localStorageStore['la_hotkey_overrides'] ?? '{}');
    expect(stored['ls-clear-test']).toBeUndefined();
  });

  it('removes entry from registry when original was already unmounted', () => {
    // Simulate: entry registered, override set, then component unmounted before clear
    const entry = makeEntry({ id: 'ghost-clear' });
    const unreg = registerHotkey(entry);
    const chord: KeyChord = {
      key: 'v', keys: ['V'],
      metaKey: false, ctrlKey: false, altKey: false, shiftKey: false,
    };
    setUserOverride('ghost-clear', chord);
    unreg(); // unmount — removes from registry but originalEntries cleared too
    expect(get(hotkeyRegistry)).toHaveLength(0);

    clearUserOverride('ghost-clear'); // should not throw; no-op on registry
    expect(get(hotkeyRegistry)).toHaveLength(0);
  });
});

// ── dispatchHotkey ─────────────────────────────────────────────────────────

describe('dispatchHotkey', () => {
  it('calls handler and returns true when a global entry matches', () => {
    const handler = vi.fn();
    registerHotkey(makeEntry({
      id: 'dispatch-test',
      scope: 'global',
      matches: (e) => e.key === '/',
      handler,
    }));
    const e = fakeKeyEvent({ key: '/' });
    const matched = dispatchHotkey(e, '/activity');
    expect(matched).toBe(true);
    expect(handler).toHaveBeenCalledOnce();
  });

  it('calls preventDefault on matched entry', () => {
    const preventDefault = vi.fn();
    registerHotkey(makeEntry({
      id: 'prevent-test',
      scope: 'global',
      matches: (e) => e.key === '/',
      handler: vi.fn(),
    }));
    const e = { ...fakeKeyEvent({ key: '/' }), preventDefault } as unknown as KeyboardEvent;
    dispatchHotkey(e, '/activity');
    expect(preventDefault).toHaveBeenCalled();
  });

  it('returns false when no entry matches', () => {
    registerHotkey(makeEntry({ matches: (e) => e.key === 'q' }));
    const result = dispatchHotkey(fakeKeyEvent({ key: 'z' }), '/activity');
    expect(result).toBe(false);
  });

  it('fires squad-dispatch-scoped entry only on /squad-dispatch route', () => {
    const handler = vi.fn();
    registerHotkey(makeEntry({
      id: 'squad-test',
      scope: 'squad-dispatch',
      matches: (e) => e.key === 'r',
      handler,
    }));
    dispatchHotkey(fakeKeyEvent({ key: 'r' }), '/activity');
    expect(handler).not.toHaveBeenCalled();

    dispatchHotkey(fakeKeyEvent({ key: 'r' }), '/squad-dispatch');
    expect(handler).toHaveBeenCalledOnce();
  });

  it('stops at the first matching entry (no double-fire)', () => {
    const h1 = vi.fn();
    const h2 = vi.fn();
    registerHotkey(makeEntry({ id: 'first', matches: (e) => e.key === 'x', handler: h1 }));
    registerHotkey(makeEntry({ id: 'second', matches: (e) => e.key === 'x', handler: h2 }));

    dispatchHotkey(fakeKeyEvent({ key: 'x' }), '/activity');
    expect(h1).toHaveBeenCalledOnce();
    expect(h2).not.toHaveBeenCalled();
  });
});

// ── groupedEntries ─────────────────────────────────────────────────────────

describe('groupedEntries', () => {
  it('groups entries by their group field', () => {
    const entries: HotkeyEntry[] = [
      makeEntry({ id: 'a', group: 'Navigation', label: 'Go home' }),
      makeEntry({ id: 'b', group: 'Navigation', label: 'Go back' }),
      makeEntry({ id: 'c', group: 'Copilot', label: 'Open chat' }),
    ];
    const groups = groupedEntries(entries);
    expect(groups).toHaveLength(2);
    const nav = groups.find(g => g.title === 'Navigation');
    expect(nav?.rows).toHaveLength(2);
    const cop = groups.find(g => g.title === 'Copilot');
    expect(cop?.rows).toHaveLength(1);
  });

  it('includes id field on each row for rebind UX', () => {
    const entries = [makeEntry({ id: 'row-id-test', group: 'G' })];
    const groups = groupedEntries(entries);
    expect(groups[0].rows[0].id).toBe('row-id-test');
  });

  it('returns empty array for no entries', () => {
    expect(groupedEntries([])).toEqual([]);
  });

  it('preserves group insertion order', () => {
    const entries: HotkeyEntry[] = [
      makeEntry({ id: '1', group: 'Alpha' }),
      makeEntry({ id: '2', group: 'Beta' }),
      makeEntry({ id: '3', group: 'Alpha' }),
    ];
    const groups = groupedEntries(entries);
    expect(groups[0].title).toBe('Alpha');
    expect(groups[1].title).toBe('Beta');
  });
});

// ── chordToMatches ─────────────────────────────────────────────────────────

describe('chordToMatches', () => {
  it('matches when all fields equal the chord', () => {
    const chord: KeyChord = {
      key: 'k', keys: ['⌘', 'K'],
      metaKey: true, ctrlKey: false, altKey: false, shiftKey: false,
    };
    const predicate = chordToMatches(chord);
    const e = fakeKeyEvent({ key: 'k', metaKey: true });
    expect(predicate(e)).toBe(true);
  });

  it('rejects when key differs', () => {
    const chord: KeyChord = {
      key: 'k', keys: ['⌘', 'K'],
      metaKey: true, ctrlKey: false, altKey: false, shiftKey: false,
    };
    const predicate = chordToMatches(chord);
    expect(predicate(fakeKeyEvent({ key: 'p', metaKey: true }))).toBe(false);
  });

  it('rejects when modifier differs', () => {
    const chord: KeyChord = {
      key: 'k', keys: ['⌘', 'K'],
      metaKey: true, ctrlKey: false, altKey: false, shiftKey: false,
    };
    const predicate = chordToMatches(chord);
    expect(predicate(fakeKeyEvent({ key: 'k', metaKey: false }))).toBe(false);
  });
});

// ── eventToChord ──────────────────────────────────────────────────────────

describe('eventToChord', () => {
  it('builds a chord from a plain key event', () => {
    const e = fakeKeyEvent({ key: 'k', metaKey: false, ctrlKey: false, altKey: false, shiftKey: false });
    const chord = eventToChord(e);
    expect(chord.key).toBe('k');
    expect(chord.keys).toEqual(['K']);
    expect(chord.metaKey).toBe(false);
  });

  it('prepends ⌘ for meta key', () => {
    const e = fakeKeyEvent({ key: 'k', metaKey: true });
    const chord = eventToChord(e);
    expect(chord.keys[0]).toBe('⌘');
    expect(chord.keys[1]).toBe('K');
  });

  it('uses readable glyph for Enter', () => {
    const e = fakeKeyEvent({ key: 'Enter' });
    const chord = eventToChord(e);
    expect(chord.keys).toContain('↵');
  });

  it('uses readable glyph for Escape', () => {
    const e = fakeKeyEvent({ key: 'Escape' });
    const chord = eventToChord(e);
    expect(chord.keys).toContain('Esc');
  });

  it('round-trips through chordToMatches', () => {
    const e = fakeKeyEvent({ key: 'k', metaKey: true, shiftKey: true });
    const chord = eventToChord(e);
    const predicate = chordToMatches(chord);
    expect(predicate(e)).toBe(true);
  });
});

// ── Component module smoke tests ───────────────────────────────────────────

describe('DiffPreview — module import', () => {
  it('module exists and default export is defined', async () => {
    const mod = await import('$lib/../components/DiffPreview.svelte');
    expect(mod.default).toBeDefined();
  });
});

describe('SquadDispatch — module import', () => {
  it('module exists and default export is defined', async () => {
    const mod = await import('$lib/../screens/SquadDispatch.svelte');
    expect(mod.default).toBeDefined();
  });
});

describe('KeymapLegend — module import', () => {
  it('module exists and default export is defined', async () => {
    const mod = await import('$lib/../components/KeymapLegend.svelte');
    expect(mod.default).toBeDefined();
  });
});
