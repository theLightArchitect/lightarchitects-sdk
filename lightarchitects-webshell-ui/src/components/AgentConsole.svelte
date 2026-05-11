<script lang="ts">
  import { AgentWS } from '$lib/ws';
  import type { AgentEvent } from '$lib/types';
  import { agentConnected, agentEvents, agentInput, isNativeAgent, currentBuildId } from '$lib/stores';

  // DOM ref for the scrolling message panel
  let scrollEl: HTMLDivElement | null = $state(null);

  // Local reactive copies for rendering
  let connected = $state(false);
  let events: AgentEvent[] = $state([]);
  let inputText = $state('');
  let buildId = $state<string | null>(null);
  let ws: AgentWS | null = $state(null);

  // Mirror store values into local state (Svelte 5 runes-friendly)
  $effect(() => {
    connected = $agentConnected;
    events = $agentEvents;
    inputText = $agentInput;
    buildId = $currentBuildId;
  });

  // Auto-scroll to bottom on new events
  $effect(() => {
    if (scrollEl && events.length > 0) {
      scrollEl.scrollTop = scrollEl.scrollHeight;
    }
  });

  // Wire AgentWS when this component mounts and a native build is active
  $effect(() => {
    if (!buildId || !$isNativeAgent) {
      ws?.disconnect();
      ws = null;
      agentConnected.set(false);
      return;
    }

    const instance = new AgentWS(
      buildId,
      (ev: AgentEvent) => {
        agentEvents.update((arr) => [...arr, ev]);
      },
      () => { agentConnected.set(true); },
      () => { agentConnected.set(false); },
    );
    instance.connect();
    ws = instance;

    return () => {
      instance.disconnect();
      ws = null;
      agentConnected.set(false);
    };
  });

  function sendMessage() {
    const text = inputText.trim();
    if (!text || !ws?.connected) return;
    ws.sendMessage(text);
    agentInput.set('');
  }

  function sendInterrupt() {
    ws?.sendInterrupt();
  }

  function sendSteer() {
    const text = inputText.trim();
    if (!text || !ws?.connected) return;
    ws.sendSteer(text);
    agentInput.set('');
  }

  function handleKey(e: KeyboardEvent) {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      sendMessage();
    }
  }

  /** Summarise token usage from events for the footer. */
  let tokenUsage = $derived(() => {
    let inp = 0, out = 0;
    for (const ev of events) {
      if (ev.type === 'token_usage') { inp += ev.input; out += ev.output; }
    }
    return { input: inp, output: out };
  });

  /** True when the agent is still running (no complete/error terminal event). */
  let isRunning = $derived(() => {
    for (const ev of events) {
      if (ev.type === 'complete' || (ev.type === 'error' && !ev.recoverable)) return false;
    }
    return events.length > 0;
  });
</script>

<div class="agent-console" data-testid="agent-console">
  <!-- Header -->
  <div class="console-head">
    <span class="console-label">AGENT BRIDGE</span>
    <span class="console-status" class:connected>
      {connected ? 'CONNECTED' : 'OFFLINE'}
    </span>
  </div>

  <!-- Message stream -->
  <div class="console-body" bind:this={scrollEl}>
    {#if events.length === 0}
      <div class="console-empty">— send a message to start the agent —</div>
    {:else}
      {#each events as ev, i (i)}
        {#if ev.type === 'text'}
          <div class="msg msg-text">{ev.chunk}</div>
        {:else if ev.type === 'thinking'}
          <div class="msg msg-thinking">💭 {ev.content}</div>
        {:else if ev.type === 'tool_start'}
          <div class="msg msg-tool">
            🔧 <strong>{ev.name}</strong>
            <code>{JSON.stringify(ev.input)}</code>
          </div>
        {:else if ev.type === 'tool_complete'}
          <div class="msg msg-tool" class:success={ev.success} class:error={!ev.success}>
            ✅ <strong>{ev.id}</strong> {ev.duration_ms}ms
            {#if ev.result}<pre>{ev.result}</pre>{/if}
          </div>
        {:else if ev.type === 'status_update'}
          <div class="msg msg-status">⏳ {ev.text}</div>
        {:else if ev.type === 'error'}
          <div class="msg msg-error" class:recoverable={ev.recoverable}>
            ⚠️ {ev.message}
          </div>
        {:else if ev.type === 'complete'}
          <div class="msg msg-complete">
            🏁 {ev.reason.kind}{#if ev.reason.message}: {ev.reason.message}{/if}
          </div>
        {:else if ev.type === 'token_usage'}
          <div class="msg msg-meta">
            📊 tokens {ev.input} → {ev.output}
          </div>
        {/if}
      {/each}
    {/if}
  </div>

  <!-- Footer meta -->
  <div class="console-foot">
    <span class="foot-meta">
      {#if tokenUsage().input > 0}
        {tokenUsage().input} → {tokenUsage().output} tokens
      {/if}
    </span>
    <span class="foot-meta">{events.length} events</span>
  </div>

  <!-- Input bar -->
  <div class="console-input-bar">
    <textarea
      class="console-textarea"
      placeholder={connected ? 'Type a message…' : 'Agent offline'}
      disabled={!connected}
      bind:value={inputText}
      onkeydown={handleKey}
    ></textarea>
    <div class="input-actions">
      <button
        class="btn btn-send"
        disabled={!connected || !inputText.trim()}
        onclick={sendMessage}
      >
        SEND
      </button>
      <button
        class="btn btn-steer"
        disabled={!connected || !inputText.trim()}
        onclick={sendSteer}
      >
        STEER
      </button>
      <button
        class="btn btn-interrupt"
        disabled={!connected || !isRunning()}
        onclick={sendInterrupt}
      >
        STOP
      </button>
    </div>
  </div>
</div>

<style>
  .agent-console {
    display: flex;
    flex-direction: column;
    height: 100%;
    overflow: hidden;
    font-family: monospace;
    font-size: 11px;
    line-height: 1.45;
  }

  .console-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 6px 10px;
    border-bottom: 1px solid var(--la-hair-faint);
    flex-shrink: 0;
  }

  .console-label {
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.14em;
    text-transform: uppercase;
    color: var(--la-text-mute);
  }

  .console-status {
    font-size: 9px;
    font-weight: 700;
    color: #ef4444;
    letter-spacing: 0.08em;
  }
  .console-status.connected {
    color: #22c55e;
  }

  .console-body {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 8px 10px;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .console-empty {
    color: var(--la-text-mute);
    font-style: italic;
    text-align: center;
    margin-top: 20px;
  }

  .msg {
    padding: 3px 6px;
    border-radius: 3px;
  }
  .msg-text { background: #1a1a2e; color: #e2e8f0; }
  .msg-thinking { color: #94a3b8; font-style: italic; }
  .msg-tool { background: #0f172a; color: #cbd5e1; }
  .msg-tool.success { border-left: 2px solid #22c55e; }
  .msg-tool.error { border-left: 2px solid #ef4444; }
  .msg-status { color: #f59e0b; font-weight: 600; }
  .msg-error { color: #ef4444; background: #2a0a0a; }
  .msg-error.recoverable { color: #f59e0b; background: #2a1a0a; }
  .msg-complete { color: #22c55e; font-weight: 700; }
  .msg-meta { color: #64748b; font-size: 10px; }

  .console-foot {
    display: flex;
    justify-content: space-between;
    padding: 4px 10px;
    border-top: 1px solid var(--la-hair-faint);
    flex-shrink: 0;
    font-size: 10px;
    color: var(--la-text-mute);
  }

  .console-input-bar {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 8px 10px;
    border-top: 1px solid var(--la-hair-faint);
    flex-shrink: 0;
  }

  .console-textarea {
    background: var(--la-bg-base);
    color: var(--la-text-main);
    border: 1px solid var(--la-hair-strong);
    border-radius: 4px;
    padding: 6px 8px;
    font-family: inherit;
    font-size: 11px;
    line-height: 1.4;
    resize: none;
    min-height: 48px;
    max-height: 120px;
  }
  .console-textarea:focus {
    outline: none;
    border-color: var(--la-accent);
  }
  .console-textarea:disabled {
    opacity: 0.5;
  }

  .input-actions {
    display: flex;
    gap: 6px;
    justify-content: flex-end;
  }

  .btn {
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.1em;
    padding: 4px 10px;
    border-radius: 3px;
    border: 1px solid var(--la-hair-strong);
    background: var(--la-bg-base);
    color: var(--la-text-main);
    cursor: pointer;
  }
  .btn:hover:not(:disabled) {
    background: var(--la-bg-raised);
  }
  .btn:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }

  .btn-send {
    background: #1e3a5f;
    border-color: #2563eb;
    color: #bfdbfe;
  }
  .btn-steer {
    background: #3f2e18;
    border-color: #d97706;
    color: #fde68a;
  }
  .btn-interrupt {
    background: #4a1818;
    border-color: #dc2626;
    color: #fecaca;
  }
</style>
