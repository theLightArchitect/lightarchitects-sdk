<!--
  @component IntentPanel
  @description Agenda/goals display — shows the operator's stated intent for the session.
  @contract none — reads session intent; no SSE
  @reads lightspaceSessionStore.intent
  @mutates none
  @api none
-->
<script lang="ts">
  import { lightspaceSessionStore } from '$lib/lightspace-stores';
</script>

{#if $lightspaceSessionStore.intent}
  <div class="ls-intent-panel">
    <div class="ls-intent-label">intent</div>
    <p class="ls-intent-text">{$lightspaceSessionStore.intent}</p>
  </div>
{/if}

<style>
.ls-intent-panel {
  padding: 10px 12px;
  border-bottom: 1px solid var(--ls-border);
}
.ls-intent-label {
  font-size: 8px;
  text-transform: uppercase;
  letter-spacing: var(--ls-tk-loose);
  color: var(--ls-text-mute);
  margin-bottom: 4px;
}
.ls-intent-text {
  font-size: 11px;
  color: var(--ls-text-bright);
  line-height: 1.5;
  margin: 0;
  word-break: break-word;
}
</style>
