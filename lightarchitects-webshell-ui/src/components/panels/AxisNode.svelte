<script lang="ts">
  import type { Snippet } from 'svelte';
  import type { PanelTree } from '$lib/types';
  import { updateFlex } from '$lib/layout';
  import DividerHandle from './DividerHandle.svelte';

  interface Props {
    node: PanelTree & { type: 'axis' };
    path: number[];
    renderNode: Snippet<[PanelTree, number[]]>;
    transitioning?: boolean;
  }

  let { node, path, renderNode, transitioning = false }: Props = $props();

  // Normalize flexes to percentages for CSS flex-basis
  let totalFlex = $derived(node.flexes.reduce((a, b) => a + b, 0));
  let pcts = $derived(node.flexes.map(f => (f / totalFlex) * 100));

  const MIN_SIZE_PX = 120;

  function handleResize(childIndex: number, delta: number) {
    // delta in pixels → convert to flex units relative to container size
    const container = document.querySelector(
      `[data-axis-path="${path.join('-')}"]`,
    ) as HTMLElement | null;
    if (!container) return;

    const containerSize = node.direction === 'row'
      ? container.offsetWidth
      : container.offsetHeight;
    if (containerSize === 0) return;

    // Convert pixel delta to flex delta
    const flexPerPx = totalFlex / containerSize;
    const flexDelta = delta * flexPerPx;

    const newFlexes = [...node.flexes];
    newFlexes[childIndex]     = newFlexes[childIndex] + flexDelta;
    newFlexes[childIndex + 1] = newFlexes[childIndex + 1] - flexDelta;

    // Enforce minimum size (120px in flex units)
    const minFlex = MIN_SIZE_PX * flexPerPx;
    if (newFlexes[childIndex] < minFlex || newFlexes[childIndex + 1] < minFlex) return;

    updateFlex(path, newFlexes);
  }
</script>

<div
  class="axis-node"
  class:transitioning
  data-direction={node.direction}
  data-axis-path={path.join('-')}
>
  {#each node.children as child, i}
    <!-- Child panel -->
    <div
      class="axis-child"
      style:flex-basis="{pcts[i]}%"
    >
      {@render renderNode(child, [...path, i])}
    </div>

    <!-- Divider between children (not after last) -->
    {#if i < node.children.length - 1}
      <DividerHandle
        direction={node.direction}
        onResize={(delta) => handleResize(i, delta)}
      />
    {/if}
  {/each}
</div>

<style>
  .axis-node {
    display: flex;
    width: 100%;
    height: 100%;
    min-width: 0;
    min-height: 0;
    overflow: hidden;
  }

  .axis-node[data-direction="column"] {
    flex-direction: column;
  }

  .axis-child {
    min-width: 0;
    min-height: 0;
    overflow: hidden;
    flex-shrink: 0;
    flex-grow: 0;
    /* Smooth flex-basis transitions when switching presets */
    transition: flex-basis 180ms cubic-bezier(0.22, 1, 0.36, 1);
  }

  /* Disable transition during active drag (would fight the pointer movement) */
  .axis-node:has(:global([data-dragging])) .axis-child,
  .axis-node.transitioning .axis-child {
    transition: none;
  }
</style>
