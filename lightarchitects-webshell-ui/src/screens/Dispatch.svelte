<script lang="ts">
  import SquadDispatch from './SquadDispatch.svelte';
  import HitlQueue from '$lib/../components/HitlQueue.svelte';

  // runId is set when navigating to /dispatch/run/:runId (orphan run context).
  // Passed through to SquadDispatch for pre-loading a prior run from history.
  let { runId }: { runId?: string } = $props();

  const tab = $derived(new URLSearchParams(location.hash.split('?')[1] ?? '').get('tab'));
</script>

{#if tab === 'approval'}
  <HitlQueue />
{:else}
  <SquadDispatch {runId} />
{/if}
