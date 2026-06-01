<!--
  @component
  First wizard step — choose how you want to run AI.

  Three mutually-exclusive tiers:
    local       → Ollama on your machine
    byok        → Bring your own API key (Anthropic / OpenAI / OpenRouter / Mistral)
    la-platform → LA managed cloud (stub — coming soon)

  Selecting a tier and pressing Continue advances:
    local       → auth  (Ollama URL config)
    byok        → provider (pick which cloud provider)
    la-platform → (stub, no navigation yet)
-->
<script lang="ts">
  import { step, selectedTier, selectedBackend, selectedAgent, type SetupTier } from '$lib/setup';

  let chosen = $state<SetupTier | null>($selectedTier);

  function proceed() {
    if (!chosen || chosen === 'la-platform') return;
    selectedTier.set(chosen);

    if (chosen === 'local') {
      selectedBackend.set('ollama-launch');
      selectedAgent.set('lightarchitects');
      step.set('auth');
    } else {
      // byok — let ProviderStep set backend + agent
      selectedBackend.set(null);
      selectedAgent.set(null);
      step.set('provider');
    }
  }
</script>

<div class="step">
  <h2 class="title">How do you want to run AI?</h2>
  <p class="hint">Choose your model source — you can change this later in settings</p>

  <div class="tiers">

    <!-- LOCAL -->
    <button
      class="tier-card"
      class:selected={chosen === 'local'}
      onclick={() => chosen = 'local'}
      data-testid="tier-local"
    >
      <div class="tier-icon">
        <svg width="48" height="48" viewBox="0 0 48 48" fill="none" xmlns="http://www.w3.org/2000/svg">
          <rect x="6" y="12" width="36" height="24" rx="3" stroke="currentColor" stroke-width="2"/>
          <rect x="10" y="16" width="6" height="6" rx="1" fill="currentColor" opacity="0.6"/>
          <rect x="10" y="26" width="6" height="2" rx="1" fill="currentColor" opacity="0.4"/>
          <rect x="10" y="30" width="4" height="2" rx="1" fill="currentColor" opacity="0.3"/>
          <circle cx="36" cy="19" r="2" fill="#00d26a"/>
          <circle cx="36" cy="26" r="2" fill="currentColor" opacity="0.3"/>
          <circle cx="36" cy="33" r="2" fill="currentColor" opacity="0.3"/>
          <line x1="20" y1="19" x2="32" y2="19" stroke="currentColor" stroke-width="1.5" opacity="0.4"/>
          <line x1="20" y1="26" x2="32" y2="26" stroke="currentColor" stroke-width="1.5" opacity="0.25"/>
          <line x1="20" y1="33" x2="32" y2="33" stroke="currentColor" stroke-width="1.5" opacity="0.2"/>
        </svg>
      </div>
      <div class="tier-body">
        <div class="tier-label">Local</div>
        <div class="tier-sub">Ollama on your machine</div>
        <div class="tier-tags">
          <span class="tag">Private</span>
          <span class="tag">Any Ollama model</span>
          <span class="tag">No API key</span>
        </div>
      </div>
    </button>

    <!-- BYOK -->
    <button
      class="tier-card"
      class:selected={chosen === 'byok'}
      onclick={() => chosen = 'byok'}
      data-testid="tier-byok"
    >
      <div class="tier-icon">
        <svg width="48" height="48" viewBox="0 0 48 48" fill="none" xmlns="http://www.w3.org/2000/svg">
          <circle cx="18" cy="22" r="9" stroke="currentColor" stroke-width="2"/>
          <circle cx="18" cy="22" r="4" fill="currentColor" opacity="0.35"/>
          <line x1="27" y1="22" x2="42" y2="22" stroke="currentColor" stroke-width="2" stroke-linecap="round"/>
          <line x1="38" y1="22" x2="38" y2="28" stroke="currentColor" stroke-width="2" stroke-linecap="round"/>
          <line x1="34" y1="22" x2="34" y2="26" stroke="currentColor" stroke-width="2" stroke-linecap="round"/>
        </svg>
      </div>
      <div class="tier-body">
        <div class="tier-label">Bring Your Own Key</div>
        <div class="tier-sub">Your API key, your models</div>
        <div class="tier-tags">
          <span class="tag">Anthropic</span>
          <span class="tag">OpenAI</span>
          <span class="tag">OpenRouter</span>
          <span class="tag">Mistral</span>
        </div>
      </div>
    </button>

    <!-- LA PLATFORM (stub) -->
    <button
      class="tier-card tier-stub"
      class:selected={chosen === 'la-platform'}
      onclick={() => chosen = 'la-platform'}
      disabled
      data-testid="tier-la-platform"
    >
      <div class="tier-icon">
        <svg width="48" height="48" viewBox="0 0 48 48" fill="none" xmlns="http://www.w3.org/2000/svg">
          <polygon points="24,4 44,36 4,36" stroke="currentColor" stroke-width="2" fill="none" opacity="0.5"/>
          <polygon points="24,14 36,32 12,32" stroke="currentColor" stroke-width="1.5" fill="currentColor" fill-opacity="0.1" opacity="0.5"/>
          <circle cx="24" cy="26" r="3" fill="currentColor" opacity="0.4"/>
        </svg>
      </div>
      <div class="tier-body">
        <div class="tier-label">LA Platform</div>
        <div class="tier-sub">Managed cloud models</div>
        <div class="tier-tags">
          <span class="tag tag-soon">Coming Soon</span>
          <span class="tag">Included with subscription</span>
        </div>
      </div>
    </button>

  </div>

  <div class="footer">
    <button class="btn-back" onclick={() => step.set('splash')}>Back</button>
    <button
      class="btn-continue"
      disabled={!chosen || chosen === 'la-platform'}
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

  .tiers {
    display: flex; gap: 1.25rem; flex-wrap: wrap; justify-content: center;
    max-width: 860px;
  }

  .tier-card {
    display: flex; align-items: flex-start; gap: 1.25rem;
    background: #0f172a; border: 1px solid #1e293b; border-radius: 14px;
    padding: 1.5rem 1.75rem; cursor: pointer; text-align: left;
    width: 260px; color: #94a3b8;
    transition: border-color 0.2s, box-shadow 0.2s, color 0.2s;
  }
  .tier-card:hover:not(:disabled) {
    border-color: #334155; color: #cbd5e1;
  }
  .tier-card.selected {
    border-color: #ff6600;
    box-shadow: 0 0 28px rgba(255, 102, 0, 0.25);
    color: #e2e8f0;
  }
  .tier-card.tier-stub {
    opacity: 0.4; cursor: not-allowed;
  }
  .tier-card.tier-stub:hover { border-color: #1e293b; color: #94a3b8; }

  .tier-icon {
    flex-shrink: 0; width: 48px; height: 48px;
    display: flex; align-items: center; justify-content: center;
    margin-top: 0.25rem;
  }

  .tier-body { display: flex; flex-direction: column; gap: 0.4rem; }
  .tier-label {
    font-family: 'Raleway', sans-serif; font-size: 1.05rem; font-weight: 700;
    color: #e2e8f0; letter-spacing: 0.03em;
  }
  .tier-stub .tier-label { color: #475569; }
  .tier-sub {
    font-family: 'IBM Plex Mono', monospace; font-size: 0.7rem; color: #475569;
  }

  .tier-tags {
    display: flex; flex-wrap: wrap; gap: 0.35rem; margin-top: 0.35rem;
  }
  .tag {
    font-family: 'IBM Plex Mono', monospace; font-size: 0.6rem;
    padding: 0.15rem 0.5rem; border-radius: 4px;
    background: #1e293b; color: #64748b; letter-spacing: 0.04em;
  }
  .tag-soon { background: #1a1a0f; color: #ca8a04; }

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
