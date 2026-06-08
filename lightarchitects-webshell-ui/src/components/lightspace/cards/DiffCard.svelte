<!--
  @component DiffCard
  @description Unified diff view with +/- colored lines.
  @contract EventType 'impl_complete' → ImplCompleteEvent
  @reads implCompleteEvents (existing store)
  @mutates none
  @api none
-->
<script lang="ts">
  import type { DiffEntry } from '$lib/lightspace-types';
  let { data }: { data: unknown } = $props();
  const d = $derived(data as { entries?: DiffEntry[]; file?: string; stats?: string } ?? {});
</script>
<div class="ls-diff">
  {#if d.file}
    <div class="ls-diff-file">{d.file}</div>
  {/if}
  {#if d.stats}
    <div class="ls-diff-stats">{d.stats}</div>
  {/if}
  <div class="ls-diff-lines">
    {#each d.entries ?? [] as entry}
      <div class="ls-diff-line ls-diff-{entry.lineType}">{entry.content}</div>
    {/each}
  </div>
</div>
<style>
.ls-diff { display: flex; flex-direction: column; gap: 4px; overflow: hidden; }
.ls-diff-file { font-size: 9px; color: var(--ls-acc); font-family: var(--ls-font-code); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.ls-diff-stats { font-size: 8px; color: var(--ls-text-mute); }
.ls-diff-lines { overflow: auto; max-height: 180px; font-family: var(--ls-font-code); font-size: 9px; }
.ls-diff-line { padding: 1px 4px; white-space: pre; }
.ls-diff-add     { background: rgba(62,207,142,0.12); color: var(--ls-acc-green); }
.ls-diff-remove  { background: rgba(224,92,92,0.12);  color: var(--ls-acc-red); }
.ls-diff-context { color: var(--ls-text-mute); }
</style>
