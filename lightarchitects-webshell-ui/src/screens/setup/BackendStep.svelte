<!--
  @component
  Backend selection step of the setup wizard.

  Displays a card grid of available backends (Claude Code, Codex, Ollama,
  LA Native). Selecting a card sets `selectedBackend` and `selectedAgent`
  in the setup stores and advances the wizard to the next step.
-->
<script lang="ts">
  import { step, selectedBackend, selectedAgent, authStatus } from '$lib/setup';
  import PolytopeIcon from '../../components/PolytopeIcon.svelte';

  const backends = [
    {
      id: 'anthropic',
      agent: 'lightarchitects',
      label: 'Claude Code',
      description: 'Anthropic Claude via Claude Code auth',
      polytopeType: 'icositetrachoron',
      authKey: 'claude' as const,
      authField: 'has_keychain_auth' as const,
      authBadge: 'OAuth detected ✓',
      comingSoon: false,
    },
    {
      id: 'openai',
      agent: 'codex',
      label: 'Codex',
      description: 'OpenAI Codex via ChatGPT auth',
      polytopeType: 'hexadecachoron',
      authKey: 'codex' as const,
      authField: 'has_keychain_auth' as const,
      authBadge: 'ChatGPT auth ✓',
      comingSoon: false,
    },
    {
      id: 'ollama-launch',
      agent: 'lightarchitects',
      label: 'Ollama Local',
      description: 'Local LLM via Ollama',
      polytopeType: 'tesseract',
      authKey: 'ollama' as const,
      authField: null,
      authBadge: null,
      comingSoon: false,
    },
    {
      id: 'mistral-vibe',
      agent: 'mistral_vibe',
      label: 'Mistral Vibe',
      description: 'Mistral AI Vibe coding agent',
      polytopeType: 'pentachoron',
      authKey: 'mistral_vibe' as const,
      authField: null,
      authBadge: null,
      comingSoon: false,
    },
    {
      id: 'ollama-cloud',
      agent: 'lightarchitects',
      label: 'Ollama Cloud',
      description: '17+ cloud models via Ollama (GLM, DeepSeek, Qwen…)',
      polytopeType: 'tesseract',
      authKey: 'ollama' as const,
      authField: null,
      authBadge: null,
      comingSoon: false,
    },
    {
      id: 'la-native',
      agent: 'lightarchitects_native',
      label: 'LA Native',
      description: 'Nemotron / Qwen3 via Ollama Cloud (1M ctx, tool-use)',
      polytopeType: 'icositetrachoron',
      authKey: 'claude' as const,
      authField: 'has_api_key' as const,
      authBadge: 'OLLAMA_API_KEY ✓',
      comingSoon: false,
    },
    {
      id: 'openrouter',
      agent: 'lightarchitects',
      label: 'OpenRouter',
      description: '200+ models via OpenRouter API',
      polytopeType: 'hexadecachoron',
      authKey: 'ollama' as const,
      authField: null,
      authBadge: null,
      comingSoon: false,
    },
    {
      id: 'la-cloud',
      agent: 'lightarchitects',
      label: 'LA Cloud',
      description: 'Light Architects managed cloud',
      polytopeType: 'icositetrachoron',
      authKey: 'claude' as const,
      authField: null,
      authBadge: null,
      comingSoon: true,
    },
  ] as const;

  let selected = $state<string | null>($selectedBackend);

  function pick(id: string, agent: string, comingSoon: boolean) {
    if (comingSoon) return;
    selected = id;
  }

  function proceed() {
    if (!selected) return;
    const b = backends.find(x => x.id === selected);
    if (!b || b.comingSoon) return;
    selectedBackend.set(b.id);
    selectedAgent.set(b.agent);
    step.set('auth');
  }

  function getAuthBadge(b: typeof backends[number]): string | null {
    if (!$authStatus) return null;
    if (b.authField === null) {
      const ollama = $authStatus.ollama;
      return ollama.reachable ? 'Reachable ✓' : null;
    }
    const status = $authStatus[b.authKey];
    return (status as unknown as Record<string,unknown>)[b.authField] ? b.authBadge : null;
  }
</script>

<div class="step">
  <h2 class="title">Choose Backend</h2>
  <p class="hint">Select the AI provider for your agent session</p>

  <div class="cards">
    {#each backends as b}
      {@const badge = getAuthBadge(b)}
      <button
        class="card"
        class:selected={selected === b.id}
        class:coming-soon={b.comingSoon}
        disabled={b.comingSoon}
        onclick={() => pick(b.id, b.agent, b.comingSoon)}
      >
        <div class="card-polytope">
          <PolytopeIcon type={b.polytopeType} size={120} color={b.comingSoon ? '#334155' : '#64748b'} />
        </div>
        <div class="card-info">
          <div class="card-label">{b.label}</div>
          <div class="card-desc">{b.description}</div>
          {#if b.comingSoon}
            <div class="badge coming-soon-badge">Coming Soon</div>
          {:else if badge}
            <div class="badge">{badge}</div>
          {/if}
        </div>
      </button>
    {/each}
  </div>

  <div class="footer">
    <button class="btn-back" onclick={() => step.set('splash')}>Back</button>
    <button class="btn-continue" disabled={!selected} onclick={proceed}>Continue</button>
  </div>
</div>

<style>
  .step { display:flex; flex-direction:column; align-items:center; gap:1.5rem; padding:2rem; height:100vh; justify-content:center; }
  .title { font-family:'Raleway',sans-serif; font-size:2rem; font-weight:700; color:#e2e8f0; margin:0; letter-spacing:0.05em; }
  .hint { font-family:'IBM Plex Mono',monospace; font-size:0.75rem; color:#475569; margin:0; letter-spacing:0.1em; }

  .cards { display:flex; flex-wrap:wrap; gap:1rem; justify-content:center; }

  .card {
    display:flex; flex-direction:column; align-items:center; gap:0.75rem;
    background:#0f172a; border:1px solid #1e293b; border-radius:12px;
    padding:1.5rem; cursor:pointer; width:200px;
    transition:border-color 0.2s, box-shadow 0.2s;
    color:inherit;
  }
  .card:hover { border-color:#334155; }
  .card.selected {
    border-color:#ff6600;
    box-shadow:0 0 24px rgba(255,102,0,0.3);
  }

  .card-polytope { height:120px; display:flex; align-items:center; justify-content:center; }
  .card-info { text-align:center; }
  .card-label { font-family:'Raleway',sans-serif; font-size:1rem; font-weight:700; color:#e2e8f0; margin-bottom:0.25rem; }
  .card-desc { font-family:'IBM Plex Mono',monospace; font-size:0.65rem; color:#475569; line-height:1.4; }
  .badge { margin-top:0.5rem; font-family:'IBM Plex Mono',monospace; font-size:0.6rem; color:#00d26a; letter-spacing:0.05em; }
  .coming-soon-badge { color:#475569; }
  .card.coming-soon { opacity:0.45; cursor:not-allowed; }
  .card.coming-soon:hover { border-color:#1e293b; }

  .footer { display:flex; gap:1rem; margin-top:1rem; }
  .btn-back { background:transparent; border:1px solid #334155; color:#64748b; padding:0.5rem 1.25rem; border-radius:6px; cursor:pointer; font-family:'IBM Plex Mono',monospace; font-size:0.8rem; transition:color 0.15s; }
  .btn-back:hover { color:#94a3b8; }
  .btn-continue { background:#ff6600; border:none; color:#fff; padding:0.5rem 1.5rem; border-radius:6px; cursor:pointer; font-family:'IBM Plex Mono',monospace; font-size:0.8rem; font-weight:600; transition:opacity 0.15s; }
  .btn-continue:disabled { opacity:0.35; cursor:not-allowed; }
</style>
