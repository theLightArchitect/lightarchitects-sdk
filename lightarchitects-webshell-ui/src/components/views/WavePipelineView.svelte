<script lang="ts">
  import type { WavePipelineViewProps, Phase, Wave, WaveTask } from '$lib/WavePipelineView.contract';

  let {
    mode,
    phases,
    selectedWaveId,
    onTaskClick,
    onGateClick,
  }: WavePipelineViewProps = $props();

  // ── Status helpers ────────────────────────────────────────────────────────

  function statusClass(status: Phase['status'] | Wave['status'] | WaveTask['status']): string {
    if (status === 'completed') return 'st-done';
    if (status === 'in_progress') return 'st-active';
    if (status === 'failed') return 'st-fail';
    return 'st-pending';
  }

  function statusGlyph(status: Phase['status']): string {
    if (status === 'completed') return '✓';
    if (status === 'in_progress') return '●';
    if (status === 'failed') return '✗';
    return '○';
  }

  function gateLabel(overall: 'pass' | 'hitl' | 'fail'): string {
    if (overall === 'pass') return 'GATE ✓';
    if (overall === 'hitl') return 'HITL ⏸';
    return 'GATE ✗';
  }

  function gateClass(overall: 'pass' | 'hitl' | 'fail'): string {
    if (overall === 'pass') return 'gate-pass';
    if (overall === 'hitl') return 'gate-hitl';
    return 'gate-fail';
  }

  function taskStatusTitle(t: WaveTask): string {
    const parts: string[] = [t.title];
    if (t.agent_key) parts.push(`Agent: ${t.agent_key}`);
    if (t.status === 'in_progress' && t.started_at) parts.push(`Started ${t.started_at.slice(11, 16)}`);
    if (t.status === 'completed' && t.completed_at) parts.push(`Done ${t.completed_at.slice(11, 16)}`);
    return parts.join(' · ');
  }
</script>

<div class="wave-pipeline" data-mode={mode}>
  {#if phases.length === 0}
    <div class="empty">No phases loaded yet.</div>
  {:else}
    <div class="phase-list">
      {#each phases as phase (phase.id)}
        <div class="phase-block" data-status={phase.status}>
          <!-- Phase header -->
          <div class="phase-header">
            <span class="phase-glyph {statusClass(phase.status)}" aria-hidden="true">
              {statusGlyph(phase.status)}
            </span>
            <span class="phase-label">{phase.label}</span>
          </div>

          {#if mode === 'full' || phase.status === 'in_progress'}
            <!-- Wave rows -->
            <div class="wave-list">
              {#each phase.waves as wave (wave.id)}
                {@const isSelected = wave.id === selectedWaveId}
                <div
                  class="wave-row"
                  class:selected={isSelected}
                  class:wave-active={wave.status === 'in_progress'}
                  aria-label="Wave: {wave.label}, status: {wave.status}"
                >
                  <span class="wave-label">{wave.label}</span>

                  <!-- Task dots -->
                  <div class="task-dots" role="list" aria-label="Tasks">
                    {#each wave.tasks as task (task.id)}
                      <button
                        class="task-dot {statusClass(task.status)}"
                        role="listitem"
                        title={taskStatusTitle(task)}
                        aria-label="{task.title} — {task.status}"
                        onclick={() => onTaskClick?.(task.id)}
                      ></button>
                    {/each}
                  </div>

                  <!-- Gate verdict badge -->
                  {#if wave.gate_verdict}
                    <button
                      class="gate-badge {gateClass(wave.gate_verdict.overall)}"
                      title="Gate: {wave.gate_verdict.overall} @ {wave.gate_verdict.evaluated_at.slice(0, 10)}"
                      onclick={() => onGateClick?.(phase.id, wave.id)}
                    >
                      {gateLabel(wave.gate_verdict.overall)}
                    </button>
                  {/if}
                </div>
              {/each}
            </div>
          {/if}

          <!-- Phase gate -->
          {#if phase.gate_verdict}
            <div class="phase-gate">
              <button
                class="phase-gate-badge {gateClass(phase.gate_verdict.overall)}"
                onclick={() => onGateClick?.(phase.id, null)}
              >
                PHASE {gateLabel(phase.gate_verdict.overall)}
              </button>
            </div>
          {/if}
        </div>
      {/each}
    </div>
  {/if}
</div>

<style>
  .wave-pipeline {
    display: flex;
    flex-direction: column;
    height: 100%;
    overflow: hidden;
    background: var(--la-bg-base, #0d1117);
    font-family: var(--la-font-mono, monospace);
    font-size: 10px;
    color: var(--la-text-base, #c9d1d9);
  }

  .empty {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--la-text-mute, #6e7681);
    font-size: 10px;
    letter-spacing: 0.08em;
    font-style: italic;
  }

  /* ── Full mode: phases side-by-side; split mode: stacked ── */
  .phase-list {
    display: flex;
    flex-direction: column;
    gap: 0;
    overflow-y: auto;
    flex: 1;
    min-height: 0;
  }

  [data-mode="full"] .phase-list {
    flex-direction: row;
    overflow-x: auto;
    overflow-y: hidden;
    align-items: stretch;
  }

  .phase-block {
    display: flex;
    flex-direction: column;
    border-bottom: 1px solid var(--la-hair-faint, rgba(255,255,255,0.06));
    padding: 8px 12px;
    flex-shrink: 0;
  }

  [data-mode="full"] .phase-block {
    border-bottom: none;
    border-right: 1px solid var(--la-hair-faint, rgba(255,255,255,0.06));
    min-width: 180px;
    max-width: 240px;
  }

  .phase-block[data-status="in_progress"] {
    background: rgba(88, 166, 255, 0.04);
  }

  .phase-header {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-bottom: 8px;
  }

  .phase-glyph {
    font-size: 10px;
    flex-shrink: 0;
  }

  .phase-label {
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.08em;
    color: var(--la-text-stark, #e6edf3);
    text-transform: uppercase;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  /* ── Wave rows ── */
  .wave-list {
    display: flex;
    flex-direction: column;
    gap: 5px;
    flex: 1;
  }

  .wave-row {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 3px 6px;
    border-radius: 3px;
    transition: background 80ms;
  }

  .wave-row.selected {
    background: rgba(88, 166, 255, 0.12);
    outline: 1px solid rgba(88, 166, 255, 0.3);
  }

  .wave-row.wave-active {
    background: rgba(23, 195, 178, 0.06);
  }

  .wave-label {
    font-size: 8px;
    color: var(--la-text-dim, #8b949e);
    white-space: nowrap;
    flex-shrink: 0;
    min-width: 44px;
  }

  /* ── Task dots ── */
  .task-dots {
    display: flex;
    gap: 3px;
    flex-wrap: wrap;
    flex: 1;
  }

  .task-dot {
    width: 8px;
    height: 8px;
    border-radius: 2px;
    border: none;
    cursor: pointer;
    transition: transform 80ms, opacity 80ms;
    flex-shrink: 0;
  }

  .task-dot:hover { transform: scale(1.3); }

  /* ── Status colors ── */
  .st-done    { background: #3fb950; color: #3fb950; }
  .st-active  { background: var(--la-agent-researcher, #17c3b2); color: var(--la-agent-researcher, #17c3b2); }
  .st-fail    { background: var(--la-agent-security, #ef4444); color: var(--la-agent-security, #ef4444); }
  .st-pending { background: var(--la-hair-strong, #30363d); color: var(--la-text-dim, #8b949e); }

  /* ── Gate badges ── */
  .gate-badge,
  .phase-gate-badge {
    font-size: 7px;
    font-weight: 700;
    letter-spacing: 0.1em;
    padding: 2px 5px;
    border-radius: 3px;
    border: none;
    cursor: pointer;
    white-space: nowrap;
    flex-shrink: 0;
    text-transform: uppercase;
    font-family: inherit;
  }

  .gate-pass  { background: rgba(63, 185, 80, 0.2);  color: #3fb950; }
  .gate-hitl  { background: rgba(249, 115, 22, 0.2); color: var(--la-agent-performance, #f97316); }
  .gate-fail  { background: rgba(239, 68, 68, 0.2);  color: var(--la-agent-security, #ef4444); }

  .phase-gate {
    margin-top: 6px;
    display: flex;
    justify-content: flex-end;
  }

  .phase-gate-badge {
    font-size: 7px;
  }

  @media (prefers-reduced-motion: reduce) {
    .task-dot { transition: none; }
    .wave-row { transition: none; }
  }
</style>
