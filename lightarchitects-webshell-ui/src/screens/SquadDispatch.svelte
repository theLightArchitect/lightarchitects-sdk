<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { registerHotkey } from '$lib/hotkeyRegistry';
  import {
    classifyTask,
    executeDispatch,
    cancelDispatch,
    streamDispatch,
    loadHistory,
    saveHistory,
    addToHistory,
    isTerminal,
    isComplete,
    isError,
    DOMAIN_AGENT_LABELS,
    DOMAIN_AGENT_COLORS,
    type DomainAgent,
    type Classification,
    type DispatchEvent,
    type AgentLiveState,
    type DispatchHistoryEntry,
    type FileAttachment,
    type AgentToolConfig,
  } from '$lib/dispatch';

  import DispatchInput from '$lib/../components/dispatch/DispatchInput.svelte';
  import AgentSelector from '$lib/../components/dispatch/AgentSelector.svelte';
  import TaskDAG from '$lib/../components/dispatch/TaskDAG.svelte';
  import LiveAgentGrid from '$lib/../components/dispatch/LiveAgentGrid.svelte';
  import AgentDetail from '$lib/../components/dispatch/AgentDetail.svelte';
  import EventStream, { type StreamRow } from '$lib/../components/EventStream.svelte';
  import DispatchCLI from '$lib/../components/cli/DispatchCLI.svelte';
  import HistoryRail from '$lib/../components/dispatch/HistoryRail.svelte';

  // ── Props (forwarded from Dispatch.svelte route shell) ─────────────────────

  // eslint-disable-next-line @typescript-eslint/no-unused-vars
  let { runId }: { runId?: string } = $props();

  // ── State ──────────────────────────────────────────────────────────────────

  type Phase = 'idle' | 'classifying' | 'ready' | 'streaming' | 'complete' | 'error';

  let phase = $state<Phase>('idle');
  let task = $state('');
  let dry = $state(false);
  let selectedAgents = $state<DomainAgent[]>([]);
  let classification = $state<Classification | null>(null);
  let dispatchId = $state<string | null>(null);
  let attachments = $state<FileAttachment[]>([]);
  let toolConfig = $state<Partial<Record<DomainAgent, AgentToolConfig>>>({});
  let events = $state<DispatchEvent[]>([]);
  let eventTimes = $state<string[]>([]); // receive-time strings, parallel index to events
  let agentStates = $state(new Map<DomainAgent, AgentLiveState>());
  let history = $state<DispatchHistoryEntry[]>(loadHistory());
  let errorMsg = $state<string | null>(null);
  let elapsedMs = $state<number | undefined>(undefined);
  let selectedAgent = $state<DomainAgent | null>(null);

  let stopStream: (() => void) | null = null;
  let classifyTimer: ReturnType<typeof setTimeout> | null = null;

  // Pre-fill task from ?task= query param when returning from /intake
  onMount(() => {
    const qs = window.location.hash.split('?')[1] ?? '';
    const params = new URLSearchParams(qs);
    const prefilled = params.get('task');
    if (prefilled) task = prefilled;
  });

  // ── EventStream adapter — converts DispatchEvent[] → StreamRow[] ─────────

  function fmtTime(d: Date): string {
    return (
      String(d.getHours()).padStart(2, '0') + ':' +
      String(d.getMinutes()).padStart(2, '0') + ':' +
      String(d.getSeconds()).padStart(2, '0')
    );
  }

  const mailboxRows = $derived.by((): StreamRow[] => {
    const rows: StreamRow[] = [];
    const base = performance.now(); // sub-ms precision — unique per row even in fast-burst derivations
    for (let idx = 0; idx < events.length; idx++) {
      const e = events[idx];
      const ts = base + idx;
      const time = eventTimes[idx] ?? fmtTime(new Date()); // receive-time, not derivation-time
      if ('MailboxMessage' in e) {
        rows.push({
          ts, time,
          source: e.MailboxMessage.agent,
          color: DOMAIN_AGENT_COLORS[e.MailboxMessage.agent] ?? '#64748b',
          text: e.MailboxMessage.text,
          severity: 'info',
        });
      } else if ('PerAgentState' in e && e.PerAgentState.message !== null) {
        rows.push({
          ts, time,
          source: e.PerAgentState.agent,
          color: DOMAIN_AGENT_COLORS[e.PerAgentState.agent] ?? '#64748b',
          text: `[${e.PerAgentState.state}] ${e.PerAgentState.message}`,
          severity: 'info',
        });
      } else if ('ToolUsage' in e) {
        const u = e.ToolUsage;
        rows.push({
          ts, time,
          source: u.agent,
          color: DOMAIN_AGENT_COLORS[u.agent] ?? '#64748b',
          text: `${u.tool} · ${u.action} [${u.status}]${u.latency_ms !== undefined ? ` ${u.latency_ms}ms` : ''}`,
          severity: u.status === 'failed' ? 'err' : u.status === 'skipped' ? 'warn' : 'info',
        });
      } else if (isComplete(e)) {
        rows.push({
          ts, time,
          source: 'system',
          color: '#22c55e',
          text: `Dispatch complete in ${(e.Complete.elapsed_ms / 1000).toFixed(1)}s`,
          severity: 'ok',
        });
      } else if (isError(e)) {
        const agent = e.Error.agent;
        rows.push({
          ts, time,
          source: agent ?? 'system',
          color: agent ? (DOMAIN_AGENT_COLORS[agent] ?? '#ef4444') : '#ef4444',
          text: e.Error.message,
          severity: 'err',
        });
      }
    }
    return rows.reverse();
  });

  // ── Auto-classify with debounce ──────────────────────────────────────────────

  $effect(() => {
    const t = task;
    if (classifyTimer) clearTimeout(classifyTimer);
    if (t.trim().length < 8 || phase === 'streaming' || phase === 'complete' || phase === 'error') return;

    classifyTimer = setTimeout(async () => {
      try {
        phase = 'classifying';
        classification = await classifyTask(t);
        // Only auto-apply if user hasn't manually selected
        if (selectedAgents.length === 0 && classification.agents.length > 0) {
          selectedAgents = [...classification.agents];
        }
        if (phase === 'classifying') phase = 'ready';
      } catch {
        if (phase === 'classifying') phase = 'idle';
      }
    }, 300);
  });

  // ── Dispatch ──────────────────────────────────────────────────────────────────

  async function dispatch(
    taskText: string,
    isDry: boolean,
    atts: FileAttachment[] = [],
    cfg: Partial<Record<DomainAgent, AgentToolConfig>> = {},
  ) {
    if (selectedAgents.length === 0) return;
    errorMsg = null;
    events = [];
    eventTimes = [];
    agentStates = new Map();
    elapsedMs = undefined;

    let id: string;
    try {
      phase = 'streaming';
      id = await executeDispatch(taskText, selectedAgents, isDry, atts, cfg);
      dispatchId = id;
      triggerDispatchFX();
    } catch (e) {
      errorMsg = (e as Error).message;
      phase = 'error';
      return;
    }

    const entry: DispatchHistoryEntry = {
      id,
      task: taskText,
      agents: [...selectedAgents],
      mode: selectedAgents.length === 1 ? 'Solo' : 'Squad',
      dry: isDry,
      startedAt: Date.now(),
      status: 'running',
    };
    history = addToHistory(entry, history);
    saveHistory(history);

    stopStream = streamDispatch(
      id,
      (e) => {
        events = [...events, e];
        eventTimes = [...eventTimes, fmtTime(new Date())];
        applyEvent(e);

        if (isTerminal(e)) {
          if (isComplete(e)) {
            elapsedMs = e.Complete.elapsed_ms;
            updateHistoryEntry(id, 'complete', elapsedMs);
            phase = 'complete';
          } else if (isError(e)) {
            errorMsg = e.Error.message;
            updateHistoryEntry(id, 'error');
            phase = 'error';
          }
        }
      },
      () => {
        // Stream closed — treat as complete if not already terminal
        if (phase === 'streaming') {
          updateHistoryEntry(id, 'complete');
          phase = 'complete';
        }
      },
    );
  }

  function applyEvent(e: DispatchEvent) {
    if ('PerAgentState' in e) {
      const { agent, state, message, files_touched, token_count, elapsed_ms } = e.PerAgentState;
      const prev = agentStates.get(agent);
      agentStates = new Map(agentStates).set(agent, {
        agent,
        state,
        messages: message
          ? [...(prev?.messages ?? []), message]
          : (prev?.messages ?? []),
        files_touched: files_touched ?? prev?.files_touched,
        token_count:   token_count   ?? prev?.token_count,
        elapsed_ms:    elapsed_ms    ?? prev?.elapsed_ms,
        last_tool:     prev?.last_tool,
      });
    } else if ('MailboxMessage' in e) {
      const { agent, text } = e.MailboxMessage;
      const existing = agentStates.get(agent);
      agentStates = new Map(agentStates).set(agent, {
        agent,
        state: existing?.state ?? 'running',
        messages: [...(existing?.messages ?? []), text],
        files_touched: existing?.files_touched,
        token_count:   existing?.token_count,
        elapsed_ms:    existing?.elapsed_ms,
        last_tool:     existing?.last_tool,
      });
    } else if ('ToolUsage' in e) {
      const { agent, tool, action, status, latency_ms } = e.ToolUsage;
      const existing = agentStates.get(agent);
      agentStates = new Map(agentStates).set(agent, {
        agent,
        state:         existing?.state ?? 'running',
        messages:      existing?.messages ?? [],
        files_touched: existing?.files_touched,
        token_count:   existing?.token_count,
        elapsed_ms:    existing?.elapsed_ms,
        last_tool:     { tool, action, status, latency_ms },
      });
    }
  }

  function updateHistoryEntry(
    id: string,
    status: DispatchHistoryEntry['status'],
    elapsed?: number,
  ) {
    history = history.map((h) =>
      h.id === id ? { ...h, status, ...(elapsed !== undefined && { elapsed_ms: elapsed }) } : h,
    );
    saveHistory(history);
  }

  // ── Cancel ────────────────────────────────────────────────────────────────────

  async function cancel() {
    if (!dispatchId) return;
    stopStream?.();
    stopStream = null;
    try {
      await cancelDispatch(dispatchId);
      updateHistoryEntry(dispatchId, 'cancelled');
    } catch {
      // best-effort
    }
    phase = 'idle';
    dispatchId = null;
  }

  // ── Reset ─────────────────────────────────────────────────────────────────────

  function reset() {
    stopStream?.();
    stopStream = null;
    phase = 'idle';
    events = [];
    eventTimes = [];
    agentStates = new Map();
    dispatchId = null;
    errorMsg = null;
    elapsedMs = undefined;
    task = '';
    selectedAgents = [];
    classification = null;
    attachments = [];
    toolConfig = {};
  }

  function replayFromHistory(entry: DispatchHistoryEntry) {
    reset();
    task = entry.task;
    selectedAgents = [...entry.agents];
    dry = entry.dry;
  }

  function clearHistory() {
    history = [];
    saveHistory([]);
  }

  onDestroy(() => {
    stopStream?.();
    if (classifyTimer) clearTimeout(classifyTimer);
    fxTimers.forEach(clearTimeout);
  });

  // ── Squad-scoped hotkeys (active only on /squad-dispatch) ───────────────────

  $effect(() => {
    const unregs = [
      registerHotkey({
        id: 'squad-reset',
        keys: ['R'],
        label: 'Reset dispatch',
        group: 'Squad Dispatch',
        scope: 'squad-dispatch',
        matches: e =>
          !e.metaKey && !e.ctrlKey && !e.altKey &&
          e.key === 'r' &&
          !(e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement),
        handler: () => reset(),
      }),
      registerHotkey({
        id: 'squad-dispatch-run',
        keys: ['⌘', '↵'],
        label: 'Dispatch task',
        group: 'Squad Dispatch',
        scope: 'squad-dispatch',
        matches: e => (e.metaKey || e.ctrlKey) && e.key === 'Enter',
        handler: () => { if (phase !== 'streaming') dispatch(task, dry, attachments, toolConfig); },
      }),
      registerHotkey({
        id: 'squad-dispatch-dry',
        keys: ['⌘', '⇧', '↵'],
        label: 'Dry-run dispatch',
        group: 'Squad Dispatch',
        scope: 'squad-dispatch',
        matches: e => (e.metaKey || e.ctrlKey) && e.shiftKey && e.key === 'Enter',
        handler: () => { if (phase !== 'streaming') dispatch(task, true); },
      }),
      registerHotkey({
        id: 'squad-focus-input',
        keys: ['⌘', 'D'],
        label: 'Focus task input',
        group: 'Squad Dispatch',
        scope: 'squad-dispatch',
        matches: e => (e.metaKey || e.ctrlKey) && e.key === 'd',
        handler: () => { inputSectionEl?.querySelector('textarea')?.focus(); },
      }),
      registerHotkey({
        id: 'squad-cancel',
        keys: ['Esc'],
        label: 'Cancel dispatch',
        group: 'Squad Dispatch',
        scope: 'squad-dispatch',
        matches: e => e.key === 'Escape',
        handler: () => { if (phase === 'streaming') cancel(); },
      }),
    ];
    return () => unregs.forEach(fn => fn());
  });

  $effect(() => {
    import('$lib/tutorial').then(({ runTutorial }) => runTutorial('t6'));
  });

  const isLive = $derived(phase === 'streaming');
  const isTerminalPhase = $derived(phase === 'complete' || phase === 'error');

  // Cinematic dispatch flash — briefly true when dispatch fires.
  let dispatchFlash = $state(false);
  let scanlineActive = $state(false);
  let scanlineKey = $state(0);
  let fxTimers: ReturnType<typeof setTimeout>[] = [];
  let inputSectionEl: HTMLElement | null = null;

  function triggerDispatchFX() {
    fxTimers.forEach(clearTimeout);
    dispatchFlash = true;
    scanlineActive = true;
    scanlineKey += 1;
    fxTimers = [
      setTimeout(() => { dispatchFlash = false; }, 450),
      setTimeout(() => { scanlineActive = false; }, 700),
    ];
  }
</script>

<!-- Cinematic flash on dispatch fire -->
{#if dispatchFlash}
  <div class="dispatch-flash" aria-hidden="true"></div>
{/if}

<div class="ops-frame" data-dispatching={isLive}>

  <!-- ── Row 1: Header strip ── -->
  <header class="header-strip">
    <div class="reg-marks">
      <span class="reg-mark">
        <span class="reg-dot {isLive ? 'live' : ''}" aria-hidden="true"></span>
        SQD-DISPATCH
      </span>
      {#if phase === 'classifying'}
        <span class="reg-mark"><span class="reg-dot warn" aria-hidden="true"></span> CLASSIFYING</span>
      {:else if isLive}
        <span class="reg-mark"><span class="reg-dot live" aria-hidden="true"></span> LIVE</span>
      {:else if phase === 'error'}
        <span class="reg-mark"><span class="reg-dot error" aria-hidden="true"></span> ERROR</span>
      {:else if phase === 'complete'}
        <span class="reg-mark">✓ COMPLETE</span>
      {/if}
    </div>
    <div class="header-title">
      <span class="ti-id">SQD</span>
      <span class="ti-name">SQUAD DISPATCH</span>
      <span class="ti-sub">OPERATOR CONSOLE</span>
    </div>
    <div class="header-tele">
      {#if elapsedMs !== undefined}
        <span><span class="lbl">T· </span><span class="val">{(elapsedMs / 1000).toFixed(1)}s</span></span>
      {/if}
      <span><span class="lbl">AGENTS· </span><span class="val">{selectedAgents.length}</span></span>
      <button
        class="new-dispatch-btn"
        onclick={() => { window.location.hash = '/intake?return=/dispatch&prefill=task'; }}
      >+ NEW DISPATCH</button>
    </div>
  </header>

  <!-- ── Row 2: History pill strip ── -->
  <div class="history-row" data-onboarding="dispatch-history" aria-label="Dispatch history">
    <HistoryRail
      {history}
      onSelect={replayFromHistory}
      onClear={clearHistory}
    />
  </div>

  <!-- ── Row 3: Command bar ── -->
  <section class="command-bar" aria-label="Dispatch controls">
    <!-- task input + classifier chips + agent selector -->
    <div class="cmd-shell" bind:this={inputSectionEl} data-onboarding="dispatch-input">
      <div class="cmd-label">
        <span class="idx">[ 01 ]</span>
        <span>TASK SPECIFICATION</span>
        <span class="sep"></span>
        {#if phase === 'classifying'}
          <span class="phase-badge phase-classifying">classifying…</span>
        {:else if isLive}
          <span class="phase-badge phase-live">● LIVE</span>
        {:else if phase === 'complete'}
          <span class="phase-badge phase-ok">✓ {elapsedMs !== undefined ? `${(elapsedMs / 1000).toFixed(1)}s` : 'DONE'}</span>
        {:else if phase === 'error'}
          <span class="phase-badge phase-err">✗ ERROR</span>
        {/if}
      </div>

      <DispatchInput
        bind:task
        bind:dry
        bind:attachments
        bind:toolConfig
        selectedAgents={selectedAgents}
        disabled={isLive}
        onSubmit={dispatch}
        onTaskChange={(t) => { task = t; }}
      />

      {#if classification && task.trim().length >= 8}
        <div class="classifier-badge">
          {#each classification.agents as agent}
            <span
              class="cls-pill"
              style="color: var(--la-agent-{agent}); border-color: color-mix(in srgb, var(--la-agent-{agent}) 40%, transparent)"
            >
              {DOMAIN_AGENT_LABELS[agent]}
            </span>
          {/each}
          {#if classification.rationale}
            <span class="cls-rationale">
              {classification.rationale.length > 80
                ? classification.rationale.slice(0, 80) + '…'
                : classification.rationale}
            </span>
          {/if}
        </div>
      {/if}

      <div class="agent-selector-wrap" data-onboarding="dispatch-agent-selector">
        <AgentSelector
          bind:selected={selectedAgents}
          {classification}
          disabled={isLive}
        />
      </div>
    </div>

    <!-- action panel: telemetry + primary CTA -->
    <div class="cmd-action">
      <div class="cmd-action-head">
        <span><span class="idx">[ 02 ]</span> DISPATCH</span>
        <span
          class="cmd-state"
          class:state-armed={selectedAgents.length > 0 && !isLive && phase !== 'error'}
          class:state-active={isLive}
          class:state-err={phase === 'error'}
        >
          ● {isLive ? 'ACTIVE' : selectedAgents.length > 0 ? 'ARMED' : 'IDLE'}
        </span>
      </div>

      <div class="cmd-count">
        <span class="cmd-count-num">{String(selectedAgents.length).padStart(2, '0')}</span>
        <span class="cmd-count-lbl">
          AGENT{selectedAgents.length !== 1 ? 'S' : ''}<br>
          {isLive ? 'RUNNING' : 'QUEUED'}
        </span>
      </div>

      {#if errorMsg}
        <p class="cmd-error-msg" role="alert">{errorMsg}</p>
      {/if}

      {#if isLive || isTerminalPhase}
        <div class="cmd-dag-mini" data-testid="task-dag-toggle">
          <TaskDAG agents={selectedAgents} {agentStates} />
        </div>
      {/if}

      {#if isLive}
        <button class="cmd-btn cmd-btn-cancel" onclick={cancel}>■ CANCEL</button>
      {:else if isTerminalPhase}
        <button class="cmd-btn cmd-btn-new" onclick={reset}>
          <span>NEW DISPATCH</span>
          <span>↺</span>
        </button>
      {:else}
        <button
          class="cmd-btn cmd-btn-dispatch"
          disabled={selectedAgents.length === 0 || !task.trim()}
          onclick={() => dispatch(task, dry, attachments, toolConfig)}
        >
          <span>DISPATCH</span>
          <span class="dispatch-arrow">▶</span>
        </button>
      {/if}
    </div>
  </section>

  <!-- ── Row 4: Rail stage (horizontal agent rails) ── -->
  <section class="rail-stage" aria-label="Live agent workspace">
    <div class="rail-stage-head">
      <span>
        <span class="idx">[ 03 ]</span>
        {#if isLive}EXECUTION STAGE · LIVE
        {:else if isTerminalPhase}EXECUTION STAGE · COMPLETE
        {:else}EXECUTION STAGE · STANDBY{/if}
      </span>
      <div class="view-toggle">
        <button class="vt-btn vt-active" disabled>RAILS</button>
        <button class="vt-btn" disabled>+ DAG</button>
      </div>
    </div>

    {#if scanlineActive}
      {#key scanlineKey}
        <div class="sd-scanline" aria-hidden="true"></div>
      {/key}
    {/if}

    <div class="rails-wrap" data-onboarding="dispatch-live-grid">
      <LiveAgentGrid
        agents={selectedAgents}
        {agentStates}
        onRetry={(agent) => { /* retry wired via dispatch */ void agent; }}
        onSelect={(agent) => { selectedAgent = agent; }}
      />
      {#if selectedAgent !== null}
        <AgentDetail
          agent={selectedAgent}
          live={agentStates.get(selectedAgent)}
          onClose={() => { selectedAgent = null; }}
          onRetry={(agent) => { selectedAgent = null; void agent; }}
        />
      {/if}
    </div>
  </section>

  <!-- ── Row 5: Mailbox ── -->
  <section class="mailbox" data-onboarding="dispatch-mailbox">
    <div class="mailbox-head">
      <span>
        <span class="idx">[ 04 ]</span>
        MAILBOX {isLive ? '· ONLINE' : '· OFFLINE'}
      </span>
      <span>
        <span class="idx">MSG </span>
        <span class="msg-ctr">{String(events.length).padStart(3, '0')}</span>
      </span>
    </div>
    <div class="mailbox-body">
      <EventStream rows={mailboxRows} newestFirst maxDisplay={200} emptyMessage="— awaiting transmission —" />
    </div>
  </section>

  <!-- ── Row 6: CLI quick-dispatch ── -->
  <DispatchCLI
    onDispatch={(t) => dispatch(t, dry, attachments, toolConfig)}
    disabled={isLive}
  />

</div>

<style>
  /* ── 5-row ops frame ── */
  .ops-frame {
    position: relative;
    width: 100%;
    height: 100%;
    display: grid;
    grid-template-rows: 40px 36px auto 1fr 184px 36px;
    background: var(--la-bg-void);
    color: var(--la-text-bright);
    overflow: hidden;
    font-family: var(--la-font-chrome);
  }

  /* ── Row 1: header ── */
  .header-strip {
    display: grid;
    grid-template-columns: 1fr auto 1fr;
    align-items: center;
    padding: 0 16px;
    border-bottom: 1px solid var(--la-hair-base);
  }

  .reg-marks { display: flex; gap: 16px; align-items: center; }
  .reg-mark {
    display: flex;
    align-items: center;
    gap: 4px;
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.18em;
    color: var(--la-text-dim);
    text-transform: uppercase;
  }
  .reg-dot {
    width: 5px;
    height: 5px;
    background: var(--la-text-mute);
    flex-shrink: 0;
    display: inline-block;
  }
  .reg-dot.live  { background: var(--la-agent-researcher); animation: heartbeat 1.6s steps(2) infinite; }
  .reg-dot.warn  { background: var(--la-agent-performance); }
  .reg-dot.error { background: var(--la-agent-security); }

  @keyframes heartbeat {
    0%, 50%   { opacity: 1; }
    51%, 100% { opacity: 0.25; }
  }

  .header-title {
    display: flex;
    align-items: baseline;
    gap: 12px;
    font-size: 11px;
    letter-spacing: 0.18em;
    justify-content: center;
  }
  .ti-id   { color: var(--la-text-mute); font-weight: 200; }
  .ti-name { color: var(--la-text-stark); font-weight: 700; }
  .ti-sub  { color: var(--la-text-dim); font-weight: 200; }

  .header-tele {
    display: flex;
    justify-content: flex-end;
    align-items: center;
    gap: 20px;
    font-size: 10px;
    letter-spacing: 0.08em;
    color: var(--la-text-dim);
    font-family: var(--la-font-mono);
  }
  .header-tele .lbl { color: var(--la-text-mute); }
  .header-tele .val { color: var(--la-text-bright); font-variant-numeric: tabular-nums; }
  .new-dispatch-btn {
    padding: 0 10px;
    height: 22px;
    font-family: inherit;
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.1em;
    text-transform: uppercase;
    background: transparent;
    border: 1px solid var(--la-focus-ring);
    color: var(--la-focus-ring);
    border-radius: 2px;
    cursor: pointer;
    transition: background 80ms, color 80ms;
  }
  .new-dispatch-btn:hover { background: var(--la-focus-ring); color: var(--la-bg-frame); }

  /* ── Row 2: history pill strip ── */
  .history-row {
    border-bottom: 1px solid var(--la-hair-base);
    overflow: hidden;
  }

  /* ── Row 3: command bar ── */
  .command-bar {
    display: grid;
    grid-template-columns: 1fr 280px;
    border-bottom: 1px solid var(--la-hair-base);
    overflow: hidden;
  }

  .cmd-shell {
    border-right: 1px solid var(--la-hair-base);
    padding: 12px 16px;
    display: flex;
    flex-direction: column;
    gap: 8px;
    overflow-y: auto;
  }

  .cmd-label {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.18em;
    color: var(--la-text-dim);
    text-transform: uppercase;
    flex-shrink: 0;
  }
  .cmd-label .idx { color: var(--la-text-mute); font-weight: 200; }
  .cmd-label .sep { flex: 1; height: 1px; background: var(--la-hair-base); }

  .phase-badge {
    font-size: 9px;
    letter-spacing: 0.04em;
    font-family: var(--la-font-mono);
    flex-shrink: 0;
  }
  .phase-classifying { color: var(--la-agent-engineer); animation: pulse 1.2s ease-in-out infinite; }
  .phase-live        { color: var(--la-agent-researcher); }
  .phase-ok          { color: var(--la-agent-researcher); }
  .phase-err         { color: var(--la-agent-security); }

  @keyframes pulse {
    0%, 100% { opacity: 1; }
    50%       { opacity: 0.4; }
  }

  .classifier-badge {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    align-items: center;
  }
  .cls-pill {
    font-size: 9px;
    font-family: var(--la-font-mono);
    letter-spacing: 0.06em;
    text-transform: uppercase;
    padding: 1px 5px;
    border: 1px solid;
    border-radius: 2px;
  }
  .cls-rationale {
    font-size: 9px;
    color: var(--la-text-dim);
    font-style: italic;
    line-height: 1.4;
    width: 100%;
    margin: 0;
  }

  .agent-selector-wrap { flex: 1; min-height: 0; }

  /* ── Action panel ── */
  .cmd-action {
    padding: 12px 16px;
    display: flex;
    flex-direction: column;
    gap: 8px;
    overflow-y: auto;
  }
  .cmd-action-head {
    display: flex;
    justify-content: space-between;
    align-items: center;
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.18em;
    color: var(--la-text-dim);
    text-transform: uppercase;
    flex-shrink: 0;
  }
  .cmd-action-head .idx { color: var(--la-text-mute); font-weight: 200; }

  .cmd-state {
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.08em;
    color: var(--la-text-dim);
    font-family: var(--la-font-mono);
  }
  .cmd-state.state-armed  { color: var(--la-agent-researcher); }
  .cmd-state.state-active { color: var(--la-agent-knowledge, var(--la-agent-researcher)); }
  .cmd-state.state-err    { color: var(--la-agent-security); }

  .cmd-count {
    display: flex;
    align-items: baseline;
    gap: 8px;
    flex-shrink: 0;
  }
  .cmd-count-num {
    font-size: 36px;
    font-weight: 200;
    color: var(--la-text-stark);
    font-variant-numeric: tabular-nums;
    line-height: 1;
    letter-spacing: -0.02em;
    font-family: var(--la-font-mono);
  }
  .cmd-count-lbl {
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.18em;
    color: var(--la-text-dim);
    line-height: 1.3;
    text-transform: uppercase;
  }

  .cmd-error-msg {
    font-size: 9px;
    color: var(--la-agent-security);
    padding: 4px 8px;
    border: 1px solid color-mix(in srgb, var(--la-agent-security) 30%, transparent);
    background: color-mix(in srgb, var(--la-agent-security) 8%, transparent);
    line-height: 1.4;
    margin: 0;
  }

  .cmd-dag-mini { flex: 1; min-height: 0; overflow: hidden; }

  .cmd-btn {
    flex-shrink: 0;
    width: 100%;
    padding: 12px 16px;
    font-family: inherit;
    font-size: 12px;
    font-weight: 700;
    letter-spacing: 0.18em;
    cursor: pointer;
    text-transform: uppercase;
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-top: auto;
    transition: all 80ms var(--la-ease-mech);
    border: 1px solid;
  }
  .dispatch-arrow { font-size: 13px; font-weight: 200; }

  .cmd-btn-dispatch {
    background: var(--la-text-stark);
    color: var(--la-bg-void);
    border-color: var(--la-text-stark);
  }
  .cmd-btn-dispatch:hover:not(:disabled) {
    background: transparent;
    color: var(--la-text-stark);
  }
  .cmd-btn-dispatch:disabled {
    background: var(--la-bg-elev-2, #16181b);
    color: var(--la-text-mute);
    border-color: var(--la-hair-base);
    cursor: not-allowed;
  }
  .cmd-btn-dispatch:active:not(:disabled) { transform: scale(0.985); }

  .cmd-btn-cancel {
    background: color-mix(in srgb, var(--la-agent-security) 10%, transparent);
    color: var(--la-agent-security);
    border-color: color-mix(in srgb, var(--la-agent-security) 50%, transparent);
  }
  .cmd-btn-cancel:hover {
    background: color-mix(in srgb, var(--la-agent-security) 20%, transparent);
    border-color: var(--la-agent-security);
  }

  .cmd-btn-new {
    background: transparent;
    color: var(--la-text-base);
    border-color: var(--la-hair-strong);
  }
  .cmd-btn-new:hover { border-color: var(--la-text-dim); color: var(--la-text-bright); }

  /* ── Row 4: rail stage ── */
  .rail-stage {
    position: relative;
    display: flex;
    flex-direction: column;
    border-bottom: 1px solid var(--la-hair-base);
    overflow: hidden;
  }

  .rail-stage-head {
    display: flex;
    justify-content: space-between;
    align-items: center;
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.18em;
    color: var(--la-text-dim);
    padding: 6px 16px;
    border-bottom: 1px solid var(--la-hair-base);
    flex-shrink: 0;
    text-transform: uppercase;
  }
  .rail-stage-head .idx { color: var(--la-text-mute); font-weight: 200; }

  .view-toggle { display: flex; border: 1px solid var(--la-hair-base); }
  .vt-btn {
    background: transparent;
    border: none;
    border-right: 1px solid var(--la-hair-base);
    color: var(--la-text-dim);
    padding: 3px 10px;
    font-family: inherit;
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.08em;
    cursor: pointer;
  }
  .vt-btn:last-child { border-right: none; }
  .vt-btn.vt-active { color: var(--la-text-stark); }
  .vt-btn:disabled { cursor: default; opacity: 0.7; }

  .rails-wrap { flex: 1; overflow: hidden; position: relative; }

  /* ── Row 5: mailbox ── */
  .mailbox {
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }
  .mailbox-head {
    display: flex;
    justify-content: space-between;
    align-items: center;
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.18em;
    color: var(--la-text-dim);
    padding: 6px 16px;
    border-bottom: 1px solid var(--la-hair-base);
    flex-shrink: 0;
    text-transform: uppercase;
  }
  .mailbox-head .idx { color: var(--la-text-mute); font-weight: 200; }
  .msg-ctr { color: var(--la-text-bright); font-variant-numeric: tabular-nums; font-family: var(--la-font-mono); }

  .mailbox-body { flex: 1; overflow-y: auto; min-height: 0; }

  /* ── cinematic dispatch flash ── */
  .dispatch-flash {
    position: fixed;
    inset: 0;
    background: var(--la-text-stark);
    pointer-events: none;
    z-index: 40;
    animation: dispatch-flash 0.45s var(--la-ease-mech) forwards;
  }
  @keyframes dispatch-flash {
    0%   { opacity: 0; }
    15%  { opacity: 0.06; }
    100% { opacity: 0; }
  }

  /* ── scanline sweep through rail stage on dispatch ── */
  .sd-scanline {
    position: absolute;
    left: 0; right: 0;
    top: -3px;
    height: 2px;
    background: linear-gradient(
      to right,
      transparent 0%,
      var(--la-agent-researcher) 30%,
      color-mix(in srgb, var(--la-agent-researcher) 80%, white) 50%,
      var(--la-agent-researcher) 70%,
      transparent 100%
    );
    pointer-events: none;
    z-index: 10;
    animation: sd-scanline 0.65s var(--la-ease-mech) forwards;
  }
  @keyframes sd-scanline {
    0%   { top: -3px; opacity: 0; }
    8%   { opacity: 1; }
    92%  { opacity: 1; }
    100% { top: calc(100% + 3px); opacity: 0; }
  }
</style>
