<script lang="ts">
  import type { PanelTree, PanelId } from '$lib/types';
  import { layoutTree, maximizedPanelId, draggingPanelId, setLayout, splitLeaf, collectPanelIds } from '$lib/layout';
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
  };

  // Track which panels are currently in the tree for visibility management
  let visiblePanels = $derived(collectPanelIds($layoutTree));
  let maxPanel = $derived($maximizedPanelId);
  let dragId = $derived($draggingPanelId);

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

  function startDrag(_e: PointerEvent, panelId: PanelId) {
    draggingPanelId.set(panelId);
    document.body.dataset.draggingPanel = panelId;
    function onUp() {
      draggingPanelId.set(null);
      delete document.body.dataset.draggingPanel;
      window.removeEventListener('pointerup', onUp);
      window.removeEventListener('pointercancel', onUp);
    }
    window.addEventListener('pointerup', onUp);
    window.addEventListener('pointercancel', onUp);
  }

  function handleDrop(targetId: PanelId, edge: 'left' | 'right' | 'top' | 'bottom') {
    const src = $draggingPanelId;
    draggingPanelId.set(null);
    if (src && src !== targetId) splitLeaf(targetId, src, edge);
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
        class:is-drag-target={dragId !== null && dragId !== node.panelId}
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
          onDragStart={(e) => startDrag(e, node.panelId)}
        />
        <div class="panel-body">
          <PanelHost panelId={node.panelId} visible={true} />
        </div>

        {#if dragId !== null && dragId !== node.panelId}
          <div class="drop-zones">
            <div class="dz dz-left"   onpointerup={() => handleDrop(node.panelId, 'left')}></div>
            <div class="dz dz-right"  onpointerup={() => handleDrop(node.panelId, 'right')}></div>
            <div class="dz dz-top"    onpointerup={() => handleDrop(node.panelId, 'top')}></div>
            <div class="dz dz-bottom" onpointerup={() => handleDrop(node.panelId, 'bottom')}></div>
          </div>
        {/if}
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

  /* ── Drag-to-split drop zones ── */
  .drop-zones {
    position: absolute;
    inset: 0;
    z-index: 40;
    pointer-events: none; /* individual zones handle pointer events */
  }

  .dz {
    position: absolute;
    pointer-events: all;
    transition: background 80ms;
  }

  /* Inset 10% on cross-axis so corners belong unambiguously to one zone */
  .dz-left  { top: 10%; bottom: 10%; left: 0;   width: 20%;  cursor: col-resize; }
  .dz-right { top: 10%; bottom: 10%; right: 0;  width: 20%;  cursor: col-resize; }
  .dz-top   { left: 10%; right: 10%; top: 0;    height: 20%; cursor: row-resize; }
  .dz-bottom { left: 10%; right: 10%; bottom: 0; height: 20%; cursor: row-resize; }

  /* ::before — always-visible 2px insertion strip (dim at rest, glow on hover) */
  .dz::before {
    content: '';
    position: absolute;
    background: var(--la-struct-primary, #00c8ff);
    opacity: 0.15;
    transition: opacity 80ms, box-shadow 80ms;
  }
  .dz:hover::before {
    opacity: 0.9;
    box-shadow: 0 0 8px rgba(0, 200, 255, 0.6);
  }
  .dz-left::before  { top: 0; bottom: 0; right: 0; width: 2px; }
  .dz-right::before { top: 0; bottom: 0; left: 0;  width: 2px; }
  .dz-top::before   { left: 0; right: 0; bottom: 0; height: 2px; }
  .dz-bottom::before { left: 0; right: 0; top: 0;  height: 2px; }

  /* ::after — directional arrow, centered in zone, appears on hover */
  .dz::after {
    position: absolute;
    top: 50%;
    left: 50%;
    transform: translate(-50%, -50%);
    font-size: 10px;
    font-family: var(--la-font-mono, monospace);
    color: var(--la-struct-primary, #00c8ff);
    opacity: 0;
    transition: opacity 80ms;
    pointer-events: none;
  }
  .dz:hover::after { opacity: 1; }
  .dz-left::after  { content: '←'; }
  .dz-right::after { content: '→'; }
  .dz-top::after   { content: '↑'; }
  .dz-bottom::after { content: '↓'; }

  .dz:hover { background: rgba(0, 200, 255, 0.08); }

  /* Stronger outline on valid drop targets */
  .panel-leaf.is-drag-target {
    outline: 1px solid rgba(0, 200, 255, 0.35);
    outline-offset: -1px;
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

  :global(body[data-dragging-panel] .panel-header) { cursor: crosshair; }
</style>
