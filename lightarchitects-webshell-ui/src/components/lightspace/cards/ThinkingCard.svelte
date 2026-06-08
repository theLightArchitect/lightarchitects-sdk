<!--
  @component ThinkingCard
  @description Collapsible Claude reasoning block.
  @contract EventType 'copilot_activity' (kind='thinking') → CopilotActivityEvent
  @reads activityFeed (existing store, kind='thinking' filter)
  @mutates none — expanded state is local
  @api none
-->
<script lang="ts">
  let { data }: { data: unknown } = $props();
  const d = $derived(data as { summary?: string; full?: string } ?? {});
  let expanded = $state(false);
</script>
<div class="ls-thinking">
  <button class="ls-thinking-toggle" onclick={() => expanded = !expanded}>
    {expanded ? '▾' : '▸'} {expanded ? 'collapse' : 'show reasoning'}
  </button>
  {#if expanded}
    <pre class="ls-thinking-body">{d.full ?? d.summary ?? '—'}</pre>
  {:else}
    <p class="ls-thinking-summary">{d.summary ?? 'reasoning in progress…'}</p>
  {/if}
</div>
<style>
.ls-thinking { display: flex; flex-direction: column; gap: 6px; }
.ls-thinking-toggle { background: none; border: none; color: var(--ls-acc); font-size: 9px; cursor: pointer; text-align: left; letter-spacing: var(--ls-tk-mid); text-transform: uppercase; }
.ls-thinking-summary { font-size: 10px; color: var(--ls-text-dim); margin: 0; line-height: 1.5; font-style: italic; }
.ls-thinking-body { font-size: 9px; color: var(--ls-text-base); white-space: pre-wrap; overflow: auto; max-height: 200px; margin: 0; font-family: var(--ls-font-code); }
</style>
