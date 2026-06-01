<!--
  @component
  Second wizard step for the BYOK tier — choose a cloud LLM provider.

  Four mutually-exclusive choices:
    anthropic   → Anthropic API (Claude models)
    openai      → OpenAI API (GPT models)
    openrouter  → OpenRouter (multi-provider hub)
    mistral-vibe → Mistral AI

  Selecting a provider and pressing Continue sets selectedBackend +
  selectedAgent ('lightarchitects' for all) and advances to 'auth'.
-->
<script lang="ts">
  import { step, selectedBackend, selectedAgent, authStatus } from '$lib/setup';

  type Provider = 'anthropic' | 'openai' | 'openrouter' | 'mistral-vibe';

  let chosen = $state<Provider | null>(null);

  const claudeHasKey  = $derived($authStatus?.claude?.has_api_key || $authStatus?.claude?.has_keychain_auth);
  const openaiHasKey  = $derived($authStatus?.codex?.has_api_key  || $authStatus?.codex?.has_keychain_auth);
  const orHasKey      = $derived($authStatus?.openrouter?.has_api_key);
  const mistralHasKey = $derived($authStatus?.mistral?.has_api_key);

  function proceed() {
    if (!chosen) return;
    selectedBackend.set(chosen);
    selectedAgent.set('lightarchitects');
    step.set('auth');
  }
</script>

<div class="step">
  <h2 class="title">Choose Provider</h2>
  <p class="hint">Which cloud LLM provider do you want to route through?</p>

  <div class="providers">

    <!-- ANTHROPIC -->
    <button
      class="provider-card"
      class:selected={chosen === 'anthropic'}
      onclick={() => chosen = 'anthropic'}
      data-testid="provider-anthropic"
    >
      <div class="provider-icon">
        <svg width="40" height="40" viewBox="0 0 40 40" fill="none" xmlns="http://www.w3.org/2000/svg">
          <polygon points="20,4 36,34 4,34" stroke="currentColor" stroke-width="2" fill="none"/>
          <polygon points="20,13 29,30 11,30" stroke="currentColor" stroke-width="1.5" fill="currentColor" fill-opacity="0.15"/>
          <circle cx="20" cy="24" r="2.5" fill="currentColor" opacity="0.8"/>
        </svg>
      </div>
      <div class="provider-body">
        <div class="provider-label">Anthropic</div>
        <div class="provider-sub">Claude Sonnet · Opus · Haiku</div>
        {#if claudeHasKey}
          <span class="key-badge">Key stored ✓</span>
        {/if}
      </div>
    </button>

    <!-- OPENAI -->
    <button
      class="provider-card"
      class:selected={chosen === 'openai'}
      onclick={() => chosen = 'openai'}
      data-testid="provider-openai"
    >
      <div class="provider-icon">
        <svg width="40" height="40" viewBox="0 0 40 40" fill="none" xmlns="http://www.w3.org/2000/svg">
          <circle cx="20" cy="20" r="14" stroke="currentColor" stroke-width="2"/>
          <path d="M20 8 C26 14, 26 26, 20 32 C14 26, 14 14, 20 8Z" stroke="currentColor" stroke-width="1.5" fill="currentColor" fill-opacity="0.12"/>
          <line x1="8" y1="14" x2="32" y2="14" stroke="currentColor" stroke-width="1" opacity="0.4"/>
          <line x1="8" y1="26" x2="32" y2="26" stroke="currentColor" stroke-width="1" opacity="0.4"/>
        </svg>
      </div>
      <div class="provider-body">
        <div class="provider-label">OpenAI</div>
        <div class="provider-sub">GPT-4o · o3 · o4-mini</div>
        {#if openaiHasKey}
          <span class="key-badge">Key stored ✓</span>
        {/if}
      </div>
    </button>

    <!-- OPENROUTER -->
    <button
      class="provider-card"
      class:selected={chosen === 'openrouter'}
      onclick={() => chosen = 'openrouter'}
      data-testid="provider-openrouter"
    >
      <div class="provider-icon">
        <svg width="40" height="40" viewBox="0 0 40 40" fill="none" xmlns="http://www.w3.org/2000/svg">
          <circle cx="8"  cy="20" r="4" stroke="currentColor" stroke-width="1.5"/>
          <circle cx="32" cy="12" r="4" stroke="currentColor" stroke-width="1.5"/>
          <circle cx="32" cy="28" r="4" stroke="currentColor" stroke-width="1.5"/>
          <line x1="12" y1="20" x2="28" y2="14" stroke="currentColor" stroke-width="1.5" opacity="0.7"/>
          <line x1="12" y1="20" x2="28" y2="26" stroke="currentColor" stroke-width="1.5" opacity="0.7"/>
          <circle cx="20" cy="17" r="2" fill="currentColor" opacity="0.5"/>
          <circle cx="20" cy="23" r="2" fill="currentColor" opacity="0.5"/>
        </svg>
      </div>
      <div class="provider-body">
        <div class="provider-label">OpenRouter</div>
        <div class="provider-sub">200+ models · one API key</div>
        {#if orHasKey}
          <span class="key-badge">Key stored ✓</span>
        {/if}
      </div>
    </button>

    <!-- MISTRAL -->
    <button
      class="provider-card"
      class:selected={chosen === 'mistral-vibe'}
      onclick={() => chosen = 'mistral-vibe'}
      data-testid="provider-mistral"
    >
      <div class="provider-icon">
        <svg width="40" height="40" viewBox="0 0 40 40" fill="none" xmlns="http://www.w3.org/2000/svg">
          <path d="M6 28 Q10 20, 14 24 Q18 28, 22 20 Q26 12, 30 16 Q34 20, 38 14"
                stroke="currentColor" stroke-width="2" fill="none" stroke-linecap="round"/>
          <path d="M6 34 Q10 26, 14 30 Q18 34, 22 26 Q26 18, 30 22 Q34 26, 38 20"
                stroke="currentColor" stroke-width="1.5" fill="none" stroke-linecap="round" opacity="0.5"/>
        </svg>
      </div>
      <div class="provider-body">
        <div class="provider-label">Mistral</div>
        <div class="provider-sub">Mistral Large · Codestral</div>
        {#if mistralHasKey}
          <span class="key-badge">Key stored ✓</span>
        {/if}
      </div>
    </button>

  </div>

  <div class="footer">
    <button class="btn-back" onclick={() => step.set('source')}>Back</button>
    <button
      class="btn-continue"
      disabled={!chosen}
      onclick={proceed}
    >
      Continue
    </button>
  </div>
</div>

<style>
  .step {
    display: flex; flex-direction: column; align-items: center;
    gap: 2rem; padding: 2rem; height: 100vh; justify-content: center;
  }
  .title {
    font-family: 'Raleway', sans-serif; font-size: 2rem; font-weight: 700;
    color: #e2e8f0; margin: 0; letter-spacing: 0.05em;
  }
  .hint {
    font-family: 'IBM Plex Mono', monospace; font-size: 0.75rem;
    color: #475569; margin: 0; letter-spacing: 0.05em;
  }

  .providers {
    display: flex; gap: 1rem; flex-wrap: wrap; justify-content: center;
    max-width: 860px;
  }

  .provider-card {
    display: flex; align-items: flex-start; gap: 1rem;
    background: #0f172a; border: 1px solid #1e293b; border-radius: 14px;
    padding: 1.25rem 1.5rem; cursor: pointer; text-align: left;
    width: 190px; color: #94a3b8;
    transition: border-color 0.2s, box-shadow 0.2s, color 0.2s;
  }
  .provider-card:hover {
    border-color: #334155; color: #cbd5e1;
  }
  .provider-card.selected {
    border-color: #ff6600;
    box-shadow: 0 0 24px rgba(255, 102, 0, 0.22);
    color: #e2e8f0;
  }

  .provider-icon {
    flex-shrink: 0; width: 40px; height: 40px;
    display: flex; align-items: center; justify-content: center;
    margin-top: 0.15rem;
  }

  .provider-body { display: flex; flex-direction: column; gap: 0.35rem; }
  .provider-label {
    font-family: 'Raleway', sans-serif; font-size: 1rem; font-weight: 700;
    color: #e2e8f0; letter-spacing: 0.03em;
  }
  .provider-sub {
    font-family: 'IBM Plex Mono', monospace; font-size: 0.65rem; color: #475569;
    line-height: 1.4;
  }
  .key-badge {
    font-family: 'IBM Plex Mono', monospace; font-size: 0.6rem;
    color: #00d26a; margin-top: 0.15rem;
  }

  .footer { display: flex; gap: 1rem; }
  .btn-back {
    background: transparent; border: 1px solid #334155; color: #64748b;
    padding: 0.5rem 1.25rem; border-radius: 6px; cursor: pointer;
    font-family: 'IBM Plex Mono', monospace; font-size: 0.8rem;
    transition: color 0.15s;
  }
  .btn-back:hover { color: #94a3b8; }
  .btn-continue {
    background: #ff6600; border: none; color: #fff;
    padding: 0.5rem 1.5rem; border-radius: 6px; cursor: pointer;
    font-family: 'IBM Plex Mono', monospace; font-size: 0.8rem; font-weight: 600;
    transition: opacity 0.15s;
  }
  .btn-continue:disabled { opacity: 0.35; cursor: not-allowed; }
</style>
