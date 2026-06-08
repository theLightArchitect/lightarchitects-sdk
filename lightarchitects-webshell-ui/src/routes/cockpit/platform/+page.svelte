<script lang="ts">
  import { scope, scopeFromParams } from '$lib/cockpit/stores/scope';
  import CockpitPlatform from '../../../screens/CockpitPlatform.svelte';

  // d0 scope — no route params needed.
  scope.set(scopeFromParams('CockpitPlatform', {}));
</script>

<CockpitPlatform />
