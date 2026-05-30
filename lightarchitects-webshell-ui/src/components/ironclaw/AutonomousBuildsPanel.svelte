<!--
@component
Top-level panel for ironclaw autonomous builds.

Composes:
- StartForm  — launch a new autonomous build (shown when no build is active)
- WaveSlotGrid — per-slot worker occupancy with task detail
- AutonomousRun — live conductor / merge / fix-agent feed
- HitlModal  — blocking ironclaw HITL escalation modal (nonce-based)

State: tracks the active build ID in local state. The HitlModal is always
mounted so it can intercept ironclaw_hitl_escalation SSE events regardless
of whether the panel is showing the start form or the running build view.
-->
<script lang="ts">
  import HitlModal    from './HitlModal.svelte';
  import WaveSlotGrid from './WaveSlotGrid.svelte';
  import StartForm    from './StartForm.svelte';
  import AutonomousRun from '$lib/../components/views/AutonomousRun.svelte';

  let activeBuildId = $state<string | null>(null);

  function onLaunched(buildId: string) {
    activeBuildId = buildId;
  }

  function clearBuild() {
    activeBuildId = null;
  }
</script>

<!-- HitlModal is always mounted — intercepts SSE regardless of view state -->
<HitlModal />

<div class="abp" data-testid="autonomous-builds-panel">
  {#if activeBuildId}
    <div class="abp-running">
      <div class="abp-running-header">
        <span class="abp-build-id">{activeBuildId.slice(0, 8)}…</span>
        <button class="abp-new-btn" onclick={clearBuild} aria-label="Start a new build">
          NEW BUILD
        </button>
      </div>

      <WaveSlotGrid buildId={activeBuildId} />

      <AutonomousRun buildId={activeBuildId} />
    </div>
  {:else}
    <StartForm {onLaunched} />
  {/if}
</div>

<style>
  .abp {
    display: flex;
    flex-direction: column;
    height: 100%;
    overflow-y: auto;
    background: var(--la-bg-frame, var(--la-bg-base));
    border: 1px solid var(--la-drawer-border, var(--la-hair-base));
    border-radius: 4px;
  }

  .abp-running {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 16px;
    height: 100%;
  }

  .abp-running-header {
    display: flex;
    align-items: center;
    gap: 12px;
  }

  .abp-build-id {
    font-family: var(--la-font-mono, monospace);
    font-size: 10px;
    color: var(--la-text-dim);
    letter-spacing: 0.05em;
  }

  .abp-new-btn {
    margin-left: auto;
    font-family: var(--la-font-mono, monospace);
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.1em;
    padding: 2px 10px;
    border: 1px solid var(--la-hair-strong);
    border-radius: 2px;
    color: var(--la-text-dim);
    background: transparent;
    cursor: pointer;
    transition: color 150ms, border-color 150ms;
  }

  .abp-new-btn:hover {
    color: var(--la-text-base);
    border-color: var(--la-focus-ring);
  }
</style>
