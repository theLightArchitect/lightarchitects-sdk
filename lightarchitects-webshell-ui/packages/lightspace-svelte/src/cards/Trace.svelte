<!--
  @component Trace — activity stream card.
  @reads content as { entries: Array<{ kind: string; text: string }> }
-->
<script lang="ts">
  let { data }: { data: unknown } = $props();
  type Entry = { kind: string; text: string };
  const entries = $derived(((data as { entries?: Entry[] } | null)?.entries) ?? []);
</script>
<div class="ls-trace-list">
  {#each entries.slice(-20) as e}
    <div class="ls-trace-row ls-trace-{e.kind}">
      <span class="ls-trace-kind">{e.kind}</span>
      <span class="ls-trace-text">{e.text}</span>
    </div>
  {/each}
  {#if entries.length === 0}
    <div class="ls-trace-empty">stream starting…</div>
  {/if}
</div>
<style>
.ls-trace-list { display: flex; flex-direction: column; gap: 3px; max-height: 180px; overflow-y: auto; }
.ls-trace-row  { display: flex; gap: 6px; align-items: baseline; font-size: 10px; }
.ls-trace-kind { font-size: 8px; text-transform: uppercase; color: var(--ls-text-ghost); min-width: 48px; }
.ls-trace-text { color: var(--ls-text); white-space: pre-wrap; word-break: break-word; }
.ls-trace-thinking .ls-trace-kind { color: var(--ls-acc-purple); }
.ls-trace-tool_use .ls-trace-kind { color: var(--ls-acc); }
.ls-trace-result .ls-trace-kind   { color: var(--ls-acc-green); }
.ls-trace-empty { font-size: 9px; color: var(--ls-text-ghost); }
</style>
