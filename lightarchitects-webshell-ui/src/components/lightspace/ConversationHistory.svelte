<!--
  @component ConversationHistory
  @description Scrollback list of conversation messages from the copilot session.
  @contract EventType 'copilot_activity' (kind='result'/'assistant') → CopilotActivityEvent
  @reads lightspaceSessionStore.conv (populated by sessionAddConvMessage)
  @mutates none
  @api none — data arrives via SSE through activityFeed → conv store
-->
<script lang="ts">
  import { lightspaceSessionStore } from '$lib/lightspace-stores';

  let listEl: HTMLElement | null = null;

  $effect(() => {
    const _ = $lightspaceSessionStore.conv.length;
    if (listEl) listEl.scrollTop = listEl.scrollHeight;
  });
</script>

<div class="ls-conv" bind:this={listEl}>
  {#each $lightspaceSessionStore.conv as msg (msg.id)}
    <div class="ls-conv-msg ls-conv-{msg.who}">
      <div class="ls-conv-meta">
        <span class="ls-conv-who">{msg.who}</span>
        <span class="ls-conv-ts">{new Date(msg.ts).toLocaleTimeString('en-US', { hour: '2-digit', minute: '2-digit', hour12: false })}</span>
      </div>
      <div class="ls-conv-text">{msg.text}</div>
    </div>
  {/each}

  {#if $lightspaceSessionStore.conv.length === 0}
    <div class="ls-conv-empty">conversation will appear here…</div>
  {/if}
</div>

<style>
.ls-conv {
  flex: 1;
  overflow-y: auto;
  padding: 10px 12px 6px;
  display: flex;
  flex-direction: column;
  gap: 12px;
  scrollbar-width: thin;
}
.ls-conv::-webkit-scrollbar { width: 4px; }
.ls-conv::-webkit-scrollbar-thumb { background: var(--ls-border-base); }

.ls-conv-msg { display: flex; flex-direction: column; gap: 3px; animation: ls-msgin 0.28s ease both; }
@keyframes ls-msgin { from { opacity: 0; transform: translateY(4px); } to { opacity: 1; transform: none; } }

.ls-conv-meta {
  display: flex; align-items: center; gap: 6px;
  font-size: 8px; text-transform: uppercase; letter-spacing: var(--ls-tk-mid);
  color: var(--ls-text-mute);
}
.ls-conv-who { font-family: var(--ls-font-display); font-weight: 700; letter-spacing: var(--ls-tk-loose); }
.ls-conv-operator .ls-conv-who { color: var(--ls-acc-amber); }
.ls-conv-copilot  .ls-conv-who { color: var(--ls-acc); }
.ls-conv-ts { opacity: 0.45; }

.ls-conv-text {
  font-size: 11px;
  line-height: 1.55;
  color: var(--ls-text-base);
  border-left: 1px solid var(--ls-border-base);
  padding-left: 9px;
}
.ls-conv-operator .ls-conv-text { color: var(--ls-text-bright); border-left-color: var(--ls-acc-amber); }
.ls-conv-copilot  .ls-conv-text { border-left-color: var(--ls-acc); }

.ls-conv-empty {
  font-style: italic;
  color: var(--ls-text-ghost);
  font-size: 10px;
  text-align: center;
  padding: 20px 0;
}
</style>
