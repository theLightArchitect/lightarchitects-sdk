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
  import Tooltip from './components/Tooltip.svelte';
  import AuthBanner from './components/AuthBanner.svelte';
  import {
    ayinStatus, startWaveTick, stopWaveTick, initializeStores, drawerHeightPx, memoryDrawerOpen,
    builds, currentBuildId, findings, logEntries, artifacts, conductorTasks, arenaStatus, alerts,
    activePlan, latestScrumReport, hotMemory, coldMemory, activeHelixNode, selectedPillar,
    expandedFindings, supervisorAlerts, siblingHealth, copilotMessages,
    intakeFormDirty,
  } from '$lib/stores';
  import { get } from 'svelte/store';
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
    SquadDispatch: () => import('./screens/SquadDispatch.svelte'),
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
    if (path === '/squad-dispatch') return 'SquadDispatch';
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

  // Responsive viewport state — drives Helix3D panel layout decisions.
  // At desktop (>=1024) the panel is the right-hand sibling of the main
  // content (current default). Below 1024 the panel is hidden and the
  // toggle button reveals it as a full-screen overlay so the WebGL scene
  // gets readable real estate. At <768 the page itself stacks vertically.
  // BREAKPOINTS tokens come from $lib/design-tokens.
  type ViewportCategory = 'mobile' | 'tablet' | 'desktop';
  let viewport = $state<ViewportCategory>('desktop');
  let showHelix = $state(true);

  function categorizeViewport(width: number): ViewportCategory {
    if (width < 768) return 'mobile';
    if (width < 1024) return 'tablet';
    return 'desktop';
  }

  function syncViewport() {
    const next = categorizeViewport(window.innerWidth);
    if (next === viewport) return;
    viewport = next;
    // Auto-collapse the helix panel when leaving desktop so users on
    // resize-down don't get a forced overlay; auto-restore on entering
    // desktop because that is where the panel "lives" by default.
    showHelix = next === 'desktop';
  }

  // Derived condition for setup gate — explicit dependency tracking in Svelte 5
  const setupDone = $derived($setupComplete && $step === 'done');

  // Tab order optimised for read-before-write (#32): operators land on
  // Activity (live state), then can scan Sitrep (squad health) or Queue
  // (existing builds) before reaching Intake (new build — write action).
  // Squad Dispatch appended last — a power-user action (Cmd+K shortcut).
  const NAV_ITEMS = [
    { label: 'Activity', hash: '/activity',      hint: 'Live trace events from running agents' },
    { label: 'Sitrep',   hash: '/sitrep',         hint: 'Squad health snapshot — agent status, alerts, uptime' },
    { label: 'Queue',    hash: '/',               hint: 'All builds — past, in-flight, and queued' },
    { label: 'Intake',   hash: '/intake',         hint: 'Start a new build (Quick or Plan mode)' },
    { label: 'Squad',    hash: '/squad-dispatch', hint: 'Dispatch agents by domain — Engineer, Security, Researcher, Ops (Cmd+K)' },
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
    // Initialize responsive viewport before first render so layout
    // matches the current window width (avoids a desktop-default flash
    // on small screens). Listener stays for live resize tracking.
    syncViewport();
    window.addEventListener('resize', syncViewport);

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

    // Warn the operator before unload if they have unsaved Intake form data.
    // The draft is also auto-persisted to localStorage (#15), so refresh
    // restores it — this guard catches accidental closes / nav-aways.
    window.addEventListener('beforeunload', beforeUnloadGuard);

    // Cmd/Ctrl+K → Squad Dispatch (global hotkey, C3).
    function handleGlobalKey(e: KeyboardEvent) {
      if ((e.metaKey || e.ctrlKey) && e.key === 'k') {
        e.preventDefault();
        navigate('/squad-dispatch');
      }
    }
    window.addEventListener('keydown', handleGlobalKey);

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
      window.removeEventListener('resize', syncViewport);
      window.removeEventListener('beforeunload', beforeUnloadGuard);
      window.removeEventListener('keydown', handleGlobalKey);
    };
  });

  // Module-scope so the listener identity stays stable across mount/unmount.
  function beforeUnloadGuard(event: BeforeUnloadEvent) {
    if (!get(intakeFormDirty)) return;
    // Browsers ignore custom strings since 2018 (Chrome 51+, Firefox 44+) and
    // show their own dialog. We just need preventDefault + returnValue set.
    event.preventDefault();
    event.returnValue = '';
  }
</script>

{#if !setupDone}
  <SetupFlow />
{:else}
<!-- Auth banner — top-of-screen affordance on 401/403 from SSE (#13). -->
<AuthBanner />
<div class="w-screen h-screen overflow-hidden bg-[#0a0a0f] text-[#e2e8f0] font-['JetBrains_Mono',monospace]">
  <!-- Responsive container:
         <768  : flex-col       (vertical stack — single-column flow)
         >=768 : flex-row       (side-by-side) — at 768..1023 the helix panel
                                still hides; at >=1024 it renders inline. -->
  <div class="flex flex-col md:flex-row" style="height: calc(100vh - {$drawerHeightPx}px);">
    <!-- Left: Main content area -->
    <div class="flex-1 flex flex-col overflow-hidden relative">
      <!-- Ambient particles — drifting helix-palette dots behind content -->
      <AmbientParticles />
      <!-- Top navigation strip -->
      <nav class="flex items-center gap-1 px-3 py-1.5 border-b border-[#1e293b] bg-[#0a0a0f] shrink-0 overflow-x-auto">
        {#each NAV_ITEMS as item}
          <Tooltip content={item.hint} side="bottom">
            <button
              onclick={() => navigate(item.hash)}
              class="shrink-0 px-3 py-1 text-[11px] rounded transition-all {isActive(item.hash) ? 'bg-[#FFD700]/15 text-[#FFD700] shadow-[0_0_8px_rgba(255,215,0,0.2)] border border-[#FFD700]/30' : 'text-[#475569] hover:text-[#FFD700] border border-transparent'}"
            >{item.label}</button>
          </Tooltip>
        {/each}
        <div class="ml-auto shrink-0 flex items-center gap-2">
          <Tooltip content="Hot · Cold · Convergences — what each agent remembers (Cmd+M)" side="bottom">
            <button
              onclick={() => memoryDrawerOpen.update(v => !v)}
              class="px-2 py-1 text-[11px] text-[#475569] hover:text-[#FFD700] transition-colors"
              title="Memory drawer (Cmd+M)"
              data-testid="memory-toggle"
            >{$memoryDrawerOpen ? 'Close Memory' : 'Memory'}</button>
          </Tooltip>
          <!-- 3D View toggle — visible at every viewport.
               Desktop (>=1024): toggles the inline right-hand panel.
               Tablet/mobile  : toggles a full-screen overlay so the WebGL
                                bloom pass gets readable real estate. -->
          <button
            onclick={() => { showHelix = !showHelix; }}
            class="px-2 py-1 text-[11px] text-[#475569] hover:text-[#FFD700] transition-colors"
            data-testid="helix-toggle"
          >{showHelix ? 'Hide 3D View' : 'Show 3D View'}</button>
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

    <!-- Desktop (>=1024): inline right-hand panel — original behavior. -->
    {#if showHelix && viewport === 'desktop'}
      <div class="lg:block lg:w-[35%] xl:w-[40%] min-w-[200px] max-w-[600px] relative border-l border-[#1e293b]" data-testid="helix-panel-inline">
        <Helix3D />
      </div>
    {/if}
  </div>

  <!-- Tablet/mobile overlay — full-screen Helix3D drawer.
       Rendered outside the flex container so it sits on top of all layout
       at high z-index. Includes a close button (top-right) since on
       narrow screens the nav toggle may be off-screen behind a scroll. -->
  {#if showHelix && viewport !== 'desktop'}
    <div
      class="fixed inset-0 z-40 bg-[#0a0a0f]"
      data-testid="helix-panel-overlay"
    >
      <button
        onclick={() => { showHelix = false; }}
        class="absolute top-3 right-3 z-50 px-3 py-1.5 text-[11px] rounded bg-[#1e293b] text-[#FFD700] hover:bg-[#FFD700]/15 border border-[#FFD700]/30 shadow-[0_0_8px_rgba(255,215,0,0.2)]"
        data-testid="helix-overlay-close"
      >Close 3D View</button>
      <Helix3D />
    </div>
  {/if}
  <StatusBar />
  <CommandPalette />
  <CopilotDrawer />
  <MemoryDrawer />
  <HelixTooltip />
  <HelixDetailPanel />
  <ScrumReport />
</div>
{/if}
