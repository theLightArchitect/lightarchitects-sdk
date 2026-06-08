<script lang="ts">
  import { page } from '$app/state';
  import { scope, scopeFromParams } from '$lib/cockpit/stores/scope';
  import CockpitBuild from '../../../../screens/CockpitBuild.svelte';

  // d2 scope — updates reactively when codename param changes.
  $effect(() => { scope.set(scopeFromParams('CockpitBuild', page.params)); });
</script>

<CockpitBuild />
