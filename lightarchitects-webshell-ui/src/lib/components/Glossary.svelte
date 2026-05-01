<script lang="ts">
  import type { Snippet } from 'svelte';
  import { tip } from '$lib/vocabulary';

  let {
    term,
    children,
  }: {
    term: string;
    children: Snippet;
  } = $props();

  let STORAGE_KEY = $derived(`la.glossary.seen.${term}`);

  let tooltipText = $derived(tip(term));
  function checkSeen(): boolean {
    try {
      return localStorage.getItem(STORAGE_KEY) === '1';
    } catch {
      return true; // localStorage unavailable — skip tooltip
    }
  }

  function markSeen() {
    try {
      localStorage.setItem(STORAGE_KEY, '1');
    } catch { /* ignore */ }
  }

  let seen = $state(checkSeen());
  let visible = $state(false);
  let hoverEl = $state<HTMLSpanElement>();

  function onEnter() {
    if (!tooltipText) return;
    if (!seen) {
      visible = true;
      markSeen();
      seen = true;
    }
  }

  function onLeave() {
    visible = false;
  }
</script>

{#if tooltipText}
  <span
    bind:this={hoverEl}
    class="glossary-wrap"
    class:glossary-first-time={!seen}
    role="presentation"
    onmouseenter={onEnter}
    onmouseleave={onLeave}
    onfocusin={onEnter}
    onfocusout={onLeave}
  >
    {@render children()}

    {#if visible}
      <span class="glossary-tooltip" role="tooltip">
        <span class="glossary-term">{term}</span>
        <span class="glossary-def">{tooltipText}</span>
      </span>
    {/if}
  </span>
{:else}
  {@render children()}
{/if}

<style>
  .glossary-wrap {
    position: relative;
    display: inline;
  }

  /* Subtle underline hint on first-time terms — disappears after seen */
  .glossary-first-time :global(*) {
    text-decoration: underline dotted var(--la-text-mute);
    text-underline-offset: 3px;
    cursor: help;
  }

  .glossary-tooltip {
    position: absolute;
    bottom: calc(100% + 6px);
    left: 0;
    z-index: var(--z-tooltip);
    display: flex;
    flex-direction: column;
    gap: 3px;
    max-width: 240px;
    padding: 7px 10px;
    background: #0d1117;
    border: 1px solid var(--la-hair-strong);
    box-shadow:
      0 4px 14px rgba(0, 0, 0, 0.5),
      0 0 0 1px rgba(255, 215, 0, 0.06);
    white-space: normal;
    pointer-events: none;
    animation: gloss-in 120ms ease-out;
  }

  .glossary-term {
    font-size: 9px;
    font-weight: 700;
    letter-spacing: var(--la-tk-loose);
    text-transform: uppercase;
    color: var(--la-focus-ring);
  }

  .glossary-def {
    font-size: 10px;
    font-weight: 400;
    letter-spacing: var(--la-tk-tight);
    color: var(--la-text-bright);
    line-height: 1.4;
  }

  @keyframes gloss-in {
    from { opacity: 0; transform: translateY(4px); }
    to   { opacity: 1; transform: translateY(0); }
  }
</style>
