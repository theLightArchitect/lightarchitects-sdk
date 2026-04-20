<script lang="ts">
  import { step } from '$lib/setup';
  import SplashStep from './SplashStep.svelte';
  import BackendStep from './BackendStep.svelte';
  import AuthStep from './AuthStep.svelte';
  import ModelStep from './ModelStep.svelte';
  import InitStep from './InitStep.svelte';
</script>

<div class="flow">
  {#if $step === 'splash'}
    <SplashStep />
  {:else if $step === 'backend'}
    <BackendStep />
  {:else if $step === 'auth'}
    <AuthStep />
  {:else if $step === 'model'}
    <ModelStep />
  {:else if $step === 'init'}
    <InitStep />
  {/if}
</div>

<style>
  .flow {
    position: fixed; inset: 0; z-index: 1000;
    background: radial-gradient(ellipse at center, #0a0a20 0%, #03030a 60%, #000000 100%);
    display: flex; align-items: center; justify-content: center;
  }
</style>
