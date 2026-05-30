<script lang="ts">
  import AutonomousBuildsPanel from '$lib/../components/ironclaw/AutonomousBuildsPanel.svelte';
</script>

<div class="ab-screen" data-testid="autonomous-builds-screen">
  <AutonomousBuildsPanel />
</div>

<style>
  .ab-screen {
    display: flex;
    flex-direction: column;
    height: 100%;
    padding: 16px;
    box-sizing: border-box;
  }
</style>
