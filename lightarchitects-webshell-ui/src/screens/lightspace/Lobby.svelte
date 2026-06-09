<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { ls } from '$lib/lightspace/state.svelte';
  import {
    createConversation,
    sendTurn,
    subscribeConversation,
    fetchRecentSessions,
  } from '$lib/lightspace/conversation.svelte';
  import { sessionAddConvMessage, sessionAppendLastConvMessage } from '$lib/lightspace-stores';

  // Holds the active SSE cleanup function. Called in onDestroy to prevent a
  // subscription leak when the parent unmounts this component mid-flight
  // (e.g. Lightspace.svelte sets ls.inLobby = false via session restore).
  let activeCleanup: (() => void) | null = null;

  onDestroy(() => { activeCleanup?.(); activeCleanup = null; });

  const greeting = $derived(() => {
    const h = new Date().getHours();
    if (h < 12) return 'Good morning';
    if (h < 18) return 'Good afternoon';
    return 'Good evening';
  });

  let focused = $state(false);
  let submitting = $state(false);
  let error = $state<string | null>(null);

  async function submit() {
    if (!ls.lobbyInput.trim() || submitting) return;
    submitting = true;
    error = null;

    const message = ls.lobbyInput;
    ls.intentText = message;

    try {
      // 1. Create the session — mints the UUID that identifies this conversation.
      const sessionId = await createConversation(message);
      ls.sessionId = sessionId;
      localStorage.setItem('la_ls_session_id', sessionId);

      // Add operator message to conversation history immediately.
      sessionAddConvMessage({ id: crypto.randomUUID(), who: 'operator', text: message, ts: Date.now() });

      // 2. Subscribe to the SSE stream BEFORE dispatching the turn so no
      //    events are missed between creation and dispatch.
      let materialized = false;
      let streamingId: string | null = null;
      const cleanup = activeCleanup = subscribeConversation(
        sessionId,
        (ev) => {
          // First event from the backend triggers workspace materialization.
          if (!materialized) {
            materialized = true;
            ls.exitLobby();
            const phases = [
              'begin', 'rail_collapsed', 'grid_revealed',
              'drawer_revealed', 'cards_streaming', 'complete',
            ] as const;
            phases.forEach((p, i) => setTimeout(() => ls.setMatPhase(p), i * 200));
          }
          // Streaming text chunks: Activity { kind: "assistant", summary: chunk }.
          if (ev.type === 'activity' && ev.kind === 'assistant' && ev.summary) {
            if (streamingId === null) {
              streamingId = crypto.randomUUID();
              sessionAddConvMessage({ id: streamingId, who: 'copilot', text: ev.summary, ts: Date.now(), kind: 'assistant' });
            } else {
              sessionAppendLastConvMessage(ev.summary);
            }
          }
          // Turn complete — reset streaming tracker.
          if (ev.type === 'done') {
            streamingId = null;
            activeCleanup = null;
            cleanup();
          }
          // Error in flight — add to conv and reset.
          if (ev.type === 'error' && materialized && ev.message) {
            sessionAddConvMessage({ id: crypto.randomUUID(), who: 'copilot', text: `Error: ${ev.message}`, ts: Date.now(), kind: 'error' });
            streamingId = null;
          }
          // Forward error events as lobby toasts when not yet materialized.
          if (ev.type === 'error' && !materialized) {
            error = ev.message ?? 'An error occurred.';
            submitting = false;
            activeCleanup = null;
            cleanup();
          }
        },
        (errMsg) => {
          if (!materialized) {
            error = errMsg;
            submitting = false;
          }
        },
      );

      // 3. Dispatch the first turn — events arrive on the stream above.
      await sendTurn(sessionId, message);
    } catch (e) {
      error = e instanceof Error ? e.message : 'Could not reach the server.';
      submitting = false;
    }
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); void submit(); }
  }

  onMount(async () => {
    ls.recentSessions = await fetchRecentSessions();
  });
</script>

<div class="la-lobby">
  <div class="la-lobby-inner">
    <h1 class="la-lobby-brand">Light<span class="acc">space</span></h1>
    <p class="la-lobby-tag">The workspace materialises around your intent.</p>
    <p class="la-lobby-greeting">
      {greeting()}, <span class="la-name">Light Architect</span> — what's on your mind?
    </p>

    <div class="la-lobby-input-wrap" class:is-focused={focused}>
      <textarea
        class="la-lobby-input"
        bind:value={ls.lobbyInput}
        onfocus={() => focused = true}
        onblur={() => focused = false}
        onkeydown={onKeydown}
        rows={5}
        autocomplete="off"
        spellcheck="false"
      ></textarea>
      {#if !ls.lobbyInput}
        <span class="la-lobby-placeholder">I want to plan the IntentCanvas SDK feature for the webshell…</span>
      {/if}
      <div class="la-lobby-controls">
        <div class="la-lobby-hints">
          <span class="la-lobby-hint"><kbd>/</kbd> slash command</span>
          <span class="la-lobby-hint"><kbd>↩</kbd> submit</span>
          <span class="la-lobby-hint"><kbd>⇧↩</kbd> newline</span>
        </div>
        <button class="la-lobby-submit" onclick={() => void submit()} disabled={submitting}>
          {submitting ? 'Connecting…' : 'Begin →'}
        </button>
      </div>
    </div>

    {#if error}
      <div class="la-lobby-error" role="alert">{error}</div>
    {/if}

    {#if ls.recentSessions.length > 0}
      <div class="la-lobby-recent">
        <div class="la-lobby-recent-h">Recent sessions · click to resume</div>
        {#each ls.recentSessions as sess}
          <button class="la-lobby-recent-row" onclick={() => { ls.sessionId = sess.id; localStorage.setItem('la_ls_session_id', sess.id); ls.inLobby = false; ls.materializing = false; ls.wsState = 'materialised'; }}>
            <span class="sid">{sess.id}</span>
            <span class="summ">{sess.summary}</span>
            <span class="ago">{sess.ago}</span>
          </button>
        {/each}
      </div>
    {/if}
  </div>
</div>

<style>
.la-lobby {
  position: fixed; inset: 0;
  display: flex; flex-direction: column;
  align-items: center; justify-content: center;
  background: radial-gradient(ellipse at center top, #0e1426 0%, var(--la-bg-base) 70%);
  z-index: 50;
  opacity: 0;
  transition: opacity var(--la-slow);
  pointer-events: none;
}

.la-lobby-inner { max-width: 680px; width: 90vw; }

.la-lobby-brand {
  font-family: var(--la-font-display); font-weight: 800;
  font-size: 64px; letter-spacing: -0.02em;
  color: var(--la-text-bright); text-align: center;
  line-height: 0.9; margin: 0 0 8px;
}
.la-lobby-brand .acc { color: var(--la-acc); font-style: italic; }

.la-lobby-tag {
  font-family: var(--la-font-serif); font-style: italic;
  font-size: 16px; text-align: center;
  color: var(--la-text-dim); letter-spacing: 0; margin: 0 0 48px;
}

.la-lobby-greeting {
  font-family: var(--la-font-serif); font-size: 20px;
  color: var(--la-text-bright); text-align: center;
  margin: 0 0 18px; letter-spacing: 0.01em;
}
.la-lobby-greeting .la-name {
  font-family: var(--la-font-display); font-weight: 700;
  font-style: normal; color: var(--la-acc);
}

.la-lobby-input-wrap {
  position: relative;
  border: 1px solid var(--la-hair-strong);
  background: var(--la-bg-card); border-radius: 3px;
  transition: border-color var(--la-fast), box-shadow var(--la-fast);
}
.la-lobby-input-wrap.is-focused {
  border-color: var(--la-acc);
  box-shadow: 0 0 0 4px rgba(77,142,255,0.08);
}

.la-lobby-input {
  width: 100%; background: transparent; border: 0;
  color: var(--la-text-bright); font-family: var(--la-font-mono);
  font-size: 14px; line-height: 1.55;
  padding: 16px 18px 60px; min-height: 130px;
  outline: 0; letter-spacing: var(--la-tk-tight); resize: none;
}
.la-lobby-placeholder {
  position: absolute; top: 16px; left: 18px;
  color: var(--la-text-ghost); font-family: var(--la-font-mono);
  font-size: 14px; pointer-events: none;
}

.la-lobby-controls {
  position: absolute; left: 14px; right: 14px; bottom: 12px;
  display: flex; justify-content: space-between; align-items: center;
  font-size: 9px; letter-spacing: var(--la-tk-mid); text-transform: uppercase;
  color: var(--la-text-mute);
}
.la-lobby-hints { display: flex; gap: 12px; }
.la-lobby-hint kbd {
  font-family: var(--la-font-mono); background: var(--la-bg-sunken);
  border: 1px solid var(--la-hair-base);
  padding: 1px 5px; border-radius: 2px;
  font-size: 9px; color: var(--la-text-dim);
}
.la-lobby-submit {
  background: var(--la-acc); color: var(--la-bg-base);
  border: 0; font-family: var(--la-font-mono);
  font-size: 9px; font-weight: 700; letter-spacing: var(--la-tk-mid);
  padding: 5px 14px; border-radius: 2px; cursor: pointer; text-transform: uppercase;
}
.la-lobby-submit:hover { background: #6aa3ff; }

.la-lobby-recent {
  margin-top: 28px; display: flex; flex-direction: column; gap: 6px;
  font-size: 10px; letter-spacing: var(--la-tk-mid);
}
.la-lobby-recent-h {
  text-transform: uppercase; color: var(--la-text-mute);
  font-size: 9px; padding-left: 4px; margin-bottom: 2px;
}
.la-lobby-recent-row {
  display: flex; gap: 10px; align-items: center;
  padding: 6px 10px;
  border: 1px solid var(--la-hair-faint);
  background: rgba(255,255,255,0.015);
  transition: all var(--la-fast);
  cursor: pointer; width: 100%;
  font-family: var(--la-font-mono); font-size: 10px;
  color: inherit; text-align: left;
}
.la-lobby-recent-row:hover { border-color: var(--la-acc); background: var(--la-bg-card); }
.la-lobby-recent-row .sid { color: var(--la-text-mute); font-size: 9px; }
.la-lobby-recent-row .summ { flex: 1; color: var(--la-text-base); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.la-lobby-recent-row:hover .summ { color: var(--la-text-bright); }
.la-lobby-recent-row .ago { color: var(--la-text-mute); font-size: 9px; }

.la-lobby-error {
  margin-top: 12px;
  padding: 10px 14px;
  border: 1px solid rgba(255, 90, 90, 0.4);
  background: rgba(255, 60, 60, 0.07);
  border-radius: 3px;
  font-family: var(--la-font-mono);
  font-size: 11px;
  color: #ff7a7a;
  letter-spacing: var(--la-tk-tight);
}

.la-lobby-submit:disabled {
  opacity: 0.6;
  cursor: not-allowed;
}
</style>
