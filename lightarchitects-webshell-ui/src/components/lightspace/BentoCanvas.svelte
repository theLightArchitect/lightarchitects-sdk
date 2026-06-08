<!--
  @component BentoCanvas
  @description 12-column bento-grid host for all Lightspace cards and skeletons.
    Cards are bucketed into 4 tier rows (Glance / Stream / Focus / Lane) matching the
    mockup's two-level layout. Eviction is handled by canvasAddCard in lightspace-stores.ts.

  @contract none — reads canvas store; no SSE events consumed directly
  @reads lightspaceCanvasStore.cards, .skeletons, .expandedCardId, .highlightCardId
  @mutates lightspaceCanvasStore.expandedCardId (card click → expand)
  @api none — all data arrives via canvasAddCard / canvasUpsertCard mutations

  @mockup-ref arch/lightspace-mockup.html → .la-canvas-grid, .la-tier, renderCanvas()
  @mockup-ref KIND_TO_TIER, sizeForCard(), promoteSoloFinalRow()
-->
<script lang="ts">
  import { lightspaceCanvasStore } from '$lib/lightspace-stores';
  import { KIND_DEFAULT_SPAN } from '$lib/lightspace-types';
  import type { BentoCardData, CardKind, CardSpan } from '$lib/lightspace-types';
  import BentoCard from './BentoCard.svelte';
  import SkeletonCard from './SkeletonCard.svelte';

  // ── Tier mapping ────────────────────────────────────────────────────────
  const KIND_TO_TIER: Record<CardKind, 1 | 2 | 3 | 4> = {
    monitor:    1, instrument: 1,
    trace:      2, thinking: 2, toolcall: 2, bash: 2, agentspawn: 2,
    artifact:   3, diff: 3, research: 3, archgallery: 3,
    branchlane: 4,
  };

  const TIER_FLEX: Record<1 | 2 | 3 | 4, string> = {
    1: '1 1 0', 2: '1 1 0', 3: '1.4 1 0', 4: '1.2 1 0',
  };

  // ── Bucketing ────────────────────────────────────────────────────────────
  interface TierBucket { tier: 1 | 2 | 3 | 4; cards: BentoCardData[]; skeletons: { id: string; kind: CardKind; span: CardSpan }[] }

  const tieredCards = $derived.by(() => {
    const canvas = $lightspaceCanvasStore;
    const buckets = new Map<1 | 2 | 3 | 4, TierBucket>();

    for (const c of canvas.cards) {
      const t = KIND_TO_TIER[c.kind] ?? 3;
      if (!buckets.has(t)) buckets.set(t, { tier: t, cards: [], skeletons: [] });
      buckets.get(t)!.cards.push(c);
    }
    for (const s of canvas.skeletons) {
      const t = KIND_TO_TIER[s.kind] ?? 3;
      if (!buckets.has(t)) buckets.set(t, { tier: t, cards: [], skeletons: [] });
      buckets.get(t)!.skeletons.push(s);
    }

    const result: TierBucket[] = [];
    for (const t of [1, 2, 3, 4] as const) {
      const b = buckets.get(t);
      if (b && (b.cards.length > 0 || b.skeletons.length > 0)) result.push(b);
    }
    return result;
  });

  function toggleExpand(id: string) {
    lightspaceCanvasStore.update(s => ({
      ...s,
      expandedCardId: s.expandedCardId === id ? null : id,
    }));
  }

  function cardSpan(c: BentoCardData): CardSpan {
    return c.span ?? KIND_DEFAULT_SPAN[c.kind] ?? 'span-6';
  }
</script>

<div class="ls-canvas-grid">
  {#if tieredCards.length === 0 && $lightspaceCanvasStore.skeletons.length === 0}
    <div class="ls-canvas-empty">
      <span class="ls-canvas-empty-glyph">◇</span>
      <span>workspace empty — cards arrive as the agent reasons</span>
    </div>
  {:else}
    {#each tieredCards as bucket (bucket.tier)}
      <div class="ls-tier" data-tier={bucket.tier} style="flex: {TIER_FLEX[bucket.tier]}">
        {#each bucket.cards as card, i (card.id)}
          {@const expanded = $lightspaceCanvasStore.expandedCardId === card.id}
          {@const shrunk = $lightspaceCanvasStore.expandedCardId !== null && !expanded}
          <BentoCard
            {card}
            span={expanded ? 'span-12' : cardSpan(card)}
            {expanded}
            {shrunk}
            highlighted={$lightspaceCanvasStore.highlightCardId === card.id}
            animDelay={(i % 4) * 70}
            ontoggleexpand={() => toggleExpand(card.id)}
          />
        {/each}
        {#each bucket.skeletons as skel, i (skel.id)}
          <SkeletonCard kind={skel.kind} span={skel.span} animDelay={i * 75} />
        {/each}
      </div>
    {/each}
  {/if}
</div>

<style>
.ls-canvas-grid {
  flex: 1;
  display: flex;
  flex-direction: column;
  gap: 12px;
  padding: 14px;
  overflow: hidden;
  min-height: 0;
}

.ls-tier {
  display: grid;
  grid-template-columns: repeat(12, 1fr);
  grid-auto-rows: minmax(116px, 1fr);
  gap: 12px;
  min-height: 0;
}

.ls-canvas-empty {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  flex: 1;
  gap: 8px;
  color: var(--ls-text-ghost);
  font-family: var(--ls-font-code);
  font-style: italic;
  font-size: 13px;
  text-align: center;
}

.ls-canvas-empty-glyph {
  font-family: var(--ls-font-display);
  font-weight: 800;
  font-size: 28px;
  color: var(--ls-text-mute);
}

/* Column spans */
:global(.ls-span-3)  { grid-column: span 3; }
:global(.ls-span-4)  { grid-column: span 4; }
:global(.ls-span-6)  { grid-column: span 6; }
:global(.ls-span-8)  { grid-column: span 8; }
:global(.ls-span-12) { grid-column: span 12; }
</style>
