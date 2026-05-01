<script lang="ts">
  import { arenaStatus, arenaStats, trainingConfig, trainingRun } from '$lib/stores';
  import { SIBLING_COLORS } from '$lib/design-tokens';
  import { api } from '$lib/api';
  import type { ArenaAgent, ExerciseType, ScoringDimension, DatasetSource } from '$lib/types';

  interface Props {
    onAgentClick?: (agent: ArenaAgent) => void;
  }

  let { onAgentClick }: Props = $props();

  // --- Training configuration ---
  const EXERCISE_TYPES: { value: ExerciseType; label: string; icon: string }[] = [
    { value: 'code_review', label: 'Code Review', icon: 'R' },
    { value: 'bug_fix', label: 'Bug Fix', icon: 'B' },
    { value: 'refactor', label: 'Refactor', icon: 'F' },
    { value: 'test_gen', label: 'Test Gen', icon: 'T' },
    { value: 'architecture', label: 'Architecture', icon: 'A' },
    { value: 'security_audit', label: 'Security Audit', icon: 'S' },
    { value: 'optimization', label: 'Optimization', icon: 'O' },
  ];

  const SCORING_DIMENSIONS: ScoringDimension[] = [
    'correctness', 'completeness', 'efficiency', 'style',
    'security', 'robustness', 'clarity', 'innovation',
  ];

  const DATASET_SOURCES: { value: DatasetSource; label: string }[] = [
    { value: 'current_project', label: 'Current Project' },
    { value: 'helix_history', label: 'Helix History' },
    { value: 'custom_path', label: 'Custom Path' },
  ];

  let trainingError = $state<string | null>(null);
  let showTraining = $state(true);
  // Cost gate — shows preview before dispatching a real training run.
  let showCostGate = $state(false);

  // Derived state
  let canStartTraining = $derived($trainingConfig.exerciseType !== '' && !$trainingRun);
  let isRunning = $derived($trainingRun?.status === 'running');
  let isComplete = $derived($trainingRun?.status === 'complete');
  let isFailed = $derived($trainingRun?.status === 'failed');
  let elapsed = $state('0:00');
  let elapsedInterval: ReturnType<typeof setInterval> | null = null;

  function updateElapsed(): void {
    const run = $trainingRun;
    if (!run?.startedAt) { elapsed = '0:00'; return; }
    const end = run.completedAt ?? Date.now();
    const secs = Math.floor((end - run.startedAt) / 1000);
    const mins = Math.floor(secs / 60);
    const remSecs = secs % 60;
    elapsed = `${mins}:${remSecs.toString().padStart(2, '0')}`;
  }

  $effect(() => {
    // Always clear first — $effect re-runs on any $trainingRun change,
    // which would leak intervals if we only clear in the else branch.
    if (elapsedInterval) { clearInterval(elapsedInterval); elapsedInterval = null; }
    if ($trainingRun?.status === 'running') {
      elapsedInterval = setInterval(updateElapsed, 1000);
    }
    updateElapsed();
    return () => { if (elapsedInterval) { clearInterval(elapsedInterval); elapsedInterval = null; } };
  });

  function requestStartTraining(): void {
    trainingError = null;
    showCostGate = true;
  }

  async function confirmStartTraining(): Promise<void> {
    showCostGate = false;
    trainingError = null;
    try {
      const result = await api.startTraining($trainingConfig);
      trainingRun.set({
        id: result.run_id,
        status: 'running',
        progress: 0,
        startedAt: Date.now(),
      });
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      if (msg.includes('404') || msg.includes('501')) {
        trainingError = 'Backend not configured — Arena training endpoint unavailable';
      } else {
        trainingError = `Training failed: ${msg}`;
      }
    }
  }

  function handleReset(): void {
    trainingRun.set(null);
    trainingError = null;
  }

  function scoreColor(score: number): string {
    if (score > 80) return '#22c55e';
    if (score >= 50) return '#f59e0b';
    return '#ef4444';
  }

  function agentStatusColor(status: ArenaAgent['status']): string {
    switch (status) {
      case 'active': return '#22c55e';
      case 'idle': return '#6b7280';
      case 'error': return '#ef4444';
    }
  }

  function formatHeartbeat(iso: string): string {
    const d = new Date(iso);
    const now = Date.now();
    const diff = now - d.getTime();
    if (diff < 10000) return 'just now';
    if (diff < 60000) return `${Math.floor(diff / 1000)}s`;
    return `${Math.floor(diff / 60000)}m`;
  }

  // Sort agents: active first, then idle, then error
  let sortedAgents = $derived(
    [...$arenaStatus.agents].sort((a, b) => {
      const order: Record<string, number> = { active: 0, idle: 1, error: 2 };
      return order[a.status] - order[b.status];
    })
  );
</script>

<div class="bg-[#111827] border border-[#1e293b] rounded-lg overflow-hidden">
  <!-- Header -->
  <div class="px-4 py-2 border-b border-[#1e293b] flex items-center justify-between">
    <h3 class="text-xs font-medium text-[#94a3b8]">ARENA STATUS</h3>
    <div class="flex items-center gap-3 text-[10px]">
      <span class="text-[#22c55e]">{$arenaStats.activeAgents} active</span>
      <span class="text-[#6b7280]">{$arenaStats.idleAgents} idle</span>
    </div>
  </div>

  <!-- Routine counts -->
  <div class="px-4 py-2 bg-[#0d1117] border-b border-[#1e293b] flex items-center gap-4">
    <div class="flex items-center gap-2">
      <span class="text-[10px] text-[#64748b]">Active Routines:</span>
      <span class="text-[12px] font-semibold text-[#22c55e]">{$arenaStatus.activeRoutines}</span>
    </div>
    <div class="flex items-center gap-2">
      <span class="text-[10px] text-[#64748b]">Queued:</span>
      <span class="text-[12px] font-semibold text-[#f59e0b]">{$arenaStatus.queuedRoutines}</span>
    </div>
  </div>

  <!-- Agent list -->
  <div class="divide-y divide-[#1e293b]">
    {#each sortedAgents as agent (agent.id)}
      {@const sibColor = SIBLING_COLORS[agent.sibling] ?? '#6b7280'}
      {@const stColor = agentStatusColor(agent.status)}

      <button
        class="w-full text-left px-4 py-2 flex items-center gap-3 hover:bg-[#0d1117] transition-colors"
        onclick={() => onAgentClick?.(agent)}
      >
        <!-- Status pulse -->
        <div class="relative flex-shrink-0">
          <div
            class="w-2 h-2 rounded-full"
            style="background-color: {stColor}; {agent.status === 'active' ? `box-shadow: 0 0 6px ${stColor}` : ''}"
          ></div>
          {#if agent.status === 'active'}
            <div
              class="absolute inset-0 w-2 h-2 rounded-full animate-ping"
              style="background-color: {stColor}; opacity: 0.5"
            ></div>
          {/if}
        </div>

        <!-- Sibling badge -->
        <div
          class="flex-shrink-0 w-6 h-6 rounded flex items-center justify-center text-[8px] font-bold"
          style="background-color: {sibColor}20; color: {sibColor}"
        >
          {agent.sibling.slice(0, 2).toUpperCase()}
        </div>

        <!-- Agent info -->
        <div class="flex-1 min-w-0">
          <div class="flex items-center gap-2">
            <span class="text-[11px] text-[#e2e8f0]">{agent.id}</span>
            <span
              class="text-[9px] px-1.5 py-0.5 rounded"
              style="background-color: {stColor}20; color: {stColor}"
            >
              {agent.status}
            </span>
          </div>
          <div class="flex items-center gap-2 text-[9px] text-[#475569]">
            <span>heartbeat: {formatHeartbeat(agent.lastHeartbeat)}</span>
            {#if agent.currentBuildId}
              <span>&middot;</span>
              <span class="text-[#FFD700]">{agent.currentBuildId.slice(-8)}</span>
            {/if}
          </div>
        </div>

        <!-- Routine count -->
        <div class="text-[10px] text-[#94a3b8]">
          {agent.routineCount} routines
        </div>
      </button>
    {/each}
  </div>

  <!-- ═══════════════════════════════════════════════════════════════════════ -->
  <!-- AGENT TRAINING SECTION                                                -->
  <!-- ═══════════════════════════════════════════════════════════════════════ -->
  <div class="border-t border-[#1e293b]">
    <!-- Training section header -->
    <button
      class="w-full px-4 py-2.5 flex items-center justify-between hover:bg-[#0d1117] transition-colors"
      onclick={() => showTraining = !showTraining}
    >
      <div class="flex items-center gap-2">
        <h3 class="text-xs font-medium text-[#94a3b8]">ARENA TRAINING</h3>
        <span class="text-[8px] px-1.5 py-0.5 rounded bg-[#FFD700]/10 text-[#FFD700] font-semibold">Pro</span>
      </div>
      <svg
        class="w-3 h-3 text-[#475569] transition-transform"
        class:rotate-180={showTraining}
        viewBox="0 0 12 12"
        fill="none"
        stroke="currentColor"
        stroke-width="1.5"
      >
        <path d="M3 5l3 3 3-3" />
      </svg>
    </button>

    {#if showTraining}
      <div class="px-4 pb-3 space-y-3">

        <!-- ─── Active training run display ─── -->
        {#if $trainingRun}
          <div class="space-y-2">
            <!-- Progress bar -->
            <div class="space-y-1">
              <div class="flex items-center justify-between">
                <span class="text-[10px] text-[#94a3b8]">
                  {isRunning ? 'Training in progress...' : isComplete ? 'Training complete' : 'Training failed'}
                </span>
                <div class="flex items-center gap-2">
                  <span class="text-[10px] text-[#475569]">{elapsed}</span>
                  <span class="text-[10px] font-mono text-[#e2e8f0]">{$trainingRun.progress}%</span>
                </div>
              </div>
              <div class="w-full h-1.5 bg-[#1e293b] rounded-full overflow-hidden">
                <div
                  class="h-full rounded-full transition-all duration-300"
                  style="width: {$trainingRun.progress}%; background: {isComplete ? '#22c55e' : isFailed ? '#ef4444' : 'linear-gradient(90deg, #FFD700, #f59e0b)'}"
                ></div>
              </div>
            </div>

            <!-- Results (when complete) -->
            {#if isComplete && $trainingRun.results}
              {@const r = $trainingRun.results}
              {@const sc = scoreColor(r.score)}
              <div class="bg-[#0d1117] rounded-lg p-3 space-y-2">
                <div class="flex items-center justify-between">
                  <span class="text-[10px] text-[#64748b]">Score</span>
                  <span
                    class="text-sm font-bold px-2 py-0.5 rounded"
                    style="background-color: {sc}20; color: {sc}"
                  >
                    {r.score}%
                  </span>
                </div>
                <div class="flex items-center gap-4 text-[10px]">
                  <span class="text-[#94a3b8]">Exercises: <span class="text-[#e2e8f0] font-medium">{r.exercises}</span></span>
                  <span class="text-[#22c55e]">Passed: {r.passed}</span>
                  <span class="text-[#ef4444]">Failed: {r.exercises - r.passed}</span>
                </div>
              </div>
            {/if}

            <!-- Failed state -->
            {#if isFailed}
              <div class="bg-[#ef4444]/10 border border-[#ef4444]/20 rounded px-3 py-2">
                <span class="text-[10px] text-[#ef4444]">Training run failed. Check backend logs for details.</span>
              </div>
            {/if}

            <!-- Reset button -->
            {#if isComplete || isFailed}
              <button
                class="w-full text-[10px] py-1.5 rounded bg-[#1e293b] text-[#94a3b8] hover:bg-[#1e293b]/80 hover:text-[#e2e8f0] transition-colors"
                onclick={handleReset}
              >
                Configure New Run
              </button>
            {/if}
          </div>

        {:else}
          <!-- ─── Configuration form ─── -->
          <div class="space-y-3">

            <!-- Exercise type selector -->
            <div class="space-y-1.5">
              <span class="text-[10px] text-[#64748b] font-medium block">Exercise Type</span>
              <div class="grid grid-cols-2 gap-1.5">
                {#each EXERCISE_TYPES as ex}
                  {@const selected = $trainingConfig.exerciseType === ex.value}
                  <button
                    class="flex items-center gap-2 px-2.5 py-1.5 rounded text-left transition-colors border {selected ? 'border-amber-500/40 bg-amber-500/5' : 'border-[#1e293b] bg-[#0d1117] hover:border-[#334155]'}"
                    onclick={() => trainingConfig.update(c => ({ ...c, exerciseType: ex.value }))}
                  >
                    <span
                      class="w-5 h-5 rounded flex items-center justify-center text-[8px] font-bold flex-shrink-0 {selected ? 'bg-amber-500/20 text-[#FFD700]' : 'bg-[#1e293b] text-[#64748b]'}"
                    >
                      {ex.icon}
                    </span>
                    <span class="text-[10px] truncate {selected ? 'text-[#e2e8f0]' : 'text-[#94a3b8]'}">
                      {ex.label}
                    </span>
                  </button>
                {/each}
              </div>
            </div>

            <!-- Scoring weight sliders -->
            <div class="space-y-1.5">
              <span class="text-[10px] text-[#64748b] font-medium block">Scoring Weights</span>
              <div class="grid grid-cols-2 gap-x-3 gap-y-1">
                {#each SCORING_DIMENSIONS as dim}
                  <div class="flex items-center gap-2">
                    <span class="text-[9px] text-[#475569] w-[60px] truncate capitalize">{dim}</span>
                    <input
                      type="range"
                      min="0"
                      max="100"
                      value={$trainingConfig.weights[dim]}
                      oninput={(e) => {
                        const val = parseInt((e.target as HTMLInputElement).value);
                        trainingConfig.update(c => ({
                          ...c,
                          weights: { ...c.weights, [dim]: val },
                        }));
                      }}
                      class="flex-1 h-1 appearance-none bg-[#1e293b] rounded-full accent-[#FFD700] cursor-pointer
                        [&::-webkit-slider-thumb]:appearance-none [&::-webkit-slider-thumb]:w-2.5 [&::-webkit-slider-thumb]:h-2.5
                        [&::-webkit-slider-thumb]:rounded-full [&::-webkit-slider-thumb]:bg-[#FFD700]"
                    />
                    <span class="text-[9px] text-[#64748b] w-5 text-right font-mono">{$trainingConfig.weights[dim]}</span>
                  </div>
                {/each}
              </div>
            </div>

            <!-- Dataset source -->
            <div class="space-y-1.5">
              <span class="text-[10px] text-[#64748b] font-medium block">Dataset Source</span>
              <div class="flex items-center gap-3">
                {#each DATASET_SOURCES as src}
                  <label class="flex items-center gap-1.5 cursor-pointer">
                    <input
                      type="radio"
                      name="dataset-source"
                      value={src.value}
                      checked={$trainingConfig.datasetSource === src.value}
                      onchange={() => trainingConfig.update(c => ({ ...c, datasetSource: src.value }))}
                      class="w-3 h-3 accent-[#FFD700]"
                    />
                    <span class="text-[10px] text-[#94a3b8]">{src.label}</span>
                  </label>
                {/each}
              </div>

              {#if $trainingConfig.datasetSource === 'custom_path'}
                <input
                  type="text"
                  placeholder="/path/to/dataset"
                  value={$trainingConfig.customPath ?? ''}
                  oninput={(e) => trainingConfig.update(c => ({ ...c, customPath: (e.target as HTMLInputElement).value }))}
                  class="w-full mt-1 px-2.5 py-1.5 text-[10px] bg-[#0d1117] border border-[#1e293b] rounded text-[#e2e8f0] placeholder-[#475569] focus:border-[#FFD700]/40 focus:outline-none"
                />
              {/if}
            </div>

            <!-- Error display -->
            {#if trainingError}
              <div class="bg-[#ef4444]/10 border border-[#ef4444]/20 rounded px-3 py-2">
                <span class="text-[10px] text-[#ef4444]">{trainingError}</span>
              </div>
            {/if}

            <!-- Cost gate preview -->
            {#if showCostGate}
              <div class="bg-[#0d1117] border border-[#f59e0b]/30 rounded-lg p-3 space-y-2">
                <div class="flex items-center gap-2">
                  <span class="text-[9px] px-1.5 py-0.5 rounded bg-[#f59e0b]/15 text-[#f59e0b] font-semibold">PREVIEW</span>
                  <span class="text-[10px] text-[#94a3b8]">Estimated run</span>
                </div>
                <div class="grid grid-cols-3 gap-2 text-center">
                  <div>
                    <div class="text-xs font-bold text-[#e2e8f0]">~10</div>
                    <div class="text-[9px] text-[#475569]">exercises</div>
                  </div>
                  <div>
                    <div class="text-xs font-bold text-[#e2e8f0]">~8m</div>
                    <div class="text-[9px] text-[#475569]">duration</div>
                  </div>
                  <div>
                    <div class="text-xs font-bold text-[#FFD700]">local</div>
                    <div class="text-[9px] text-[#475569]">no API cost</div>
                  </div>
                </div>
                <div class="flex gap-2 pt-1">
                  <button
                    class="flex-1 py-1.5 rounded text-[10px] bg-[#1e293b] text-[#94a3b8] hover:text-[#e2e8f0] transition-colors"
                    onclick={() => showCostGate = false}
                  >Cancel</button>
                  <button
                    class="flex-1 py-1.5 rounded text-[10px] font-medium bg-gradient-to-r from-[#FFD700] to-[#f59e0b] text-[#0d1117] hover:brightness-110 transition-all"
                    onclick={confirmStartTraining}
                  >Confirm &amp; Run</button>
                </div>
              </div>
            {:else}
              <!-- Start button -->
              <button
                class="w-full py-2 rounded text-[11px] font-medium transition-all {canStartTraining ? 'bg-gradient-to-r from-[#FFD700] to-[#f59e0b] text-[#0d1117] hover:brightness-110' : 'bg-[#1e293b] text-[#475569] cursor-not-allowed'}"
                disabled={!canStartTraining}
                onclick={requestStartTraining}
              >
                Start Training
              </button>
            {/if}
          </div>
        {/if}
      </div>
    {/if}
  </div>
</div>
