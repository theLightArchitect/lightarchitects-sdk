<script lang="ts">
  import { activeBuild, buildStats, sparklineBuilds } from '$lib/stores';
  import type { Polytope4DType } from '$lib/polytopes4d-canvas2d';

  const isIdle = $derived($buildStats.inProgress === 0 && $buildStats.completed + $buildStats.failed > 0);

  const lastActiveLabel = $derived.by(() => {
    const latest = $sparklineBuilds[$sparklineBuilds.length - 1];
    if (!latest?.updatedAt) return null;
    const ms = Date.now() - new Date(latest.updatedAt).getTime();
    const mins = Math.floor(ms / 60_000);
    if (mins < 60) return `${mins}m ago`;
    const hrs = Math.floor(mins / 60);
    if (hrs < 24) return `${hrs}h ago`;
    return `${Math.floor(hrs / 24)}d ago`;
  });

  function sparklinePath(builds: { confidence: number; status: string }[]): string {
    if (builds.length < 2) return '';
    const W = 120, H = 36, pad = 4;
    const xStep = (W - pad * 2) / (builds.length - 1);
    const points = builds.map((b, i) => {
      const x = pad + i * xStep;
      const y = H - pad - (b.confidence * (H - pad * 2));
      return `${x.toFixed(1)},${y.toFixed(1)}`;
    });
    return `M ${points.join(' L ')}`;
  }

  function dotColor(status: string): string {
    if (status === 'completed') return 'var(--la-semantic-ok)';
    if (status === 'failed')    return 'var(--la-semantic-error)';
    return 'var(--la-semantic-warn)';
  }
</script>

<div class="card-label">
  BUILD HEALTH
  {#if isIdle}
    <span class="idle-badge">IDLE</span>
    {#if lastActiveLabel}<span class="dim-note">last: {lastActiveLabel}</span>{/if}
  {/if}
</div>

<div class="sparkline-wrap">
  {#if $sparklineBuilds.length >= 2}
    <svg class="sparkline" viewBox="0 0 120 36" preserveAspectRatio="none">
      <path class="spark-line" d={sparklinePath($sparklineBuilds)} />
      {#each $sparklineBuilds as b, i}
        {@const W = 120}
        {@const H = 36}
        {@const pad = 4}
        {@const xStep = (W - pad * 2) / ($sparklineBuilds.length - 1)}
        {@const x = pad + i * xStep}
        {@const y = H - pad - (b.confidence * (H - pad * 2))}
        <circle cx={x} cy={y} r="2.5" fill={dotColor(b.status)} />
      {/each}
    </svg>
  {:else}
    <div class="spark-empty">no history</div>
  {/if}
</div>

<div class="health-stats">
  <div class="hs-row"><span class="hs-val ok">{$buildStats.completed}</span><span class="hs-key">done</span></div>
  <div class="hs-row"><span class="hs-val act">{$buildStats.inProgress}</span><span class="hs-key">active</span></div>
  <div class="hs-row"><span class="hs-val err">{$buildStats.failed}</span><span class="hs-key">failed</span></div>
  <div class="hs-row"><span class="hs-val dim">{$buildStats.pending}</span><span class="hs-key">queued</span></div>
</div>

{#if $activeBuild}
  <div class="active-build-row">
    <span class="ab-label">active</span>
    <span class="ab-name">{$activeBuild.codename ?? $activeBuild.name}</span>
    <span class="ab-conf">{Math.round(($activeBuild.confidence ?? 0) * 100)}%</span>
  </div>
{/if}

<style>
  .card-label { font-size: 9px; font-weight: 700; letter-spacing: var(--la-tk-loose); color: var(--la-text-mute); display: flex; align-items: center; gap: 6px; flex-shrink: 0; }
  .idle-badge { background: var(--la-semantic-warn); color: #000; font-size: 8px; padding: 1px 4px; }
  .dim-note { color: var(--la-text-mute); font-weight: 400; }
  .sparkline-wrap { height: 40px; }
  .sparkline { width: 100%; height: 100%; }
  .spark-line { fill: none; stroke: var(--la-struct-primary); stroke-width: 1.5; }
  .spark-empty { display: flex; align-items: center; justify-content: center; height: 100%; color: var(--la-text-mute); font-size: 9px; }
  .health-stats { display: flex; gap: 8px; }
  .hs-row { display: flex; flex-direction: column; align-items: center; gap: 1px; }
  .hs-val { font-size: 14px; font-weight: 600; }
  .hs-val.ok  { color: var(--la-semantic-ok); }
  .hs-val.act { color: var(--la-struct-primary); }
  .hs-val.err { color: var(--la-semantic-error); }
  .hs-val.dim { color: var(--la-text-mute); }
  .hs-key { font-size: 8px; color: var(--la-text-mute); }
  .active-build-row { display: flex; gap: 6px; align-items: center; border-top: 1px solid var(--la-hair-faint); padding-top: 6px; font-size: 10px; }
  .ab-label { color: var(--la-text-mute); }
  .ab-name  { color: var(--la-struct-primary); flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .ab-conf  { color: var(--la-semantic-ok); }
</style>
