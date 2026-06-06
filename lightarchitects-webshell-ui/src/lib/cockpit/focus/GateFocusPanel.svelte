<script lang="ts">
  import type { GateDim } from '$lib/cockpit/stores/selection';

  interface Props { codename: string; phase: number; gate: GateDim; }
  let { codename, phase, gate }: Props = $props();

  const GATE_LABELS: Record<GateDim, string> = {
    A: 'Architecture',
    S: 'Security',
    Q: 'Quality',
    C: 'Canon',
    O: 'Operations',
    P: 'Performance',
    K: 'Knowledge',
    D: 'Documentation',
    T: 'Testing',
    R: 'Research+Risk',
  };
  const GATE_OWNERS: Record<GateDim, string> = {
    A: 'CORSO',     S: 'SERAPH',  Q: 'CORSO',
    C: 'LÆX',      O: 'EVA',     P: 'EVA / AYIN',
    K: 'SOUL',      D: 'SOUL',    T: 'CORSO',
    R: 'QUANTUM',
  };
</script>

<div class="focus-panel" data-focus-kind="gate">
  <header class="focus-hdr">
    <span class="focus-kind">GATE [{gate}]</span>
    <span class="focus-label">{GATE_LABELS[gate]}</span>
  </header>
  <section class="focus-body">
    <div class="field-row">
      <span class="field-label">BUILD</span>
      <span class="field-value field-mono">{codename}</span>
    </div>
    <div class="field-row">
      <span class="field-label">PHASE</span>
      <span class="field-value">{phase}</span>
    </div>
    <div class="field-row">
      <span class="field-label">GATE</span>
      <span class="field-value gate-badge">[{gate}] {GATE_LABELS[gate]}</span>
    </div>
    <div class="field-row">
      <span class="field-label">OWNER</span>
      <span class="field-value">{GATE_OWNERS[gate]}</span>
    </div>
  </section>
</div>

<style>
  .focus-panel { display: flex; flex-direction: column; height: 100%; overflow: hidden; }
  .focus-hdr {
    display: flex; align-items: center; gap: 8px;
    padding: 10px 12px; border-bottom: 1px solid var(--la-hair-base, rgba(255,255,255,0.06));
    flex-shrink: 0;
  }
  .focus-kind { font-size: 8px; font-weight: 700; letter-spacing: 0.14em; color: var(--scope-accent, #4da6ff); opacity: 0.7; }
  .focus-label { font-size: 10px; color: var(--la-text-dim, #888); }
  .focus-body { flex: 1; overflow-y: auto; padding: 8px 12px; display: flex; flex-direction: column; gap: 8px; }
  .field-row { display: flex; align-items: baseline; gap: 8px; }
  .field-label { font-size: 8px; font-weight: 700; letter-spacing: 0.1em; color: var(--la-text-mute, #555); min-width: 72px; flex-shrink: 0; }
  .field-value { font-size: 10px; color: var(--la-text-dim, #888); }
  .field-mono { font-family: var(--font-mono, monospace); font-size: 9px; }
  .gate-badge { font-family: var(--font-mono, monospace); color: var(--scope-accent, #4da6ff); font-size: 9px; }
</style>
