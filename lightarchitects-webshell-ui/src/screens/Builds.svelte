<script lang="ts">
  // Canonical Builds screen. Previously BuildQueue.svelte — kept as an alias
  // for backward compatibility. This wrapper lets app.svelte import 'Builds'
  // while preserving the existing component until a dedicated rewrite lands.
  import BuildQueue from './BuildQueue.svelte';
</script>

<BuildQueue />
