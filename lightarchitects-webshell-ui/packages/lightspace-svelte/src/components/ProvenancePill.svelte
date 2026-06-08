<!--
  @component ProvenancePill — renders Provenance with source link + optional span trace icon.
-->
<script lang="ts">
  import type { Provenance } from '../types';
  let { prov }: { prov: Provenance } = $props();
</script>
<div class="ls-prov-pill" title="agent: {prov.agent} · source: {prov.source}">
  <span class="ls-prov-agent">{prov.agent}</span>
  {#if prov.span_id}
    <span class="ls-prov-span" aria-label="trace span">[⟳]</span>
  {/if}
</div>
<style>
.ls-prov-pill  { display: inline-flex; gap: 4px; align-items: center; font-size: 8px; color: var(--ls-text-ghost); }
.ls-prov-agent { color: var(--ls-acc); font-family: var(--ls-font-code); }
.ls-prov-span  { color: var(--ls-acc-purple); cursor: default; }
</style>
