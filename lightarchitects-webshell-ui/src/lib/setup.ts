import { writable, get } from 'svelte/store';
import { resolveToken } from './auth';
import { authProfile, ollamaConfig } from './stores';
import type { AuthProfile } from './types';

export type SetupStep = 'splash' | 'source' | 'provider' | 'auth' | 'model' | 'init' | 'done';
export type SetupTier = 'local' | 'byok' | 'la-platform';

export interface ClaudeAuthStatus {
  has_keychain_auth: boolean;
  has_api_key: boolean;
  login_method: string;
  login_source?: string;
}

export interface CodexAuthStatus {
  has_keychain_auth: boolean;
  has_api_key: boolean;
  login_method: string;
  login_source?: string;
}

export interface OllamaAuthStatus {
  base_url: string;
  reachable: boolean;
}

export interface MistralAuthStatus {
  has_api_key: boolean;
  login_source?: string;
}

export interface OpenRouterAuthStatus {
  has_api_key: boolean;
  login_source?: string;
}

export interface OllamaCloudAuthStatus {
  has_api_key: boolean;
  login_source?: string;
}

export interface DeepSeekAuthStatus {
  has_api_key: boolean;
  login_source?: string;
}

export interface GoogleVertexAuthStatus {
  has_service_account: boolean;
  project_id?: string;
}

/** BYOK provider selected in ProviderStep. */
export type Provider =
  | 'anthropic'
  | 'openai'
  | 'ollama-cloud'
  | 'deepseek'
  | 'google-vertex'
  | 'mistral';

export interface AuthStatus {
  claude: ClaudeAuthStatus;
  codex: CodexAuthStatus;
  ollama: OllamaAuthStatus;
  mistral: MistralAuthStatus;
  openrouter?: OpenRouterAuthStatus;
  ollama_cloud: OllamaCloudAuthStatus;
  deepseek: DeepSeekAuthStatus;
  google_vertex: GoogleVertexAuthStatus;
}

export interface SetupConfig {
  agent: string;
  backend: string;
  model: string | null;
  ollama_base_url: string | null;
  api_key_stored: boolean;
}

export interface ModelOption {
  id: string;
  label: string;
  tier: string;
  family?: string;
  tool_use?: boolean;
  vision?: boolean;
  context_k?: number;
}

export interface SetupInfo {
  setup_complete: boolean;
  auth_status: AuthStatus;
  config: SetupConfig | null;
  /**
   * Session UUID pre-seeded via the webshell's `--resume-session <id>` CLI
   * flag (set by the gateway's `launch_webshell` MCP action when invoked
   * from the `/webshell` slash command inside a running Claude Code / Codex
   * session). When present, the chat drawer forwards it on the first
   * `createBuild` call so the conversation resumes from where the terminal
   * session left off.
   */
  resume_session?: string | null;
  /** Working directory the webshell was launched from. */
  cwd: string;
}

export interface SaveRequest {
  agent: string;
  backend: string;
  model: string | null;
  ollama_base_url: string | null;
  api_key?: string;
}

// --- Stores ---
/** Current setup wizard step. Drives which screen is mounted. */
export const step = writable<SetupStep>('splash');
/** Whether the setup wizard has been completed at least once. */
export const setupComplete = writable<boolean>(false);
/** True while any setup API request is in-flight. */
export const setupLoading = writable<boolean>(false);
/** Error message from the most recent failed setup API call, or null. */
export const setupError = writable<string | null>(null);

/**
 * Resolves once `loadSetupInfo()` has completed (success or failure).
 * SplashStep awaits this to avoid the 2.5s blind timer race.
 */
let _resolveSetupInfo: (() => void) | null = null;
export const setupInfoLoaded: Promise<void> = new Promise((resolve) => {
  _resolveSetupInfo = resolve;
});

/** Which top-level tier was chosen: local, byok, or la-platform. */
export const selectedTier = writable<SetupTier | null>(null);
/** The chosen backend slug (e.g. "anthropic", "openrouter", "ollama-launch"). */
export const selectedBackend = writable<string | null>(null);
/** The chosen agent archetype (e.g. "lightarchitects", "codex"). */
export const selectedAgent = writable<string | null>(null);
/** The chosen model ID for the selected backend. */
export const selectedModel = writable<string | null>(null);
/** Transient Ollama Cloud API key typed by the operator; cleared on successful save. */
export const apiKeyInput = writable<string>('');
export const ollamaBaseUrlInput = writable<string>('http://localhost:11434');
export const authStatus = writable<AuthStatus | null>(null);
export const persistedConfig = writable<SetupConfig | null>(null);
/** Models returned by `/api/setup/models` for the current backend selection. */
export const availableModels = writable<ModelOption[]>([]);
/** Controls whether the settings panel is open. */
export const settingsOpen = writable<boolean>(false);

/**
 * Session UUID the backend was launched with via `--resume-session`.
 *
 * The chat drawer consumes it on its first `ensureBuild()` and sets the
 * store back to `null` so a manual new-build later doesn't accidentally
 * inherit the same resume. The `/webshell` plugin slash command is the
 * primary producer of this value.
 */
export const pendingResumeSessionId = writable<string | null>(null);

/** Working directory from the webshell's launch context — used as default CWD for builds. */
export const serverCwd = writable<string>('/tmp');

// --- Actions ---
/** Fetch current setup state from `/api/setup/info` and hydrate all setup stores. */
export async function loadSetupInfo(): Promise<void> {
  setupLoading.set(true);
  setupError.set(null);
  try {
    const resp = await fetch('/api/setup/info');
    if (!resp.ok) throw new Error(`setup/info: ${resp.status}`);
    const data: SetupInfo = await resp.json();
    setupComplete.set(data.setup_complete);
    authStatus.set(data.auth_status);
    persistedConfig.set(data.config ?? null);
    if (data.resume_session) {
      pendingResumeSessionId.set(data.resume_session);
    }
    if (data.cwd) {
      serverCwd.set(data.cwd);
    }
    step.set(data.setup_complete ? 'done' : 'splash');
    if (data.setup_complete && data.config) {
      selectedBackend.set(data.config.backend);
      selectedAgent.set(data.config.agent);
      selectedModel.set(data.config.model ?? null);
      ollamaBaseUrlInput.set(data.config.ollama_base_url ?? 'http://localhost:11434');
      applyPersistedConfig(data.config);
    }
  } catch (e) {
    setupError.set(String(e));
  } finally {
    setupLoading.set(false);
    _resolveSetupInfo?.();
    _resolveSetupInfo = null;
  }
}

/**
 * Fetch available models for the given backend from `/api/setup/models`.
 * Updates {@link availableModels} with the returned list.
 */
export async function loadModels(backend: string, baseUrl?: string): Promise<void> {
  setupLoading.set(true);
  setupError.set(null);
  try {
    const url = new URL('/api/setup/models', window.location.origin);
    url.searchParams.set('backend', backend);
    if (baseUrl) url.searchParams.set('base_url', baseUrl);
    const resp = await fetch(url.toString());
    if (!resp.ok) throw new Error(`setup/models: ${resp.status}`);
    const data = await resp.json();
    availableModels.set(data.models ?? []);
  } catch (e) {
    setupError.set(String(e));
  } finally {
    setupLoading.set(false);
  }
}

/**
 * Auto-complete setup when a canonical credential source has been detected
 * (via the lightarchitects::credentials SDK module). Bypasses the Backend
 * → Auth → Model steps and advances directly to Init.
 *
 * Returns `true` if the skip fired (caller should not advance manually),
 * `false` if the conditions were not met.
 */
export async function autoCompleteFromInherited(
  agent: string,
  backend: string,
): Promise<boolean> {
  try {
    const token = resolveToken();
    const resp = await fetch('/api/setup/save', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json', Authorization: `Bearer ${token}` },
      body: JSON.stringify({ agent, backend, model: null, ollama_base_url: null }),
    });
    if (!resp.ok) return false;
    selectedAgent.set(agent);
    selectedBackend.set(backend);
    setupComplete.set(true);
    // Land on 'done' directly — 'init' would show a progress screen the user
    // doesn't need when auth was already inherited from the parent session.
    step.set('done');
    return true;
  } catch {
    return false;
  }
}

/**
 * POST the current wizard selections to `/api/setup/save` and advance to the init step.
 * Clears {@link apiKeyInput} on success.
 */
export async function saveSetup(): Promise<void> {
  const agent = get(selectedAgent);
  const backend = get(selectedBackend);
  if (!agent || !backend) return;
  setupLoading.set(true);
  setupError.set(null);
  try {
    const token = resolveToken();
    const body: SaveRequest = {
      agent,
      backend,
      model: get(selectedModel),
      ollama_base_url: backend.includes('ollama') ? get(ollamaBaseUrlInput) : null,
    };
    const apiKey = get(apiKeyInput);
    const reqBody: Record<string, unknown> = { ...body };
    if (apiKey) reqBody.api_key = apiKey;
    const resp = await fetch('/api/setup/save', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json', Authorization: `Bearer ${token}` },
      body: JSON.stringify(reqBody),
    });
    if (!resp.ok) throw new Error(`setup/save: ${resp.status}`);
    setupComplete.set(true);
    step.set('init');
    apiKeyInput.set('');
  } catch (e) {
    setupError.set(String(e));
  } finally {
    setupLoading.set(false);
  }
}

/** DELETE `/api/setup/reset` to wipe the persisted config and return the wizard to the splash step. */
export async function resetSetup(): Promise<void> {
  setupLoading.set(true);
  setupError.set(null);
  try {
    const token = resolveToken();
    const resp = await fetch('/api/setup/reset', {
      method: 'DELETE',
      headers: { Authorization: `Bearer ${token}` },
    });
    if (!resp.ok) throw new Error(`setup/reset: ${resp.status}`);
    setupComplete.set(false);
    step.set('splash');
    selectedTier.set(null);
    selectedBackend.set(null);
    selectedAgent.set(null);
    selectedModel.set(null);
    persistedConfig.set(null);
    settingsOpen.set(false);
  } catch (e) {
    setupError.set(String(e));
  } finally {
    setupLoading.set(false);
  }
}

/** Hydrate backend-specific stores from a persisted {@link SetupConfig} on startup. */
export function applyPersistedConfig(cfg: SetupConfig): void {
  if (cfg.backend === 'lightarchitects') {
    authProfile.set('lightarchitects');
  } else {
    const isOllama = cfg.backend.includes('ollama');
    authProfile.set(isOllama ? 'ollama' : ('anthropic' as AuthProfile));
    if (isOllama && cfg.ollama_base_url) {
      ollamaConfig.set({
        baseUrl: cfg.ollama_base_url,
        model: cfg.model ?? '',
        apiKey: '',
      });
    }
  }
}
