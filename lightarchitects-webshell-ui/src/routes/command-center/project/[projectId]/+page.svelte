<script lang="ts">
  import { page } from '$app/state';
  import { scope, scopeFromParams } from '$lib/cockpit/stores/scope';
  import CockpitProject from '../../../../screens/CockpitProject.svelte';

  // d1 scope — updates reactively when projectId param changes.
  $effect(() => { scope.set(scopeFromParams('CockpitProject', page.params)); });
</script>

<CockpitProject />
