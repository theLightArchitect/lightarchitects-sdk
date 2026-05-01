<script lang="ts">
  let {
    phases,
    agentColor,
    justDispatched = false,
  }: {
    phases: { id: string; status: 'pending' | 'active' | 'complete' }[];
    agentColor?: string;
    justDispatched?: boolean;
  } = $props();
</script>

<div
  class="phase-strip"
  style:--rc={agentColor ?? 'var(--la-text-mute)'}
>
  {#each phases as phase, i (phase.id)}
    <div
      class="phase-square"
      class:phase-pop-entry={justDispatched && phase.status === 'active'}
      data-status={phase.status}
      title={phase.id}
    ></div>
  {/each}
</div>

<style>
  .phase-strip {
    display: flex;
    gap: 3px;
  }

  .phase-square {
    width: 30px;
    height: 14px;
    border: 1px solid var(--la-hair-base);
    position: relative;
    transition: border-color var(--la-t-snap);
  }

  /* Fill overlay for active */
  .phase-square::after {
    content: "";
    position: absolute;
    inset: 0;
    background: var(--rc);
    opacity: 0;
    transition: opacity var(--la-t-snap) var(--la-ease-mech);
  }

  /* Center dot for complete */
  .phase-square::before {
    content: "";
    position: absolute;
    width: 4px;
    height: 4px;
    top: 50%;
    left: 50%;
    transform: translate(-50%, -50%) scale(0);
    background: var(--rc);
    transition: transform var(--la-t-snap) var(--la-ease-mech);
  }

  .phase-square[data-status="active"]::after {
    opacity: 0.9;
    animation: phase-flicker 0.8s steps(3) infinite;
  }
  .phase-square[data-status="active"] { border-color: var(--rc); }
  .phase-square[data-status="complete"] { border-color: var(--rc); }
  .phase-square[data-status="complete"]::before {
    transform: translate(-50%, -50%) scale(1);
  }

  @keyframes phase-flicker {
    0%, 60%, 100% { opacity: 0.9; }
    30%, 80%      { opacity: 0.5; }
  }

  /* First-phase pop-in animation on fresh dispatch */
  .phase-pop-entry {
    animation: phase-pop 0.4s var(--la-ease-mech) 0.2s backwards;
  }

  @keyframes phase-pop {
    0%   { transform: scale(0.8); opacity: 0; }
    100% { transform: scale(1);   opacity: 1; }
  }
</style>
