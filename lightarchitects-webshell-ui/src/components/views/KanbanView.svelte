<script lang="ts">
  import { activeBuild, findings } from '$lib/stores';
  import { QUALITY_GATE_COLORS, QUALITY_GATES } from '$lib/design-tokens';
  import type { Pillar, PillarGate, Finding } from '$lib/types';

  const PILLAR_LABEL: Record<Pillar, string> = {
    ARCH: 'PLAN',
    SEC:  'RESEARCH',
    QUAL: 'IMPLEMENT',
    PERF: 'HARDEN',
    TEST: 'VERIFY',
    DOC:  'SHIP',
    OPS:  'LEARN',
  };

  const SEVERITY_ORDER = { critical: 0, error: 1, warning: 2, info: 3 } as const;

  let build = $derived($activeBuild);

  // Findings indexed by pillar for the active build
  let findingsByPillar = $derived.by(() => {
    if (!build) return {} as Record<Pillar, Finding[]>;
    const map: Record<string, Finding[]> = {};
    for (const f of $findings) {
      if (f.buildId !== build.id) continue;
      (map[f.pillar] ??= []).push(f);
    }
    // Sort by severity within each pillar
    for (const k of Object.keys(map)) {
      map[k].sort((a, b) => (SEVERITY_ORDER[a.severity] ?? 9) - (SEVERITY_ORDER[b.severity] ?? 9));
    }
    return map as Record<Pillar, Finding[]>;
  });

  function pillarColor(pillar: Pillar): string {
    return QUALITY_GATE_COLORS[pillar] ?? 'var(--la-text-mute)';
  }

  function statusGlyph(status: PillarGate['status']): string {
    switch (status) {
      case 'passed':      return '✓';
      case 'failed':      return '✗';
      case 'in_progress': return '▶';
      case 'blocked':     return '⊘';
      default:            return '○';
    }
  }

  function statusColor(status: PillarGate['status']): string {
    switch (status) {
      case 'passed':      return 'var(--la-agent-researcher)';
      case 'failed':      return 'var(--la-agent-security)';
      case 'in_progress': return 'var(--la-agent-performance)';
      case 'blocked':     return 'var(--la-text-mute)';
      default:            return 'var(--la-text-mute)';
    }
  }

  function severityColor(sev: Finding['severity']): string {
    switch (sev) {
      case 'critical': return 'var(--la-agent-security)';
      case 'error':    return '#f97316';
      case 'warning':  return 'var(--la-agent-performance)';
      default:         return 'var(--la-text-mute)';
    }
  }
</script>

<div class="kanban-wrap" data-testid="kanban-view">
  {#if !build}
    <div class="kanban-empty">— no build selected —</div>
  {:else}
    <div class="kanban-board">
      {#each QUALITY_GATES as pillar, colIdx}
        {@const gate = build.pillars.find(p => p.pillar === pillar)}
        {@const color = pillarColor(pillar)}
        {@const pFindings = findingsByPillar[pillar] ?? []}

        <div class="kanban-col" style="--pc: {color}">
          <!-- Column header -->
          <div class="col-head">
            <span class="col-label">{PILLAR_LABEL[pillar]}</span>
            <span class="col-pillar">{pillar}</span>
            {#if gate}
              <span class="col-status" style="color: {statusColor(gate.status)}">
                {statusGlyph(gate.status)}
              </span>
            {/if}
          </div>

          <!-- Confidence bar -->
          {#if gate}
            <div class="col-conf-wrap">
              <div
                class="col-conf-bar"
                style="width: {Math.round(gate.confidence * 100)}%; background: {color}"
              ></div>
            </div>
          {/if}

          <!-- Finding cards -->
          <div class="col-cards">
            {#if pFindings.length === 0}
              <div class="col-empty">— clear —</div>
            {:else}
              {#each pFindings as f (f.id)}
                <div class="finding-card" data-severity={f.severity}>
                  <span class="fc-sev" style="color: {severityColor(f.severity)}">{f.severity.toUpperCase()}</span>
                  <span class="fc-title">{f.title}</span>
                  {#if f.file}
                    <span class="fc-file">{f.file}{f.line ? `:${f.line}` : ''}</span>
                  {/if}
                </div>
              {/each}
            {/if}
          </div>
        </div>

        {#if colIdx < QUALITY_GATES.length - 1}
          <div class="gate-sep" data-state={gate?.status ?? 'pending'}>
            <div class="gate-line"></div>
            <span class="gate-sep-lbl">{pillar.slice(0, 1)}</span>
            <span class="gate-sep-icon">{statusGlyph(gate?.status ?? 'pending')}</span>
            <div class="gate-line"></div>
          </div>
        {/if}
      {/each}
    </div>
  {/if}
</div>

<style>
  .kanban-wrap {
    height: 100%;
    overflow-x: auto;
    overflow-y: hidden;
    padding: 12px 16px;
  }

  .kanban-empty {
    height: 100%;
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--la-text-mute);
    font-size: 11px;
    letter-spacing: 0.12em;
    font-style: italic;
  }

  .kanban-board {
    display: flex;
    flex-direction: row;
    align-items: stretch;
    height: 100%;
    min-width: 1232px; /* 7 × 160px columns + 6 × 24px separators */
  }

  .kanban-col {
    --pc: var(--la-text-mute);
    flex: 1;
    min-width: 160px;
    display: flex;
    flex-direction: column;
    gap: 4px;
    border: 1px solid var(--la-hair-faint);
    border-top: 2px solid var(--pc);
    background: var(--la-bg-base);
    overflow: hidden;
  }

  /* ── gate separator between kanban columns ── */
  .gate-sep {
    flex-shrink: 0;
    width: 24px;
    display: flex;
    flex-direction: column;
    align-items: center;
    padding: 8px 0;
    gap: 3px;
  }
  .gate-line {
    flex: 1;
    width: 1px;
    background: rgba(71, 85, 105, 0.3);
  }
  .gate-sep-lbl {
    font-size: 8px;
    font-weight: 700;
    letter-spacing: 0.1em;
    color: var(--la-text-mute);
    writing-mode: vertical-rl;
    text-orientation: mixed;
  }
  .gate-sep-icon {
    font-size: 10px;
    color: var(--la-text-mute);
  }
  .gate-sep[data-state="passed"] .gate-sep-lbl,
  .gate-sep[data-state="passed"] .gate-sep-icon { color: var(--la-agent-researcher, #4dffe6); }
  .gate-sep[data-state="failed"] .gate-sep-lbl,
  .gate-sep[data-state="failed"] .gate-sep-icon { color: var(--la-agent-security, #ff4d4d); }
  .gate-sep[data-state="in_progress"] .gate-sep-lbl,
  .gate-sep[data-state="in_progress"] .gate-sep-icon { color: var(--la-agent-performance, #ff8e3c); }
  .gate-sep[data-state="passed"] .gate-line { background: color-mix(in srgb, var(--la-agent-researcher, #4dffe6) 30%, transparent); }
  .gate-sep[data-state="failed"] .gate-line  { background: color-mix(in srgb, var(--la-agent-security, #ff4d4d) 30%, transparent); }

  .col-head {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 6px 10px 4px;
    border-bottom: 1px solid var(--la-hair-faint);
  }

  .col-label {
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.14em;
    text-transform: uppercase;
    color: var(--pc);
    flex: 1;
  }

  .col-pillar {
    font-size: 8px;
    color: var(--la-text-mute);
    letter-spacing: 0.08em;
  }

  .col-status {
    font-size: 10px;
    font-weight: 700;
  }

  .col-conf-wrap {
    height: 2px;
    background: var(--la-hair-faint);
    flex-shrink: 0;
  }

  .col-conf-bar {
    height: 100%;
    opacity: 0.6;
    transition: width 0.4s ease;
    min-width: 2px;
  }

  .col-cards {
    flex: 1;
    overflow-y: auto;
    padding: 6px 8px;
    display: flex;
    flex-direction: column;
    gap: 4px;
    scrollbar-width: thin;
    scrollbar-color: var(--la-hair-base) transparent;
  }

  .col-empty {
    font-size: 9px;
    color: var(--la-text-mute);
    font-style: italic;
    letter-spacing: 0.08em;
    text-align: center;
    padding: 8px 0;
  }

  .finding-card {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 5px 7px;
    border: 1px solid var(--la-hair-faint);
    background: var(--la-bg-elev-1);
    transition: border-color 80ms;
  }
  .finding-card:hover { border-color: var(--la-hair-base); }

  .fc-sev {
    font-size: 8px;
    font-weight: 700;
    letter-spacing: 0.1em;
  }

  .fc-title {
    font-size: 10px;
    color: var(--la-text-base);
    line-height: 1.3;
  }

  .fc-file {
    font-size: 8px;
    color: var(--la-text-mute);
    font-variant-numeric: tabular-nums;
    letter-spacing: 0.02em;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
