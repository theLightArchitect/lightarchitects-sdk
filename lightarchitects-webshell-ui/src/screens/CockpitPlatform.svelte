<script lang="ts">
  import CockpitShell from '$lib/cockpit/shell/CockpitShell.svelte';
  import PresetChips from '$lib/../components/Cockpit/PresetChips.svelte';
  import TargetBreadcrumb from '$lib/../components/Cockpit/TargetBreadcrumb.svelte';
  import QuickPickPalette from '$lib/../components/Cockpit/QuickPickPalette.svelte';
  import HITLInbox from '$lib/../components/Cockpit/HITLInbox.svelte';
  import NorthstarPulseCard from '$lib/../components/Cockpit/NorthstarPulseCard.svelte';
  import StrandMosaicCard from '$lib/../components/Cockpit/StrandMosaicCard.svelte';
  import SmartDispatchCard from '$lib/../components/Cockpit/SmartDispatchCard.svelte';
  import SquadConstellationCard from '$lib/../components/Cockpit/SquadConstellationCard.svelte';
  import { activeBuild, buildStats } from '$lib/stores';
  import { selectedTarget } from '$lib/cockpit/stores';

  // d0 — /cockpit/platform
  interface StrategyEntry { id: string; label: string; cls: 'L0'|'L2'; sibling: string; registered: boolean; description: string; }
  const STRATEGIES: StrategyEntry[] = [
    { id: 'build',           label: 'Build',         cls: 'L2', sibling: 'CORSO',  registered: true,  description: 'LASDLC build pipeline (6–7 phases)' },
    { id: 'secure',          label: 'Secure',         cls: 'L2', sibling: 'SERAPH', registered: true,  description: 'Security assessment loop' },
    { id: 'scrum',           label: 'Scrum',          cls: 'L2', sibling: 'AYIN',   registered: true,  description: 'Multi-sibling squad review' },
    { id: 'enrich',          label: 'Enrich',         cls: 'L2', sibling: 'EVA',    registered: true,  description: 'EVA 8-layer memory enrichment' },
    { id: 'gate',            label: 'Gate',           cls: 'L2', sibling: 'LÆX',   registered: true,  description: 'LASDLC 7-gate V0→Q→S→I→N→D→V' },
    { id: 'scope_governor',  label: 'Scope Governor', cls: 'L2', sibling: 'SERAPH', registered: true,  description: '5-gate AND-scope validation' },
    { id: 'bcra',            label: 'BCRA',           cls: 'L0', sibling: 'SERAPH', registered: false, description: 'FAIR/Bowtie blast-score risk analysis' },
    { id: 'drain',           label: 'Drain',          cls: 'L0', sibling: 'CORSO',  registered: false, description: 'Bounded queue-drain processor' },
    { id: 'multipass_verify',label: 'Multi-Pass',     cls: 'L0', sibling: 'CORSO',  registered: false, description: 'N-pass independent verification' },
    { id: 'red_team',        label: 'Red Team',       cls: 'L0', sibling: 'SERAPH', registered: false, description: 'SERAPH 5-phase adversarial assessment' },
  ];
  let selectedStrategy = $state<string | null>(null);
  function selectStrategy(id: string, registered: boolean) { if (!registered) return; selectedStrategy = selectedStrategy === id ? null : id; }
</script>

<CockpitShell>
  <div class="cockpit-platform">

    <!-- Platform header -->
    <header class="platform-hdr">
      <span class="platform-title">PLATFORM</span>
      <span class="platform-depth-badge">D0</span>
      <div class="hdr-right">
        <PresetChips />
        <div class="hdr-badges">
          {#if $activeBuild}
            <span class="badge badge-active">{$activeBuild.codename ?? $activeBuild.id.slice(0, 8)}</span>
          {:else}
            <span class="badge badge-idle">no active build</span>
          {/if}
          <span class="badge badge-stat">{$buildStats.inProgress} active</span>
        </div>
      </div>
    </header>
    <TargetBreadcrumb />
    <QuickPickPalette />

    <!-- Bento grid — d0 scope -->
    <div class="bento-d0">

      <!-- ── NORTHSTAR PULSE ──────────────────────────────────────────────────── -->
      <div class="card card-northstar" data-area="northstar" data-card-role="northstar-pulse">
        <div class="card-label">NORTHSTAR PULSE</div>
        <NorthstarPulseCard />
      </div>

      <!-- ── STRAND MOSAIC ──────────────────────────────────────────────────── -->
      <div class="card card-mosaic" data-area="mosaic" data-card-role="strand-mosaic">
        <div class="card-label">STRAND MOSAIC</div>
        <StrandMosaicCard />
      </div>

      <!-- ── SMART DISPATCH ─────────────────────────────────────────────────── -->
      <div class="card card-dispatch" data-area="dispatch" data-card-role="smart-dispatch">
        <div class="card-label">SMART DISPATCH</div>
        <SmartDispatchCard />
      </div>

      <!-- ── SQUAD CONSTELLATION ────────────────────────────────────────────── -->
      <div class="card card-constellation" data-area="constellation" data-card-role="squad-constellation">
        <div class="card-label">SQUAD CONSTELLATION</div>
        <SquadConstellationCard />
      </div>

      <!-- ── HITL INBOX ──────────────────────────────────────────────────────── -->
      <div class="card card-inbox" data-area="inbox" data-card-role="hitl-inbox">
        <div class="card-label">
          HITL INBOX
          {#if $selectedTarget?.type === 'pr'}<span class="dim-note">target selected</span>{/if}
        </div>
        <HITLInbox />
      </div>

      <!-- ── STRATEGY CATALOGUE ──────────────────────────────────────────────── -->
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

    </div><!-- /bento-d0 -->
  </div><!-- /cockpit-platform -->
</CockpitShell>

<style>
  .cockpit-platform { display: flex; flex-direction: column; height: 100%; padding: 12px 16px; overflow-y: auto; gap: 12px; }

  .platform-hdr { display: flex; align-items: center; gap: 10px; padding-bottom: 8px; border-bottom: 1px solid var(--la-hair-base); }
  .platform-title { font-size: 13px; font-weight: 700; letter-spacing: var(--la-tk-mid); color: var(--scope-accent, var(--scope-d0)); }
  .platform-depth-badge { font-size: 9px; font-weight: 700; letter-spacing: 0.1em; color: var(--scope-accent, var(--scope-d0)); padding: 1px 5px; border: 1px solid var(--scope-accent, var(--scope-d0)); }
  .hdr-right { margin-left: auto; display: flex; align-items: center; gap: 8px; }
  .hdr-badges { display: flex; gap: 6px; }
  .badge { font-size: 9px; padding: 2px 6px; letter-spacing: var(--la-tk-mid); border: 1px solid var(--la-hair-base); color: var(--la-text-dim); }
  .badge-active { border-color: var(--la-semantic-ok); color: var(--la-semantic-ok); }
  .badge-stat   { border-color: var(--la-hair-strong); color: var(--la-text-dim); }
  .badge-idle   { border-color: var(--la-hair-faint); color: var(--la-text-mute); }

  .bento-d0 {
    display: grid;
    grid-template-columns: 1fr 1fr 1fr;
    grid-template-rows: auto auto auto;
    grid-template-areas:
      "northstar mosaic inbox"
      "dispatch constellation inbox"
      "strategies strategies strategies";
    gap: 12px;
    flex: 1;
    min-height: 0;
  }

  .card { background: var(--la-bg-panel); border: 1px solid var(--la-hair-base); padding: 12px; display: flex; flex-direction: column; gap: 8px; min-height: 0; overflow: hidden; }
  .card-label { font-size: 9px; font-weight: 700; letter-spacing: var(--la-tk-loose); color: var(--la-text-mute); display: flex; align-items: center; gap: 6px; flex-shrink: 0; }
  .card-northstar     { grid-area: northstar; }
  .card-mosaic        { grid-area: mosaic; }
  .card-dispatch      { grid-area: dispatch; }
  .card-constellation { grid-area: constellation; }
  .card-inbox         { grid-area: inbox; overflow-y: auto; }
  .card-strategies    { grid-area: strategies; }
  .dim-note { color: var(--la-text-mute); font-weight: 400; }

  /* Strategy catalogue */
  .strat-grid { display: grid; grid-template-columns: repeat(5, 1fr); gap: 6px; }
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
</style>
