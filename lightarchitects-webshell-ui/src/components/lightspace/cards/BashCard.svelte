<!--
  @component BashCard
  @description Shell output with exit code + duration.
  @contract EventType 'copilot_activity' (kind='tool_use', raw.name starts 'Bash') → CopilotActivityEvent
  @reads activityFeed (existing store, Bash tool filter)
  @mutates none
  @api none
-->
<script lang="ts">
  let { data }: { data: unknown } = $props();
  const d = $derived(data as { output?: string; exitCode?: number; durationMs?: number } ?? {});
  const ok = $derived(d.exitCode === 0 || d.exitCode === undefined);
</script>
<div class="ls-bash">
  <div class="ls-bash-meta">
    {#if d.exitCode !== undefined}
      <span class="ls-bash-exit" class:ls-bash-ok={ok} class:ls-bash-err={!ok}>
        exit {d.exitCode}
      </span>
    {/if}
    {#if d.durationMs}
      <span class="ls-bash-dur">{(d.durationMs / 1000).toFixed(1)}s</span>
    {/if}
  </div>
  <pre class="ls-bash-output">{d.output ?? ''}</pre>
</div>
<style>
.ls-bash { display: flex; flex-direction: column; gap: 5px; }
.ls-bash-meta { display: flex; gap: 8px; font-size: 8px; text-transform: uppercase; letter-spacing: var(--ls-tk-mid); }
.ls-bash-exit { color: var(--ls-text-mute); }
.ls-bash-ok   { color: var(--ls-acc-green); }
.ls-bash-err  { color: var(--ls-acc-red); }
.ls-bash-dur  { color: var(--ls-text-ghost); }
.ls-bash-output { background: var(--ls-sunken); border: 1px solid var(--ls-border); padding: 6px 8px; font-size: 9px; color: var(--ls-text-base); white-space: pre-wrap; overflow: auto; max-height: 150px; margin: 0; font-family: var(--ls-font-code); }
</style>
