<script lang="ts">
  import { siblingHealth, conductorStats, alertStats, supervisorAlerts, alerts, helixTextureMode, helixEntries } from '$lib/stores';
  import { SIBLING_POLYTOPES } from '$lib/design-tokens';
  import { TEXTURE_MODES, TEXTURE_LABELS, type TextureMode } from '$lib/helix/procedural-textures';

  // Derived: sibling online count
  const onlineCount = $derived(
    Object.values($siblingHealth).filter(h => h.status === 'online').length
  );
  const totalSiblings = $derived(Object.keys($siblingHealth).length || 6);

  // Derived: first offline sibling name (for status label)
  const firstOffline = $derived(
    Object.values($siblingHealth).find(h => h.status !== 'online')?.id ?? null
  );

  // Derived: health label
  const healthLabel = $derived(
    onlineCount === totalSiblings
      ? `${onlineCount}/${totalSiblings} ONLINE`
      : `${onlineCount}/${totalSiblings} · ${(firstOffline ?? 'AGENT').toUpperCase()} OFFLINE`
  );

  // Ticking clock — drives the 60s window on latestFail reactively
  let now = $state(Date.now());
  $effect(() => {
    const id = setInterval(() => { now = Date.now(); }, 5_000);
    return () => clearInterval(id);
  });

  // Derived: latest supervisor FAIL in the last 60 s
  const latestFail = $derived(
    $supervisorAlerts.find(a => a.verdict === 'FAIL' && (now - a.timestamp) < 60_000) ?? null
  );

  // Derived: first unacknowledged CRIT alert
  const critAlert = $derived(
    $alerts.find(a => a.severity === 'critical' && !a.acknowledged) ?? null
  );

  // Banner shape — plain object with only what the template needs; avoids synthesising
  // a structurally invalid SupervisorAlert for the critAlert path.
  type BannerEntry = { id: string; timestamp: number; label: string; sibling: string; message: string };

  const urgentBanner = $derived.by((): BannerEntry | null => {
    if (latestFail) return {
      id: latestFail.id,
      timestamp: latestFail.timestamp,
      label: latestFail.verdict,
      sibling: latestFail.sibling,
      message: latestFail.message,
    };
    if (critAlert) return {
      id: critAlert.id,
      timestamp: Date.parse(critAlert.timestamp),
      label: critAlert.severity.toUpperCase(),
      sibling: critAlert.siblingId ?? 'system',
      message: critAlert.title,
    };
    return null;
  });

  // Polytope legend rows
  const legendRows = $derived(Object.entries(SIBLING_POLYTOPES).map(([sib, p]) => ({
    sib,
    label: p.label,
    vertices: p.vertices,
    edges: p.edges,
  })));

  // Memory pressure
  const stepCount = $derived($helixEntries.length);

  function fmtTime(ms: number): string {
    return new Date(ms).toTimeString().slice(0, 8);
  }
</script>

<div class="helix-hud" aria-label="Helix HUD overlay" data-testid="helix-hud">

  <!-- Top-left: Polytope legend -->
  <div class="hud-legend" data-testid="helix-hud-legend">
    <div class="hud-section-label">POLYTOPES</div>
    {#each legendRows as row (row.sib)}
      <div class="legend-row">
        <span class="legend-sib">{row.sib.toUpperCase()}</span>
        <span class="legend-type">{row.label}</span>
        <span class="legend-meta">{row.vertices}v · {row.edges}e</span>
      </div>
    {/each}
  </div>

  <!-- Top-right: Ecosystem readout -->
  <div class="hud-readout" data-testid="helix-hud-readout">
    <div class="readout-row" class:readout-warn={onlineCount < totalSiblings}>
      <span class="readout-label">AGENTS</span>
      <span class="readout-val">{healthLabel}</span>
    </div>
    <div class="readout-row" class:readout-active={$conductorStats.activeTasks > 0}>
      <span class="readout-label">DISPATCH</span>
      <span class="readout-val">
        {$conductorStats.activeTasks} RUNNING · {$conductorStats.queueDepth} QUEUED
      </span>
    </div>
    {#if $alertStats.critical > 0 || $alertStats.error > 0}
      <div class="readout-row readout-crit">
        <span class="readout-label">ALERTS</span>
        <span class="readout-val">
          {#if $alertStats.critical > 0}{$alertStats.critical} CRIT{/if}
          {#if $alertStats.error > 0} {$alertStats.error} ERR{/if}
          {#if $alertStats.warning > 0} {$alertStats.warning} WARN{/if}
        </span>
      </div>
    {:else if $alertStats.warning > 0}
      <div class="readout-row readout-warn">
        <span class="readout-label">ALERTS</span>
        <span class="readout-val">{$alertStats.warning} WARN</span>
      </div>
    {:else}
      <div class="readout-row readout-ok">
        <span class="readout-label">GATES</span>
        <span class="readout-val">OK</span>
      </div>
    {/if}
    <div class="readout-row">
      <span class="readout-label">MEMORY</span>
      <span class="readout-val">Steps {stepCount.toLocaleString()}</span>
    </div>
  </div>

  <!-- Urgent notification banner -->
  {#if urgentBanner}
    <div class="hud-banner" role="alert" aria-live="assertive" data-testid="helix-hud-banner">
      <span class="banner-verdict">{urgentBanner.label}</span>
      <span class="banner-source">{urgentBanner.sibling.toUpperCase()}</span>
      <span class="banner-msg">{urgentBanner.message}</span>
      <span class="banner-time">{fmtTime(urgentBanner.timestamp)}</span>
    </div>
  {/if}

  <!-- Bottom-left: Texture mode toggle -->
  <div class="hud-texture-toggle" data-testid="helix-hud-texture-toggle">
    <div class="hud-section-label">TEXTURE</div>
    <div class="toggle-row">
      {#each TEXTURE_MODES as mode (mode)}
        <button
          class="tex-btn"
          class:tex-active={$helixTextureMode === mode}
          onclick={() => helixTextureMode.set(mode)}
          title={TEXTURE_LABELS[mode]}
          aria-pressed={$helixTextureMode === mode}
        >
          {mode.slice(0, 4).toUpperCase()}
        </button>
      {/each}
    </div>
    <div class="tex-label">{TEXTURE_LABELS[$helixTextureMode]}</div>
  </div>

</div>

<style>
  .helix-hud {
    position: absolute;
    inset: 0;
    pointer-events: none;
    z-index: 10;
    font-family: var(--la-font-mono);
  }

  /* ── Legend (top-left) ────────────────────────────────────── */
  .hud-legend {
    position: absolute;
    top: 12px;
    left: 12px;
    background: color-mix(in srgb, var(--la-bg-void, #050508) 80%, transparent);
    border: 1px solid var(--la-hair-base);
    padding: 8px 10px;
    min-width: 200px;
  }

  .hud-section-label {
    font-size: 8px;
    letter-spacing: 0.14em;
    color: var(--la-text-mute, #475569);
    text-transform: uppercase;
    margin-bottom: 6px;
  }

  .legend-row {
    display: flex;
    gap: 6px;
    align-items: baseline;
    margin-bottom: 3px;
  }

  .legend-sib {
    font-size: 9px;
    letter-spacing: 0.08em;
    color: var(--la-text-dim, #64748b);
    width: 46px;
    flex-shrink: 0;
  }

  .legend-type {
    font-size: 9px;
    color: var(--la-text-bright, #f1f5f9);
    flex: 1;
    letter-spacing: 0.04em;
  }

  .legend-meta {
    font-size: 8px;
    color: var(--la-text-mute, #475569);
    letter-spacing: 0.06em;
  }

  /* ── Readout (top-right) ──────────────────────────────────── */
  .hud-readout {
    position: absolute;
    top: 12px;
    right: 12px;
    background: color-mix(in srgb, var(--la-bg-void, #050508) 80%, transparent);
    border: 1px solid var(--la-hair-base);
    padding: 8px 10px;
    min-width: 200px;
  }

  .readout-row {
    display: flex;
    gap: 8px;
    align-items: baseline;
    margin-bottom: 3px;
  }

  .readout-label {
    font-size: 8px;
    letter-spacing: 0.12em;
    color: var(--la-text-mute, #475569);
    text-transform: uppercase;
    width: 56px;
    flex-shrink: 0;
  }

  .readout-val {
    font-size: 9px;
    color: var(--la-text-bright, #f1f5f9);
    letter-spacing: 0.06em;
  }

  .readout-ok .readout-val  { color: var(--la-ok, #22c55e); }
  .readout-warn .readout-val { color: var(--la-warn, #f59e0b); }
  .readout-crit .readout-val { color: var(--la-err, #ef4444); }
  .readout-active .readout-val { color: var(--la-accent, #00BFFF); }

  /* ── Urgent banner (top-center) ──────────────────────────── */
  .hud-banner {
    position: absolute;
    top: 0;
    left: 50%;
    transform: translateX(-50%);
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 4px 14px;
    background: color-mix(in srgb, var(--la-err, #ef4444) 20%, var(--la-bg-void, #050508));
    border-bottom: 1px solid var(--la-err, #ef4444);
    border-left: 1px solid var(--la-err, #ef4444);
    border-right: 1px solid var(--la-err, #ef4444);
    white-space: nowrap;
  }

  .banner-verdict {
    font-size: 9px;
    letter-spacing: 0.12em;
    color: var(--la-err, #ef4444);
    font-weight: 700;
  }

  .banner-source {
    font-size: 9px;
    color: var(--la-text-dim, #64748b);
    letter-spacing: 0.08em;
  }

  .banner-msg {
    font-size: 9px;
    color: var(--la-text-bright, #f1f5f9);
    letter-spacing: 0.04em;
    max-width: 320px;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .banner-time {
    font-size: 8px;
    color: var(--la-text-mute, #475569);
    letter-spacing: 0.06em;
  }

  /* ── Texture toggle (bottom-left) ────────────────────────── */
  .hud-texture-toggle {
    position: absolute;
    bottom: 12px;
    left: 12px;
    pointer-events: auto;
    background: color-mix(in srgb, var(--la-bg-void, #050508) 80%, transparent);
    border: 1px solid var(--la-hair-base);
    padding: 8px 10px;
  }

  .toggle-row {
    display: flex;
    gap: 4px;
    margin-bottom: 4px;
  }

  .tex-btn {
    font-family: var(--la-font-mono);
    font-size: 8px;
    letter-spacing: 0.1em;
    color: var(--la-text-dim, #64748b);
    background: transparent;
    border: 1px solid var(--la-hair-base);
    padding: 2px 6px;
    cursor: pointer;
    transition: color 0.1s, border-color 0.1s;
  }

  .tex-btn:hover {
    color: var(--la-text-bright, #f1f5f9);
    border-color: var(--la-text-mute, #475569);
  }

  .tex-btn.tex-active {
    color: var(--la-accent, #00BFFF);
    border-color: var(--la-accent, #00BFFF);
    background: color-mix(in srgb, var(--la-accent, #00BFFF) 10%, transparent);
  }

  .tex-label {
    font-size: 8px;
    letter-spacing: 0.1em;
    color: var(--la-text-mute, #475569);
    text-transform: uppercase;
  }
</style>
