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
  import DiffPreview from './components/DiffPreview.svelte';
  import KeymapLegend from './components/KeymapLegend.svelte';
  import HelixLegend from './components/HelixLegend.svelte';
  import {
    ayinStatus, startWaveTick, stopWaveTick, initializeStores, drawerHeightPx, memoryDrawerOpen,
    builds, currentBuildId, findings, logEntries, artifacts, conductorTasks, arenaStatus, alerts,
    activePlan, latestScrumReport, hotMemory, coldMemory, activeHelixNode, selectedPillar,
    expandedFindings, supervisorAlerts, siblingHealth, copilotMessages,
    intakeFormDirty, authStatus,
  } from '$lib/stores';
  import { get } from 'svelte/store';
  import { setupComplete, step, loadSetupInfo, selectedBackend, selectedModel, selectedAgent } from '$lib/setup';
  import { connectGlobalSSE, disconnectGlobalSSE } from '$lib/sse';
  import { saveSettingsDebounced } from '$lib/settings-persistence';
  import { registerHotkey, dispatchHotkey } from '$lib/hotkeyRegistry';

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
  // Panel is hidden by default; user toggles it with the "Show 3D View" button.
  // Below 1024 the panel always renders as a full-screen overlay.
  // At <768 the page itself stacks vertically.
  type ViewportCategory = 'mobile' | 'tablet' | 'desktop';
  let viewport = $state<ViewportCategory>('desktop');
  let showHelix = $state(false);

  // Resizable helix panel — persisted to localStorage.
  const HELIX_WIDTH_KEY = 'la.helixPanelWidth';
  let helixWidth = $state<number>((() => {
    try { return parseInt(localStorage.getItem(HELIX_WIDTH_KEY) ?? '380', 10) || 380; }
    catch { return 380; }
  })());
  let isResizing = $state(false);

  function startResize(e: MouseEvent) {
    e.preventDefault();
    isResizing = true;
    const startX = e.clientX;
    const startWidth = helixWidth;
    function onMove(ev: MouseEvent) {
      const delta = startX - ev.clientX;
      helixWidth = Math.min(700, Math.max(220, startWidth + delta));
    }
    function onUp() {
      isResizing = false;
      try { localStorage.setItem(HELIX_WIDTH_KEY, String(helixWidth)); } catch { /* ok */ }
      window.removeEventListener('mousemove', onMove);
      window.removeEventListener('mouseup', onUp);
    }
    window.addEventListener('mousemove', onMove);
    window.addEventListener('mouseup', onUp);
  }

  function categorizeViewport(width: number): ViewportCategory {
    if (width < 768) return 'mobile';
    if (width < 1024) return 'tablet';
    return 'desktop';
  }

  function syncViewport() {
    const next = categorizeViewport(window.innerWidth);
    if (next === viewport) return;
    viewport = next;
    // Only auto-collapse when leaving desktop — never auto-restore (user decides).
    if (next !== 'desktop') showHelix = false;
  }

  // Derived condition for setup gate — explicit dependency tracking in Svelte 5
  const setupDone = $derived($setupComplete && $step === 'done');

  // Tab order optimised for read-before-write (#32): operators land on
  // Activity (live state), then can scan Sitrep (squad health) or Queue
  // (existing builds) before reaching Intake (new build — write action).
  // Squad Dispatch appended last — a power-user action (Cmd+K shortcut).
  const NAV_ITEMS = [
    { label: 'Activity', hash: '/activity',      hint: 'Live trace events from running agents',                                         separator: false },
    { label: 'Sitrep',   hash: '/sitrep',         hint: 'Squad health snapshot — agent status, alerts, uptime',                         separator: false },
    { label: 'Queue',    hash: '/',               hint: 'All builds — past, in-flight, and queued',                                     separator: false },
    { label: 'Intake',   hash: '/intake',         hint: 'Start a new build (Quick or Plan mode)',                                       separator: false },
    { label: 'Squad',    hash: '/squad-dispatch', hint: 'Dispatch agents by domain — Engineer, Security, Researcher, Ops (Cmd+K)',      separator: true  },
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
        // Wave 3 P0s: AuthBanner status (#13), Intake dirty state (#15)
        authStatus, intakeFormDirty,
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

    // Global hotkeys — all entries registered into hotkeyRegistry so
    // KeymapLegend stays accurate automatically. dispatchHotkey() routes
    // to the correct handler based on scope + current route.
    const unregGlobalKeys = [
      registerHotkey({
        id: 'global-squad-dispatch',
        keys: ['⌘', 'K'],
        label: 'Open Squad Dispatch',
        group: 'Navigation',
        scope: 'global',
        matches: e => (e.metaKey || e.ctrlKey) && e.key === 'k',
        handler: () => navigate('/squad-dispatch'),
      }),
      registerHotkey({
        id: 'global-keymap-legend',
        keys: ['⌘', '/'],
        label: 'Open keyboard shortcuts',
        group: 'Navigation',
        scope: 'global',
        matches: e => (e.metaKey || e.ctrlKey) && e.key === '/',
        handler: () => window.dispatchEvent(new CustomEvent('la:toggle-keymap-legend')),
      }),
      registerHotkey({
        id: 'global-tab-1',
        keys: ['1'],
        label: 'Go to Activity',
        group: 'Navigation',
        scope: 'global',
        matches: e => !e.metaKey && !e.ctrlKey && !e.altKey && e.key === '1' && !(e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement),
        handler: () => navigate('/activity'),
      }),
      registerHotkey({
        id: 'global-tab-2',
        keys: ['2'],
        label: 'Go to Sitrep',
        group: 'Navigation',
        scope: 'global',
        matches: e => !e.metaKey && !e.ctrlKey && !e.altKey && e.key === '2' && !(e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement),
        handler: () => navigate('/sitrep'),
      }),
      registerHotkey({
        id: 'global-tab-3',
        keys: ['3'],
        label: 'Go to Queue',
        group: 'Navigation',
        scope: 'global',
        matches: e => !e.metaKey && !e.ctrlKey && !e.altKey && e.key === '3' && !(e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement),
        handler: () => navigate('/'),
      }),
      registerHotkey({
        id: 'global-tab-4',
        keys: ['4'],
        label: 'Go to Intake',
        group: 'Navigation',
        scope: 'global',
        matches: e => !e.metaKey && !e.ctrlKey && !e.altKey && e.key === '4' && !(e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement),
        handler: () => navigate('/intake'),
      }),
      registerHotkey({
        id: 'global-tab-5',
        keys: ['5'],
        label: 'Go to Squad Dispatch',
        group: 'Navigation',
        scope: 'global',
        matches: e => !e.metaKey && !e.ctrlKey && !e.altKey && e.key === '5' && !(e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement),
        handler: () => navigate('/squad-dispatch'),
      }),
      registerHotkey({
        id: 'global-copilot',
        keys: ['⌃', '`'],
        label: 'Toggle Copilot drawer',
        group: 'Drawers',
        scope: 'global',
        matches: e => e.ctrlKey && e.key === '`',
        handler: () => window.dispatchEvent(new CustomEvent('la:toggle-copilot')),
      }),
      registerHotkey({
        id: 'global-memory',
        keys: ['⌘', 'M'],
        label: 'Toggle Memory drawer',
        group: 'Drawers',
        scope: 'global',
        matches: e => (e.metaKey || e.ctrlKey) && e.key === 'm',
        handler: () => memoryDrawerOpen.update(v => !v),
      }),
    ];

    function handleGlobalKey(e: KeyboardEvent) {
      dispatchHotkey(e, $currentRoute);
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
      unregGlobalKeys.forEach(fn => fn());
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
<!-- Diff-preview modal — operator-gated FS mutation flow (#47).
     Listens for `la:fs-mutation-pending` events; backend interception layer
     follows once mantis Phase 3 rebases on main (filed per #88-#92). -->
<DiffPreview />
<!-- Keymap legend modal — Cmd+/ toggles, Esc dismisses (#4). -->
<KeymapLegend />
<HelixLegend />
<!-- Corner brackets — fixed-position tactical frame (#99 AES-3).
     CSS lives in tokens.css .corner-bracket rules.
     Animate to researcher-green during active dispatch via data-dispatching. -->
<div class="corner-bracket tl" aria-hidden="true"></div>
<div class="corner-bracket tr" aria-hidden="true"></div>
<div class="corner-bracket bl" aria-hidden="true"></div>
<div class="corner-bracket br" aria-hidden="true"></div>
<div class="w-screen h-screen overflow-hidden bg-[#08090a] text-[var(--la-text-bright)] font-['JetBrains_Mono',monospace]">
  <!-- Responsive container:
         <768  : flex-col       (vertical stack — single-column flow)
         >=768 : flex-row       (side-by-side) — at 768..1023 the helix panel
                                still hides; at >=1024 it renders inline. -->
  <div class="flex flex-col md:flex-row" style="height: calc(100vh - {$drawerHeightPx}px);">
    <!-- Left: Main content area -->
    <div class="flex-1 flex flex-col overflow-hidden relative">
      <!-- Ambient particles — drifting helix-palette dots behind content -->
      <AmbientParticles />
      <!-- Top navigation strip — underline-only active indicator (#23) -->
      <nav class="flex items-stretch gap-1 px-3 border-b border-[#1e293b] bg-[#0a0a0f] shrink-0 overflow-x-auto">
        {#each NAV_ITEMS as item}
          {#if item.separator}
            <span class="self-center text-[#1e293b] select-none px-0.5" aria-hidden="true">·</span>
          {/if}
          <Tooltip content={item.hint} side="bottom">
            <button
              onclick={() => navigate(item.hash)}
              class="shrink-0 px-3 text-[11px] transition-all self-stretch flex items-center border-b-2 {isActive(item.hash) ? 'border-[#FFD700] text-[#FFD700]' : 'border-transparent text-[#475569] hover:text-[#94a3b8]'}"
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
          <Tooltip content="Toggle the 3D knowledge graph panel — live helix of agent memory strands" side="bottom">
            <button
              onclick={() => { showHelix = !showHelix; }}
              class="px-2 py-1 text-[11px] text-[#475569] hover:text-[#FFD700] transition-colors"
              data-testid="helix-toggle"
            >{showHelix ? 'Hide 3D View' : 'Show 3D View'}</button>
          </Tooltip>
          <Tooltip content="What is the Helix? — color map of agents and LASDLC quality gates" side="bottom">
            <button
              onclick={() => { window.dispatchEvent(new CustomEvent('la:toggle-helix-legend')); }}
              class="px-1.5 py-1 text-[11px] text-[#334155] hover:text-[#f0c040] transition-colors rounded"
              aria-label="What is the Helix?"
              data-testid="helix-legend-trigger"
            >?</button>
          </Tooltip>
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
          <ActiveScreen />
        {/key}
      {/if}
    </div>

    <!-- Desktop (>=1024): inline right-hand panel, user-resizable. -->
    {#if showHelix && viewport === 'desktop'}
      <!-- Drag handle — sits on the seam between content and helix panel. -->
      <button
        class="w-1 shrink-0 cursor-col-resize hover:bg-[#FFD700]/30 active:bg-[#FFD700]/50 transition-colors border-l border-[#1e293b] p-0 bg-transparent {isResizing ? 'bg-[#FFD700]/40' : ''}"
        aria-label="Resize helix panel"
        onmousedown={startResize}
        data-testid="helix-resize-handle"
      ></button>
      <div
        class="relative shrink-0 overflow-hidden"
        style="width: {helixWidth}px"
        data-testid="helix-panel-inline"
      >
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
