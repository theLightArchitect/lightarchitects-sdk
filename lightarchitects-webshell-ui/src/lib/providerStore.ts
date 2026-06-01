// ============================================================================
// providerStore — centralised LiteLLM provider config state
// ============================================================================
// Single source of truth for the active provider. ProviderPill, ProviderSettings,
// and any component that needs to know the current model subscribe here instead
// of making their own GET /api/litellm/config calls.

import { writable } from 'svelte/store';
import { authHeaders } from '$lib/auth';

export interface ProviderConfig {
  base_url: string;
  model: string;
  has_key: boolean;
  updated_at: string;
}

/** Active provider config. `null` while loading, `undefined` on fetch failure. */
export const providerConfig = writable<ProviderConfig | null | undefined>(null);

/** Load current config from backend and update the store. */
export async function loadProvider(): Promise<void> {
  try {
    const res = await fetch('/api/litellm/config', {
      headers: authHeaders(),
      credentials: 'same-origin',
    });
    if (!res.ok) { providerConfig.set(undefined); return; }
    const data: ProviderConfig = await res.json();
    providerConfig.set(data);
  } catch {
    providerConfig.set(undefined);
  }
}

export interface SaveProviderArgs {
  base_url: string;
  model: string;
  /** Required when configuring for the first time; omit (empty string) to keep existing key. */
  api_key: string;
}

/**
 * Save provider config to the backend (keychain + SQLite + AppState) then
 * refresh the store. Fires `la:litellm-config-saved` for legacy listeners.
 *
 * @throws on HTTP error or network failure — caller handles UI feedback.
 */
export async function saveProvider(args: SaveProviderArgs): Promise<void> {
  const res = await fetch('/api/litellm/config', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json', ...authHeaders() },
    credentials: 'same-origin',
    body: JSON.stringify({
      base_url: args.base_url.trim(),
      api_key: args.api_key,
      model: args.model.trim(),
    }),
  });
  if (res.status !== 204) {
    const text = await res.text().catch(() => `HTTP ${res.status}`);
    throw new Error(text || `HTTP ${res.status}`);
  }
  // Optimistically update the store so all subscribers reflect the change immediately.
  providerConfig.update(prev => ({
    base_url: args.base_url.trim(),
    model: args.model.trim(),
    has_key: args.api_key.trim().length > 0 || (prev?.has_key ?? false),
    updated_at: new Date().toISOString(),
  }));
  // Notify legacy listeners (e.g. ProviderPill if not yet migrated).
  window.dispatchEvent(new CustomEvent('la:litellm-config-saved'));
}
