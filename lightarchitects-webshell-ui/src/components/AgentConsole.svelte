<script lang="ts">
  import { AgentWS } from '$lib/ws';
  import type { AgentEvent } from '$lib/types';
  import { agentConnected, agentEvents, agentInput, isNativeAgent, currentBuildId } from '$lib/stores';
  import PolytopeNode from './PolytopeNode.svelte';

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

  // ── HITL permission requests ─────────────────────────────────────────────

  interface PendingPermission {
    requestId: string;
    tool: string;
    summary: string;
    deadline: number; // unix ms — auto-deny when now >= deadline
    timeoutSecs: number;
  }

  let pendingPermissions = $state<PendingPermission[]>([]);
  let now = $state(Date.now());

  // 1-second ticker: update countdown display + auto-deny expired requests
  $effect(() => {
    const timer = setInterval(() => {
      now = Date.now();
      const expired = pendingPermissions.filter(p => now >= p.deadline);
      for (const p of expired) ws?.sendDeny(p.requestId, 'timeout');
      if (expired.length) pendingPermissions = pendingPermissions.filter(p => now < p.deadline);
    }, 1000);
    return () => clearInterval(timer);
  });

  // Track processed request IDs so the events $effect never double-adds
  const _seenPermIds = new Set<string>();

  $effect(() => {
    for (const ev of events) {
      if (ev.type === 'permission_request' && !_seenPermIds.has(ev.call_id)) {
        _seenPermIds.add(ev.call_id);
        pendingPermissions = [...pendingPermissions, {
          requestId: ev.call_id,
          tool: ev.tool,
          summary: ev.summary,
          deadline: Date.now() + ev.timeout_secs * 1000,
          timeoutSecs: ev.timeout_secs,
        }];
      }
    }
  });

  function approvePermission(requestId: string) {
    ws?.sendApprove(requestId);
    pendingPermissions = pendingPermissions.filter(p => p.requestId !== requestId);
  }

  function denyPermission(requestId: string) {
    ws?.sendDeny(requestId);
    pendingPermissions = pendingPermissions.filter(p => p.requestId !== requestId);
  }

  // ── Role injection (Governor / Worker / Default) ─────────────────────────
  // Loads canon prompts from the soul helix API and injects via SetSystemPrompt.
  // UI is ready; backend SetSystemPrompt variant (#161) must ship from
  // feat/squad-comms-session-per-build before injection takes effect.

  type AgentRole = 'default' | 'governor' | 'worker';

  let activeRole = $state<AgentRole>('default');
  let roleInjecting = $state(false);
  let roleError = $state<string | null>(null);

  const ROLE_HELIX_PATHS: Record<Exclude<AgentRole, 'default'>, string> = {
    governor: 'user/standards/canon/governor-system-prompt.md',
    worker:   'user/standards/canon/worker-system-prompt.md',
  };

  async function setRole(role: AgentRole) {
    if (!ws?.connected) return;
    if (role === 'default') {
      // Clear system prompt by sending an empty string — backend treats '' as "use default"
      ws.sendSystemPrompt('');
      activeRole = 'default';
      roleError = null;
      return;
    }

    roleInjecting = true;
    roleError = null;

    try {
      const path = ROLE_HELIX_PATHS[role];
      const res = await fetch(`/api/soul/entries/${path}`, {
        headers: { Authorization: `Bearer ${localStorage.getItem('la_token') ?? ''}` },
      });
      if (!res.ok) throw new Error(`helix ${res.status}`);
      const data = await res.json() as { content?: string; body?: string };
      const text = data.content ?? data.body ?? '';
      if (!text) throw new Error('empty prompt');
      ws.sendSystemPrompt(text);
      activeRole = role;
    } catch (e) {
      roleError = e instanceof Error ? e.message : String(e);
    } finally {
      roleInjecting = false;
    }
  }

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

  /** Total tool calls fired this session — proxy for agent loop depth. */
  let toolCallCount = $derived(() => events.filter(e => e.type === 'tool_start').length);

  /** Sum of verify retries across all VERIFY cycles — self-correction count. */
  let verifyRetries = $derived(() =>
    events.reduce((n, e) => n + (e.type === 'verify_complete' ? e.retries_used : 0), 0)
  );

  /** True when the agent is still running (no complete/error terminal event). */
  let isRunning = $derived(() => {
    for (const ev of events) {
      if (ev.type === 'complete' || (ev.type === 'error' && !ev.recoverable)) return false;
    }
    return events.length > 0;
  });

  // ── Per-event expand state (thinking + tool detail rows) ─────────────────
  const expandedThoughts = new Set<number>();
  let expandedThoughtsVersion = $state(0);

  function toggleThought(index: number) {
    if (expandedThoughts.has(index)) expandedThoughts.delete(index);
    else expandedThoughts.add(index);
    expandedThoughtsVersion++;
  }

  const expandedRows = new Set<number>();
  let expandedRowsVersion = $state(0);

  function toggleRow(index: number) {
    if (expandedRows.has(index)) expandedRows.delete(index);
    else expandedRows.add(index);
    expandedRowsVersion++;
  }
</script>

<div class="agent-console" data-testid="agent-console">
  <!-- Header -->
  <div class="console-head">
    <span class="console-label">AGENT BRIDGE</span>
    <span class="console-status" class:connected>
      {connected ? 'CONNECTED' : 'OFFLINE'}
    </span>
  </div>

  <!-- Role selector -->
  <div class="role-bar" aria-label="Agent role">
    <span class="role-label">ROLE</span>
    {#each (['default', 'governor', 'worker'] as AgentRole[]) as role}
      <button
        class="role-btn"
        class:active={activeRole === role}
        class:loading={roleInjecting && activeRole !== role}
        disabled={!connected || roleInjecting}
        onclick={() => setRole(role)}
        data-role={role}
      >
        {role === 'default' ? 'DEFAULT' : role === 'governor' ? 'GOVERNOR' : 'WORKER'}
      </button>
    {/each}
    {#if roleError}
      <span class="role-err" title={roleError}>ERR</span>
    {/if}
  </div>

  <!-- Message stream -->
  <div class="console-body" bind:this={scrollEl}>
    {#if events.length === 0}
      <div class="console-empty">— send a message to start the agent —</div>
    {:else}
      {#each events as ev, i (i)}
        {#if ev.type === 'text'}
          <div class="msg msg-text">
            <PolytopeNode type="pentachoron" color="#3b82f6" size={20} />
            <span>{ev.chunk}</span>
          </div>

        {:else if ev.type === 'thinking'}
          {@const isExp = expandedThoughts.has(i) && expandedThoughtsVersion >= 0}
          <div class="msg msg-thinking-row">
            <PolytopeNode type="doubleHelix4D" color="#64748b" size={44} expanded={isExp} onclick={() => toggleThought(i)} />
            <div class="thinking-right">
              <button class="thinking-label-btn" onclick={() => toggleThought(i)}>
                Reasoning <span class="thinking-len">{ev.content.length} chars</span>
              </button>
              {#if isExp}
                <div class="thinking-body">{ev.content}</div>
              {/if}
            </div>
          </div>

        {:else if ev.type === 'tool_start'}
          {@const rowExp = expandedRows.has(i) && expandedRowsVersion >= 0}
          <div class="msg msg-tool">
            <PolytopeNode type="hexadecachoron" color="#0ea5e9" size={28} expanded={rowExp} onclick={() => toggleRow(i)} />
            <div class="tool-content">
              <strong>{ev.name}</strong>
              {#if rowExp}
                <code class="tool-input">{JSON.stringify(ev.input, null, 2)}</code>
              {/if}
            </div>
          </div>

        {:else if ev.type === 'tool_complete'}
          {@const rowExp = expandedRows.has(i) && expandedRowsVersion >= 0}
          <div class="msg msg-tool" class:success={ev.success} class:error={!ev.success}>
            <PolytopeNode type="pentachoron" color={ev.success ? '#22c55e' : '#ef4444'} size={28} expanded={rowExp} onclick={ev.result ? () => toggleRow(i) : undefined} />
            <div class="tool-content">
              <strong>{ev.id}</strong> {ev.duration_ms}ms
              {#if rowExp && ev.result}
                <pre class="tool-result">{ev.result}</pre>
              {/if}
            </div>
          </div>

        {:else if ev.type === 'status_update'}
          <div class="msg msg-status">
            <PolytopeNode type="duoprism34" color="#f59e0b" size={24} />
            <span>{ev.text}</span>
          </div>

        {:else if ev.type === 'error'}
          <div class="msg msg-error" class:recoverable={ev.recoverable}>
            <PolytopeNode type="hexadecachoron" color={ev.recoverable ? '#f59e0b' : '#ef4444'} size={24} />
            <span>{ev.message}</span>
          </div>

        {:else if ev.type === 'complete'}
          <div class="msg msg-complete">
            <PolytopeNode type="tesseract" color="#22c55e" size={32} />
            <span>{ev.reason.kind}{#if ev.reason.message}: {ev.reason.message}{/if}</span>
          </div>

        {:else if ev.type === 'token_usage'}
          <div class="msg msg-meta">
            <PolytopeNode type="duoprism55" color="#475569" size={20} />
            <span>tokens {ev.input} → {ev.output}</span>
          </div>

        {:else if ev.type === 'pick_classified'}
          <div class="msg msg-meta">
            <PolytopeNode type="tesseract" color="#3b82f6" size={20} />
            <span>mode: <strong>{ev.mode}</strong></span>
          </div>

        {:else if ev.type === 'discover_injected'}
          <div class="msg msg-meta">
            <PolytopeNode type="rectified5cell" color="#475569" size={20} />
            <span>context: {ev.entry_count} entries · {ev.chars_injected} chars</span>
          </div>

        {:else if ev.type === 'verify_complete'}
          <div class="msg msg-verify" class:passed={ev.passed} class:failed={!ev.passed}>
            <PolytopeNode type="rectified5cell" color={ev.passed ? '#22c55e' : '#ef4444'} size={24} />
            <span>VERIFY {ev.passed ? 'passed' : 'failed'}{ev.retries_used > 0 ? ` (${ev.retries_used} retries)` : ''}</span>
          </div>

        {:else if ev.type === 'verify_failed'}
          <div class="msg msg-error">
            <PolytopeNode type="rectified5cell" color="#ef4444" size={24} />
            <span>VERIFY exhausted — {ev.reason}</span>
          </div>

        {:else if ev.type === 'reflect_complete'}
          <div class="msg msg-meta">
            <PolytopeNode type="icositetrachoron" color="#a78bfa" size={20} />
            <span>reflect: sig {(ev.significance * 100).toFixed(0)}%{ev.enrich_triggered ? ' · enriched' : ''}</span>
          </div>

        {:else if ev.type === 'cost_gate_check'}
          <div class="msg msg-cost-gate">
            <PolytopeNode type="duoprism34" color="#f59e0b" size={24} />
            <span>cost gate: ${ev.projected_usd.toFixed(4)} / ${ev.gate_usd.toFixed(2)} limit</span>
          </div>

        {:else if ev.type === 'security_violation'}
          <div class="msg msg-error">
            <PolytopeNode type="hexadecachoron" color="#ef4444" size={24} />
            <span>{ev.event_type} via {ev.tool}{ev.path ? ` → ${ev.path}` : ''}: {ev.detail}</span>
          </div>

        {:else if ev.type === 'sandbox_blocked'}
          <div class="msg msg-error">
            <PolytopeNode type="tesseract" color="#f97316" size={24} />
            <span>{ev.tool} blocked: {ev.attempted_path} — {ev.reason}</span>
          </div>

        {:else if ev.type === 'resource_limit_hit'}
          <div class="msg msg-error">
            <PolytopeNode type="duoprism34" color="#f97316" size={24} />
            <span>{ev.tool} hit {ev.limit_type} limit: {ev.value}/{ev.max}</span>
          </div>

        {:else if ev.type === 'provider_fallback'}
          <div class="msg msg-meta">
            <PolytopeNode type="duoprism64" color="#475569" size={20} />
            <span>provider: {ev.from} → {ev.to}</span>
          </div>

        {:else if ev.type === 'lenses_selected'}
          <div class="msg msg-meta">
            <PolytopeNode type="icositetrachoron" color="#a78bfa" size={20} />
            <span>lenses (tier {ev.tier}): {ev.lenses.join(', ')}</span>
          </div>

        {:else if ev.type === 'lens_assessment'}
          <div class="msg msg-meta">
            <PolytopeNode type="icositetrachoron" color="#a78bfa" size={20} />
            <span>{ev.sibling} ({(ev.confidence * 100).toFixed(0)}%){ev.finding ? `: ${ev.finding}` : ' — no finding'}</span>
          </div>

        {:else if ev.type === 'squad_suggestion'}
          <div class="msg msg-meta">
            <PolytopeNode type="dualCompound" color="#14b8a6" size={24} />
            <span>suggest: <strong>{ev.preset}</strong> — {ev.reason}</span>
          </div>

        {:else if ev.type === 'strand_bump'}
          <div class="msg msg-meta">
            <PolytopeNode type="duoprism53" color="#a78bfa" size={20} />
            <span>strand {ev.strand} +{ev.delta.toFixed(2)}</span>
          </div>

        {:else if ev.type === 'exec_server_status'}
          <div class="msg msg-meta">
            <PolytopeNode type="duoprism83" color="#475569" size={20} />
            <span>exec server: {ev.connected ? 'up' : 'down'}{ev.pid ? ` (pid ${ev.pid})` : ''}</span>
          </div>

        {:else if ev.type === 'child_agent_forked'}
          <div class="msg msg-child">
            <PolytopeNode type="dualCompound" color="#7dd3fc" size={24} />
            <span><strong>{ev.child_name}</strong> forked · {ev.cwd.split('/').at(-1)}</span>
          </div>

        {:else if ev.type === 'child_agent_completed'}
          <div class="msg msg-child" class:success={ev.success} class:error={!ev.success}>
            <PolytopeNode type="dualCompound" color={ev.success ? '#22c55e' : '#ef4444'} size={24} />
            <span><strong>{ev.child_name}</strong> — {ev.summary.slice(0, 120)}</span>
          </div>

        {:else if ev.type === 'plan_queue_ready'}
          <div class="msg msg-plan-queue">
            <PolytopeNode type="hexacosichoron" color="#6366f1" size={28} />
            <span>plan ready: {ev.actions.length} action{ev.actions.length !== 1 ? 's' : ''} queued</span>
          </div>

        {:else if ev.type === 'permission_request'}
          {@const pending = pendingPermissions.find(p => p.requestId === ev.call_id)}
          {#if pending}
            <div class="msg msg-permission-card" role="group" aria-label="Permission request for {ev.tool}">
              <div class="perm-header">
                <PolytopeNode type="icositetrachoron" color="#f59e0b" size={36} expanded={true} />
                <span class="perm-tool">{ev.tool}</span>
                <span class="perm-timer">{Math.max(0, Math.ceil((pending.deadline - now) / 1000))}s</span>
              </div>
              <code class="perm-input">{pending.summary.slice(0, 240)}</code>
              <div class="perm-bar-track">
                <div class="perm-bar-fill" style:width="{Math.max(0, (pending.deadline - now) / (pending.timeoutSecs * 10))}%"></div>
              </div>
              <div class="perm-actions">
                <button class="perm-btn perm-approve" onclick={() => approvePermission(ev.call_id)}>APPROVE</button>
                <button class="perm-btn perm-deny"    onclick={() => denyPermission(ev.call_id)}>DENY</button>
              </div>
            </div>
          {:else}
            <div class="msg msg-permission-resolved">
              <PolytopeNode type="icositetrachoron" color="#475569" size={20} />
              <span><strong>{ev.tool}</strong> — resolved</span>
            </div>
          {/if}
        {/if}
      {/each}
    {/if}
  </div>

  <!-- Footer meta -->
  <div class="console-foot">
    <span class="foot-meta">
      {#if tokenUsage().input > 0}
        {tokenUsage().input}→{tokenUsage().output} tok
      {/if}
    </span>
    <span class="foot-meta foot-counters">
      {#if toolCallCount() > 0}
        <span class="foot-badge">{toolCallCount()} tools</span>
      {/if}
      {#if verifyRetries() > 0}
        <span class="foot-badge foot-badge-warn">{verifyRetries()} retries</span>
      {/if}
      <span>{events.length} ev</span>
    </span>
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

  /* ── role selector ── */
  .role-bar {
    display: flex;
    align-items: center;
    gap: 5px;
    padding: 4px 10px;
    border-bottom: 1px solid var(--la-hair-faint);
    flex-shrink: 0;
    background: var(--la-bg-elev-1);
  }
  .role-label {
    font-size: 8px;
    font-weight: 700;
    letter-spacing: 0.14em;
    color: var(--la-text-mute);
    flex-shrink: 0;
  }
  .role-btn {
    font-size: 8px;
    font-weight: 700;
    letter-spacing: 0.08em;
    padding: 2px 7px;
    border: 1px solid var(--la-hair-base);
    background: transparent;
    color: var(--la-text-mute);
    cursor: pointer;
    border-radius: 2px;
    transition: border-color 80ms, color 80ms, background 80ms;
  }
  .role-btn:hover:not(:disabled) {
    border-color: var(--la-text-dim);
    color: var(--la-text-base);
  }
  .role-btn:disabled { opacity: 0.4; cursor: not-allowed; }
  .role-btn.active[data-role="default"] {
    border-color: var(--la-text-mute);
    color: var(--la-text-dim);
    background: rgba(71, 85, 105, 0.15);
  }
  .role-btn.active[data-role="governor"] {
    border-color: #00c8ff;
    color: #00c8ff;
    background: rgba(0, 200, 255, 0.08);
  }
  .role-btn.active[data-role="worker"] {
    border-color: #4dffe6;
    color: #4dffe6;
    background: rgba(77, 255, 230, 0.08);
  }
  .role-err {
    font-size: 8px;
    font-weight: 700;
    color: #ef4444;
    letter-spacing: 0.06em;
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
    display: flex;
    align-items: flex-start;
    gap: 7px;
    padding: 3px 6px;
    border-radius: 3px;
  }
  .msg-meta, .msg-status, .msg-verify, .msg-complete,
  .msg-permission-resolved { align-items: center; }
  .msg span { flex: 1; min-width: 0; word-break: break-word; }
  .msg-text { background: #1a1a2e; color: #e2e8f0; }
  /* ── thinking row ── */
  .msg-thinking-row {
    display: flex;
    align-items: flex-start;
    gap: 7px;
    padding: 2px 6px;
  }
  .thinking-right {
    display: flex;
    flex-direction: column;
    gap: 4px;
    min-width: 0;
    flex: 1;
  }
  .thinking-label-btn {
    background: none;
    border: none;
    padding: 0;
    cursor: pointer;
    text-align: left;
    font-family: inherit;
    font-size: 10px;
    font-weight: 600;
    color: #94a3b8;
    letter-spacing: 0.04em;
  }
  .thinking-label-btn:hover { color: #cbd5e1; }
  .thinking-len {
    font-weight: 400;
    color: #475569;
    font-size: 9px;
    margin-left: 4px;
  }
  .thinking-body {
    padding: 6px 8px;
    background: rgba(15, 23, 42, 0.6);
    border-left: 2px solid #334155;
    border-radius: 0 3px 3px 0;
    color: #94a3b8;
    font-style: italic;
    font-size: 10px;
    line-height: 1.55;
    white-space: pre-wrap;
    word-break: break-word;
    max-height: 320px;
    overflow-y: auto;
  }

  /* ── tool content ── */
  .tool-content {
    display: flex;
    flex-direction: column;
    gap: 4px;
    min-width: 0;
    flex: 1;
    padding-top: 1px;
  }
  .tool-input {
    display: block;
    font-size: 9px;
    color: #64748b;
    white-space: pre-wrap;
    word-break: break-all;
    max-height: 120px;
    overflow-y: auto;
  }
  .tool-result {
    font-size: 9px;
    color: #94a3b8;
    white-space: pre-wrap;
    word-break: break-word;
    max-height: 80px;
    overflow-y: auto;
    margin: 0;
  }
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

  .foot-counters {
    display: flex;
    align-items: center;
    gap: 5px;
  }
  .foot-badge {
    font-size: 8px;
    font-weight: 700;
    padding: 1px 5px;
    border-radius: 2px;
    background: rgba(71, 85, 105, 0.25);
    color: var(--la-text-dim);
    letter-spacing: 0.05em;
  }
  .foot-badge-warn {
    background: rgba(245, 158, 11, 0.15);
    color: #f59e0b;
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

  /* ── Phase 5–14 event types ── */
  .msg-verify {
    font-weight: 600;
    font-size: 10px;
    padding: 2px 6px;
  }
  .msg-verify.passed { color: #22c55e; }
  .msg-verify.failed { color: #ef4444; }
  .msg-cost-gate {
    background: #1a1208;
    border-left: 2px solid #f59e0b;
    color: #fbbf24;
    padding: 3px 6px;
    font-size: 10px;
    font-weight: 600;
  }
  .msg-child {
    color: #7dd3fc;
    font-size: 10px;
    padding: 2px 6px;
    border-left: 2px solid #0369a1;
  }
  .msg-child.success { border-left-color: #22c55e; }
  .msg-child.error   { border-left-color: #ef4444; color: #fca5a5; }
  .msg-plan-queue {
    background: #0f172a;
    border-left: 2px solid #6366f1;
    color: #a5b4fc;
    padding: 3px 6px;
    font-size: 10px;
    font-weight: 600;
  }

  /* ── HITL permission card ── */
  .msg-permission-card {
    background: #1a1208;
    border: 1px solid #d97706;
    border-radius: 4px;
    padding: 8px 10px;
    display: flex;
    flex-direction: column;
    gap: 5px;
    animation: perm-appear 120ms ease-out;
  }
  @keyframes perm-appear {
    from { opacity: 0; transform: translateY(-4px); }
    to   { opacity: 1; transform: translateY(0); }
  }
  .perm-header {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .perm-tool {
    flex: 1;
    font-weight: 700;
    font-size: 10px;
    color: #fbbf24;
    letter-spacing: 0.06em;
  }
  .perm-timer {
    font-size: 10px;
    font-weight: 700;
    color: #f59e0b;
    min-width: 24px;
    text-align: right;
  }
  .perm-input {
    font-size: 9px;
    color: #94a3b8;
    white-space: pre-wrap;
    word-break: break-all;
    line-height: 1.4;
    max-height: 60px;
    overflow: hidden;
  }
  .perm-bar-track {
    height: 2px;
    background: rgba(217, 119, 6, 0.2);
    border-radius: 1px;
    overflow: hidden;
  }
  .perm-bar-fill {
    height: 100%;
    background: #f59e0b;
    border-radius: 1px;
    transition: width 1s linear;
  }
  .perm-actions {
    display: flex;
    gap: 6px;
  }
  .perm-btn {
    font-size: 8px;
    font-weight: 700;
    letter-spacing: 0.1em;
    padding: 3px 10px;
    border-radius: 2px;
    cursor: pointer;
    border: 1px solid;
  }
  .perm-approve {
    background: #052e16;
    border-color: #16a34a;
    color: #bbf7d0;
  }
  .perm-approve:hover { background: #14532d; }
  .perm-deny {
    background: #450a0a;
    border-color: #dc2626;
    color: #fecaca;
  }
  .perm-deny:hover { background: #7f1d1d; }
  .msg-permission-resolved {
    color: #64748b;
    font-size: 10px;
    font-style: italic;
  }
</style>
