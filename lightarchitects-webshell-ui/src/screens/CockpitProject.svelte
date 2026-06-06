<script lang="ts">
  import { scope } from '$lib/cockpit/stores/scope';
  import type { RouteScope } from '$lib/cockpit/stores/scope';
  import CockpitShell from '$lib/cockpit/shell/CockpitShell.svelte';
  import BuildHealthCard from '$lib/../components/Cockpit/BuildHealthCard.svelte';
  import HitlEscalationsCard from '$lib/../components/Cockpit/HitlEscalationsCard.svelte';
  import HITLInbox from '$lib/../components/Cockpit/HITLInbox.svelte';
  import StrandMosaicCard from '$lib/../components/Cockpit/StrandMosaicCard.svelte';
  import PRMetadataBlock from '$lib/../components/Cockpit/PRMetadataBlock.svelte';
  import PRVerbSurface from '$lib/../components/Cockpit/PRVerbSurface.svelte';
  import { navigate } from '$lib/routes';
  import { builds } from '$lib/stores';
  import { selectedTarget } from '$lib/cockpit/stores';
  import { select } from '$lib/cockpit/stores/selection';

  // d1 — /cockpit/project/:projectId
  const currentScope = $derived($scope as Extract<RouteScope, { kind: 'project' }> | null);

  const STATUS_COLOR: Record<string, string> = {
    in_progress: 'var(--la-struct-primary)', completed: 'var(--la-semantic-ok)',
    failed: 'var(--la-semantic-error)', queued: 'var(--la-text-mute)', paused: 'var(--la-semantic-warn)',
  };

  interface StrategyEntry { id: string; label: string; cls: 'L0'|'L2'; sibling: string; registered: boolean; description: string; }
  const STRATEGIES: StrategyEntry[] = [
    { id: 'build',          label: 'Build',         cls: 'L2', sibling: 'CORSO',  registered: true,  description: 'LASDLC build pipeline (6–7 phases)' },
    { id: 'secure',         label: 'Secure',         cls: 'L2', sibling: 'SERAPH', registered: true,  description: 'Security assessment loop' },
    { id: 'scrum',          label: 'Scrum',          cls: 'L2', sibling: 'AYIN',   registered: true,  description: 'Multi-sibling squad review' },
    { id: 'enrich',         label: 'Enrich',         cls: 'L2', sibling: 'EVA',    registered: true,  description: 'EVA 8-layer memory enrichment' },
    { id: 'gate',           label: 'Gate',           cls: 'L2', sibling: 'LÆX',   registered: true,  description: 'LASDLC 7-gate V0→Q→S→I→N→D→V' },
    { id: 'scope_governor', label: 'Scope Governor', cls: 'L2', sibling: 'SERAPH', registered: true,  description: '5-gate AND-scope validation' },
    { id: 'bcra',           label: 'BCRA',           cls: 'L0', sibling: 'SERAPH', registered: false, description: 'FAIR/Bowtie blast-score risk analysis' },
    { id: 'drain',          label: 'Drain',          cls: 'L0', sibling: 'CORSO',  registered: false, description: 'Bounded queue-drain processor' },
    { id: 'multipass_verify',label:'Multi-Pass',     cls: 'L0', sibling: 'CORSO',  registered: false, description: 'N-pass independent verification' },
    { id: 'red_team',       label: 'Red Team',       cls: 'L0', sibling: 'SERAPH', registered: false, description: 'SERAPH 5-phase adversarial assessment' },
  ];
  let selectedStrategy = $state<string | null>(null);
  function selectStrategy(id: string, registered: boolean) { if (!registered) return; selectedStrategy = selectedStrategy === id ? null : id; }

  // PR detail
  function parsePrUrl(htmlUrl: string): { owner: string; repo: string; number: number } | null {
    const m = htmlUrl.match(/^https:\/\/github\.com\/([^/]+)\/([^/]+)\/pull\/(\d+)$/);
    if (!m) return null;
    return { owner: m[1], repo: m[2], number: parseInt(m[3], 10) };
  }
  const selectedPr = $derived.by(() => { const t = $selectedTarget; if (!t || t.type !== 'pr') return null; return parsePrUrl(t.id); });
  let prHeadSha = $state('');
  $effect(() => { if (!selectedPr) prHeadSha = ''; });
</script>

<CockpitShell>
  <div class="cockpit-project">
    <!-- Project header -->
    <header class="project-hdr">
      <span class="project-title">{currentScope?.project_id ?? '…'}</span>
      <span class="project-depth-badge">PROJECT</span>
    </header>

    <!-- Bento grid — d1 scope -->
    <div class="bento-d1">

      <!-- ── BUILD HEALTH ────────────────────────────────────────────── -->
      <div class="card card-health" data-area="health" data-card-role="build-health">
        <BuildHealthCard />
      </div>

      <!-- ── ESCALATIONS / HITL ────────────────────────────────────────── -->
      <div class="card card-hitl" data-area="escalations" data-card-role="hitl-escalations">
        <HitlEscalationsCard />
      </div>

      <!-- ── BUILDS RAIL ────────────────────────────────────────────────── -->
      <div class="card card-builds" data-area="builds" data-card-role="builds-rail">
        <div class="card-label">BUILDS <span class="dim-note">{$builds.length} total</span></div>
        {#if $builds.length === 0}
          <div class="empty-state">no builds yet</div>
        {:else}
          <div class="builds-rail">
            {#each $builds.slice().sort((a, b) => b.updatedAt > a.updatedAt ? 1 : -1).slice(0, 12) as b (b.id)}
              <button class="build-row" class:build-row-active={b.status === 'in_progress'} onclick={() => { select({ kind: 'build', codename: b.codename ?? b.id }, $scope); navigate('/cockpit/build/:codename', { codename: b.codename ?? b.id }); }}>
                <span class="br-dot" style="background:{STATUS_COLOR[b.status] ?? 'var(--la-text-mute)'}"></span>
                <span class="br-name">{b.codename ?? b.name}</span>
                <span class="br-pillar">{b.currentPillar ?? ''}</span>
                <span class="br-conf">{Math.round(b.confidence * 100)}%</span>
              </button>
            {/each}
            {#if $builds.length > 12}<div class="br-more">+{$builds.length - 12} more</div>{/if}
          </div>
        {/if}
      </div>

      <!-- ── HITL INBOX ──────────────────────────────────────────────────── -->
      <div class="card card-inbox" data-area="inbox" data-card-role="hitl-inbox">
        <div class="card-label">HITL INBOX {#if $selectedTarget?.type === 'pr'}<span class="dim-note">target selected</span>{/if}</div>
        <HITLInbox />
      </div>

      <!-- ── STRATEGY CATALOGUE ────────────────────────────────────────── -->
      <div class="card card-strategies" data-area="strategies" data-card-role="strategy-catalogue">
        <div class="card-label">
          STRATEGIES
          <span class="dim-note">{STRATEGIES.filter(s => s.registered).length} reg · {STRATEGIES.filter(s => !s.registered).length} exec</span>
          {#if selectedStrategy}<button class="strat-clear-btn" onclick={() => { selectedStrategy = null; }}>✕</button>{/if}
        </div>
        <div class="strat-grid">
          {#each STRATEGIES as s (s.id)}
            <button class="strat-tile" class:strat-tile-l2={s.cls === 'L2'} class:strat-tile-l0={s.cls === 'L0'} class:strat-tile-selected={selectedStrategy === s.id} class:strat-tile-disabled={!s.registered} onclick={() => selectStrategy(s.id, s.registered)} disabled={!s.registered} title={s.registered ? `Click to select ${s.label}` : 'L0 — executor injection required'} aria-pressed={selectedStrategy === s.id}>
              <div class="strat-top"><span class="strat-label">{s.label}</span><span class="strat-cls" class:strat-cls-l2={s.cls === 'L2'} class:strat-cls-l0={s.cls === 'L0'}>{s.cls}</span></div>
              <div class="strat-desc">{s.description}</div>
              <div class="strat-bot"><span class="strat-sib">{s.sibling}</span>{#if !s.registered}<span class="strat-exec-badge">executor</span>{/if}</div>
            </button>
          {/each}
        </div>
      </div>

      <!-- ── STRAND MOSAIC ──────────────────────────────────────────────── -->
      <div class="card card-mosaic" data-area="mosaic" data-card-role="strand-mosaic">
        <div class="card-label">STRAND MOSAIC</div>
        <StrandMosaicCard />
      </div>

    </div><!-- /bento-d1 -->

    <!-- ── PR DETAIL PANEL ────────────────────────────────────────────── -->
    {#if selectedPr}
      <div class="pr-detail-panel" data-card-role="pr-detail-panel">
        <div class="pr-detail-header">
          <span class="pr-detail-label">PR REVIEW</span>
          <span class="pr-detail-target">{$selectedTarget?.label ?? ''}</span>
          <button class="pr-detail-close" onclick={() => selectedTarget.set(null)} aria-label="Close PR detail">✕</button>
        </div>
        <div class="pr-detail-body">
          <div class="pr-detail-meta"><PRMetadataBlock owner={selectedPr.owner} repo={selectedPr.repo} prNumber={selectedPr.number} onHeadSha={(sha) => { prHeadSha = sha; }} /></div>
          <div class="pr-detail-verbs"><PRVerbSurface owner={selectedPr.owner} repo={selectedPr.repo} prNumber={selectedPr.number} headSha={prHeadSha} /></div>
        </div>
      </div>
    {/if}

  </div><!-- /cockpit-project -->
</CockpitShell>

<style>
  .cockpit-project { display: flex; flex-direction: column; height: 100%; padding: 12px 16px; overflow-y: auto; gap: 12px; }
  .project-hdr { display: flex; align-items: center; gap: 10px; padding-bottom: 8px; border-bottom: 1px solid var(--la-hair-base); }
  .project-title { font-size: 13px; font-weight: 700; letter-spacing: var(--la-tk-mid); color: var(--scope-accent, var(--scope-d1)); }
  .project-depth-badge { font-size: 9px; font-weight: 700; letter-spacing: 0.1em; color: var(--scope-accent, var(--scope-d1)); padding: 1px 5px; border: 1px solid var(--scope-accent, var(--scope-d1)); }

  .bento-d1 {
    display: grid;
    grid-template-columns: 1fr 1fr 1fr;
    grid-template-rows: auto auto auto;
    grid-template-areas:
      "health escalations builds"
      "inbox inbox strategies"
      "mosaic mosaic mosaic";
    gap: 12px;
    flex: 1;
    min-height: 0;
  }

  .card { background: var(--la-bg-panel); border: 1px solid var(--la-hair-base); padding: 12px; display: flex; flex-direction: column; gap: 8px; min-height: 0; overflow: hidden; }
  .card-label { font-size: 9px; font-weight: 700; letter-spacing: var(--la-tk-loose); color: var(--la-text-mute); display: flex; align-items: center; gap: 6px; flex-shrink: 0; }
  .card-health     { grid-area: health; }
  .card-hitl       { grid-area: escalations; }
  .card-builds     { grid-area: builds; overflow-y: auto; }
  .card-inbox      { grid-area: inbox; overflow-y: auto; }
  .card-strategies { grid-area: strategies; overflow-y: auto; }
  .card-mosaic     { grid-area: mosaic; }
  .empty-state     { color: var(--la-text-mute); font-size: 10px; }
  .dim-note        { color: var(--la-text-mute); font-weight: 400; }

  /* Builds rail */
  .builds-rail { display: flex; flex-direction: column; gap: 3px; }
  .build-row { display: flex; align-items: center; gap: 6px; background: none; border: none; padding: 4px 0; cursor: pointer; width: 100%; font-size: 9px; }
  .build-row:hover { background: rgba(255,255,255,0.04); }
  .build-row-active .br-name { color: var(--la-struct-primary); }
  .br-dot { width: 6px; height: 6px; border-radius: 50%; flex-shrink: 0; }
  .br-name { flex: 1; text-align: left; color: var(--la-text-dim); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .br-pillar { color: var(--la-text-mute); }
  .br-conf { color: var(--la-text-secondary); }
  .br-more { font-size: 8px; color: var(--la-text-mute); padding: 2px 0; }

  /* Strategy catalogue */
  .strat-grid { display: grid; grid-template-columns: 1fr 1fr; gap: 4px; }
  .strat-tile { background: var(--la-bg-base); border: 1px solid var(--la-hair-base); padding: 6px; text-align: left; cursor: pointer; display: flex; flex-direction: column; gap: 2px; }
  .strat-tile-l2 { border-color: var(--la-struct-primary); }
  .strat-tile-l0 { opacity: 0.5; cursor: default; }
  .strat-tile-selected { background: rgba(100,160,255,0.08); }
  .strat-tile-disabled { opacity: 0.4; cursor: not-allowed; }
  .strat-top { display: flex; justify-content: space-between; align-items: center; }
  .strat-label { font-size: 9px; font-weight: 700; color: var(--la-text-dim); }
  .strat-cls { font-size: 8px; }
  .strat-cls-l2 { color: var(--la-struct-primary); }
  .strat-cls-l0 { color: var(--la-text-mute); }
  .strat-desc { font-size: 8px; color: var(--la-text-mute); line-height: 1.3; }
  .strat-bot { display: flex; align-items: center; gap: 4px; }
  .strat-sib { font-size: 7px; color: var(--la-text-mute); }
  .strat-exec-badge { font-size: 7px; color: var(--la-semantic-warn); border: 1px solid var(--la-semantic-warn); padding: 0 3px; }
  .strat-clear-btn { background: none; border: none; color: var(--la-text-mute); cursor: pointer; font-size: 10px; padding: 0; }

  /* PR detail */
  .pr-detail-panel { background: var(--la-bg-panel); border: 1px solid var(--la-hair-strong); margin-top: 12px; }
  .pr-detail-header { display: flex; align-items: center; gap: 8px; padding: 8px 12px; border-bottom: 1px solid var(--la-hair-base); }
  .pr-detail-label { font-size: 9px; font-weight: 700; color: var(--la-text-mute); letter-spacing: 0.08em; }
  .pr-detail-target { font-size: 10px; color: var(--la-struct-primary); flex: 1; }
  .pr-detail-close { background: none; border: none; color: var(--la-text-mute); cursor: pointer; font-size: 12px; }
  .pr-detail-body { display: flex; }
  .pr-detail-meta  { flex: 1; padding: 12px; border-right: 1px solid var(--la-hair-base); }
  .pr-detail-verbs { flex: 1; padding: 12px; }
</style>
