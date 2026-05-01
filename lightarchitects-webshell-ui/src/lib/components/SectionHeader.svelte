<script lang="ts">
  import type { Snippet } from 'svelte';

  let {
    number,
    label,
    metadata,
    children,
  }: {
    number: number | string;
    label: string;
    metadata?: string;
    children?: Snippet;
  } = $props();

  let idx = $derived(
    typeof number === 'number'
      ? `[ ${String(number).padStart(2, '0')} ]`
      : `[ ${number} ]`
  );
</script>

<div class="section-header">
  <span class="section-left">
    <span class="idx">{idx}</span>
    <span class="label">{label}</span>
    {#if metadata}
      <span class="sep">·</span>
      <span class="meta">{metadata}</span>
    {/if}
  </span>
  {#if children}{@render children()}{/if}
</div>

<style>
  .section-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    font-size: 9px;
    font-weight: 700;
    letter-spacing: var(--la-tk-loose);
    color: var(--la-text-dim);
    text-transform: uppercase;
    padding: 6px 16px;
    border-bottom: 1px solid var(--la-hair-faint);
    flex-shrink: 0;
  }
  .section-left {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .idx {
    color: var(--la-text-mute);
    font-weight: 200;
  }
  .sep {
    color: var(--la-text-mute);
  }
  .meta {
    color: var(--la-text-mute);
    font-weight: 400;
    letter-spacing: var(--la-tk-mid);
    text-transform: none;
  }
</style>
