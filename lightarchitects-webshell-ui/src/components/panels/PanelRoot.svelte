<script lang="ts">
  import type { PanelTree, PanelId } from '$lib/types';
  import { layoutTree, maximizedPanelId, setLayout, collectPanelIds } from '$lib/layout';
  import AxisNode from './AxisNode.svelte';
  import TabGroupNode from './TabGroupNode.svelte';
  import PanelHeader from './PanelHeader.svelte';
  import PanelHost from './PanelHost.svelte';

  const PANEL_META: Record<PanelId, { label: string; icon: string; color: string }> = {
    'copilot':       { label: 'Copilot',      icon: '◈', color: 'var(--la-struct-primary)' },
    'terminal':      { label: 'Terminal',      icon: '⌨', color: 'var(--la-text-dim)' },
    'git-forest':    { label: 'Git Forest',    icon: '⬡', color: 'var(--la-struct-primary)' },
    'agent-console': { label: 'Agent Console', icon: '◉', color: 'var(--la-agent-researcher)' },
    'file-diff':     { label: 'Diff',          icon: '⊞', color: 'var(--la-agent-quality)' },
    'file-explorer': { label: 'Explorer',      icon: '⊟', color: 'var(--la-text-dim)' },
    'build-status':  { label: 'Build Status',  icon: '◧', color: 'var(--la-agent-security)' },
    'findings':      { label: 'Findings',      icon: '⊛', color: 'var(--la-semantic-warn)' },
    'helix':         { label: 'Helix',         icon: '⬡', color: 'var(--la-struct-accent)' },
    'ayin-traces':   { label: 'AYIN Traces',   icon: '◎', color: 'var(--la-agent-ops, #f97316)' },
  };

  // Track which panels are currently in the tree for visibility management
  let visiblePanels = $derived(collectPanelIds($layoutTree));
  let maxPanel = $derived($maximizedPanelId);

  function removeLeaf(panelId: PanelId) {
    // Simple removal: replace the tree with a filtered copy.
    // If the removed leaf was the only child of a parent, collapse the parent.
    function prune(node: PanelTree): PanelTree | null {
      if (node.type === 'leaf') return node.panelId === panelId ? null : node;
      if (node.type === 'tabgroup') {
        const tabs = node.tabs.filter(t => t !== panelId);
        if (tabs.length === 0) return null;
        return { ...node, tabs, activeIndex: Math.min(node.activeIndex, tabs.length - 1) };
      }
      // axis: prune children, drop null, collapse to single child if needed
      const newChildren: PanelTree[] = [];
      const newFlexes: number[] = [];
      for (let i = 0; i < node.children.length; i++) {
        const pruned = prune(node.children[i]);
        if (pruned !== null) { newChildren.push(pruned); newFlexes.push(node.flexes[i]); }
      }
      if (newChildren.length === 0) return null;
      if (newChildren.length === 1) return newChildren[0];
      return { ...node, children: newChildren, flexes: newFlexes };
    }
    const pruned = prune($layoutTree);
    if (pruned) setLayout(pruned);
  }


</script>

<!-- FLIP maximize overlay — panels behind dimmed when one is maximized -->
<div class="panel-root" class:has-maximized={maxPanel !== null} data-testid="mosaic-root">

  {#snippet renderNode(node: PanelTree, path: number[])}
    {#if node.type === 'axis'}
      <AxisNode {node} {path} {renderNode} />

    {:else if node.type === 'tabgroup'}
      <TabGroupNode
        {node}
        {path}
        renderPanelContent={(panelId, visible) => renderLeafContent(panelId, visible)}
        onClose={removeLeaf}
      />

    {:else if node.type === 'leaf'}
      {@const meta = PANEL_META[node.panelId as PanelId]}
      {#if meta}
      <div
        class="panel-leaf"
        class:is-maximized={maxPanel === node.panelId}
        class:is-dimmed={maxPanel !== null && maxPanel !== node.panelId}
        data-panel-id={node.panelId}
        data-testid="panel-leaf-{node.panelId}"
        inert={maxPanel !== null && maxPanel !== node.panelId ? true : undefined}
      >
        <PanelHeader
          panelId={node.panelId}
          label={meta.label}
          icon={meta.icon}
          color={meta.color}
          onClose={() => removeLeaf(node.panelId)}
        />
        <div class="panel-body">
          <PanelHost panelId={node.panelId} visible={true} />
        </div>
      </div>
      {/if}
    {/if}
  {/snippet}

  {#snippet renderLeafContent(panelId: PanelId, visible: boolean)}
    <PanelHost {panelId} {visible} />
  {/snippet}

  {@render renderNode($layoutTree, [])}
</div>

<style>
  .panel-root {
    display: flex;
    width: 100%;
    height: 100%;
    min-width: 0;
    min-height: 0;
    overflow: hidden;
    position: relative;
  }

  /* ── Leaf panel ── */
  .panel-leaf {
    display: flex;
    flex-direction: column;
    width: 100%;
    height: 100%;
    min-width: 0;
    min-height: 0;
    overflow: hidden;
    border-right: 1px solid var(--la-hair-base, #1e293b);
    position: relative;
    transition: opacity 150ms ease-out, filter 200ms ease-out;
  }

  /* Entrance animation on DOM insertion (drag-to-split creates a new leaf) */
  @starting-style {
    .panel-leaf { opacity: 0; }
  }

  .panel-body {
    flex: 1;
    min-height: 0;
    overflow: hidden;
    display: flex;
  }

  /* Dimmed background panels during maximize */
  .panel-leaf.is-dimmed {
    opacity: 0.12;
    filter: blur(2px) saturate(0.2);
    pointer-events: none;
  }

  /* Maximized panel overlays everything */
  .panel-leaf.is-maximized {
    position: fixed !important;
    inset: 0;
    z-index: var(--z-modal, 100);
    border: none;
    opacity: 1;
    filter: none;
    pointer-events: all;
    background: var(--la-bg-base, #0a0a12);
  }


  /* FLIP maximize animation — :global() required: classes added imperatively via classList.add(),
     not via Svelte class: directive, so the compiler would otherwise tree-shake these rules. */
  :global(.panel-leaf.is-maximizing) {
    animation: panel-expand 220ms cubic-bezier(0.16, 1, 0.3, 1) forwards;
  }
  :global(.panel-leaf.is-restoring) {
    animation: panel-collapse 160ms cubic-bezier(0.55, 0, 1, 0.45) forwards;
  }

  @keyframes panel-expand {
    from {
      transform:
        translate(var(--origin-x, 0), var(--origin-y, 0))
        scaleX(calc(var(--origin-w, 100vw) / 100vw))
        scaleY(calc(var(--origin-h, 100vh) / 100vh));
      transform-origin: top left;
    }
    to { transform: none; }
  }

  @keyframes panel-collapse {
    from { transform: none; opacity: 1; }
    to {
      transform:
        translate(var(--origin-x, 0), var(--origin-y, 0))
        scaleX(calc(var(--origin-w, 100vw) / 100vw))
        scaleY(calc(var(--origin-h, 100vh) / 100vh));
      transform-origin: top left;
      opacity: 0;
    }
  }

</style>
