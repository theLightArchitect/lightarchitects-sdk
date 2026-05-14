<script lang="ts">
  import { contextUsage } from '$lib/stores';

  const LEVEL_COLORS: Record<string, string> = {
    l1: '#facc15',  // yellow-400
    l2: '#f97316',  // orange-500
    l3: '#ef4444',  // red-500
  };

  $: usage = $contextUsage;
  $: pct = usage ? Math.min(100, usage.usage_pct * 100) : 0;
  $: color = usage?.level ? (LEVEL_COLORS[usage.level] ?? '#22d3ee') : '#22d3ee';
  $: label = usage
    ? `${Math.round(pct)}% · ${(usage.used / 1000).toFixed(1)}k / ${(usage.budget / 1000).toFixed(0)}k tokens${usage.level ? ` · ${usage.level.toUpperCase()} compacting` : ''}`
    : null;
</script>

{#if usage}
  <div
    class="context-bar shrink-0"
    data-testid="context-bar"
    title={label ?? ''}
    aria-label={label ?? 'Context usage'}
    role="progressbar"
    aria-valuenow={pct}
    aria-valuemin={0}
    aria-valuemax={100}
  >
    <div class="context-bar__track">
      <div
        class="context-bar__fill"
        style="width: {pct}%; background: {color};"
      ></div>
    </div>
    {#if usage.level}
      <span class="context-bar__badge" style="color: {color};">{usage.level.toUpperCase()}</span>
    {/if}
  </div>
{/if}

<style>
  .context-bar {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 2px 8px;
    background: var(--la-bg-frame);
    border-bottom: 1px solid var(--la-drawer-border);
    height: 14px;
  }

  .context-bar__track {
    flex: 1;
    height: 3px;
    background: rgba(255, 255, 255, 0.08);
    border-radius: 2px;
    overflow: hidden;
  }

  .context-bar__fill {
    height: 100%;
    border-radius: 2px;
    transition: width 0.6s ease-out, background 0.3s;
  }

  .context-bar__badge {
    font-size: 8px;
    font-weight: 700;
    letter-spacing: 0.05em;
    opacity: 0.85;
    flex-shrink: 0;
  }
</style>
