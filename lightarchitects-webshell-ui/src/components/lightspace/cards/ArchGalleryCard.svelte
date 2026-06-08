<!--
  @component ArchGalleryCard
  @description Architecture diagram thumbnail grid.
  @contract _audit pending_ — no EventType yet; renders demo-only static data.
    Wire production events when gateway emits architecture diagram events.
  @reads none — demo data via props
  @mutates none
  @api none (_audit pending_)
-->
<script lang="ts">
  let { data }: { data: unknown } = $props();
  const d = $derived(data as { diagrams?: { name: string; type: string }[] } ?? {});
</script>
<div class="ls-arch-gallery">
  {#each d.diagrams ?? [] as diag}
    <div class="ls-arch-thumb">
      <span class="ls-arch-type">{diag.type}</span>
      <span class="ls-arch-name">{diag.name}</span>
    </div>
  {/each}
  {#if !d.diagrams?.length}
    <span class="ls-arch-empty">awaiting diagrams…</span>
  {/if}
</div>
<style>
.ls-arch-gallery { display: flex; flex-wrap: wrap; gap: 8px; }
.ls-arch-thumb { display: flex; flex-direction: column; gap: 3px; padding: 8px 10px; border: 1px solid var(--ls-border); background: var(--ls-sunken); min-width: 80px; }
.ls-arch-type { font-size: 7px; text-transform: uppercase; letter-spacing: var(--ls-tk-loose); color: var(--ls-acc-purple); }
.ls-arch-name { font-size: 9px; color: var(--ls-text-base); }
.ls-arch-empty { font-style: italic; color: var(--ls-text-ghost); font-size: 10px; }
</style>
