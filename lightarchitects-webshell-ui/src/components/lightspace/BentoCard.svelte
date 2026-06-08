<!--
  @component BentoCard
  @description Card dispatcher — reads card.kind and renders the appropriate
    card kind component. Handles the card chrome (header, kind label, title,
    expand/shrink, tier dot, provenance footer).

  @contract varies — delegates to kind-specific card component for @reads/@mutates/@api
  @reads lightspaceCanvasStore (via props from BentoCanvas)
  @mutates lightspaceCanvasStore.expandedCardId (via ontoggleexpand prop)
  @api none

  @mockup-ref arch/lightspace-mockup.html → .la-card, renderCardHtml(), renderCardBody()
-->
<script lang="ts">
  import type { BentoCardData, CardSpan } from '$lib/lightspace-types';
  import { canvasRestoreFromTomb } from '$lib/lightspace-stores';
  import MonitorCard     from './cards/MonitorCard.svelte';
  import InstrumentCard  from './cards/InstrumentCard.svelte';
  import TraceCard       from './cards/TraceCard.svelte';
  import ThinkingCard    from './cards/ThinkingCard.svelte';
  import ToolCallCard    from './cards/ToolCallCard.svelte';
  import BashCard        from './cards/BashCard.svelte';
  import AgentSpawnCard  from './cards/AgentSpawnCard.svelte';
  import DiffCard        from './cards/DiffCard.svelte';
  import ArtifactCard    from './cards/ArtifactCard.svelte';
  import ResearchCard    from './cards/ResearchCard.svelte';
  import ArchGalleryCard from './cards/ArchGalleryCard.svelte';
  import BranchLaneCard  from './cards/BranchLaneCard.svelte';

  interface Props {
    card: BentoCardData;
    span: CardSpan;
    expanded?: boolean;
    shrunk?: boolean;
    highlighted?: boolean;
    animDelay?: number;
    ontoggleexpand: () => void;
  }

  let {
    card, span, expanded = false, shrunk = false,
    highlighted = false, animDelay = 0, ontoggleexpand,
  }: Props = $props();

  const KIND_LABEL: Partial<Record<typeof card.kind, string>> = {
    monitor:    'STATUS',
    instrument: 'METRICS',
    trace:      'ACTIVITY',
    archgallery:'DIAGRAMS',
    agentspawn: 'AGENT',
    branchlane: 'PHASES',
  };

  const KIND_CLASS: Partial<Record<typeof card.kind, string>> = {
    monitor:    'kind-monitor',
    instrument: 'kind-instrument',
    trace:      'kind-trace',
    thinking:   'kind-thinking',
    toolcall:   'kind-toolcall',
    bash:       'kind-bash',
    agentspawn: 'kind-agentspawn',
    diff:       'kind-diff',
    artifact:   'kind-artifact',
    research:   'kind-research',
    archgallery:'kind-archgallery',
    branchlane: 'kind-branchlane',
  };
</script>

<section
  class="ls-card ls-span-{span.replace('span-', '')} {KIND_CLASS[card.kind] ?? ''}"
  class:ls-card-expanded={expanded}
  class:ls-card-shrunk={shrunk}
  class:ls-card-highlighted={highlighted}
  style="animation-delay: {animDelay}ms"
  onclick={ontoggleexpand}
  role="button"
  tabindex="0"
  onkeydown={(e) => e.key === 'Enter' && ontoggleexpand()}
>
  <!-- Tier dot — operator-visible tier affordance -->
  <span class="ls-card-tier-dot" aria-hidden="true"></span>

  <header class="ls-card-head">
    <span class="ls-card-kind">{KIND_LABEL[card.kind] ?? card.kind.toUpperCase()}</span>
    <span class="ls-card-title">{card.title}</span>
    <div class="ls-card-ctrls" onclick={(e) => e.stopPropagation()} role="presentation">
      <button class="ls-card-ctrl" title="minimize" aria-label="minimize card">_</button>
      <button class="ls-card-ctrl" title="graduate to drawer" aria-label="graduate card">↳</button>
      <button class="ls-card-ctrl" title="detach" aria-label="detach card">×</button>
    </div>
  </header>

  <!-- Kind-specific card body -->
  <div class="ls-card-body">
    {#if card.kind === 'monitor'}
      <MonitorCard data={card.data} />
    {:else if card.kind === 'instrument'}
      <InstrumentCard data={card.data} />
    {:else if card.kind === 'trace'}
      <TraceCard data={card.data} />
    {:else if card.kind === 'thinking'}
      <ThinkingCard data={card.data} />
    {:else if card.kind === 'toolcall'}
      <ToolCallCard data={card.data} />
    {:else if card.kind === 'bash'}
      <BashCard data={card.data} />
    {:else if card.kind === 'agentspawn'}
      <AgentSpawnCard data={card.data} />
    {:else if card.kind === 'diff'}
      <DiffCard data={card.data} />
    {:else if card.kind === 'artifact'}
      <ArtifactCard data={card.data} />
    {:else if card.kind === 'research'}
      <ResearchCard data={card.data} />
    {:else if card.kind === 'archgallery'}
      <ArchGalleryCard data={card.data} />
    {:else if card.kind === 'branchlane'}
      <BranchLaneCard data={card.data} />
    {/if}
  </div>

  <footer class="ls-card-foot">
    <span class="ls-prov-trace" aria-hidden="true">⊕ trace</span>
  </footer>
</section>

<style>
.ls-card {
  background: var(--ls-card);
  border: 1px solid var(--ls-border-base);
  border-left: 3px solid var(--kind-color, var(--ls-border-strong));
  position: relative;
  display: flex;
  flex-direction: column;
  min-height: 0;
  overflow: hidden;
  cursor: pointer;
  transition: border-color var(--ls-fast), opacity var(--ls-mid), transform var(--ls-mid);
  animation: ls-card-in var(--ls-mid) both;
}

@keyframes ls-card-in {
  from { opacity: 0; transform: translateY(8px); }
  to   { opacity: 1; transform: none; }
}

.ls-card:hover { border-color: var(--ls-border-accent); }
.ls-card.ls-card-expanded {
  border-color: var(--ls-acc) !important;
  box-shadow: 0 0 0 1.5px var(--ls-acc), 0 12px 40px rgba(0,0,0,0.45);
  z-index: 10;
}
.ls-card.ls-card-shrunk { opacity: 0.36; transform: scale(0.97); }
.ls-card.ls-card-highlighted {
  border-color: var(--ls-acc) !important;
  box-shadow: 0 0 0 1px var(--ls-acc), 0 0 24px rgba(77,142,255,0.28);
}

/* Kind colors via CSS custom property */
.ls-card.kind-monitor    { --kind-color: var(--ls-kind-monitor); }
.ls-card.kind-instrument { --kind-color: var(--ls-kind-instrument); }
.ls-card.kind-trace      { --kind-color: var(--ls-kind-trace); }
.ls-card.kind-thinking   { --kind-color: var(--ls-kind-thinking); }
.ls-card.kind-toolcall   { --kind-color: var(--ls-kind-toolcall); }
.ls-card.kind-bash       { --kind-color: var(--ls-kind-bash); }
.ls-card.kind-agentspawn { --kind-color: var(--ls-kind-agentspawn); }
.ls-card.kind-diff       { --kind-color: var(--ls-kind-diff); }
.ls-card.kind-artifact   { --kind-color: var(--ls-kind-artifact); }
.ls-card.kind-research   { --kind-color: var(--ls-kind-research); }
.ls-card.kind-archgallery{ --kind-color: var(--ls-kind-archgallery); }
.ls-card.kind-branchlane { --kind-color: var(--ls-kind-branchlane); }

.ls-card-tier-dot {
  position: absolute;
  top: 8px; right: 8px;
  width: 6px; height: 6px;
  border-radius: 50%;
  background: var(--ls-border-base);
  opacity: 0.6;
}

.ls-card-head {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 7px 11px;
  border-bottom: 1px solid rgba(255,255,255,0.04);
  font-size: 9px;
  letter-spacing: var(--ls-tk-loose);
  text-transform: uppercase;
}
.ls-card-kind {
  font-family: var(--ls-font-display);
  font-weight: 700;
  color: var(--kind-color, var(--ls-text-dim));
  font-size: 9px;
}
.ls-card-title {
  font-family: var(--ls-font-code);
  font-weight: 500;
  color: var(--ls-text-bright);
  text-transform: none;
  letter-spacing: var(--ls-tk-tight);
  font-size: 11px;
  flex: 1;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.ls-card-ctrls { display: flex; gap: 4px; }
.ls-card-ctrl {
  background: transparent; border: 0;
  color: var(--ls-text-ghost);
  font-size: 11px; cursor: pointer; padding: 0 3px;
  transition: color var(--ls-fast);
}
.ls-card-ctrl:hover { color: var(--ls-text-bright); }

.ls-card-body { flex: 1; overflow: hidden; padding: 10px 11px; font-size: 10px; }

.ls-card-foot {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 5px 11px;
  border-top: 1px solid rgba(255,255,255,0.04);
  font-size: 8px;
  letter-spacing: var(--ls-tk-mid);
  text-transform: uppercase;
  color: var(--ls-text-ghost);
}
.ls-prov-trace { margin-left: auto; }
</style>
