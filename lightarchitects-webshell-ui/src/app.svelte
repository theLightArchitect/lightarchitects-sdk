<script lang="ts">
  import { writable } from 'svelte/store';
  import { onMount } from 'svelte';
  import Helix3D from './components/Helix3D.svelte';
  import StatusBar from './components/StatusBar.svelte';
  import CommandPalette from './components/CommandPalette.svelte';
  import SetupFlow from './screens/setup/SetupFlow.svelte';
  import CopilotDrawer from './components/CopilotDrawer.svelte';
  import MemoryDrawer from './components/MemoryDrawer.svelte';
  import { ayinStatus, startWaveTick, stopWaveTick, initializeStores, drawerHeightPx, memoryDrawerOpen } from '$lib/stores';
  import { setupComplete, step, loadSetupInfo } from '$lib/setup';
  import { connectGlobalSSE, disconnectGlobalSSE } from '$lib/sse';

  // Route store — simple hash-based routing for SPA
  export const currentRoute = writable<string>(window.location.hash.slice(1) || '/');

  // Lazy-loaded screens (code-split per route)
  const screenModules = {
    BuildQueue: () => import('./screens/BuildQueue.svelte'),
    Workspace:  () => import('./screens/Workspace.svelte'),
    Intake:     () => import('./screens/Intake.svelte'),
    Sitrep:     () => import('./screens/Sitrep.svelte'),
  };

  type ScreenModule = { default: any };

  let ActiveScreen = $state<any>(null);
  let screenLoading = $state(true);

  function resolveScreenKey(path: string): keyof typeof screenModules {
    if (path.startsWith('/workspace')) return 'Workspace';
    if (path === '/intake') return 'Intake';
    if (path === '/sitrep') return 'Sitrep';
    return 'BuildQueue';
  }

  async function loadScreen(path: string) {
    screenLoading = true;
    const key = resolveScreenKey(path);
    try {
      const mod: ScreenModule = await screenModules[key]();
      ActiveScreen = mod.default;
    } catch (err) {
      console.error('Failed to load screen:', key, err);
      // Fallback: try direct import
      try {
        const mod: ScreenModule = await screenModules['BuildQueue']();
        ActiveScreen = mod.default;
      } catch {
        ActiveScreen = null;
      }
    } finally {
      screenLoading = false;
    }
  }

  let showHelix = $state(true);

  const NAV_ITEMS = [
    { label: 'Queue',  hash: '/'       },
    { label: 'Intake', hash: '/intake' },
    { label: 'Sitrep', hash: '/sitrep' },
  ];

  function navigate(hash: string) {
    window.location.hash = hash;
  }

  let activeRoute = $derived($currentRoute);

  function isActive(hash: string): boolean {
    if (hash === '/') return activeRoute === '/' || activeRoute === '';
    return activeRoute.startsWith(hash);
  }

  function handleHashChange() {
    const path = window.location.hash.slice(1) || '/';
    currentRoute.set(path);
    loadScreen(path);
  }

  onMount(() => {
    loadSetupInfo(); // check setup state before anything else
    startWaveTick();
    ayinStatus.set('reconnecting');
    loadScreen(window.location.hash.slice(1) || '/');
    initializeStores(); // non-blocking; errors caught internally
    connectGlobalSSE(); // Phase 10.9 — global helix_entry / soul_promotion / strand_activation stream

    return () => {
      stopWaveTick();
      disconnectGlobalSSE();
    };
  });

  window.addEventListener('hashchange', handleHashChange);
</script>

{#if !$setupComplete || $step !== 'done'}
  <SetupFlow />
{/if}

<div class="w-screen h-screen overflow-hidden bg-[#0a0a0f] text-[#e2e8f0] font-['JetBrains_Mono',monospace]" class:hidden={!$setupComplete || $step !== 'done'}>
  <div class="flex" style="height: calc(100vh - {$drawerHeightPx}px);">
    <!-- Left: Main content area -->
    <div class="flex-1 flex flex-col overflow-hidden">
      <!-- Top navigation strip -->
      <nav class="flex items-center gap-1 px-3 py-1.5 border-b border-[#1e293b] bg-[#0a0a0f] shrink-0 overflow-x-auto">
        {#each NAV_ITEMS as item}
          <button
            onclick={() => navigate(item.hash)}
            class="shrink-0 px-3 py-1 text-[11px] rounded transition-colors {isActive(item.hash) ? 'bg-[#1e293b] text-[#e2e8f0]' : 'text-[#475569] hover:text-[#94a3b8]'}"
          >{item.label}</button>
        {/each}
        <div class="ml-auto shrink-0 flex items-center gap-2">
          <button
            onclick={() => memoryDrawerOpen.update(v => !v)}
            class="px-2 py-1 text-[11px] text-[#475569] hover:text-[#94a3b8] transition-colors"
            title="Memory drawer (Cmd+M)"
            data-testid="memory-toggle"
          >{$memoryDrawerOpen ? 'Close Memory' : 'Memory'}</button>
          <div class="hidden lg:flex items-center gap-2">
            <button
              onclick={() => { showHelix = !showHelix; }}
              class="px-2 py-1 text-[11px] text-[#475569] hover:text-[#94a3b8] transition-colors"
            >{showHelix ? 'Hide 3D' : 'Show 3D'}</button>
          </div>
        </div>
      </nav>

      {#if screenLoading}
        <div class="flex-1 flex items-center justify-center">
          <div class="flex items-center gap-3">
            <div class="w-4 h-4 border-2 border-[#7C3AED] border-t-transparent rounded-full animate-spin"></div>
            <span class="text-xs text-[#64748b]">Loading...</span>
          </div>
        </div>
      {:else if ActiveScreen}
        <ActiveScreen />
      {/if}
    </div>

    <!-- Right: 3D Helix panel — CSS hides below lg (1024px); JS toggle controls at lg+ -->
    {#if showHelix}
      <div class="hidden lg:block lg:w-[35%] xl:w-[40%] min-w-[200px] max-w-[600px] relative border-l border-[#1e293b]">
        <Helix3D />
      </div>
    {/if}
  </div>
  <StatusBar />
  <CommandPalette />
  <CopilotDrawer />
  <MemoryDrawer />
</div>