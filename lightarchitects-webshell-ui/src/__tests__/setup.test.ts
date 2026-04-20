import { describe, it, expect, beforeEach, vi } from 'vitest';
import { get } from 'svelte/store';
import {
  step, setupComplete, setupLoading, setupError,
  selectedBackend, selectedAgent, selectedModel,
  ollamaBaseUrlInput, authStatus, persistedConfig, availableModels,
  apiKeyInput, settingsOpen,
  loadSetupInfo, loadModels, saveSetup, resetSetup,
} from '$lib/setup';

// Minimal mock helpers
function makeSetupInfo(complete: boolean) {
  return {
    setup_complete: complete,
    auth_status: {
      claude: { has_keychain_auth: true, has_api_key: false, login_method: 'oauth' },
      codex: { has_keychain_auth: false, has_api_key: false, login_method: 'none' },
      ollama: { base_url: 'http://localhost:11434', reachable: true },
    },
    config: complete ? {
      agent: 'lightarchitects',
      backend: 'anthropic',
      model: 'claude-sonnet-4-6',
      ollama_base_url: null,
      api_key_stored: false,
    } : null,
  };
}

function mockFetch(status: number, body: unknown) {
  return vi.fn().mockResolvedValue({
    ok: status >= 200 && status < 300,
    status,
    json: () => Promise.resolve(body),
  });
}

beforeEach(() => {
  // Reset all stores to defaults
  step.set('splash');
  setupComplete.set(false);
  setupLoading.set(false);
  setupError.set(null);
  selectedBackend.set(null);
  selectedAgent.set(null);
  selectedModel.set(null);
  ollamaBaseUrlInput.set('http://localhost:11434');
  authStatus.set(null);
  persistedConfig.set(null);
  availableModels.set([]);
  apiKeyInput.set('');
  settingsOpen.set(false);
  vi.restoreAllMocks();
});

describe('loadSetupInfo', () => {
  it('happy path: not yet complete', async () => {
    vi.stubGlobal('fetch', mockFetch(200, makeSetupInfo(false)));
    await loadSetupInfo();
    expect(get(setupComplete)).toBe(false);
    expect(get(step)).toBe('splash');
    expect(get(authStatus)).not.toBeNull();
    expect(get(setupLoading)).toBe(false);
    expect(get(setupError)).toBeNull();
  });

  it('happy path: setup complete — skips to done and hydrates stores', async () => {
    vi.stubGlobal('fetch', mockFetch(200, makeSetupInfo(true)));
    await loadSetupInfo();
    expect(get(setupComplete)).toBe(true);
    expect(get(step)).toBe('done');
    expect(get(selectedBackend)).toBe('anthropic');
    expect(get(selectedAgent)).toBe('lightarchitects');
    expect(get(selectedModel)).toBe('claude-sonnet-4-6');
    expect(get(persistedConfig)).not.toBeNull();
  });

  it('network error sets setupError and clears loading', async () => {
    vi.stubGlobal('fetch', vi.fn().mockRejectedValue(new Error('Network failure')));
    await loadSetupInfo();
    expect(get(setupError)).toContain('Network failure');
    expect(get(setupLoading)).toBe(false);
    expect(get(setupComplete)).toBe(false);
  });

  it('non-200 response sets setupError', async () => {
    vi.stubGlobal('fetch', mockFetch(500, {}));
    await loadSetupInfo();
    expect(get(setupError)).toContain('500');
    expect(get(setupLoading)).toBe(false);
  });
});

describe('loadModels', () => {
  it('populates availableModels from response', async () => {
    const models = [
      { id: 'claude-sonnet-4-6', label: 'Sonnet 4.6', tier: 'balanced' },
      { id: 'claude-opus-4-7', label: 'Opus 4.7', tier: 'capable' },
    ];
    vi.stubGlobal('fetch', mockFetch(200, { models }));
    await loadModels('anthropic');
    expect(get(availableModels)).toHaveLength(2);
    expect(get(availableModels)[0].id).toBe('claude-sonnet-4-6');
  });

  it('forwards base_url param for ollama', async () => {
    const spy = mockFetch(200, { models: [] });
    vi.stubGlobal('fetch', spy);
    await loadModels('ollama-launch', 'http://myhost:11434');
    const url: string = spy.mock.calls[0][0];
    expect(url).toContain('base_url=http');
    expect(url).toContain('backend=ollama-launch');
  });

  it('404 response sets error', async () => {
    vi.stubGlobal('fetch', mockFetch(404, {}));
    await loadModels('anthropic');
    expect(get(setupError)).toContain('404');
  });
});

describe('saveSetup', () => {
  it('happy path: sets step to init and clears api key', async () => {
    selectedBackend.set('anthropic');
    selectedAgent.set('lightarchitects');
    selectedModel.set('claude-sonnet-4-6');
    apiKeyInput.set('sk-test-key');
    vi.stubGlobal('fetch', mockFetch(200, { ok: true }));
    await saveSetup();
    expect(get(setupComplete)).toBe(true);
    expect(get(step)).toBe('init');
    expect(get(apiKeyInput)).toBe('');
    expect(get(setupLoading)).toBe(false);
  });

  it('includes Authorization header', async () => {
    selectedBackend.set('anthropic');
    selectedAgent.set('lightarchitects');
    const spy = mockFetch(200, { ok: true });
    vi.stubGlobal('fetch', spy);
    await saveSetup();
    const init = spy.mock.calls[0][1];
    expect(init.headers['Authorization']).toMatch(/^Bearer /);
  });

  it('401 sets error, does not advance step', async () => {
    selectedBackend.set('anthropic');
    selectedAgent.set('lightarchitects');
    vi.stubGlobal('fetch', mockFetch(401, {}));
    await saveSetup();
    expect(get(setupError)).toContain('401');
    expect(get(step)).toBe('splash');
  });

  it('no-op when backend not selected', async () => {
    const spy = vi.fn();
    vi.stubGlobal('fetch', spy);
    await saveSetup();
    expect(spy).not.toHaveBeenCalled();
  });
});

describe('resetSetup', () => {
  it('clears all state and returns to splash', async () => {
    setupComplete.set(true);
    step.set('done');
    selectedBackend.set('anthropic');
    persistedConfig.set({ agent: 'lightarchitects', backend: 'anthropic', model: null, ollama_base_url: null, api_key_stored: false });
    settingsOpen.set(true);
    vi.stubGlobal('fetch', mockFetch(200, null));
    await resetSetup();
    expect(get(setupComplete)).toBe(false);
    expect(get(step)).toBe('splash');
    expect(get(selectedBackend)).toBeNull();
    expect(get(persistedConfig)).toBeNull();
    expect(get(settingsOpen)).toBe(false);
  });

  it('non-200 sets error', async () => {
    vi.stubGlobal('fetch', mockFetch(403, {}));
    await resetSetup();
    expect(get(setupError)).toContain('403');
    expect(get(step)).toBe('splash');
  });
});
