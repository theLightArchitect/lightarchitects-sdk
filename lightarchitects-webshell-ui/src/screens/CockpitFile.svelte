<script lang="ts">
  import { scope } from '$lib/cockpit/stores/scope';
  import type { RouteScope } from '$lib/cockpit/stores/scope';
  import CockpitShell from '$lib/cockpit/shell/CockpitShell.svelte';

  // d3 — /cockpit/file/:codename/:filePath*
  // Phase 4 Wave B will mount the file-viewer layout here.
  const currentScope = $derived($scope as Extract<RouteScope, { kind: 'file' }> | null);
</script>

<CockpitShell>
  <div class="cockpit-stub" data-scope="file">
    <p class="stub-label">{currentScope?.file_path ?? '…'}</p>
    <p class="stub-note">Build: {currentScope?.codename ?? '…'} · Phase 4 Wave B</p>
  </div>
</CockpitShell>

<style>
  .cockpit-stub {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    height: 100%;
    gap: 0.5rem;
    color: var(--text-muted, #888);
    font-family: var(--font-mono, monospace);
    text-align: center;
    padding: 1rem;
  }
  .stub-label { font-size: 0.9rem; word-break: break-all; }
  .stub-note  { font-size: 0.75rem; opacity: 0.6; }
</style>
