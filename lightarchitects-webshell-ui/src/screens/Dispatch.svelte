<script lang="ts">
  import { page } from '$app/state';
  import SquadDispatch from './SquadDispatch.svelte';
  import HitlQueue from '$lib/../components/HitlQueue.svelte';

  // runId comes from the SvelteKit route param /dispatch/run/[runId].
  const runId = $derived(page.params.runId);

  // ?tab= query param (e.g. /dispatch?tab=approval).
  const tab = $derived(page.url.searchParams.get('tab'));
</script>

{#if tab === 'approval'}
  <HitlQueue />
{:else}
  <SquadDispatch {runId} />
{/if}
