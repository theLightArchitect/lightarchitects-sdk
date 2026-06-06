<script lang="ts">
  import { scope } from '$lib/cockpit/stores/scope';
  import type { RouteScope } from '$lib/cockpit/stores/scope';
  import CockpitShell from '$lib/cockpit/shell/CockpitShell.svelte';

  // d1 — /cockpit/project/:projectId
  // Phase 4 Wave B will mount the project command-centre layout here.
  const currentScope = $derived($scope as Extract<RouteScope, { kind: 'project' }> | null);
</script>

<CockpitShell>
  <div class="cockpit-stub" data-scope="project">
    <p class="stub-label">Project — {currentScope?.project_id ?? '…'}</p>
    <p class="stub-note">Phase 4 Wave B</p>
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
  }
  .stub-label { font-size: 1rem; }
  .stub-note  { font-size: 0.75rem; opacity: 0.6; }
</style>
