<script lang="ts">
  import { writable } from 'svelte/store';
  import { onMount } from 'svelte';
  import Helix3D from './components/Helix3D.svelte';
  import StatusBar from './components/StatusBar.svelte';
  import CommandPalette from './components/CommandPalette.svelte';
  import SetupFlow from './screens/setup/SetupFlow.svelte';
  import CopilotDrawer from './components/CopilotDrawer.svelte';
  import MemoryDrawer from './components/MemoryDrawer.svelte';
  import AmbientParticles from './components/AmbientParticles.svelte';
  import HelixTooltip from './components/HelixTooltip.svelte';
  import HelixDetailPanel from './components/HelixDetailPanel.svelte';
  import ScrumReport from './components/ScrumReport.svelte';
  import {
    ayinStatus, startWaveTick, stopWaveTick, initializeStores, drawerHeightPx, memoryDrawerOpen,
    builds, currentBuildId, findings, logEntries, artifacts, conductorTasks, arenaStatus, alerts,
    activePlan, latestScrumReport, hotMemory, coldMemory, activeHelixNode, selectedPillar,
    expandedFindings, supervisorAlerts, siblingHealth, copilotMessages,
  } from '$lib/stores';
  import { setupComplete, step, loadSetupInfo, selectedBackend, selectedModel, selectedAgent } from '$lib/setup';
  import { connectGlobalSSE, disconnectGlobalSSE } from '$lib/sse';
  import { saveSettingsDebounced } from '$lib/settings-persistence';

  // Track persisted stores — save on any change after initial load.
  // Uses store.subscribe() instead of $effect to avoid Svelte 5's reactive
  // signal graph entirely. $effect + store reads creates a hub node that
  // triggers effect_update_depth_exceeded when any other effect writes to
  // these stores during the same rendering cycle.
  let settingsUnsubs: (() => void)[] = [];

  // Route store — simple hash-based routing for SPA
  export const currentRoute = writable<string>(window.location.hash.slice(1) || '/');

  // Lazy-loaded screens (code-split per route)
  const screenModules = {
    Activity:      () => import('./screens/Activity.svelte'),
    BuildQueue:    () => import('./screens/BuildQueue.svelte'),
    Workspace:     () => import('./screens/Workspace.svelte'),
    Intake:        () => import('./screens/Intake.svelte'),
    Sitrep:        () => import('./screens/Sitrep.svelte'),
    ProjectDetail: () => import('./screens/ProjectDetail.svelte'),
  };

  type ScreenModule = { default: any };

  let ActiveScreen = $state<any>(null);
  let screenLoading = $state(true);

  function resolveScreenKey(path: string): keyof typeof screenModules {
    if (path === '/activity') return 'Activity';
    if (path.startsWith('/workspace')) return 'Workspace';
    if (path === '/intake') return 'Intake';
    if (path === '/sitrep') return 'Sitrep';
    if (path.startsWith('/project/')) return 'ProjectDetail';
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

  // Derived condition for setup gate — explicit dependency tracking in Svelte 5
  const setupDone = $derived($setupComplete && $step === 'done');

  const NAV_ITEMS = [
    { label: 'Activity', hash: '/activity' },
    { label: 'Queue',    hash: '/'         },
    { label: 'Intake',   hash: '/intake'   },
    { label: 'Sitrep',   hash: '/sitrep'   },
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
    // E2E hook — lets Playwright bypass setup flow by setting stores directly.
    // Guarded by DEV so it's tree-shaken in production builds (CORSO sec review).
    if (import.meta.env.DEV) {
      (window as any).__e2e = {
        setupComplete, step,
        // Stores for E2E data injection (Workspace, ScrumReport, Helix, etc.)
        builds, currentBuildId, findings, logEntries, artifacts,
        conductorTasks, arenaStatus, alerts, activePlan,
        latestScrumReport, hotMemory, coldMemory, activeHelixNode,
        selectedPillar, expandedFindings, supervisorAlerts,
        siblingHealth, copilotMessages,
      };
    }
    startWaveTick();
    ayinStatus.set('reconnecting');
    loadScreen(window.location.hash.slice(1) || '/');
    const initializeStoresPromise = initializeStores(); // non-blocking; errors caught internally
    connectGlobalSSE(); // Phase 10.9 — global helix_entry / soul_promotion / strand_activation stream
    window.addEventListener('hashchange', handleHashChange);

    // Subscribe to persisted stores. .subscribe() fires synchronously with
    // the current value — the `initialized` flag ensures we skip those AND
    // skip the writes from initializeStores() → loadPersistedSettings() →
    // applySettings(), which would otherwise trigger a redundant POST of
    // the just-loaded settings back to the server.
    let initialized = false;
    const trigger = () => { if (initialized) saveSettingsDebounced(); };
    settingsUnsubs = [
      drawerHeightPx.subscribe(trigger),
      memoryDrawerOpen.subscribe(trigger),
      selectedBackend.subscribe(trigger),
      selectedModel.subscribe(trigger),
      selectedAgent.subscribe(trigger),
    ];
    // Only enable persistence after initializeStores finishes loading
    // persisted settings — prevents redundant write-back on startup.
    initializeStoresPromise.then(() => { initialized = true; });

    return () => {
      stopWaveTick();
      disconnectGlobalSSE();
      settingsUnsubs.forEach(fn => fn());
      window.removeEventListener('hashchange', handleHashChange);
    };
  });
</script>

{#if !setupDone}
  <SetupFlow />
{:else}
<div class="w-screen h-screen overflow-hidden bg-[#0a0a0f] text-[#e2e8f0] font-['JetBrains_Mono',monospace]">
  <div class="flex" style="height: calc(100vh - {$drawerHeightPx}px);">
    <!-- Left: Main content area -->
    <div class="flex-1 flex flex-col overflow-hidden relative">
      <!-- Ambient particles — drifting helix-palette dots behind content -->
      <AmbientParticles />
      <!-- Top navigation strip -->
      <nav class="flex items-center gap-1 px-3 py-1.5 border-b border-[#1e293b] bg-[#0a0a0f] shrink-0 overflow-x-auto">
        {#each NAV_ITEMS as item}
          <button
            onclick={() => navigate(item.hash)}
            class="shrink-0 px-3 py-1 text-[11px] rounded transition-all {isActive(item.hash) ? 'bg-[#FFD700]/15 text-[#FFD700] shadow-[0_0_8px_rgba(255,215,0,0.2)] border border-[#FFD700]/30' : 'text-[#475569] hover:text-[#FFD700] border border-transparent'}"
          >{item.label}</button>
        {/each}
        <div class="ml-auto shrink-0 flex items-center gap-2">
          <button
            onclick={() => memoryDrawerOpen.update(v => !v)}
            class="px-2 py-1 text-[11px] text-[#475569] hover:text-[#FFD700] transition-colors"
            title="Memory drawer (Cmd+M)"
            data-testid="memory-toggle"
          >{$memoryDrawerOpen ? 'Close Memory' : 'Memory'}</button>
          <div class="hidden lg:flex items-center gap-2">
            <button
              onclick={() => { showHelix = !showHelix; }}
              class="px-2 py-1 text-[11px] text-[#475569] hover:text-[#FFD700] transition-colors"
            >{showHelix ? 'Hide 3D' : 'Show 3D'}</button>
          </div>
        </div>
      </nav>

      {#if screenLoading}
        <div class="flex-1 flex items-center justify-center">
          <div class="flex items-center gap-3">
            <div class="w-4 h-4 border-2 border-[#FFD700] border-t-transparent rounded-full animate-spin shadow-[0_0_6px_rgba(255,215,0,0.4)]"></div>
            <span class="text-xs text-[#64748b]">Loading...</span>
          </div>
        </div>
      {:else if ActiveScreen}
        {#key ActiveScreen}
          <svelte:component this={ActiveScreen} />
        {/key}
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
  <HelixTooltip />
  <HelixDetailPanel />
  <ScrumReport />
</div>
{/if}