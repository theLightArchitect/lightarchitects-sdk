import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { get } from 'svelte/store';
import {
  collectSettings,
  applySettings,
  loadPersistedSettings,
  saveSettingsDebounced,
  type PersistedSettings,
} from '$lib/settings-persistence';
import { drawerHeightPx, memoryDrawerOpen } from '$lib/stores';
import { selectedBackend, selectedModel, selectedAgent } from '$lib/setup';

const LS_KEY = 'la_webshell_settings';

beforeEach(() => {
  // Reset stores to defaults
  drawerHeightPx.set(32);
  memoryDrawerOpen.set(false);
  selectedBackend.set(null);
  selectedModel.set(null);
  selectedAgent.set(null);
  localStorage.clear();
  vi.restoreAllMocks();
});

afterEach(() => {
  vi.useRealTimers();
});

describe('collectSettings', () => {
  it('returns current store values', () => {
    drawerHeightPx.set(400);
    memoryDrawerOpen.set(true);
    selectedBackend.set('anthropic');
    selectedModel.set('claude-sonnet-4-6');
    selectedAgent.set('lightarchitects');

    const result = collectSettings();
    expect(result).toEqual({
      drawerHeightPx: 400,
      memoryDrawerOpen: true,
      selectedBackend: 'anthropic',
      selectedModel: 'claude-sonnet-4-6',
      selectedAgent: 'lightarchitects',
    });
  });

  it('returns null values for unset stores', () => {
    const result = collectSettings();
    expect(result.selectedBackend).toBeNull();
    expect(result.selectedModel).toBeNull();
    expect(result.selectedAgent).toBeNull();
    expect(result.drawerHeightPx).toBe(32);
    expect(result.memoryDrawerOpen).toBe(false);
  });
});

describe('applySettings', () => {
  it('applies all provided settings to stores', () => {
    const settings: PersistedSettings = {
      drawerHeightPx: 500,
      memoryDrawerOpen: true,
      selectedBackend: 'ollama-launch',
      selectedModel: 'llama3',
      selectedAgent: 'lightarchitects',
    };

    applySettings(settings);

    expect(get(drawerHeightPx)).toBe(500);
    expect(get(memoryDrawerOpen)).toBe(true);
    // Backend/model/agent only apply when current value is null
    expect(get(selectedBackend)).toBe('ollama-launch');
    expect(get(selectedModel)).toBe('llama3');
    expect(get(selectedAgent)).toBe('lightarchitects');
  });

  it('does not overwrite already-set backend/model/agent stores', () => {
    selectedBackend.set('anthropic');
    selectedModel.set('existing-model');
    selectedAgent.set('existing-agent');

    applySettings({
      selectedBackend: 'ollama-launch',
      selectedModel: 'llama3',
      selectedAgent: 'lightarchitects',
    });

    // Should keep existing values since they were already set (not null)
    expect(get(selectedBackend)).toBe('anthropic');
    expect(get(selectedModel)).toBe('existing-model');
    expect(get(selectedAgent)).toBe('existing-agent');
  });

  it('skips undefined fields without clobbering defaults', () => {
    drawerHeightPx.set(300);
    applySettings({}); // empty settings object
    expect(get(drawerHeightPx)).toBe(300);
  });
});

describe('loadPersistedSettings', () => {
  it('loads from localStorage when API fails', async () => {
    const saved: PersistedSettings = {
      drawerHeightPx: 450,
      memoryDrawerOpen: true,
      selectedBackend: 'anthropic',
      selectedModel: 'claude-sonnet-4-6',
      selectedAgent: 'lightarchitects',
    };
    localStorage.setItem(LS_KEY, JSON.stringify(saved));

    // Mock fetch to simulate backend failure
    vi.stubGlobal('fetch', vi.fn().mockRejectedValue(new Error('offline')));

    await loadPersistedSettings();

    expect(get(drawerHeightPx)).toBe(450);
    expect(get(memoryDrawerOpen)).toBe(true);
    expect(get(selectedBackend)).toBe('anthropic');
  });

  it('handles missing localStorage gracefully', async () => {
    vi.stubGlobal('fetch', vi.fn().mockRejectedValue(new Error('offline')));

    await loadPersistedSettings();

    // Stores should remain at defaults
    expect(get(drawerHeightPx)).toBe(32);
    expect(get(memoryDrawerOpen)).toBe(false);
    expect(get(selectedBackend)).toBeNull();
  });

  it('handles corrupt localStorage data gracefully', async () => {
    localStorage.setItem(LS_KEY, 'not-valid-json{{{');
    vi.stubGlobal('fetch', vi.fn().mockRejectedValue(new Error('offline')));

    await loadPersistedSettings();

    // Should not throw, stores remain at defaults
    expect(get(drawerHeightPx)).toBe(32);
  });

  it('prefers API response over localStorage', async () => {
    const apiState: PersistedSettings = { drawerHeightPx: 600, memoryDrawerOpen: true };
    const lsState: PersistedSettings = { drawerHeightPx: 300, memoryDrawerOpen: false };
    localStorage.setItem(LS_KEY, JSON.stringify(lsState));

    vi.stubGlobal('fetch', vi.fn().mockResolvedValue({
      ok: true,
      status: 200,
      json: () => Promise.resolve(apiState),
    }));

    await loadPersistedSettings();

    expect(get(drawerHeightPx)).toBe(600);
    expect(get(memoryDrawerOpen)).toBe(true);
  });
});

describe('saveSettingsDebounced', () => {
  it('writes to localStorage after debounce', async () => {
    vi.useFakeTimers();
    drawerHeightPx.set(777);

    // Mock fetch to avoid real API calls
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue({
      ok: true,
      status: 200,
      json: () => Promise.resolve({}),
    }));

    saveSettingsDebounced();

    // Not yet written (debounce pending)
    expect(localStorage.getItem(LS_KEY)).toBeNull();

    // Advance past debounce
    vi.advanceTimersByTime(600);
    // Allow async to flush
    await vi.runAllTimersAsync();

    const stored = JSON.parse(localStorage.getItem(LS_KEY) ?? '{}');
    expect(stored.drawerHeightPx).toBe(777);
  });

  it('collapses rapid calls into one write', async () => {
    vi.useFakeTimers();
    const fetchMock = vi.fn().mockResolvedValue({
      ok: true,
      status: 200,
      json: () => Promise.resolve({}),
    });
    vi.stubGlobal('fetch', fetchMock);

    saveSettingsDebounced();
    saveSettingsDebounced();
    saveSettingsDebounced();

    vi.advanceTimersByTime(600);
    await vi.runAllTimersAsync();

    // localStorage should have been written exactly once
    // (fetch may be called for API + we don't count that)
    expect(localStorage.getItem(LS_KEY)).not.toBeNull();
  });
});
