<script lang="ts">
  import { scope } from '$lib/cockpit/stores/scope';
  import type { RouteScope } from '$lib/cockpit/stores/scope';

  // d2 — /cockpit/build/:codename
  // Phase 4 Wave B will mount the build-portal layout here.
  const currentScope = $derived($scope as Extract<RouteScope, { kind: 'build' }> | null);
</script>

<div class="cockpit-stub" data-scope="build">
  <p class="stub-label">Build — {currentScope?.codename ?? '…'}</p>
  <p class="stub-note">Phase 4 Wave B</p>
</div>

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
