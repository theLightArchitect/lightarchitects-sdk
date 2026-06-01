<script lang="ts">
  import {
    step, selectedBackend, selectedTier, authStatus, apiKeyInput, ollamaBaseUrlInput,
    loadModels, setupLoading,
  } from '$lib/setup';

  type AuthMode = 'existing' | 'apikey';
  let authMode = $state<AuthMode>('existing');
  let showKey = $state(false);
  let ollamaReachable = $state<boolean | null>(null);
  let testingOllama = $state(false);

  const backend = $derived($selectedBackend ?? '');
  const isOllamaLocal  = $derived(backend === 'ollama-launch' || backend === 'ollama_launch');
  const isClaude       = $derived(backend === 'anthropic');
  const isCodex        = $derived(backend === 'openai');
  const isOllamaCloud  = $derived(backend === 'ollama-cloud');
  const isDeepSeek     = $derived(backend === 'deepseek');
  const isGoogleVertex = $derived(backend === 'google-vertex');
  const isMistral      = $derived(backend === 'mistral' || backend === 'mistral-vibe');

  const claudeAuth      = $derived($authStatus?.claude);
  const codexAuth       = $derived($authStatus?.codex);
  const mistralAuth     = $derived($authStatus?.mistral);
  const ollamaCloudAuth = $derived($authStatus?.ollama_cloud);
  const deepseekAuth    = $derived($authStatus?.deepseek);
  const vertexAuth      = $derived($authStatus?.google_vertex);

  const backStep = $derived($selectedTier === 'local' ? 'source' : 'provider');

  async function testOllama() {
    testingOllama = true;
    ollamaReachable = null;
    try {
      await loadModels(backend, $ollamaBaseUrlInput);
      ollamaReachable = true;
    } catch {
      ollamaReachable = false;
    } finally {
      testingOllama = false;
    }
  }

  const needsKeyEntry = $derived(isOllamaCloud || isDeepSeek || isGoogleVertex || isMistral);

  const canProceed = $derived(
    isOllamaLocal ? (ollamaReachable === true) :
    needsKeyEntry ? $apiKeyInput.trim().length > 0 :
    authMode === 'existing' ? true :
    $apiKeyInput.trim().length > 0
  );

  function proceed() {
    if (!canProceed) return;
    step.set('model');
  }
</script>

<div class="step">
  <h2 class="title">Authentication</h2>

  {#if isClaude}
    <p class="hint">How should the agent authenticate with Anthropic?</p>
    <div class="radio-group">
      <label class="radio-label">
        <input type="radio" bind:group={authMode} value="existing" />
        <span>
          Use existing Claude Code auth
          {#if claudeAuth?.has_keychain_auth}<span class="auth-badge">OAuth detected ✓</span>{/if}
          {#if claudeAuth?.login_source && claudeAuth.login_source !== 'none'}
            <span class="auth-source">Source: {claudeAuth.login_source}</span>
          {/if}
        </span>
      </label>
      <label class="radio-label">
        <input type="radio" bind:group={authMode} value="apikey" />
        <span>Enter API key</span>
      </label>
    </div>
  {:else if isCodex}
    <p class="hint">How should Codex authenticate with OpenAI?</p>
    <div class="radio-group">
      <label class="radio-label">
        <input type="radio" bind:group={authMode} value="existing" />
        <span>
          Use existing Codex auth
          {#if codexAuth?.has_keychain_auth}<span class="auth-badge">ChatGPT auth ✓</span>{/if}
          {#if codexAuth?.login_source && codexAuth.login_source !== 'none'}
            <span class="auth-source">Source: {codexAuth.login_source}</span>
          {/if}
        </span>
      </label>
      <label class="radio-label">
        <input type="radio" bind:group={authMode} value="apikey" />
        <span>Enter OpenAI API key</span>
      </label>
    </div>
  {:else if isOllamaLocal}
    <p class="hint">Configure your Ollama endpoint</p>
    <div class="ollama-form">
      <label class="field-label" for="ollama-url-input">Base URL</label>
      <input
        id="ollama-url-input"
        class="input"
        type="url"
        bind:value={$ollamaBaseUrlInput}
        placeholder="http://localhost:11434"
      />
      <button class="btn-test" onclick={testOllama} disabled={testingOllama}>
        {testingOllama ? 'Testing…' : 'Test'}
      </button>
      {#if ollamaReachable === true}
        <span class="reachable-badge">Reachable ✓</span>
      {:else if ollamaReachable === false}
        <span class="unreachable-badge">Unreachable ✗</span>
      {/if}
    </div>
  {:else if isOllamaCloud}
    <p class="hint">Enter your Ollama Cloud Bearer token</p>
    {#if ollamaCloudAuth?.has_api_key}
      <span class="auth-badge standalone">Token stored ✓</span>
    {/if}
  {:else if isDeepSeek}
    <p class="hint">Enter your DeepSeek API key</p>
    {#if deepseekAuth?.has_api_key}
      <span class="auth-badge standalone">Key stored ✓</span>
    {/if}
  {:else if isGoogleVertex}
    <p class="hint">Paste your GCP service account JSON (or file path)</p>
    {#if vertexAuth?.has_service_account}
      <span class="auth-badge standalone">Service account stored ✓</span>
      {#if vertexAuth.project_id}
        <span class="auth-source standalone">Project: {vertexAuth.project_id}</span>
      {/if}
    {/if}
  {:else if isMistral}
    <p class="hint">Enter your Mistral API key</p>
    {#if mistralAuth?.has_api_key}
      <span class="auth-badge standalone">Key stored ✓</span>
    {/if}
  {/if}

  {#if (isClaude || isCodex) && authMode === 'apikey' || needsKeyEntry}
    <div class="key-field">
      <label class="field-label" for="api-key-input">
        {#if isClaude}Anthropic API Key
        {:else if isCodex}OpenAI API Key
        {:else if isOllamaCloud}Ollama Cloud Bearer Token
        {:else if isDeepSeek}DeepSeek API Key
        {:else if isGoogleVertex}Service Account JSON
        {:else}Mistral API Key
        {/if}
      </label>
      <div class="key-wrap">
        <input
          id="api-key-input"
          class="input"
          type={showKey ? 'text' : 'password'}
          bind:value={$apiKeyInput}
          placeholder={isGoogleVertex ? '{"type":"service_account",...}' : 'sk-...'}
          autocomplete="off"
          spellcheck="false"
        />
        <button class="toggle-vis" onclick={() => showKey = !showKey}>{showKey ? '🙈' : '👁'}</button>
      </div>
      <p class="key-note">Never stored in browser storage — sent once to the backend.</p>
    </div>
  {/if}

  <div class="footer">
    <button class="btn-back" onclick={() => step.set(backStep)}>Back</button>
    <button class="btn-continue" disabled={!canProceed || $setupLoading} onclick={proceed}>
      Continue
    </button>
  </div>
</div>

<style>
  .step { display:flex; flex-direction:column; align-items:center; gap:1.25rem; padding:2rem; height:100vh; justify-content:center; max-width:480px; margin:0 auto; }
  .title { font-family:'Raleway',sans-serif; font-size:2rem; font-weight:700; color:#e2e8f0; margin:0; }
  .hint { font-family:'IBM Plex Mono',monospace; font-size:0.75rem; color:#475569; margin:0; }

  .radio-group { display:flex; flex-direction:column; gap:0.75rem; width:100%; }
  .radio-label { display:flex; align-items:center; gap:0.75rem; color:#94a3b8; font-family:'IBM Plex Mono',monospace; font-size:0.85rem; cursor:pointer; }
  .auth-badge { margin-left:0.5rem; color:#00d26a; font-size:0.7rem; }
  .auth-badge.standalone { margin-left:0; font-family:'IBM Plex Mono',monospace; font-size:0.8rem; }
  .auth-source { display:block; margin-top:0.25rem; margin-left:1.25rem; color:#64748b; font-size:0.65rem; font-family:'IBM Plex Mono',monospace; }
  .auth-source.standalone { margin-left:0; }

  .ollama-form { display:flex; align-items:center; gap:0.75rem; flex-wrap:wrap; width:100%; }
  .key-field { display:flex; flex-direction:column; gap:0.5rem; width:100%; }
  .field-label { font-family:'IBM Plex Mono',monospace; font-size:0.7rem; color:#475569; letter-spacing:0.1em; }
  .input { background:#0f172a; border:1px solid #334155; color:#e2e8f0; border-radius:6px; padding:0.5rem 0.75rem; font-family:'IBM Plex Mono',monospace; font-size:0.85rem; flex:1; min-width:200px; }
  .input:focus { outline:none; border-color:#ff6600; }
  .key-wrap { display:flex; align-items:center; gap:0.5rem; }
  .toggle-vis { background:none; border:none; cursor:pointer; font-size:1rem; }
  .key-note { font-family:'IBM Plex Mono',monospace; font-size:0.65rem; color:#334155; margin:0; }

  .btn-test { background:#1e293b; border:1px solid #334155; color:#94a3b8; padding:0.4rem 1rem; border-radius:6px; cursor:pointer; font-family:'IBM Plex Mono',monospace; font-size:0.75rem; white-space:nowrap; }
  .btn-test:disabled { opacity:0.5; cursor:not-allowed; }
  .reachable-badge { color:#00d26a; font-family:'IBM Plex Mono',monospace; font-size:0.75rem; }
  .unreachable-badge { color:#ef4444; font-family:'IBM Plex Mono',monospace; font-size:0.75rem; }

  .footer { display:flex; gap:1rem; margin-top:1rem; }
  .btn-back { background:transparent; border:1px solid #334155; color:#64748b; padding:0.5rem 1.25rem; border-radius:6px; cursor:pointer; font-family:'IBM Plex Mono',monospace; font-size:0.8rem; }
  .btn-back:hover { color:#94a3b8; }
  .btn-continue { background:#ff6600; border:none; color:#fff; padding:0.5rem 1.5rem; border-radius:6px; cursor:pointer; font-family:'IBM Plex Mono',monospace; font-size:0.8rem; font-weight:600; transition:opacity 0.15s; }
  .btn-continue:disabled { opacity:0.35; cursor:not-allowed; }
</style>
