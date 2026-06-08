<!--
  @component LeftSidebar
  @description Collapsible left panel containing IntentPanel, LobbyInput, and ConversationHistory.
    Collapses to a narrow icon strip on materialize (rail_collapsed phase).
  @contract none — orchestrator panel; delegates to child components
  @reads lightspaceUiStore.sidebarOpen
  @mutates lightspaceUiStore.sidebarOpen (collapse toggle)
  @api none
  @mockup-ref arch/lightspace-mockup.html → .la-rail, .la-rail-head, .la-rail-collapse
-->
<script lang="ts">
  import { lightspaceUiStore, lightspaceSessionStore } from '$lib/lightspace-stores';
  import IntentPanel from './IntentPanel.svelte';
  import LobbyInput from './LobbyInput.svelte';
  import ConversationHistory from './ConversationHistory.svelte';

  const open = $derived($lightspaceUiStore.sidebarOpen);
</script>

<aside class="ls-sidebar" class:ls-sidebar-open={open} class:ls-sidebar-collapsed={!open}>
  <div class="ls-sidebar-head">
    <span class="ls-sidebar-glyph">◆</span>
    {#if open}
      <span class="ls-sidebar-title">Copilot</span>
    {/if}
    <button
      class="ls-sidebar-collapse"
      onclick={() => lightspaceUiStore.update(s => ({ ...s, sidebarOpen: !s.sidebarOpen }))}
      aria-label={open ? 'Collapse sidebar' : 'Expand sidebar'}
    >{open ? '‹' : '›'}</button>
  </div>

  {#if open}
    <IntentPanel />
    <ConversationHistory />
    <LobbyInput />
  {:else}
    <div class="ls-sidebar-icons">
      <div class="ls-sidebar-icon" title="Expand">◆</div>
    </div>
  {/if}
</aside>

<style>
.ls-sidebar {
  display: flex;
  flex-direction: column;
  background: var(--ls-panel);
  border-right: 1px solid var(--ls-border-base);
  overflow: hidden;
  transition: width var(--ls-mid), opacity var(--ls-slow);
}
.ls-sidebar-open    { width: 280px; }
.ls-sidebar-collapsed { width: 48px; }

.ls-sidebar-head {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 9px 12px;
  border-bottom: 1px solid var(--ls-border-base);
  font-family: var(--ls-font-display);
  font-weight: 700;
  font-size: 10px;
  letter-spacing: var(--ls-tk-loose);
  color: var(--ls-text-bright);
  text-transform: uppercase;
}
.ls-sidebar-glyph { color: var(--ls-acc); font-size: 12px; }
.ls-sidebar-title { flex: 1; }
.ls-sidebar-collapse {
  background: transparent; border: 0;
  color: var(--ls-text-mute); cursor: pointer;
  font-size: 11px; padding: 0;
  transition: color var(--ls-fast);
}
.ls-sidebar-collapse:hover { color: var(--ls-text-bright); }

.ls-sidebar-icons {
  display: flex; flex-direction: column;
  gap: 12px; padding: 18px 8px; align-items: center;
}
.ls-sidebar-icon {
  width: 28px; height: 28px;
  display: flex; align-items: center; justify-content: center;
  border: 1px solid var(--ls-border-base); background: var(--ls-card);
  color: var(--ls-text-dim); font-size: 12px; cursor: pointer;
}
</style>
