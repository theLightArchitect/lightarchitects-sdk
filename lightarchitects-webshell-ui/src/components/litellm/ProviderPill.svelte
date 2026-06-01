<script lang="ts">
  import { providerConfig, loadProvider } from '$lib/providerStore';

  let { onclick }: { onclick: () => void } = $props();

  $effect(() => {
    if ($providerConfig === null) void loadProvider();
  });

  let display = $derived(() => {
    const cfg = $providerConfig;
    if (cfg === null) return { vendor: null, model: '…', configured: false };
    if (!cfg?.model) return { vendor: null, model: 'no provider', configured: false };
    const slash = cfg.model.indexOf('/');
    if (slash !== -1) {
      return {
        vendor: cfg.model.slice(0, slash),
        model: cfg.model.slice(slash + 1, slash + 22),
        configured: !!(cfg.has_key && cfg.model),
      };
    }
    return { vendor: null, model: cfg.model.slice(0, 22), configured: !!(cfg.has_key && cfg.model) };
  });
</script>

<button
  class="hdr-provider-pill"
  class:hdr-provider-pill--live={display().configured}
  {onclick}
  title="Provider: {$providerConfig?.model ?? 'not configured'} · click to configure"
  data-testid="provider-pill"
>
  <span class="pp-dot" class:pp-dot--live={display().configured}></span>
  <span class="pp-text">
    {#if display().vendor}
      <span class="pp-vendor">{display().vendor}</span><span class="pp-slash">/</span>
    {/if}<span class="pp-model">{display().model}</span>
  </span>
</button>

<style>
  .hdr-provider-pill {
    display: flex;
    align-items: center;
    gap: 6px;
    height: 100%;
    padding: 0 10px 0 9px;
    font-family: var(--la-font-mono, monospace);
    font-size: 9px;
    letter-spacing: 0.04em;
    color: var(--la-text-dim, #6b7280);
    background: transparent;
    border: none;
    border-left: 1px solid var(--la-drawer-border, #1c2028);
    cursor: pointer;
    white-space: nowrap;
    transition: background 0.12s, color 0.12s;
  }

  .hdr-provider-pill--live {
    color: var(--la-focus-ring, #FFD700);
  }

  .hdr-provider-pill:hover {
    background: color-mix(in srgb, var(--la-focus-ring, #FFD700) 6%, transparent);
    color: var(--la-focus-ring, #FFD700);
  }

  .pp-dot {
    width: 5px;
    height: 5px;
    border-radius: 50%;
    background: var(--la-text-dim, #6b7280);
    flex-shrink: 0;
    transition: background 0.3s;
  }

  .pp-dot--live {
    background: #22c55e;
    animation: dot-pulse 2.8s ease-in-out infinite;
  }

  @keyframes dot-pulse {
    0%, 100% { box-shadow: 0 0 0 0 rgba(34, 197, 94, 0.5); }
    50%       { box-shadow: 0 0 0 3.5px rgba(34, 197, 94, 0); }
  }

  .pp-text {
    display: flex;
    align-items: baseline;
  }

  .pp-vendor {
    font-size: 8px;
    opacity: 0.5;
  }

  .pp-slash {
    font-size: 8px;
    opacity: 0.3;
    margin: 0 0.5px;
  }

  .hdr-provider-pill--live .pp-vendor,
  .hdr-provider-pill--live .pp-slash {
    color: inherit;
  }
</style>
