<!--
  @component LobbyInput
  @description Full-screen lobby textarea on initial state; inline input in rail during session.
    On submit → POST /api/lightshell/runs (production) or starts TIMELINE (demo).
  @contract none — submit handled by parent Lightspace.svelte; input stores lobbyInput
  @reads lightspaceSessionStore.lobbyInput, .mode, .runStatus
  @mutates lightspaceSessionStore.lobbyInput (bind:value), .runStatus on submit
  @api POST /api/lightshell/runs (production mode)
-->
<script lang="ts">
  import { lightspaceSessionStore } from '$lib/lightspace-stores';

  function handleKey(e: KeyboardEvent) {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      // Parent Lightspace.svelte hooks the submit event via custom event dispatch
      document.dispatchEvent(new CustomEvent('ls:lobby-submit', {
        detail: { intent: $lightspaceSessionStore.lobbyInput },
      }));
    }
  }
</script>

<div class="ls-lobby-input-wrap">
  <textarea
    class="ls-lobby-input"
    bind:value={$lightspaceSessionStore.lobbyInput}
    onkeydown={handleKey}
    placeholder="type intent, or /command…"
    rows={3}
    disabled={$lightspaceSessionStore.runStatus === 'running'}
    spellcheck="false"
    autocomplete="off"
  ></textarea>
  <div class="ls-lobby-hints">
    <span><kbd>/</kbd> cmd</span>
    <span><kbd>↩</kbd> submit</span>
    <span><kbd>⇧↩</kbd> newline</span>
  </div>
</div>

<style>
.ls-lobby-input-wrap {
  border-top: 1px solid var(--ls-border-base);
  padding: 9px 10px;
  background: var(--ls-sunken);
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.ls-lobby-input {
  background: var(--ls-card);
  color: var(--ls-text-bright);
  border: 1px solid var(--ls-border-base);
  font-family: var(--ls-font-code);
  font-size: 11px;
  line-height: 1.55;
  padding: 7px 9px;
  resize: none;
  outline: 0;
  width: 100%;
  letter-spacing: var(--ls-tk-tight);
}
.ls-lobby-input:focus { border-color: var(--ls-acc); }
.ls-lobby-input:disabled { opacity: 0.5; cursor: not-allowed; }
.ls-lobby-hints {
  display: flex;
  gap: 10px;
  font-size: 8px;
  text-transform: uppercase;
  letter-spacing: var(--ls-tk-mid);
  color: var(--ls-text-mute);
}
.ls-lobby-hints kbd {
  background: var(--ls-sunken);
  border: 1px solid var(--ls-border-base);
  padding: 1px 4px;
  font-size: 8px;
  color: var(--ls-text-dim);
  font-family: var(--ls-font-code);
}
</style>
