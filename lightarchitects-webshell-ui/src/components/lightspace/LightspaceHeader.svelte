<!--
  @component LightspaceHeader
  @description Top status bar — build name, run state badge, materialize phase,
    demo/production mode pill. Demo/prod toggle is the primary mode switch.
  @contract none — reads session store; no SSE
  @reads lightspaceSessionStore.buildId, .runStatus, .mode, .materializePhase
  @mutates lightspaceSessionStore.mode (toggle button)
  @api none
  @mockup-ref arch/lightspace-mockup.html → .la-topbar, .la-tb-pill, .la-shipped-badge
-->
<script lang="ts">
  import { lightspaceSessionStore } from '$lib/lightspace-stores';

  const STATUS_COLOR: Record<string, string> = {
    idle:       'var(--ls-text-mute)',
    connecting: 'var(--ls-acc-amber)',
    running:    'var(--ls-acc-green)',
    complete:   'var(--ls-acc-green)',
    error:      'var(--ls-acc-red)',
  };
</script>

<div class="ls-header">
  <!-- Brand -->
  <span class="ls-header-brand">Light<span class="ls-header-brand-acc">space</span></span>

  {#if $lightspaceSessionStore.buildId}
    <span class="ls-header-sep">·</span>
    <span class="ls-header-build">{$lightspaceSessionStore.buildId}</span>
  {/if}

  <!-- Status pill -->
  <div class="ls-header-pills">
    <span
      class="ls-header-pill"
      style="--dot-color: {STATUS_COLOR[$lightspaceSessionStore.runStatus]}"
    >
      <span class="ls-header-dot"></span>
      {$lightspaceSessionStore.runStatus}
    </span>
  </div>

  <!-- Mode toggle -->
  <button
    class="ls-header-mode"
    class:ls-header-mode-demo={$lightspaceSessionStore.mode === 'demo'}
    onclick={() => lightspaceSessionStore.update(s => ({
      ...s, mode: s.mode === 'demo' ? 'production' : 'demo',
    }))}
    title="Toggle demo / production mode"
  >
    {$lightspaceSessionStore.mode}
  </button>
</div>

<style>
.ls-header {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 0 14px;
  height: 38px;
  flex-shrink: 0;
  border-bottom: 1px solid var(--ls-border-base);
  background: linear-gradient(180deg, var(--ls-panel) 0%, var(--ls-bg) 100%);
  font-size: 10px;
  letter-spacing: var(--ls-tk-mid);
  text-transform: uppercase;
  color: var(--ls-text-dim);
}
.ls-header-brand {
  font-family: var(--ls-font-display);
  font-weight: 700;
  font-size: 12px;
  letter-spacing: var(--ls-tk-loose);
  color: var(--ls-text-bright);
}
.ls-header-brand-acc { color: var(--ls-acc); }
.ls-header-sep { color: rgba(255,255,255,0.12); }
.ls-header-build { font-size: 9px; font-family: var(--ls-font-code); color: var(--ls-text-dim); }

.ls-header-pills { flex: 1; display: flex; gap: 8px; justify-content: center; }
.ls-header-pill {
  display: inline-flex; align-items: center; gap: 5px;
  padding: 2px 8px; border: 1px solid var(--ls-border-base);
  font-size: 9px; background: var(--ls-sunken);
}
.ls-header-dot {
  width: 5px; height: 5px; border-radius: 50%;
  background: var(--dot-color, var(--ls-text-mute));
  box-shadow: 0 0 5px var(--dot-color, transparent);
}

.ls-header-mode {
  background: transparent; border: 1px solid var(--ls-border-base);
  color: var(--ls-text-dim); font-family: var(--ls-font-code);
  font-size: 9px; letter-spacing: var(--ls-tk-mid); text-transform: uppercase;
  padding: 3px 8px; cursor: pointer;
  transition: all var(--ls-fast);
}
.ls-header-mode:hover { color: var(--ls-text-bright); border-color: var(--ls-acc); }
.ls-header-mode-demo { color: var(--ls-acc-amber); border-color: rgba(245,166,35,0.3); }
</style>
