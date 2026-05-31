<script lang="ts">
  import { authHeaders } from '$lib/auth';

  let { onclick }: { onclick: () => void } = $props();

  let model = $state('');
  let hasKey = $state(false);
  let loading = $state(true);

  async function refresh() {
    try {
      const resp = await fetch('/api/litellm/config', { headers: authHeaders() });
      if (!resp.ok) return;
      const data: { model: string; has_key: boolean } = await resp.json();
      model = data.model;
      hasKey = data.has_key;
    } catch { /* non-fatal */ } finally {
      loading = false;
    }
  }

  $effect(() => { void refresh(); });

  // External refresh trigger — parent dispatches `la:litellm-config-saved` after a save.
  $effect(() => {
    const handler = () => { void refresh(); };
    window.addEventListener('la:litellm-config-saved', handler);
    return () => window.removeEventListener('la:litellm-config-saved', handler);
  });

  // Truncate "anthropic/claude-opus-4-7" → "claude-opus-4-7" (drop prefix)
  let displayModel = $derived(
    loading ? '…'
    : !model ? 'no provider'
    : model.includes('/') ? model.split('/').slice(1).join('/').slice(0, 18)
    : model.slice(0, 18)
  );
</script>

<button
  class="hdr-provider-pill {!hasKey || !model ? 'hdr-provider-pill--uncfg' : ''}"
  {onclick}
  title="LiteLLM provider: {model || 'not configured'} · click to switch"
  data-testid="provider-pill"
>
  <span class="provider-dot" class:provider-dot--ok={hasKey && !!model}></span>
  {displayModel}
</button>

<style>
  .hdr-provider-pill {
    display: flex;
    align-items: center;
    gap: 4px;
    height: 100%;
    padding: 0 8px;
    font-family: var(--la-font-mono, monospace);
    font-size: 9px;
    letter-spacing: 0.04em;
    color: var(--la-focus-ring, #FFD700);
    background: transparent;
    border: none;
    border-left: 1px solid var(--la-drawer-border, #1c2028);
    cursor: pointer;
    white-space: nowrap;
    transition: background 0.12s, color 0.12s;
  }
  .hdr-provider-pill:hover {
    background: rgba(255, 215, 0, 0.06);
  }
  .hdr-provider-pill--uncfg {
    color: var(--la-text-dim, #6b7280);
  }
  .hdr-provider-pill--uncfg:hover {
    color: var(--la-agent-performance, #f59e0b);
  }
  .provider-dot {
    width: 5px;
    height: 5px;
    border-radius: 50%;
    background: var(--la-text-dim, #6b7280);
    flex-shrink: 0;
  }
  .provider-dot--ok {
    background: #22c55e;
    box-shadow: 0 0 4px #22c55e80;
  }
</style>
