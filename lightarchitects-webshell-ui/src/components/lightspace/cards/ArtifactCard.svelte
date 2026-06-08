<!--
  @component ArtifactCard
  @description Shipped file card — graduated artifact preview.
  @contract EventType 'impl_complete' → ImplCompleteEvent
  @reads implCompleteEvents (existing store)
  @mutates lightspaceFilesStore (via graduate event → file added to drawer)
  @api GET /api/builds/:id/decisions (for artifact metadata)
-->
<script lang="ts">
  let { data }: { data: unknown } = $props();
  const d = $derived(data as { name?: string; mime?: string; size?: string; prov?: string } ?? {});
</script>
<div class="ls-artifact">
  <div class="ls-artifact-header">
    <span class="ls-artifact-mime">{(d.mime ?? 'file').toUpperCase()}</span>
    <span class="ls-artifact-name">{d.name ?? 'artifact'}</span>
  </div>
  {#if d.size}
    <span class="ls-artifact-meta">{d.size}</span>
  {/if}
  {#if d.prov}
    <span class="ls-artifact-prov">{d.prov}</span>
  {/if}
</div>
<style>
.ls-artifact { display: flex; flex-direction: column; gap: 5px; }
.ls-artifact-header { display: flex; align-items: center; gap: 8px; }
.ls-artifact-mime { font-family: var(--ls-font-display); font-weight: 700; font-size: 7px; letter-spacing: var(--ls-tk-loose); color: var(--ls-acc-green); padding: 2px 4px; border: 1px solid rgba(62,207,142,0.3); }
.ls-artifact-name { font-size: 11px; color: var(--ls-text-bright); font-weight: 500; word-break: break-all; }
.ls-artifact-meta, .ls-artifact-prov { font-size: 9px; color: var(--ls-text-mute); }
</style>
