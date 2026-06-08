<script lang="ts">
  // Copilot conversation rail — left panel.
  // Receives messages from ls.conv; also provides the inline input for
  // operator follow-up messages (wired to real copilot when connected).
  import { ls } from '$lib/lightspace/state.svelte';

  let inputVal = $state('');
  let convEl: HTMLElement | null = null;  // DOM ref — no reactivity needed

  // Auto-scroll when new messages arrive. Reading ls.conv.length inside
  // the effect body registers the dependency naturally.
  $effect(() => {
    if (ls.conv.length > 0 && convEl) convEl.scrollTop = convEl.scrollHeight;
  });

  function sendMessage() {
    if (!inputVal.trim()) return;
    ls.addConv({ who: 'operator', text: inputVal });
    inputVal = '';
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); sendMessage(); }
  }
</script>

<aside class="la-rail">
  <div class="la-rail-head">
    <span class="glyph">◆</span>
    <span class="label">Copilot</span>
    <button class="la-rail-collapse" onclick={() => ls.railCollapsed = !ls.railCollapsed} aria-label="Collapse rail">‹</button>
  </div>

  <!-- Collapsed icon strip — visible only when rail-collapsed -->
  <div class="la-rail-iconstrip">
    <button class="la-rail-icon" title="Expand copilot" onclick={() => ls.railCollapsed = false}>◆</button>
  </div>

  <div class="la-rail-conv" bind:this={convEl}>
    {#each ls.conv as msg, i (i)}
      <div class="la-msg"
           class:is-operator={msg.who === 'operator'}
           class:is-copilot={msg.who === 'copilot'}
           class:is-tool={msg.isTool}
           class:is-corso={msg.agentClass === 'corso'}
           class:is-eva={msg.agentClass === 'eva'}
           class:is-soul={msg.agentClass === 'soul'}
           class:is-quantum={msg.agentClass === 'quantum'}
           class:is-ayin={msg.agentClass === 'ayin'}>
        <div class="la-msg-meta">
          <span class="who {msg.agentClass ?? msg.who}">
            {msg.isTool ? `tool · ${msg.who}` : msg.who}
          </span>
          {#if msg.time}<span class="time">{msg.time}</span>{/if}
        </div>
        <div class="la-msg-body">{msg.text}</div>
        {#if msg.cardLink}
          <div class="la-msg-card-link">{msg.cardLinkLabel ?? 'view card'}</div>
        {/if}
      </div>
    {/each}

    {#if ls.conv.length === 0 && !ls.inLobby}
      <div class="la-msg-typing" aria-label="Copilot is thinking">
        <span></span><span></span><span></span>
      </div>
    {/if}
  </div>

  <div class="la-rail-input-wrap">
    <textarea
      class="la-rail-input"
      bind:value={inputVal}
      onkeydown={onKeydown}
      placeholder="Continue the conversation…"
      rows={2}
    ></textarea>
    <button class="la-rail-send" onclick={sendMessage}>Send</button>
  </div>
</aside>

<style>
.la-rail {
  grid-area: rail;
  display: flex; flex-direction: column;
  background: var(--la-bg-panel);
  border-right: 1px solid var(--la-hair-base);
  overflow: hidden;
  transition: opacity var(--la-slow);
}

.la-rail-head {
  display: flex; align-items: center; gap: 8px;
  padding: 9px 12px; border-bottom: 1px solid var(--la-hair-base);
  font-family: var(--la-font-display); font-weight: 700;
  font-size: 10px; letter-spacing: var(--la-tk-loose);
  color: var(--la-text-bright); text-transform: uppercase;
}
.la-rail-head .glyph { color: var(--la-acc); font-size: 12px; }
.la-rail-collapse {
  margin-left: auto; background: transparent; border: 0;
  color: var(--la-text-mute); cursor: pointer;
  font-family: var(--la-font-mono); font-size: 11px;
  transition: color var(--la-fast);
}
.la-rail-collapse:hover { color: var(--la-text-bright); }

.la-rail-iconstrip {
  display: none; flex-direction: column; gap: 12px;
  padding: 18px 8px; align-items: center;
}
/* Collapsed state controlled by parent .la-root.rail-collapsed via :global */
:global(.la-root.rail-collapsed) .la-rail-head .label,
:global(.la-root.rail-collapsed) .la-rail-conv,
:global(.la-root.rail-collapsed) .la-rail-input-wrap { display: none; }
:global(.la-root.rail-collapsed) .la-rail-iconstrip { display: flex; }

.la-rail-icon {
  width: 28px; height: 28px;
  display: flex; align-items: center; justify-content: center;
  border: 1px solid var(--la-hair-base); background: var(--la-bg-card);
  color: var(--la-text-dim); font-family: var(--la-font-display);
  font-size: 12px; cursor: pointer; border-radius: 2px;
}

.la-rail-conv {
  flex: 1; overflow-y: auto; padding: 12px 12px 8px;
  display: flex; flex-direction: column; gap: 16px;
  scrollbar-width: thin;
}
.la-rail-conv::-webkit-scrollbar { width: 4px; }
.la-rail-conv::-webkit-scrollbar-thumb { background: var(--la-hair-base); }

/* Messages */
.la-msg { display: flex; flex-direction: column; gap: 4px; animation: la-msgin 0.28s ease both; }
@keyframes la-msgin { from { opacity: 0; transform: translateY(4px); } to { opacity: 1; transform: none; } }

.la-msg-meta {
  display: flex; align-items: center; gap: 6px;
  font-size: 8px; text-transform: uppercase; letter-spacing: var(--la-tk-mid);
  color: var(--la-text-mute);
}
.la-msg-meta .who { font-family: var(--la-font-display); font-weight: 700; letter-spacing: var(--la-tk-loose); }
.la-msg-meta .who.copilot  { color: var(--la-acc); }
.la-msg-meta .who.operator { color: var(--la-acc3); }
.la-msg-meta .who.corso    { color: #ff7a7a; }
.la-msg-meta .who.eva      { color: #93f0c2; }
.la-msg-meta .who.soul     { color: #c5a8ff; }
.la-msg-meta .who.ayin     { color: var(--la-warn); }
.la-msg-meta .who.quantum  { color: var(--la-info); }
.la-msg-meta .who.seraph   { color: var(--la-err); }
.la-msg-meta .time { opacity: 0.45; font-feature-settings: 'tnum'; }

.la-msg-body {
  font-size: 11px; line-height: 1.55; color: var(--la-text-base);
  border-left: 1px solid var(--la-hair-base); padding-left: 9px;
}
.la-msg.is-operator .la-msg-body { color: var(--la-text-bright); border-left-color: var(--la-acc3); }
.la-msg.is-copilot  .la-msg-body { border-left-color: var(--la-acc); }
.la-msg.is-tool     .la-msg-body {
  font-size: 10px; background: var(--la-bg-card);
  border: 1px solid var(--la-hair-faint); border-left: 1px solid var(--la-acc);
  padding: 6px 9px; color: var(--la-text-dim);
}
.la-msg.is-corso .la-msg-body  { border-left-color: #ff7a7a; }
.la-msg.is-eva   .la-msg-body  { border-left-color: #93f0c2; }
.la-msg.is-soul  .la-msg-body  { border-left-color: #c5a8ff; }
.la-msg.is-quantum .la-msg-body { border-left-color: var(--la-info); }
.la-msg.is-ayin .la-msg-body   { border-left-color: var(--la-warn); }

.la-msg-card-link {
  margin-top: 4px; font-size: 9px; color: var(--la-acc);
  letter-spacing: var(--la-tk-mid); text-transform: uppercase; cursor: pointer;
}
.la-msg-card-link::before { content: "↳ "; opacity: 0.6; }

.la-msg-typing {
  display: inline-flex; gap: 3px; align-items: center;
  font-style: italic; color: var(--la-text-mute); padding-left: 9px;
}
.la-msg-typing span {
  width: 4px; height: 4px; border-radius: 50%;
  background: var(--la-text-mute); animation: la-dot 1.2s infinite;
}
.la-msg-typing span:nth-child(2) { animation-delay: 0.18s; }
.la-msg-typing span:nth-child(3) { animation-delay: 0.36s; }
@keyframes la-dot { 50% { background: var(--la-text-bright); transform: translateY(-1px); } }

.la-rail-input-wrap {
  border-top: 1px solid var(--la-hair-base); padding: 9px 10px;
  background: var(--la-bg-sunken); display: flex; gap: 6px; align-items: flex-end;
}
.la-rail-input {
  flex: 1; background: var(--la-bg-card); color: var(--la-text-bright);
  border: 1px solid var(--la-hair-base); border-radius: 2px;
  font-family: var(--la-font-mono); font-size: 11px;
  padding: 7px 9px; resize: none; outline: 0;
  letter-spacing: var(--la-tk-tight);
}
.la-rail-input:focus { border-color: var(--la-acc); }
.la-rail-send {
  background: var(--la-acc); border: 0; color: var(--la-bg-base);
  font-family: var(--la-font-mono); font-size: 9px; font-weight: 700;
  padding: 7px 12px; border-radius: 2px; cursor: pointer;
  text-transform: uppercase; letter-spacing: var(--la-tk-mid);
}
</style>
