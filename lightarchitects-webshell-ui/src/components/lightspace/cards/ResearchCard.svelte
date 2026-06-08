<!--
  @component ResearchCard
  @description Research fragment — context7, arXiv, helix citations.
  @contract _audit pending_ — no EventType yet; renders demo-only static data.
    Wire production events in a follow-on build when gateway emits research events.
  @reads none — demo data via props
  @mutates none
  @api none (_audit pending_ — will need Context7 / helix search)
-->
<script lang="ts">
  let { data }: { data: unknown } = $props();
  const d = $derived(data as { source?: string; excerpt?: string; confidence?: number; tier?: string } ?? {});
</script>
<div class="ls-research">
  {#if d.source}
    <div class="ls-research-src">{d.source}</div>
  {/if}
  <p class="ls-research-excerpt">{d.excerpt ?? 'research fragment'}</p>
  {#if d.confidence !== undefined}
    <div class="ls-research-conf">conf {d.confidence.toFixed(2)} · {d.tier ?? 'UNVERIFIED'}</div>
  {/if}
</div>
<style>
.ls-research { display: flex; flex-direction: column; gap: 6px; }
.ls-research-src { font-family: var(--ls-font-code); font-size: 9px; color: var(--ls-acc); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.ls-research-excerpt { font-size: 10px; color: var(--ls-text-base); line-height: 1.5; margin: 0; }
.ls-research-conf { font-size: 8px; color: var(--ls-text-mute); text-transform: uppercase; letter-spacing: var(--ls-tk-mid); }
</style>
