<script lang="ts">
  // Bento card canvas — right main panel.
  // Renders cards bucketed into 4 tier rows with 12-column grid per tier.
  import { ls } from '$lib/lightspace/state.svelte';
  import {
    KIND_TO_TIER, KIND_SPAN, KIND_COLOR, KIND_DISPLAY, PRESET_KINDS,
    type Card, type ViewPreset,
  } from '$lib/lightspace/types';

  const PRESETS: ViewPreset[] = ['all', 'agents', 'research', 'diffs'];

  const visibleCards = $derived.by(() => {
    const filter = PRESET_KINDS[ls.viewPreset];
    return filter ? ls.cards.filter(c => filter.has(c.kind)) : ls.cards;
  });

  interface TierGroup { tier: number; cards: Card[] }

  const tieredCards = $derived.by(() => {
    const buckets = new Map<number, Card[]>();
    for (const c of visibleCards) {
      const t = c.tier ?? KIND_TO_TIER[c.kind] ?? 3;
      if (!buckets.has(t)) buckets.set(t, []);
      buckets.get(t)!.push(c);
    }
    const result: TierGroup[] = [];
    for (const t of [1, 2, 3, 4]) {
      const cs = buckets.get(t);
      if (cs?.length) result.push({ tier: t, cards: cs });
    }
    return result;
  });

  function cardSpan(c: Card): string {
    return c.span ?? KIND_SPAN[c.kind] ?? 'span-6';
  }

  function kindColor(c: Card): string {
    return KIND_COLOR[c.kind] ?? 'var(--la-hair-strong)';
  }

  function kindLabel(c: Card): string {
    return KIND_DISPLAY[c.kind] ?? c.kind.toUpperCase();
  }

  function toggleExpand(id: string) {
    ls.expandedCardId = ls.expandedCardId === id ? null : id;
  }
</script>

<main class="la-canvas">

  <!-- ── Canvas header ──────────────────────────────────────────────── -->
  <div class="la-canvas-head">
    <span class="intent">
      {#if ls.intentVerb}
        <span class="verb">{ls.intentVerb}</span>
      {/if}
      {ls.intentText}
    </span>
    {#if ls.isShipped}
      <span class="la-shipped-badge">Wave 2a Shipped</span>
    {/if}
    <span class="intent-class">
      intent · <b>{ls.intentClass}</b>
      {#if ls.cards.length} · {ls.cards.length} cards{/if}
      {#if ls.files.length} · {ls.files.length} files{/if}
    </span>
  </div>

  <!-- ── View preset bar ────────────────────────────────────────────── -->
  <div class="la-preset-bar">
    <span class="pb-label">view</span>
    {#each PRESETS as preset}
      <button
        class="la-preset-btn"
        class:active={ls.viewPreset === preset}
        onclick={() => ls.viewPreset = preset}
      >
        {preset.charAt(0).toUpperCase() + preset.slice(1)}
      </button>
    {/each}
  </div>

  <!-- ── Bento grid ─────────────────────────────────────────────────── -->
  <div class="la-canvas-grid">
    {#if visibleCards.length === 0}
      <div class="la-empty-canvas">
        <div class="glyph">◇</div>
        <div>workspace empty — cards arrive as the agent reasons</div>
      </div>
    {:else}
      {#each tieredCards as bucket (bucket.tier)}
        <div class="la-tier" data-tier={bucket.tier}>
          {#each bucket.cards as card, i (card.id)}
            {@const expanded = ls.expandedCardId === card.id}
            {@const shrunk   = ls.expandedCardId !== null && !expanded}
            <section
              class="la-card kind-{card.kind} {expanded ? 'span-12' : cardSpan(card)}"
              class:is-expanded={expanded}
              class:is-shrunk={shrunk}
              class:is-highlighted={ls.highlightCardId === card.id}
              data-id={card.id}
              data-tier={bucket.tier}
              style="--kind-color:{kindColor(card)};animation-delay:{(i % 4) * 70}ms"
              onclick={() => toggleExpand(card.id)}
            >
              <span class="la-card-tier-dot" title="Tier {bucket.tier}"></span>
              <header class="la-card-head">
                <span class="la-card-kind">{kindLabel(card)}</span>
                <span class="la-card-title">{card.title}</span>
                <div class="la-card-ctrls" onclick={(e) => e.stopPropagation()}>
                  <button class="la-card-ctrl" title="minimize">_</button>
                  <button class="la-card-ctrl" title="graduate">↳</button>
                  <button class="la-card-ctrl" title="remove" onclick={() => ls.removeCard(card.id)}>×</button>
                </div>
              </header>
              <div class="la-card-body">{@html card.body}</div>
              <footer class="la-card-foot">
                {#if card.prov}
                  <span class="la-prov {card.prov.agent}">
                    <span class="ag">{card.prov.agent.toUpperCase()}</span>
                    {#if card.prov.src}<span class="la-prov-src">{card.prov.src}</span>{/if}
                  </span>
                {/if}
                {#if card.conf}
                  <span class="la-conf" class:verified={card.conf.tier === 'VERIFIED'}>
                    <span class="lab">conf</span>
                    <span class="val">{card.conf.value.toFixed(2)}</span>
                    <span class="tier-lbl">{card.conf.tier}</span>
                  </span>
                {/if}
                {#if card.contradicts}
                  <span class="la-contradicts">1 contradicts</span>
                {/if}
                <span class="la-prov-trace">⊕ trace {card.prov?.spanId ?? ''}</span>
              </footer>
            </section>
          {/each}
        </div>
      {/each}
    {/if}
  </div>

</main>

<style>
.la-canvas {
  grid-area: canvas; position: relative;
  display: flex; flex-direction: column;
  background:
    radial-gradient(circle, rgba(255,255,255,0.028) 1px, transparent 1px),
    var(--la-bg-base);
  background-size: 28px 28px, 100% 100%;
  transition: opacity var(--la-slow); overflow: hidden;
}

/* Canvas header */
.la-canvas-head {
  display: flex; align-items: center; gap: 10px;
  padding: 7px 14px; border-bottom: 1px solid var(--la-hair-faint);
  font-size: 9px; text-transform: uppercase;
  letter-spacing: var(--la-tk-loose); color: var(--la-text-mute);
}
.la-canvas-head .intent {
  color: var(--la-text-bright); font-family: var(--la-font-mono);
  font-weight: 500; text-transform: none;
  letter-spacing: var(--la-tk-tight); font-size: 11px;
  flex: 1; min-width: 0; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
}
.la-canvas-head .intent .verb {
  color: var(--la-acc); background: rgba(77,142,255,0.08);
  padding: 1px 6px; margin-right: 4px; border-radius: 2px; font-weight: 600;
}
.la-canvas-head .intent-class { color: var(--la-text-dim); font-size: 9px; white-space: nowrap; }
.la-canvas-head .intent-class b { color: var(--la-acc2); }

.la-shipped-badge {
  display: inline-flex; align-items: center; gap: 7px;
  padding: 3px 10px;
  background: rgba(57,255,138,0.10); border: 1px solid var(--la-ok);
  border-radius: 2px; color: var(--la-ok);
  font-family: var(--la-font-display); font-weight: 800;
  font-size: 10px; letter-spacing: var(--la-tk-loose); text-transform: uppercase;
  animation: ship-in 0.55s ease both;
}
@keyframes ship-in { from { opacity: 0; transform: scale(0.85); } to { opacity: 1; transform: scale(1); } }

/* Preset bar */
.la-preset-bar {
  display: flex; align-items: center; gap: 6px;
  padding: 6px 14px; border-bottom: 1px solid var(--la-hair-faint);
  font-size: 9px; letter-spacing: var(--la-tk-mid); text-transform: uppercase;
}
.pb-label { color: var(--la-text-ghost); }
.la-preset-btn {
  background: transparent; border: 1px solid var(--la-hair-base);
  color: var(--la-text-dim); font-family: var(--la-font-mono);
  font-size: 9px; letter-spacing: var(--la-tk-mid); text-transform: uppercase;
  padding: 3px 8px; cursor: pointer; border-radius: 2px; transition: all var(--la-fast);
}
.la-preset-btn:hover { color: var(--la-text-bright); border-color: var(--la-acc); }
.la-preset-btn.active { color: var(--la-bg-base); background: var(--la-acc); border-color: var(--la-acc); }

/* Bento grid */
.la-canvas-grid {
  flex: 1; display: flex; flex-direction: column;
  gap: 12px; padding: 14px; overflow: hidden; min-height: 0;
}
.la-tier {
  display: grid; grid-template-columns: repeat(12, 1fr);
  grid-auto-rows: minmax(116px, 1fr); gap: 12px; min-height: 0;
}
.la-tier[data-tier="1"] { flex: 1 1 0; }
.la-tier[data-tier="2"] { flex: 1 1 0; }
.la-tier[data-tier="3"] { flex: 1.4 1 0; }
.la-tier[data-tier="4"] { flex: 1.2 1 0; }

.la-empty-canvas {
  display: flex; flex-direction: column;
  align-items: center; justify-content: center; flex: 1;
  color: var(--la-text-ghost);
  font-family: var(--la-font-serif); font-style: italic;
  font-size: 13px; text-align: center;
}
.la-empty-canvas .glyph {
  font-family: var(--la-font-display); font-weight: 800;
  font-size: 28px; letter-spacing: var(--la-tk-loose);
  color: var(--la-text-mute); margin-bottom: 8px;
}

/* Column spans */
.span-3  { grid-column: span 3; }
.span-4  { grid-column: span 4; }
.span-6  { grid-column: span 6; }
.span-8  { grid-column: span 8; }
.span-12 { grid-column: span 12; }

/* Cards */
.la-card {
  background: var(--la-bg-card);
  border: 1px solid var(--la-hair-base);
  border-left: 3px solid var(--kind-color, var(--la-hair-strong));
  position: relative; display: flex; flex-direction: column;
  min-height: 0; overflow: hidden; cursor: pointer;
  transition: border-color var(--la-fast), opacity var(--la-mid), transform var(--la-mid);
  animation: la-cardin var(--la-mid) both;
}
@keyframes la-cardin { from { opacity: 0; transform: translateY(8px); } to { opacity: 1; transform: none; } }
.la-card:hover { border-color: var(--la-hair-accent); }
.la-card.is-expanded {
  border-color: var(--la-acc) !important;
  box-shadow: 0 0 0 1.5px var(--la-acc), 0 12px 40px rgba(0,0,0,0.45);
  z-index: 10; position: relative;
}
.la-card.is-shrunk { opacity: 0.36; transform: scale(0.97); }
.la-card.is-highlighted {
  border-color: var(--la-acc) !important;
  box-shadow: 0 0 0 1px var(--la-acc), 0 0 24px rgba(77,142,255,0.28);
}

/* Tier dot */
.la-card-tier-dot {
  position: absolute; top: 8px; right: 8px;
  width: 6px; height: 6px; border-radius: 50%;
  background: var(--la-hair-base); opacity: 0.6;
}
.la-card[data-tier="1"] .la-card-tier-dot { background: var(--la-info); }
.la-card[data-tier="2"] .la-card-tier-dot { background: var(--la-ok); }
.la-card[data-tier="3"] .la-card-tier-dot { background: var(--la-acc2); }
.la-card[data-tier="4"] .la-card-tier-dot { background: var(--la-acc3); }

.la-card-head {
  display: flex; align-items: center; gap: 8px;
  padding: 7px 11px; border-bottom: 1px solid var(--la-hair-faint);
  font-size: 9px; letter-spacing: var(--la-tk-loose); text-transform: uppercase;
}
.la-card-kind {
  font-family: var(--la-font-display); font-weight: 700;
  color: var(--kind-color, var(--la-text-dim));
  font-size: 9px; letter-spacing: var(--la-tk-loose);
}
.la-card-title {
  font-family: var(--la-font-mono); font-weight: 500;
  color: var(--la-text-bright); text-transform: none;
  letter-spacing: var(--la-tk-tight); font-size: 11px;
  flex: 1; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
}
.la-card-ctrls { display: flex; gap: 4px; }
.la-card-ctrl {
  background: transparent; border: 0; color: var(--la-text-ghost);
  font-family: var(--la-font-mono); font-size: 11px;
  cursor: pointer; padding: 0 3px; transition: color var(--la-fast);
}
.la-card-ctrl:hover { color: var(--la-text-bright); }

.la-card-body { flex: 1; overflow: hidden; padding: 10px 11px; font-size: 10px; }

.la-card-foot {
  display: flex; align-items: center; gap: 8px;
  padding: 5px 11px; border-top: 1px solid var(--la-hair-faint);
  font-size: 8px; letter-spacing: var(--la-tk-mid); text-transform: uppercase;
  color: var(--la-text-ghost);
}
.la-prov { display: inline-flex; align-items: center; gap: 4px; }
.la-prov .ag { font-family: var(--la-font-display); font-weight: 700; font-size: 8px; letter-spacing: var(--la-tk-mid); }
.la-prov.copilot  .ag { color: var(--la-acc); }
.la-prov.quantum  .ag { color: var(--la-info); }
.la-prov.corso    .ag { color: #ff7a7a; }
.la-prov.eva      .ag { color: #93f0c2; }
.la-prov.soul     .ag { color: #c5a8ff; }
.la-prov-src { color: var(--la-text-ghost); font-size: 8px; }
.la-prov-trace { color: var(--la-text-ghost); margin-left: auto; }

.la-conf {
  display: inline-flex; gap: 3px; align-items: center;
  padding: 1px 5px; background: rgba(255,255,255,0.06);
  border: 1px solid var(--la-hair-base); font-size: 8px;
}
.la-conf .lab { color: var(--la-text-ghost); }
.la-conf .val { font-size: 9px; color: var(--la-text-base); }
.la-conf .tier-lbl { color: var(--la-text-ghost); font-size: 7px; }
.la-conf.verified .val::after { content: " ⛉"; color: var(--la-ok); }
.la-contradicts { color: var(--la-warn); }

/* ── Card body helper classes (used via {@html}) ─────────────────── */
:global(.la-mon-grid)  { display: grid; grid-template-columns: 1fr 1fr; gap: 6px; }
:global(.la-mon-cell)  { display: flex; flex-direction: column; gap: 2px; padding: 5px 7px; background: var(--la-bg-sunken); border: 1px solid var(--la-hair-faint); }
:global(.la-mon-cell .k) { font-size: 7px; text-transform: uppercase; letter-spacing: var(--la-tk-loose); color: var(--la-text-ghost); }
:global(.la-mon-cell .v) { font-size: 11px; color: var(--la-text-bright); }
:global(.la-mon-cell .v.ok) { color: var(--la-ok); }

:global(.la-agent-row) { display: flex; align-items: center; gap: 8px; margin-bottom: 6px; }
:global(.ag-name) { font-family: var(--la-font-display); font-weight: 700; font-size: 10px; letter-spacing: var(--la-tk-loose); }
:global(.ag-name.quantum) { color: var(--la-info); }
:global(.ag-name.corso)   { color: #ff7a7a; }
:global(.ag-name.eva)     { color: #93f0c2; }
:global(.ag-state) { font-size: 9px; color: var(--la-ok); text-transform: uppercase; letter-spacing: var(--la-tk-mid); }
:global(.la-agent-prog) { height: 3px; background: var(--la-bg-sunken); border-radius: 2px; overflow: hidden; margin-bottom: 6px; }
:global(.la-agent-prog .fill) { height: 100%; background: linear-gradient(90deg, var(--la-acc), var(--la-ok)); transition: width 0.6s ease; }
:global(.ag-task) { font-size: 9px; color: var(--la-text-dim); line-height: 1.4; }

:global(.la-cite-row) { display: flex; align-items: center; gap: 8px; margin-bottom: 6px; }
:global(.la-cite-row .src) { font-family: var(--la-font-mono); font-size: 9px; color: var(--la-acc); }
:global(.la-cite-row .conf.ok) { font-size: 8px; text-transform: uppercase; color: var(--la-ok); }
:global(.la-cite-body) { font-size: 10px; color: var(--la-text-base); line-height: 1.5; }

:global(.la-trace-list) { display: flex; flex-direction: column; gap: 5px; }
:global(.la-trace-row) { display: flex; gap: 8px; font-size: 9px; }
:global(.la-trace-row .k) { font-family: var(--la-font-display); font-weight: 700; font-size: 8px; letter-spacing: var(--la-tk-loose); color: var(--la-text-mute); text-transform: uppercase; min-width: 48px; }
:global(.la-trace-row .v) { color: var(--la-text-base); line-height: 1.4; }

:global(.la-doc-preview) { display: flex; flex-direction: column; gap: 4px; }
:global(.la-doc-field) { display: flex; gap: 8px; font-size: 9px; }
:global(.la-doc-field .k) { color: var(--la-text-ghost); text-transform: uppercase; font-size: 8px; letter-spacing: var(--la-tk-mid); min-width: 64px; }
:global(.la-doc-field .v) { color: var(--la-text-base); }
:global(.la-doc-field .v.acc) { color: var(--la-acc2); }
:global(.la-doc-field .v.dim) { color: var(--la-text-dim); }
</style>
