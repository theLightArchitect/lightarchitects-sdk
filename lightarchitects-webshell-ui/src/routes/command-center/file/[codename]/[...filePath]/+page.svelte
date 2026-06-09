<script lang="ts">
  import { page } from '$app/state';
  import { scope, scopeFromParams } from '$lib/cockpit/stores/scope';
  import CockpitFile from '../../../../../screens/CockpitFile.svelte';

  // d3 scope — filePath is a rest param ([...filePath]) providing the full path string.
  $effect(() => { scope.set(scopeFromParams('CockpitFile', page.params)); });
</script>

<CockpitFile />
