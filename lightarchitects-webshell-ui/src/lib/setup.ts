import { writable, get } from 'svelte/store';
import { resolveToken } from './auth';
import { authProfile, ollamaConfig } from './stores';
import type { AuthProfile } from './types';

export type SetupStep = 'splash' | 'backend' | 'auth' | 'model' | 'init' | 'done';

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

export interface AuthStatus {
  claude: ClaudeAuthStatus;
  codex: CodexAuthStatus;
  ollama: OllamaAuthStatus;
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
export const step = writable<SetupStep>('splash');
export const setupComplete = writable<boolean>(false);
export const setupLoading = writable<boolean>(false);
export const setupError = writable<string | null>(null);

/**
 * Resolves once `loadSetupInfo()` has completed (success or failure).
 * SplashStep awaits this to avoid the 2.5s blind timer race.
 */
let _resolveSetupInfo: (() => void) | null = null;
export const setupInfoLoaded: Promise<void> = new Promise((resolve) => {
  _resolveSetupInfo = resolve;
});

export const selectedBackend = writable<string | null>(null);
export const selectedAgent = writable<string | null>(null);
export const selectedModel = writable<string | null>(null);
export const apiKeyInput = writable<string>('');
export const ollamaBaseUrlInput = writable<string>('http://localhost:11434');
export const authStatus = writable<AuthStatus | null>(null);
export const persistedConfig = writable<SetupConfig | null>(null);
export const availableModels = writable<ModelOption[]>([]);
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

// Hydrate existing stores from persisted config on load
export function applyPersistedConfig(cfg: SetupConfig): void {
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
