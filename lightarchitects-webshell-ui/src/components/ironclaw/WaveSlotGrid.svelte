<!--
@component
Enhanced 7-slot worker occupancy grid with per-slot task detail.

Consumed from `workerSlots` store (Phase 6 ironclaw-spine). When the backend
populates `slots[]`, each cell shows the assigned task_id / domain. Falls back
to a simple active/idle indicator when per-slot data is absent.

Props:
- `buildId` — filters events to the active build (pass-through guard)
-->
<script lang="ts">
  import { workerSlots } from '$lib/stores';
  import type { SlotDetail } from '$lib/types';

  let { buildId }: { buildId: string } = $props();

  const CAPACITY = 7;

  let gauge     = $derived($workerSlots?.build_id === buildId ? $workerSlots : null);
  let active    = $derived(gauge?.active   ?? 0);
  let capacity  = $derived(gauge?.capacity ?? CAPACITY);
  let waveIdx   = $derived(gauge?.wave_index ?? 0);

  let slotMap = $derived.by<Map<number, SlotDetail>>(() => {
    const m = new Map<number, SlotDetail>();
    if (gauge?.slots) {
      for (const s of gauge.slots) m.set(s.slot_index, s);
    }
    return m;
  });
</script>

<div class="wsg" data-testid="wave-slot-grid" data-build-id={buildId}>
  <div class="wsg-header">
    <span class="wsg-label">WORKER SLOTS</span>
    <span class="wsg-wave">W{waveIdx}</span>
    <span class="wsg-count">{active}/{capacity}</span>
  </div>

  <div
    class="wsg-grid"
    role="meter"
    aria-label="Worker slot occupancy"
    aria-valuenow={active}
    aria-valuemax={capacity}
  >
    {#each { length: capacity } as _, i}
      {@const detail = slotMap.get(i)}
      {@const isActive = i < active}
      <div
        class="wsg-cell"
        class:wsg-cell-active={isActive}
        class:wsg-cell-idle={!isActive}
        aria-label="Slot {i + 1}: {isActive ? 'active' : 'idle'}{detail?.task_id ? ` — ${detail.task_id}` : ''}"
      >
        <span class="wsg-slot-num">{i + 1}</span>
        {#if detail && isActive}
          {#if detail.task_id}
            <span class="wsg-task-id" title={detail.task_id}>{detail.task_id.slice(0, 12)}</span>
          {/if}
          {#if detail.domain}
            <span class="wsg-domain">{detail.domain}</span>
          {/if}
        {:else if !isActive}
          <span class="wsg-idle-label">idle</span>
        {/if}
      </div>
    {/each}
  </div>

  {#if !gauge}
    <span class="wsg-waiting">waiting for slot data…</span>
  {/if}
</div>

<style>
  .wsg {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .wsg-header {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .wsg-label {
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.1em;
    color: var(--la-text-label);
  }

  .wsg-wave {
    font-size: 9px;
    color: var(--la-focus-ring);
    font-variant-numeric: tabular-nums;
  }

  .wsg-count {
    font-size: 9px;
    color: var(--la-text-dim);
    font-variant-numeric: tabular-nums;
    margin-left: auto;
  }

  .wsg-grid {
    display: grid;
    grid-template-columns: repeat(7, 1fr);
    gap: 4px;
  }

  .wsg-cell {
    display: flex;
    flex-direction: column;
    align-items: center;
    padding: 5px 3px;
    border-radius: 3px;
    border: 1px solid var(--la-hair-strong);
    background: var(--la-bg-elev-2);
    gap: 2px;
    min-height: 42px;
    transition: background 0.18s, border-color 0.18s;
    overflow: hidden;
  }

  .wsg-cell.wsg-cell-active {
    background: color-mix(in srgb, var(--la-focus-ring) 12%, var(--la-bg-elev-2));
    border-color: var(--la-focus-ring);
  }

  .wsg-slot-num {
    font-size: 8px;
    font-weight: 700;
    color: var(--la-text-dim);
    font-variant-numeric: tabular-nums;
  }

  .wsg-cell-active .wsg-slot-num {
    color: var(--la-focus-ring);
  }

  .wsg-task-id {
    font-family: var(--la-font-mono, monospace);
    font-size: 7px;
    color: var(--la-text-bright);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    width: 100%;
    text-align: center;
  }

  .wsg-domain {
    font-size: 7px;
    color: var(--la-text-label);
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }

  .wsg-idle-label {
    font-size: 7px;
    color: var(--la-text-dim);
    opacity: 0.5;
  }

  .wsg-waiting {
    font-size: 9px;
    color: var(--la-text-dim);
    font-style: italic;
  }
</style>
