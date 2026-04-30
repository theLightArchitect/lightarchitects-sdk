<script lang="ts">
  import { onDestroy } from 'svelte';
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
    type DomainAgent,
    type Classification,
    type DispatchEvent,
    type AgentLiveState,
    type DispatchHistoryEntry,
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

  async function dispatch(taskText: string, isDry: boolean) {
    if (selectedAgents.length === 0) return;
    errorMsg = null;
    events = [];
    agentStates = new Map();
    elapsedMs = undefined;

    let id: string;
    try {
      phase = 'streaming';
      id = await executeDispatch(taskText, selectedAgents, isDry);
      dispatchId = id;
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
      const { agent, state, message } = e.PerAgentState;
      agentStates = new Map(agentStates).set(agent, {
        agent,
        state,
        messages: message
          ? [...(agentStates.get(agent)?.messages ?? []), message]
          : (agentStates.get(agent)?.messages ?? []),
      });
    } else if ('MailboxMessage' in e) {
      const { agent, text } = e.MailboxMessage;
      const existing = agentStates.get(agent);
      agentStates = new Map(agentStates).set(agent, {
        agent,
        state: existing?.state ?? 'running',
        messages: [...(existing?.messages ?? []), text],
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
  });

  const isLive = $derived(phase === 'streaming');
  const isTerminalPhase = $derived(phase === 'complete' || phase === 'error');
</script>

<div class="flex h-full bg-[#070d1a] text-[#e2e8f0] overflow-hidden">

  <!-- ── Left rail: input + agent selector ── -->
  <div class="flex flex-col w-72 flex-shrink-0 border-r border-[#0f172a] overflow-y-auto">
    <div class="px-3 pt-3 pb-1 border-b border-[#0f172a]">
      <div class="flex items-center justify-between mb-2">
        <h2 class="text-[11px] font-semibold text-[#e2e8f0] tracking-wide uppercase">
          Squad Dispatch
        </h2>
        {#if phase === 'classifying'}
          <span class="text-[9px] text-[#3b82f6] animate-pulse">Classifying…</span>
        {:else if phase === 'complete'}
          <span class="text-[9px] text-[#10b981]">
            ✓ {elapsedMs !== undefined ? `${(elapsedMs / 1000).toFixed(1)}s` : 'Done'}
          </span>
        {:else if phase === 'error'}
          <span class="text-[9px] text-[#ef4444]">✗ Error</span>
        {/if}
      </div>

      <DispatchInput
        bind:task
        bind:dry
        disabled={isLive}
        onSubmit={dispatch}
        onTaskChange={(t) => { task = t; }}
      />
    </div>

    <div class="px-3 py-2 border-b border-[#0f172a]">
      <AgentSelector
        bind:selected={selectedAgents}
        {classification}
        disabled={isLive}
      />
    </div>

    {#if isLive || isTerminalPhase}
      <div class="px-3 py-2 border-b border-[#0f172a]">
        <div class="flex items-center justify-between mb-1">
          <span class="text-[9px] text-[#475569] uppercase tracking-wider">Pipeline</span>
        </div>
        <TaskDAG agents={selectedAgents} {agentStates} />
      </div>
    {/if}

    <div class="flex gap-2 px-3 py-2">
      {#if isLive}
        <button
          onclick={cancel}
          class="flex-1 py-1 text-[10px] rounded border border-[#ef4444]/40
                 text-[#ef4444] hover:border-[#ef4444] hover:bg-[#ef4444]/10 transition-colors"
        >
          Cancel
        </button>
      {:else if isTerminalPhase}
        <button
          onclick={reset}
          class="flex-1 py-1 text-[10px] rounded border border-[#1e293b]
                 text-[#94a3b8] hover:border-[#334155] transition-colors"
        >
          New Dispatch
        </button>
      {/if}
    </div>
  </div>

  <!-- ── Centre: live agent grid + mailbox stream ── -->
  <div class="flex flex-col flex-1 min-w-0 overflow-hidden">

    <!-- Agent cards -->
    <div class="px-3 pt-3 pb-2 border-b border-[#0f172a]">
      <p class="text-[9px] text-[#475569] uppercase tracking-wider mb-2">Live agents</p>
      <LiveAgentGrid agents={isLive || isTerminalPhase ? selectedAgents : []} {agentStates} />
    </div>

    <!-- Error banner -->
    {#if errorMsg}
      <div class="mx-3 mt-2 px-3 py-2 rounded border border-[#ef4444]/30 bg-[#ef4444]/10
                  text-[10px] text-[#ef4444]">
        {errorMsg}
      </div>
    {/if}

    <!-- Mailbox stream -->
    <div class="flex-1 min-h-0 relative px-3 py-2">
      <p class="text-[9px] text-[#475569] uppercase tracking-wider mb-1">Event stream</p>
      <div class="h-full">
        <MailboxStream {events} />
      </div>
    </div>
  </div>

  <!-- ── Right rail: history ── -->
  <div class="w-52 flex-shrink-0 border-l border-[#0f172a] overflow-y-auto">
    <HistoryRail
      {history}
      onSelect={replayFromHistory}
      onClear={clearHistory}
    />
  </div>

</div>
