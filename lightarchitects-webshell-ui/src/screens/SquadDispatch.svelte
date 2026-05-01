<script lang="ts">
  import { onDestroy } from 'svelte';
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
    type DomainAgent,
    type Classification,
    type DispatchEvent,
    type AgentLiveState,
    type DispatchHistoryEntry,
    type FileAttachment,
  } from '$lib/dispatch';

  import DispatchInput from '$lib/../components/dispatch/DispatchInput.svelte';
  import AgentSelector from '$lib/../components/dispatch/AgentSelector.svelte';
  import TaskDAG from '$lib/../components/dispatch/TaskDAG.svelte';
  import LiveAgentGrid from '$lib/../components/dispatch/LiveAgentGrid.svelte';
  import MailboxStream from '$lib/../components/dispatch/MailboxStream.svelte';
  import HistoryRail from '$lib/../components/dispatch/HistoryRail.svelte';

  // ── State ──────────────────────────────────────────────────────────────────

  type Phase = 'idle' | 'classifying' | 'ready' | 'streaming' | 'complete' | 'error';

  let phase = $state<Phase>('idle');
  let task = $state('');
  let dry = $state(false);
  let selectedAgents = $state<DomainAgent[]>([]);
  let classification = $state<Classification | null>(null);
  let dispatchId = $state<string | null>(null);
  let attachments = $state<FileAttachment[]>([]);
  let events = $state<DispatchEvent[]>([]);
  let agentStates = $state(new Map<DomainAgent, AgentLiveState>());
  let history = $state<DispatchHistoryEntry[]>(loadHistory());
  let errorMsg = $state<string | null>(null);
  let elapsedMs = $state<number | undefined>(undefined);

  let stopStream: (() => void) | null = null;
  let classifyTimer: ReturnType<typeof setTimeout> | null = null;

  // ── Auto-classify with debounce ──────────────────────────────────────────────

  $effect(() => {
    const t = task;
    if (classifyTimer) clearTimeout(classifyTimer);
    if (t.trim().length < 8 || phase === 'streaming') return;

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

  async function dispatch(taskText: string, isDry: boolean, atts: FileAttachment[] = []) {
    if (selectedAgents.length === 0) return;
    errorMsg = null;
    events = [];
    agentStates = new Map();
    elapsedMs = undefined;

    let id: string;
    try {
      phase = 'streaming';
      id = await executeDispatch(taskText, selectedAgents, isDry, atts);
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
    agentStates = new Map();
    dispatchId = null;
    errorMsg = null;
    elapsedMs = undefined;
    task = '';
    selectedAgents = [];
    classification = null;
    attachments = [];
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
        handler: () => { if (phase !== 'streaming') dispatch(task, dry); },
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

<div class="sd-root" data-dispatching={isLive}>

  <!-- ── Left rail: input + agent selector ── -->
  <aside class="sd-rail sd-rail-left" aria-label="Dispatch controls">

    <header class="sd-section-header">
      <span class="sd-section-label">Squad Dispatch</span>
      {#if phase === 'classifying'}
        <span class="sd-phase sd-phase-classifying">classifying…</span>
      {:else if phase === 'streaming'}
        <span class="sd-phase sd-phase-live">● live</span>
      {:else if phase === 'complete'}
        <span class="sd-phase sd-phase-ok">
          ✓ {elapsedMs !== undefined ? `${(elapsedMs / 1000).toFixed(1)}s` : 'done'}
        </span>
      {:else if phase === 'error'}
        <span class="sd-phase sd-phase-err">✗ error</span>
      {/if}
    </header>

    <div class="sd-section-body" data-onboarding="dispatch-input" bind:this={inputSectionEl}>
      <DispatchInput
        bind:task
        bind:dry
        bind:attachments
        disabled={isLive}
        onSubmit={dispatch}
        onTaskChange={(t) => { task = t; }}
      />
    </div>

    {#if classification && task.trim().length >= 8}
      <div class="sd-classifier-badge">
        <div class="sd-classifier-agents">
          {#each classification.agents as agent}
            <span
              class="sd-agent-pill"
              style="color: var(--la-agent-{agent}); border-color: color-mix(in srgb, var(--la-agent-{agent}) 40%, transparent)"
            >
              {DOMAIN_AGENT_LABELS[agent]}
            </span>
          {/each}
        </div>
        {#if classification.rationale}
          <p class="sd-classifier-rationale">
            {classification.rationale.length > 80
              ? classification.rationale.slice(0, 80) + '…'
              : classification.rationale}
          </p>
        {/if}
      </div>
    {/if}

    <div class="sd-section-body" data-onboarding="dispatch-agent-selector">
      <AgentSelector
        bind:selected={selectedAgents}
        {classification}
        disabled={isLive}
      />
    </div>

    {#if isLive || isTerminalPhase}
      <div class="sd-section-body" data-testid="task-dag-toggle">
        <p class="sd-meta-label">Pipeline</p>
        <TaskDAG agents={selectedAgents} {agentStates} />
      </div>
    {/if}

    <div class="sd-action-row">
      {#if isLive}
        <button class="sd-btn sd-btn-danger" onclick={cancel}>Cancel</button>
      {:else if isTerminalPhase}
        <button class="sd-btn sd-btn-ghost" onclick={reset}>New Dispatch</button>
      {/if}
    </div>
  </aside>

  <!-- ── Centre: live agents + event stream ── -->
  <main class="sd-centre" aria-label="Live agent workspace">

    <!-- Scanline sweep overlay — plays once per dispatch -->
    {#if scanlineActive}
      {#key scanlineKey}
        <div class="sd-scanline" aria-hidden="true"></div>
      {/key}
    {/if}

    <section class="sd-agents-section" data-onboarding="dispatch-live-grid">
      <p class="sd-meta-label">Live agents</p>
      <LiveAgentGrid agents={isLive || isTerminalPhase ? selectedAgents : []} {agentStates} />
    </section>

    {#if errorMsg}
      <div class="sd-error-banner" role="alert">{errorMsg}</div>
    {/if}

    <section class="sd-stream-section" data-onboarding="dispatch-mailbox">
      <p class="sd-meta-label">Event stream</p>
      <MailboxStream {events} />
    </section>
  </main>

  <!-- ── Right rail: history ── -->
  <aside class="sd-rail sd-rail-right" data-onboarding="dispatch-history" aria-label="Dispatch history">
    <HistoryRail
      {history}
      onSelect={replayFromHistory}
      onClear={clearHistory}
    />
  </aside>

</div>

<style>
  /* ── Root shell ── */
  .sd-root {
    position: relative;
    display: flex;
    height: 100%;
    background: var(--la-bg-void);
    color: var(--la-text-bright);
    overflow: hidden;
    font-family: var(--la-font-chrome);
  }

  /* ── Rails ── */
  .sd-rail {
    display: flex;
    flex-direction: column;
    flex-shrink: 0;
    overflow-y: auto;
  }
  .sd-rail-left  { width: 17rem; border-right: 1px solid var(--la-hair-base); }
  .sd-rail-right { width: 13rem; border-left:  1px solid var(--la-hair-base); }

  /* ── Centre column ── */
  .sd-centre {
    position: relative;
    display: flex;
    flex-direction: column;
    flex: 1;
    min-width: 0;
    overflow: hidden;
  }

  /* ── Section primitives ── */
  .sd-section-header {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 12px 8px;
    border-bottom: 1px solid var(--la-hair-base);
    flex-shrink: 0;
  }
  .sd-section-label {
    font-size: 10px;
    font-weight: 600;
    letter-spacing: var(--la-tk-loose);
    text-transform: uppercase;
    color: var(--la-text-stark);
    flex: 1;
  }
  .sd-section-body {
    padding: 8px 12px;
    border-bottom: 1px solid var(--la-hair-base);
  }
  .sd-agents-section {
    padding: 10px 12px 8px;
    border-bottom: 1px solid var(--la-hair-base);
    flex-shrink: 0;
  }
  .sd-stream-section {
    flex: 1;
    min-height: 0;
    padding: 8px 12px;
    display: flex;
    flex-direction: column;
  }
  .sd-meta-label {
    font-size: 9px;
    letter-spacing: var(--la-tk-mid);
    text-transform: uppercase;
    color: var(--la-text-dim);
    margin: 0 0 6px;
  }

  /* ── Phase badges ── */
  .sd-phase {
    font-size: 9px;
    font-family: var(--la-font-mono);
    letter-spacing: 0.04em;
  }
  .sd-phase-classifying { color: var(--la-agent-engineer); animation: pulse 1.2s ease-in-out infinite; }
  .sd-phase-live        { color: var(--la-agent-researcher); }
  .sd-phase-ok          { color: var(--la-agent-researcher); }
  .sd-phase-err         { color: var(--la-agent-security); }

  /* ── Action row ── */
  .sd-action-row {
    display: flex;
    gap: 8px;
    padding: 8px 12px;
    margin-top: auto;
  }
  .sd-btn {
    flex: 1;
    padding: 4px 0;
    font-size: 10px;
    font-family: var(--la-font-chrome);
    border-radius: var(--la-radius-md);
    border: 1px solid transparent;
    cursor: pointer;
    transition: all var(--la-t-snap) var(--la-ease-mech);
  }
  .sd-btn-danger {
    background: var(--la-danger-bg);
    border-color: var(--la-danger-stroke);
    color: var(--la-danger-text);
  }
  .sd-btn-danger:hover { box-shadow: 0 0 0 2px var(--la-danger-glow); }
  .sd-btn-ghost {
    background: transparent;
    border-color: var(--la-hair-strong);
    color: var(--la-text-base);
  }
  .sd-btn-ghost:hover { border-color: var(--la-text-dim); color: var(--la-text-bright); }

  /* ── Error banner ── */
  .sd-error-banner {
    margin: 8px 12px 0;
    padding: 6px 10px;
    border-radius: var(--la-radius-md);
    background: var(--la-danger-bg);
    border: 1px solid var(--la-danger-stroke);
    color: var(--la-danger-text);
    font-size: 10px;
  }

  /* ── Cinematic: full-screen flash on dispatch ── */
  .dispatch-flash {
    position: fixed;
    inset: 0;
    background: var(--la-agent-researcher);
    pointer-events: none;
    z-index: 40;
    animation: dispatch-flash 0.45s var(--la-ease-mech) forwards;
  }
  @keyframes dispatch-flash {
    0%   { opacity: 0; }
    15%  { opacity: 0.12; }
    100% { opacity: 0; }
  }

  /* ── Cinematic: scanline sweep in centre column ── */
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

  /* ── Corner bracket pulse when dispatching ── */
  [data-dispatching="true"] {
    --corner-bracket-size: 18px;
  }

  @keyframes pulse {
    0%, 100% { opacity: 1; }
    50%       { opacity: 0.4; }
  }

  /* ── Classifier badge ── */
  .sd-classifier-badge {
    padding: 6px 12px;
    border-bottom: 1px solid var(--la-hair-base);
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .sd-classifier-agents {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }
  .sd-agent-pill {
    font-size: 9px;
    font-family: var(--la-font-mono);
    letter-spacing: 0.06em;
    text-transform: uppercase;
    padding: 1px 5px;
    border-radius: 2px;
    border: 1px solid;
  }
  .sd-classifier-rationale {
    font-size: 9px;
    color: var(--la-text-dim);
    margin: 0;
    line-height: 1.4;
    font-style: italic;
  }
</style>
