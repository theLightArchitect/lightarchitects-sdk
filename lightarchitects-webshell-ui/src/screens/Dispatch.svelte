<script lang="ts">
  import SquadDispatch from './SquadDispatch.svelte';

  // runId is set when navigating to /dispatch/run/:runId (orphan run context).
  // Passed through to SquadDispatch for pre-loading a prior run from history.
  let { runId }: { runId?: string } = $props();
</script>

<SquadDispatch {runId} />
