<script lang="ts">
  import { onMount } from 'svelte';
  import { goto } from '$app/navigation';

  // Wave A AD-05: Cockpit.svelte is now a redirect stub.
  // All content lives in CockpitPlatform.svelte (verbatim copy).
  // This stub exists for any legacy deep-links; the canonical URL is /cockpit/platform.
  onMount(() => goto('/command-center/platform'));
</script>
