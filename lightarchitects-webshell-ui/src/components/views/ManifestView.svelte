<script lang="ts">
  import { activeBuild, findings, artifacts } from '$lib/stores';
  import { QUALITY_GATE_COLORS } from '$lib/design-tokens';
  import type { PillarGate, Pillar } from '$lib/types';

  const PILLAR_LABEL: Record<Pillar, string> = {
    ARCH: 'Plan', SEC: 'Research', QUAL: 'Implement',
    PERF: 'Harden', TEST: 'Verify', DOC: 'Ship', OPS: 'Learn',
  };

  let build = $derived($activeBuild);

  let findingCounts = $derived.by(() => {
    if (!build) return {} as Record<string, number>;
    const counts: Record<string, number> = {};
    for (const f of $findings) {
      if (f.buildId !== build.id) continue;
      counts[f.pillar] = (counts[f.pillar] ?? 0) + 1;
    }
    return counts;
  });

  let buildArtifacts = $derived(
    $artifacts.filter(a => build && a.buildId === build.id)
  );

  function statusLabel(status: PillarGate['status']): string {
    switch (status) {
      case 'passed':      return '✓ PASSED';
      case 'failed':      return '✗ FAILED';
      case 'in_progress': return '▶ IN PROGRESS';
      case 'blocked':     return '⊘ BLOCKED';
      default:            return '○ PENDING';
    }
  }

  function statusColor(status: PillarGate['status']): string {
    switch (status) {
      case 'passed':      return 'var(--la-agent-researcher)';
      case 'failed':      return 'var(--la-agent-security)';
      case 'in_progress': return 'var(--la-agent-performance)';
      default:            return 'var(--la-text-mute)';
    }
  }

  function formatDate(iso: string): string {
    try { return new Date(iso).toLocaleString('en-US', { hour12: false, dateStyle: 'short', timeStyle: 'short' }); }
    catch { return iso; }
  }
</script>

<div class="manifest-wrap" data-testid="manifest-view">
  {#if !build}
    <div class="manifest-empty">— no build selected —</div>
  {:else}
    <div class="manifest-scroll">

      <!-- Build identity -->
      <section class="manifest-section">
        <div class="section-head">
          <span class="section-label">BUILD IDENTITY</span>
        </div>
        <div class="kv-grid">
          <span class="kv-key">NAME</span>
          <span class="kv-val">{build.name}</span>
          <span class="kv-key">ID</span>
          <span class="kv-val kv-mono">{build.id}</span>
          <span class="kv-key">META-SKILL</span>
          <span class="kv-val">{build.metaSkill ?? '—'}</span>
          <span class="kv-key">STATUS</span>
          <span class="kv-val">{build.status.toUpperCase()}</span>
          {#if build.priority}
            <span class="kv-key">PRIORITY</span>
            <span class="kv-val">{build.priority.toUpperCase()}</span>
          {/if}
          <span class="kv-key">CONFIDENCE</span>
          <span class="kv-val">{Math.round(build.confidence * 100)}%</span>
          <span class="kv-key">CREATED</span>
          <span class="kv-val kv-mono">{formatDate(build.createdAt)}</span>
          <span class="kv-key">UPDATED</span>
          <span class="kv-val kv-mono">{formatDate(build.updatedAt)}</span>
          {#if build.description}
            <span class="kv-key">DESCRIPTION</span>
            <span class="kv-val">{build.description}</span>
          {/if}
        </div>
      </section>

      <!-- Phase gates -->
      <section class="manifest-section">
        <div class="section-head">
          <span class="section-label">PHASE GATES</span>
          <span class="section-meta">{build.pillars.length} gates</span>
        </div>
        <div class="gates-list">
          {#each build.pillars as gate (gate.pillar)}
            {@const color = QUALITY_GATE_COLORS[gate.pillar] ?? '#666'}
            {@const nFindings = findingCounts[gate.pillar] ?? 0}
            <div class="gate-row">
              <div class="gate-col-bar" style="background: {color}"></div>
              <div class="gate-info">
                <div class="gate-head">
                  <span class="gate-pillar" style="color: {color}">{gate.pillar}</span>
                  <span class="gate-phase">{PILLAR_LABEL[gate.pillar]}</span>
                </div>
                <div class="gate-status" style="color: {statusColor(gate.status)}">
                  {statusLabel(gate.status)}
                </div>
              </div>
              <div class="gate-conf">
                <div class="conf-track">
                  <div class="conf-fill" style="width: {Math.round(gate.confidence * 100)}%; background: {color}"></div>
                </div>
                <span class="conf-pct">{Math.round(gate.confidence * 100)}%</span>
              </div>
              {#if nFindings > 0}
                <span class="gate-findings">{nFindings} finding{nFindings !== 1 ? 's' : ''}</span>
              {/if}
              {#if gate.exitGate}
                <span class="gate-exit" class:pass={gate.exitGate.passed} class:fail={!gate.exitGate.passed}>
                  EXIT {gate.exitGate.passed ? '✓' : '✗'}
                </span>
              {/if}
            </div>
          {/each}
        </div>
      </section>

      <!-- Artifacts -->
      {#if buildArtifacts.length > 0}
        <section class="manifest-section">
          <div class="section-head">
            <span class="section-label">ARTIFACTS</span>
            <span class="section-meta">{buildArtifacts.length}</span>
          </div>
          <div class="artifact-list">
            {#each buildArtifacts as art (art.id)}
              <div class="art-row">
                <span class="art-type">{art.type.toUpperCase()}</span>
                <span class="art-name">{art.name}</span>
                <span class="art-size">{(art.size / 1024).toFixed(1)}KB</span>
                <span class="art-ts kv-mono">{formatDate(art.createdAt)}</span>
              </div>
            {/each}
          </div>
        </section>
      {/if}

    </div>
  {/if}
</div>

<style>
  .manifest-wrap {
    display: flex;
    flex-direction: column;
    height: 100%;
    overflow: hidden;
  }

  .manifest-empty {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--la-text-mute);
    font-size: 11px;
    letter-spacing: 0.12em;
    font-style: italic;
  }

  .manifest-scroll {
    flex: 1;
    overflow-y: auto;
    min-height: 0;
    padding: 16px 20px;
    display: flex;
    flex-direction: column;
    gap: 20px;
    scrollbar-width: thin;
    scrollbar-color: var(--la-hair-base) transparent;
  }

  /* ── Section ── */
  .manifest-section {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .section-head {
    display: flex;
    align-items: center;
    gap: 8px;
    padding-bottom: 4px;
    border-bottom: 1px solid var(--la-hair-base);
  }

  .section-label {
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.18em;
    text-transform: uppercase;
    color: var(--la-text-mute);
  }

  .section-meta {
    font-size: 9px;
    color: var(--la-text-mute);
    opacity: 0.6;
    font-variant-numeric: tabular-nums;
  }

  /* ── Key-value grid ── */
  .kv-grid {
    display: grid;
    grid-template-columns: 100px 1fr;
    gap: 3px 12px;
    font-size: 11px;
  }

  .kv-key {
    font-size: 8px;
    font-weight: 700;
    letter-spacing: 0.14em;
    text-transform: uppercase;
    color: var(--la-text-mute);
    padding-top: 1px;
  }

  .kv-val { color: var(--la-text-base); }
  .kv-mono { font-variant-numeric: tabular-nums; font-size: 10px; color: var(--la-text-dim); }

  /* ── Gate rows ── */
  .gates-list {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .gate-row {
    display: grid;
    grid-template-columns: 3px 140px 1fr auto auto;
    gap: 10px;
    align-items: center;
    padding: 6px 8px 6px 0;
    border-bottom: 1px solid var(--la-hair-faint);
  }

  .gate-col-bar {
    width: 3px;
    height: 100%;
    min-height: 28px;
    opacity: 0.6;
    flex-shrink: 0;
  }

  .gate-info {
    display: flex;
    flex-direction: column;
    gap: 1px;
  }

  .gate-head { display: flex; align-items: baseline; gap: 6px; }
  .gate-pillar { font-size: 10px; font-weight: 700; letter-spacing: 0.08em; }
  .gate-phase { font-size: 10px; color: var(--la-text-dim); }

  .gate-status {
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.08em;
  }

  .gate-conf {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .conf-track {
    width: 80px;
    height: 3px;
    background: var(--la-hair-faint);
    flex-shrink: 0;
  }

  .conf-fill {
    height: 100%;
    opacity: 0.6;
    min-width: 2px;
    transition: width 0.4s ease;
  }

  .conf-pct {
    font-size: 9px;
    color: var(--la-text-mute);
    font-variant-numeric: tabular-nums;
    width: 30px;
    text-align: right;
  }

  .gate-findings {
    font-size: 9px;
    color: var(--la-agent-performance);
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }

  .gate-exit {
    font-size: 8px;
    font-weight: 700;
    letter-spacing: 0.1em;
    white-space: nowrap;
  }
  .gate-exit.pass { color: var(--la-agent-researcher); }
  .gate-exit.fail { color: var(--la-agent-security); }

  /* ── Artifact rows ── */
  .artifact-list {
    display: flex;
    flex-direction: column;
    gap: 1px;
  }

  .art-row {
    display: grid;
    grid-template-columns: 70px 1fr 60px 140px;
    gap: 12px;
    align-items: center;
    padding: 5px 8px;
    border-bottom: 1px solid var(--la-hair-faint);
    font-size: 10px;
  }
  .art-type {
    font-size: 8px;
    font-weight: 700;
    letter-spacing: 0.1em;
    color: var(--la-text-mute);
  }
  .art-name { color: var(--la-text-base); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .art-size { color: var(--la-text-mute); font-variant-numeric: tabular-nums; text-align: right; font-size: 9px; }
  .art-ts { font-size: 9px; color: var(--la-text-mute); font-variant-numeric: tabular-nums; }
</style>
