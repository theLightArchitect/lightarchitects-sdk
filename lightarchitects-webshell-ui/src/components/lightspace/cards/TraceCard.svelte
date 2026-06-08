<!--
  @component TraceCard
  @description Decision feed / activity stream from copilot turns.
  @contract EventType 'copilot_activity' (kind ≠ thinking, tool_use) → CopilotActivityEvent
  @reads activityFeed (existing store, filtered source='copilot')
  @mutates none
  @api none
-->
<script lang="ts">
  let { data }: { data: unknown } = $props();
  const d = $derived(data as { entries?: { kind: string; text: string }[] } ?? {});
</script>
<div class="ls-trace-list">
  {#each d.entries ?? [] as entry}
    <div class="ls-trace-row">
      <span class="ls-trace-kind">{entry.kind}</span>
      <span class="ls-trace-text">{entry.text}</span>
    </div>
  {/each}
  {#if !d.entries?.length}
    <span class="ls-trace-empty">activity stream empty</span>
  {/if}
</div>
<style>
.ls-trace-list { display: flex; flex-direction: column; gap: 5px; overflow: hidden; }
.ls-trace-row { display: flex; gap: 8px; font-size: 9px; }
.ls-trace-kind { font-family: var(--ls-font-display); font-weight: 700; font-size: 8px; letter-spacing: var(--ls-tk-loose); color: var(--ls-text-mute); text-transform: uppercase; min-width: 48px; }
.ls-trace-text { color: var(--ls-text-base); line-height: 1.4; word-break: break-word; }
.ls-trace-empty { font-style: italic; color: var(--ls-text-ghost); font-size: 10px; }
</style>
