<!--
  @component ToolCallCard
  @description Tool invocation + result display.
  @contract EventType 'copilot_activity' (kind='tool_use') → CopilotActivityEvent
  @reads activityFeed (existing store, kind='tool_use' filter, non-Bash)
  @mutates none
  @api none
-->
<script lang="ts">
  let { data }: { data: unknown } = $props();
  const d = $derived(data as { name?: string; args?: string; result?: string } ?? {});
</script>
<div class="ls-tool">
  <div class="ls-tool-name">{d.name ?? 'tool_use'}</div>
  {#if d.args}
    <pre class="ls-tool-args">{d.args}</pre>
  {/if}
  {#if d.result}
    <div class="ls-tool-result-label">result</div>
    <pre class="ls-tool-result">{d.result}</pre>
  {/if}
</div>
<style>
.ls-tool { display: flex; flex-direction: column; gap: 5px; }
.ls-tool-name { font-family: var(--ls-font-display); font-weight: 700; font-size: 10px; color: var(--ls-acc-amber); letter-spacing: var(--ls-tk-tight); }
.ls-tool-args, .ls-tool-result { background: var(--ls-sunken); border: 1px solid var(--ls-border); padding: 5px 7px; font-size: 9px; color: var(--ls-text-dim); white-space: pre-wrap; overflow: auto; max-height: 100px; margin: 0; font-family: var(--ls-font-code); }
.ls-tool-result-label { font-size: 7px; text-transform: uppercase; letter-spacing: var(--ls-tk-loose); color: var(--ls-text-mute); }
</style>
