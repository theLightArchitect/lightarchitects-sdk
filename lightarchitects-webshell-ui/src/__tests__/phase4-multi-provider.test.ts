import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { get } from 'svelte/store';
import {
  loadModels,
  saveSetup,
  selectedBackend,
  selectedAgent,
  selectedModel,
  availableModels,
  setupError,
  setupLoading,
  ollamaBaseUrlInput,
  apiKeyInput,
  type ModelOption,
} from '$lib/setup';

beforeEach(() => {
  selectedBackend.set(null);
  selectedAgent.set(null);
  selectedModel.set(null);
  availableModels.set([]);
  setupError.set(null);
  setupLoading.set(false);
  ollamaBaseUrlInput.set('http://localhost:11434');
  apiKeyInput.set('');
  vi.restoreAllMocks();
});

afterEach(() => {
  vi.restoreAllMocks();
});

// --- loadModels ---

describe('loadModels — ollama-cloud', () => {
  it('fetches with ollama-cloud backend param and populates availableModels', async () => {
    const mockModels: ModelOption[] = [
      { id: 'glm4-9b', label: 'GLM-4-9B', tier: 'fast', family: 'GLM' },
      { id: 'deepseek-v3', label: 'DeepSeek V3', tier: 'capable', family: 'DeepSeek' },
    ];
    const fetchMock = vi.fn().mockResolvedValue({
      ok: true,
      status: 200,
      json: () => Promise.resolve({ models: mockModels }),
    });
    vi.stubGlobal('fetch', fetchMock);

    await loadModels('ollama-cloud');

    const calledUrl = fetchMock.mock.calls[0][0] as string;
    expect(calledUrl).toContain('backend=ollama-cloud');
    expect(calledUrl).toContain('/api/setup/models');
    expect(get(availableModels)).toHaveLength(2);
    expect(get(availableModels)[0].id).toBe('glm4-9b');
  });

  it('passes base_url param when provided', async () => {
    const fetchMock = vi.fn().mockResolvedValue({
      ok: true,
      status: 200,
      json: () => Promise.resolve({ models: [] }),
    });
    vi.stubGlobal('fetch', fetchMock);

    await loadModels('ollama-cloud', 'http://my-cloud:11434');

    const calledUrl = fetchMock.mock.calls[0][0] as string;
    expect(calledUrl).toContain('base_url=http%3A%2F%2Fmy-cloud%3A11434');
  });
});

describe('loadModels — openrouter', () => {
  it('fetches with openrouter backend param and populates availableModels', async () => {
    const mockModels: ModelOption[] = [
      { id: 'openai/gpt-4o', label: 'GPT-4o', tier: 'flagship' },
    ];
    const fetchMock = vi.fn().mockResolvedValue({
      ok: true,
      status: 200,
      json: () => Promise.resolve({ models: mockModels }),
    });
    vi.stubGlobal('fetch', fetchMock);

    await loadModels('openrouter');

    const calledUrl = fetchMock.mock.calls[0][0] as string;
    expect(calledUrl).toContain('backend=openrouter');
    expect(get(availableModels)[0].id).toBe('openai/gpt-4o');
  });
});

describe('loadModels — error handling', () => {
  it('sets setupError when the backend returns a non-ok status', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue({
      ok: false,
      status: 503,
    }));

    await loadModels('ollama-cloud');

    expect(get(setupError)).toMatch(/503/);
    expect(get(availableModels)).toHaveLength(0);
  });
});

// --- saveSetup ---

describe('saveSetup — body shape', () => {
  it('includes ollama_base_url for ollama-cloud backend', async () => {
    selectedBackend.set('ollama-cloud');
    selectedAgent.set('lightarchitects');
    selectedModel.set('glm4-9b');
    ollamaBaseUrlInput.set('http://cloud.ollama.test:11434');

    const fetchMock = vi.fn().mockResolvedValue({
      ok: true,
      status: 200,
      json: () => Promise.resolve({}),
    });
    vi.stubGlobal('fetch', fetchMock);

    await saveSetup();

    const body = JSON.parse(fetchMock.mock.calls[0][1].body as string);
    expect(body.backend).toBe('ollama-cloud');
    expect(body.ollama_base_url).toBe('http://cloud.ollama.test:11434');
    expect(body.model).toBe('glm4-9b');
  });

  it('sends null ollama_base_url for openrouter backend', async () => {
    selectedBackend.set('openrouter');
    selectedAgent.set('lightarchitects');
    selectedModel.set('openai/gpt-4o');

    const fetchMock = vi.fn().mockResolvedValue({
      ok: true,
      status: 200,
      json: () => Promise.resolve({}),
    });
    vi.stubGlobal('fetch', fetchMock);

    await saveSetup();

    const body = JSON.parse(fetchMock.mock.calls[0][1].body as string);
    expect(body.backend).toBe('openrouter');
    expect(body.ollama_base_url).toBeNull();
  });
});

// --- selectedModel store ---

describe('selectedModel — store wiring', () => {
  it('selectedModel.set updates get(selectedModel) immediately', () => {
    selectedModel.set('qwen2.5-72b');
    expect(get(selectedModel)).toBe('qwen2.5-72b');
  });
});
