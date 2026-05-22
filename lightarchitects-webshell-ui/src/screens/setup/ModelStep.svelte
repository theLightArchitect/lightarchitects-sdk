<script lang="ts">
  import {
    step, selectedBackend, selectedModel, availableModels,
    setupLoading, setupError, loadModels, saveSetup, ollamaBaseUrlInput,
  } from '$lib/setup';

  $effect(() => {
    loadModels($selectedBackend ?? 'anthropic', $selectedBackend?.includes('ollama') ? $ollamaBaseUrlInput : undefined);
  });

  const tierOrder: Record<string, number> = { fast: 1, balanced: 2, capable: 3, flagship: 4 };
  const sorted = $derived([...$availableModels].sort((a, b) => {
    const fa = a.family ?? '';
    const fb = b.family ?? '';
    if (fa !== fb) return fa.localeCompare(fb);
    return (tierOrder[a.tier] ?? 99) - (tierOrder[b.tier] ?? 99);
  }));

  /** Group models by family for ollama-cloud; return null for flat display. */
  const grouped = $derived.by<Map<string, typeof $availableModels> | null>(() => {
    if (!$availableModels.some(m => m.family)) return null;
    const map = new Map<string, typeof $availableModels>();
    for (const m of sorted) {
      const fam = m.family ?? 'Other';
      if (!map.has(fam)) map.set(fam, []);
      map.get(fam)!.push(m);
    }
    return map;
  });

  // Default selection
  $effect(() => {
    if ($availableModels.length > 0 && !$selectedModel) {
      if ($selectedBackend === 'anthropic') {
        const sonnet = $availableModels.find(m => m.id.includes('sonnet'));
        selectedModel.set(sonnet?.id ?? $availableModels[0].id);
      } else {
        selectedModel.set($availableModels[0].id);
      }
    }
  });

  let launching = $state(false);

  async function launch() {
    if (!$selectedModel || launching) return;
    launching = true;
    try {
      await saveSetup();
    } finally {
      launching = false;
    }
  }

  const tierColors: Record<string, string> = { balanced: '#ff6600', capable: '#b44aff', fast: '#00d26a' };
</script>

<div class="step">
  <h2 class="title">Choose Model</h2>
  <p class="hint">Select the model for your {$selectedBackend ?? 'agent'} session</p>

  {#if $setupLoading}
    <div class="loading">Loading models…</div>
  {:else if $setupError}
    <div class="error">{$setupError}</div>
  {:else if sorted.length === 0}
    <div class="empty">No models available for this backend.</div>
  {:else if grouped}
    <div class="families">
      {#each [...grouped.entries()] as [family, models]}
        <div class="family-group">
          <div class="family-label">{family}</div>
          <div class="grid">
            {#each models as m}
              <button
                class="model-card"
                class:selected={$selectedModel === m.id}
                onclick={() => selectedModel.set(m.id)}
              >
                <div class="model-id">{m.label || m.id}</div>
                <div class="model-tier" style="color:{tierColors[m.tier] ?? '#64748b'}">{m.tier}</div>
                <div class="model-badges">
                  {#if m.tool_use}<span class="mbadge tool">tools</span>{/if}
                  {#if m.vision}<span class="mbadge vision">vision</span>{/if}
                  {#if m.context_k}<span class="mbadge ctx">{m.context_k}K</span>{/if}
                </div>
              </button>
            {/each}
          </div>
        </div>
      {/each}
    </div>
  {:else}
    <div class="grid">
      {#each sorted as m}
        <button
          class="model-card"
          class:selected={$selectedModel === m.id}
          onclick={() => selectedModel.set(m.id)}
        >
          <div class="model-id">{m.label || m.id}</div>
          <div class="model-tier" style="color:{tierColors[m.tier] ?? '#64748b'}">{m.tier}</div>
        </button>
      {/each}
    </div>
  {/if}

  <div class="footer">
    <button class="btn-back" onclick={() => step.set('auth')}>← Back</button>
    <button
      class="btn-launch"
      disabled={!$selectedModel || $setupLoading || launching}
      onclick={launch}
    >
      {launching ? 'Launching…' : 'Launch →'}
    </button>
  </div>
</div>

<style>
  .step { display:flex; flex-direction:column; align-items:center; gap:1.5rem; padding:2rem; height:100vh; justify-content:center; }
  .title { font-family:'Raleway',sans-serif; font-size:2rem; font-weight:700; color:#e2e8f0; margin:0; }
  .hint { font-family:'IBM Plex Mono',monospace; font-size:0.75rem; color:#475569; margin:0; }
  .loading,.empty { font-family:'IBM Plex Mono',monospace; font-size:0.8rem; color:#475569; }
  .error { font-family:'IBM Plex Mono',monospace; font-size:0.8rem; color:#ef4444; }

  .grid { display:grid; grid-template-columns:repeat(auto-fill,minmax(160px,1fr)); gap:0.75rem; max-width:600px; width:100%; }
  .model-card {
    background:#0f172a; border:1px solid #1e293b; border-radius:8px;
    padding:1rem; cursor:pointer; text-align:left;
    transition:border-color 0.2s, box-shadow 0.2s;
  }
  .model-card:hover { border-color:#334155; }
  .model-card.selected { border-color:#ff6600; box-shadow:0 0 16px rgba(255,102,0,0.25); }
  .model-id { font-family:'IBM Plex Mono',monospace; font-size:0.75rem; color:#94a3b8; word-break:break-all; }
  .model-tier { font-family:'IBM Plex Mono',monospace; font-size:0.65rem; margin-top:0.4rem; letter-spacing:0.05em; text-transform:uppercase; }
  .model-badges { display:flex; gap:0.25rem; flex-wrap:wrap; margin-top:0.4rem; }
  .mbadge { font-family:'IBM Plex Mono',monospace; font-size:0.55rem; padding:0.1rem 0.35rem; border-radius:3px; letter-spacing:0.05em; }
  .mbadge.tool { background:#1e3a5f; color:#60a5fa; }
  .mbadge.vision { background:#2d1b4e; color:#c084fc; }
  .mbadge.ctx { background:#1a2e1a; color:#4ade80; }

  .families { display:flex; flex-direction:column; gap:1.25rem; max-width:700px; width:100%; overflow-y:auto; max-height:55vh; }
  .family-group {}
  .family-label { font-family:'Raleway',sans-serif; font-size:0.7rem; font-weight:700; color:#475569; letter-spacing:0.15em; text-transform:uppercase; margin-bottom:0.5rem; }

  .footer { display:flex; gap:1rem; }
  .btn-back { background:transparent; border:1px solid #334155; color:#64748b; padding:0.5rem 1.25rem; border-radius:6px; cursor:pointer; font-family:'IBM Plex Mono',monospace; font-size:0.8rem; }
  .btn-back:hover { color:#94a3b8; }
  .btn-launch { background:#ff6600; border:none; color:#fff; padding:0.5rem 1.5rem; border-radius:6px; cursor:pointer; font-family:'IBM Plex Mono',monospace; font-size:0.8rem; font-weight:600; }
  .btn-launch:disabled { opacity:0.35; cursor:not-allowed; }
</style>
