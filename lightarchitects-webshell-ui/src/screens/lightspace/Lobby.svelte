<script lang="ts">
  import { ls } from '$lib/lightspace/state.svelte';
  import { goto } from '$app/navigation';

  const greeting = $derived(() => {
    const h = new Date().getHours();
    if (h < 12) return 'Good morning';
    if (h < 18) return 'Good afternoon';
    return 'Good evening';
  });

  let focused = $state(false);

  function submit() {
    if (!ls.lobbyInput.trim()) return;
    ls.intentText = ls.lobbyInput;
    ls.exitLobby();
    const phases = ['begin', 'rail_collapsed', 'grid_revealed', 'drawer_revealed', 'cards_streaming', 'complete'] as const;
    phases.forEach((p, i) => setTimeout(() => ls.setMatPhase(p), i * 200));
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); submit(); }
  }
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
        <button class="la-lobby-submit" onclick={submit}>Begin →</button>
      </div>
    </div>

    {#if ls.recentSessions.length > 0}
      <div class="la-lobby-recent">
        <div class="la-lobby-recent-h">Recent sessions · click to resume</div>
        {#each ls.recentSessions as sess}
          <button class="la-lobby-recent-row" onclick={() => goto('/builds')}>
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
</style>
