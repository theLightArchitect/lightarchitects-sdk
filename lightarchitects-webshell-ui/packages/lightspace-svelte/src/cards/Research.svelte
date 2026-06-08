<!--
  @component Research — citation fragment card.
  @reads content as { summary: string; sources?: string[]; confidence?: number }
-->
<script lang="ts">
  let { data }: { data: unknown } = $props();
  type D = { summary?: string; sources?: string[]; confidence?: number };
  const d = $derived((data as D | null) ?? {});
</script>
<div class="ls-res-wrap">
  <p class="ls-res-summary">{d.summary ?? '—'}</p>
  {#if (d.sources?.length ?? 0) > 0}
    <ul class="ls-res-sources">
      {#each (d.sources ?? []) as src}
        <li class="ls-res-source">{src}</li>
      {/each}
    </ul>
  {/if}
  {#if d.confidence !== undefined}
    <div class="ls-res-conf">confidence: {Math.round(d.confidence * 100)}%</div>
  {/if}
</div>
<style>
.ls-res-wrap    { display: flex; flex-direction: column; gap: 5px; }
.ls-res-summary { font-size: 10px; color: var(--ls-text); margin: 0; }
.ls-res-sources { margin: 0; padding: 0 0 0 12px; }
.ls-res-source  { font-size: 8px; color: var(--ls-text-dim); list-style: disc; }
.ls-res-conf    { font-size: 8px; color: var(--ls-acc-green); text-align: right; }
</style>
