// ============================================================================
// Settings persistence — saves UI preferences to browser-state API (with
// localStorage fallback) and restores them on app startup.
// ============================================================================

import { get } from 'svelte/store';
import { api } from './api';
import { drawerHeightPx, memoryDrawerOpen, activeSkin } from './stores';
import { selectedBackend, selectedModel, selectedAgent } from './setup';
import { DEFAULT_SKIN, type HelixSkin } from '$lib/helix-skin';

/** localStorage key used when the backend API is unreachable. */
const LS_KEY = 'la_webshell_settings';

/** Shape of persisted UI settings. Never includes tokens or API keys. */
export interface PersistedSettings {
  drawerHeightPx?: number;
  memoryDrawerOpen?: boolean;
  selectedBackend?: string | null;
  selectedModel?: string | null;
  selectedAgent?: string | null;
  activeSkin?: HelixSkin;
}

// --- Debounce timer handle ---
let debounceTimer: ReturnType<typeof setTimeout> | null = null;
const DEBOUNCE_MS = 500;

/** Collect current UI state into a serializable object. */
export function collectSettings(): PersistedSettings {
  return {
    drawerHeightPx: get(drawerHeightPx),
    memoryDrawerOpen: get(memoryDrawerOpen),
    selectedBackend: get(selectedBackend),
    selectedModel: get(selectedModel),
    selectedAgent: get(selectedAgent),
    activeSkin: get(activeSkin),
  };
}

/**
 * Persist the current settings. Tries the backend API first; on failure
 * falls back to localStorage so preferences survive even when the server
 * is down.
 */
async function persistSettings(settings: PersistedSettings): Promise<void> {
  // Always write to localStorage as a baseline
  try {
    localStorage.setItem(LS_KEY, JSON.stringify(settings));
  } catch {
    // Storage quota exceeded or private browsing — silently ignore
  }

  // Attempt backend persistence
  try {
    await api.postBrowserState(settings);
  } catch {
    // Backend unreachable — localStorage fallback is already in place
  }
}

/**
 * Save settings with debounce. Call this whenever a setting changes.
 * Multiple rapid calls within `DEBOUNCE_MS` are collapsed into one write.
 */
export function saveSettingsDebounced(): void {
  if (debounceTimer !== null) {
    clearTimeout(debounceTimer);
  }
  debounceTimer = setTimeout(() => {
    debounceTimer = null;
    const settings = collectSettings();
    void persistSettings(settings);
  }, DEBOUNCE_MS);
}

/**
 * Apply loaded settings to the corresponding Svelte stores.
 * Only applies values that are present (not undefined) to avoid
 * clobbering defaults with stale or missing data.
 */
export function applySettings(settings: PersistedSettings): void {
  // Runtime type guards — localStorage data may come from an older build
  // with differently-typed fields.
  if (typeof settings.drawerHeightPx === 'number' && Number.isFinite(settings.drawerHeightPx)) {
    drawerHeightPx.set(settings.drawerHeightPx);
  }
  if (typeof settings.memoryDrawerOpen === 'boolean') {
    memoryDrawerOpen.set(settings.memoryDrawerOpen);
  }
  // Backend/model/agent are only applied if the setup module hasn't already
  // hydrated them from the server config (which takes priority).
  if (typeof settings.selectedBackend === 'string' && settings.selectedBackend && get(selectedBackend) === null) {
    selectedBackend.set(settings.selectedBackend);
  }
  if (typeof settings.selectedModel === 'string' && get(selectedModel) === null) {
    selectedModel.set(settings.selectedModel);
  }
  if (typeof settings.selectedAgent === 'string' && settings.selectedAgent && get(selectedAgent) === null) {
    selectedAgent.set(settings.selectedAgent);
  }
  // Restore helix skin — merge with DEFAULT_SKIN to fill any missing fields
  // added in newer versions of the schema.
  if (settings.activeSkin && typeof settings.activeSkin === 'object' && settings.activeSkin.version === 1) {
    activeSkin.set({
      ...DEFAULT_SKIN,
      ...settings.activeSkin,
      colors: { ...DEFAULT_SKIN.colors, ...(settings.activeSkin.colors ?? {}) },
      glow: { ...DEFAULT_SKIN.glow, ...(settings.activeSkin.glow ?? {}) },
      atmosphere: { ...DEFAULT_SKIN.atmosphere, ...(settings.activeSkin.atmosphere ?? {}) },
      rails: { ...DEFAULT_SKIN.rails, ...(settings.activeSkin.rails ?? {}) },
    });
  }
}

/**
 * Load persisted settings on startup. Tries the backend API first; falls
 * back to localStorage if the API is unavailable.
 */
export async function loadPersistedSettings(): Promise<void> {
  let settings: PersistedSettings | null = null;

  // Try backend API first
  try {
    const state = await api.getBrowserState() as PersistedSettings | null;
    if (state && typeof state === 'object') {
      settings = state;
    }
  } catch {
    // Backend unreachable — fall through to localStorage
  }

  // Fall back to localStorage
  if (!settings) {
    try {
      const raw = localStorage.getItem(LS_KEY);
      if (raw) {
        settings = JSON.parse(raw) as PersistedSettings;
      }
    } catch {
      // Corrupt or missing — start fresh
    }
  }

  if (settings) {
    applySettings(settings);
  }
}
