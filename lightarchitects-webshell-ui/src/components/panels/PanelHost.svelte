<script lang="ts">
  import type { PanelId, PanelContext } from '$lib/types';
  import { activePanelContext, panelNavRequest } from '$lib/layout';

  // Lazy-loaded panel components — only the import() calls are listed,
  // actual components are only mounted once and hidden/shown via CSS.
  import GitForest from '$lib/../components/topology/GitForest.svelte';
  import AgentConsole from '$lib/../components/AgentConsole.svelte';
  import Helix3D from '$lib/../components/Helix3D.svelte';
  import CopilotDrawer from '$lib/../components/CopilotDrawer.svelte';
  import DiffPreview from '$lib/../components/DiffPreview.svelte';
  import AyinTracesPanel from '$lib/../components/panels/AyinTracesPanel.svelte';

  interface Props {
    panelId: PanelId;
    visible?: boolean;
  }

  let { panelId, visible = true }: Props = $props();

  // Panel metadata for header + context writes
  const PANEL_META: Record<PanelId, { label: string; icon: string; color: string }> = {
    'copilot':       { label: 'Copilot',        icon: '◈', color: 'var(--la-struct-primary)' },
    'terminal':      { label: 'Terminal',        icon: '⌨', color: 'var(--la-text-dim)' },
    'git-forest':    { label: 'Git Forest',      icon: '⬡', color: 'var(--la-agent-devops, #00c8ff)' },
    'agent-console': { label: 'Agent Console',   icon: '◉', color: 'var(--la-agent-researcher)' },
    'file-diff':     { label: 'Diff',            icon: '⊞', color: 'var(--la-agent-quality)' },
    'file-explorer': { label: 'Explorer',        icon: '⊟', color: 'var(--la-text-dim)' },
    'build-status':  { label: 'Build Status',    icon: '◧', color: 'var(--la-agent-security)' },
    'findings':      { label: 'Findings',        icon: '⊛', color: 'var(--la-semantic-warn)' },
    'helix':         { label: 'Helix',           icon: '⬡', color: 'var(--la-struct-accent)' },
    'ayin-traces':   { label: 'AYIN Traces',     icon: '◎', color: 'var(--la-agent-ops, #f97316)' },
  };

  let meta = $derived(PANEL_META[panelId]);

  function handleFocus() {
    let ctx: PanelContext | null = null;
    if (panelId === 'git-forest') ctx = { type: 'git-forest', repoName: 'lightarchitects-sdk' };
    else if (panelId === 'helix')  ctx = { type: 'helix' };
    else if (panelId === 'terminal') ctx = { type: 'terminal', recentOutput: '' };
    else if (panelId === 'ayin-traces') ctx = { type: 'ayin-traces' };
    activePanelContext.set(ctx);
  }

  // Copilot panel wires itself — only show for non-drawer usage
  let copilotPanelMode = $derived(panelId === 'copilot');
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<!-- Never unmount — visibility controlled by display:none to preserve WebGL/PTY state -->
<div
  class="panel-host"
  role="region"
  aria-label={meta.label}
  style:display={visible ? 'flex' : 'none'}
  onfocus={handleFocus}
  data-panel-id={panelId}
>
  {#if panelId === 'git-forest'}
    <GitForest />

  {:else if panelId === 'agent-console'}
    <AgentConsole />

  {:else if panelId === 'helix'}
    <div class="panel-fill">
      <Helix3D />
    </div>

  {:else if panelId === 'copilot'}
    <!-- Embedded copilot panel (not the bottom drawer) -->
    <div class="panel-fill copilot-embed">
      <CopilotDrawer />
    </div>

  {:else if panelId === 'file-diff'}
    <div class="panel-fill">
      <DiffPreview />
    </div>

  {:else if panelId === 'terminal'}
    <div class="panel-fill panel-stub">
      <span class="stub-icon">⌨</span>
      <span class="stub-label">TERMINAL</span>
      <span class="stub-note">PTY extraction in Phase 2</span>
    </div>

  {:else if panelId === 'file-explorer'}
    <div class="panel-fill panel-stub">
      <span class="stub-icon">⊟</span>
      <span class="stub-label">FILE EXPLORER</span>
      <span class="stub-note">Phase 2</span>
    </div>

  {:else if panelId === 'build-status'}
    <div class="panel-fill panel-stub">
      <span class="stub-icon">◧</span>
      <span class="stub-label">BUILD STATUS</span>
      <span class="stub-note">Phase 2</span>
    </div>

  {:else if panelId === 'findings'}
    <div class="panel-fill panel-stub">
      <span class="stub-icon">⊛</span>
      <span class="stub-label">FINDINGS</span>
      <span class="stub-note">Phase 2</span>
    </div>

  {:else if panelId === 'ayin-traces'}
    <div class="panel-fill">
      <AyinTracesPanel />
    </div>
  {/if}
</div>

<style>
  .panel-host {
    flex-direction: column;
    width: 100%;
    height: 100%;
    min-width: 0;
    min-height: 0;
    overflow: hidden;
    background: var(--la-bg-base, #0a0a12);
  }

  .panel-fill {
    flex: 1;
    min-height: 0;
    min-width: 0;
    overflow: hidden;
  }

  .panel-stub {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 6px;
    opacity: 0.35;
  }
  .stub-icon  { font-size: 24px; color: var(--la-text-mute); }
  .stub-label { font-size: 10px; font-family: var(--la-font-mono); letter-spacing: 0.12em; color: var(--la-text-dim); }
  .stub-note  { font-size: 9px; color: var(--la-text-mute); }

  .copilot-embed {
    display: flex;
    flex-direction: column;
  }
</style>
