<script lang="ts">
  export type GateStatus = 'pending' | 'active' | 'passed' | 'failed' | 'blocked';

  export interface GateEntry {
    id: string;
    status: GateStatus;
  }

  interface Props {
    /** Explicit gate entries. If omitted, derived from passed/total. */
    gates?: GateEntry[];
    /** Number of passed gates (used when gates array not provided). */
    passed?: number;
    /** Total gates to show (default 7 — canonical LASDLC set). */
    total?: number;
    /** Show gate ID labels below dots. */
    labels?: boolean;
  }

  const LASDLC_7 = ['A', 'S', 'Q', 'C', 'O', 'K', 'T'] as const;

  let { gates, passed = 0, total = 7, labels = false }: Props = $props();

  let entries = $derived.by((): GateEntry[] => {
    if (gates) return gates;
    const ids = LASDLC_7.slice(0, total);
    return ids.map((id, i): GateEntry => ({
      id,
      status: i < passed ? 'passed' : i === passed ? 'active' : 'pending',
    }));
  });
</script>

<div class="gate-strip" class:labeled={labels}>
  {#each entries as gate}
    <div class="gate-dot" data-status={gate.status} title="Gate {gate.id} · {gate.status}">
      {#if labels}<span class="gate-lbl">{gate.id}</span>{/if}
    </div>
  {/each}
</div>

<style>
  .gate-strip {
    display: flex;
    gap: 3px;
    align-items: center;
  }
  .gate-strip.labeled {
    flex-direction: column;
    gap: 2px;
    align-items: stretch;
  }

  .gate-dot {
    width: 12px;
    height: 7px;
    border: 1px solid var(--la-hair-base);
    position: relative;
    flex-shrink: 0;
    transition: border-color 80ms, background 80ms;
  }

  /* pending — empty */
  .gate-dot[data-status="pending"] {
    border-color: var(--la-hair-base);
    background: transparent;
  }

  /* active — pulsing amber fill */
  .gate-dot[data-status="active"] {
    border-color: #f59e0b;
    background: #f59e0b22;
    animation: gate-pulse 1.4s steps(2) infinite;
  }
  @keyframes gate-pulse {
    0%, 49%   { opacity: 1; }
    50%, 100% { opacity: 0.5; }
  }

  /* passed — solid cyan fill with center pip */
  .gate-dot[data-status="passed"] {
    border-color: #22c55e;
    background: #22c55e22;
  }
  .gate-dot[data-status="passed"]::after {
    content: '';
    position: absolute;
    width: 3px;
    height: 3px;
    top: 50%;
    left: 50%;
    transform: translate(-50%, -50%);
    background: #22c55e;
  }

  /* failed — red border + fill */
  .gate-dot[data-status="failed"] {
    border-color: #ef4444;
    background: #ef444422;
  }

  /* blocked — muted diagonal pattern */
  .gate-dot[data-status="blocked"] {
    border-color: #475569;
    background: repeating-linear-gradient(
      45deg,
      transparent,
      transparent 2px,
      #47556922 2px,
      #47556922 4px
    );
  }

  .gate-lbl {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 6px;
    font-weight: 700;
    letter-spacing: 0.05em;
    color: var(--la-text-mute);
    pointer-events: none;
  }
</style>
