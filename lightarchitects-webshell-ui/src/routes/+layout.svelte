<script lang="ts">
  // Root layout — shared chrome for every route.
  // Replaces src/app.svelte: all routing logic removed (SvelteKit handles it).
  // Navigation: goto() via $lib/navigation or $app/navigation directly.
  // Current route: page.url.pathname from $app/state.

  import { onMount } from 'svelte';
  import { page } from '$app/state';
  import { afterNavigate, goto } from '$app/navigation';

  import Helix3D from '../components/Helix3D.svelte';
  import CommandPalette from '../components/CommandPalette.svelte';
  import SetupFlow from '../screens/setup/SetupFlow.svelte';
  import CopilotDrawer from '../components/CopilotDrawer.svelte';
  import CopilotSurface from '../components/CopilotSurface.svelte';
  import MemoryDrawer from '../components/MemoryDrawer.svelte';
  import PolytopeButton from '../components/PolytopeButton.svelte';
  import CopilotCornerButton from '../components/CopilotCornerButton.svelte';
  import StreamButton from '../components/StreamButton.svelte';
  import StreamDrawer from '../components/StreamDrawer.svelte';
  import NavDropdown from '../components/NavDropdown.svelte';
  import AmbientParticles from '../components/AmbientParticles.svelte';
  import HelixTooltip from '../components/HelixTooltip.svelte';
  import HelixDetailPanel from '../components/HelixDetailPanel.svelte';
  import ScrumReport from '../components/ScrumReport.svelte';
  import NotificationStack from '../components/notifications/NotificationStack.svelte';
  import Tooltip from '../components/Tooltip.svelte';
  import AuthBanner from '../components/AuthBanner.svelte';
  import DiffPreview from '../components/DiffPreview.svelte';
  import KeymapLegend from '../components/KeymapLegend.svelte';
  import HelixLegend from '../components/HelixLegend.svelte';
  import CornerBrackets from '$lib/components/CornerBrackets.svelte';
  import QuestionCard from '$lib/components/QuestionCard.svelte';
  import ScanLines from '../components/atmosphere/ScanLines.svelte';
  import ProjectPicker from '../components/ProjectPicker.svelte';
  import ActiveBuildsChip from '../components/ActiveBuildsChip.svelte';
  import AutoModeChip from '../components/AutoModeChip.svelte';
  import GlobalEventsOverlay from '../components/GlobalEventsOverlay.svelte';
  import StatsTopbar from '../components/StatsTopbar.svelte';
  import StatusBar from '../components/StatusBar.svelte';

  import {
    ayinStatus, startWaveTick, stopWaveTick, initializeStores, drawerHeightPx, drawerWidthPx, memoryDrawerOpen,
    builds, currentBuildId, findings, logEntries, artifacts, conductorTasks, arenaStatus, alerts,
    activePlan, latestScrumReport, hotMemory, coldMemory, activeHelixNode, selectedPillar,
    expandedFindings, supervisorAlerts, siblingHealth, copilotMessages, strategyHitl,
    intakeFormDirty, authStatus, commandPaletteOpen, eventsOverlayOpen, streamDrawerWidthPx,
    streamDrawerOpen, streamDrawerActiveTabs, type StreamDrawerTab,
    pendingQuestions, copilotSurfaceOpen,
    currentRoute,
  } from '$lib/stores';

  import { get } from 'svelte/store';
  import { setupComplete, step, loadSetupInfo, selectedBackend, selectedModel, selectedAgent, settingsOpen } from '$lib/setup';
  import SettingsOverlay from '../components/SettingsOverlay.svelte';
  import { connectGlobalSSE, disconnectGlobalSSE } from '$lib/sse';
  import { authHeaders } from '$lib/auth';
  import { saveSettingsDebounced } from '$lib/settings-persistence';
  import { registerHotkey, dispatchHotkey } from '$lib/hotkeyRegistry';
  import { scope, scopeFromParams } from '$lib/cockpit/stores/scope';
  import { startLayoutSync } from '$lib/layout-sync';

  // Children slot — SvelteKit injects the matched +page.svelte here.
  let { children } = $props();

  // Track persisted stores — save on any change after initial load.
  let settingsUnsubs: (() => void)[] = [];

  function openStreamTab(tab: StreamDrawerTab) {
    streamDrawerActiveTabs.update(tabs => tabs.includes(tab) ? tabs : [...tabs, tab]);
    streamDrawerOpen.set(true);
  }

  // ── Viewport + Helix panel ──────────────────────────────────────────────────
  type ViewportCategory = 'mobile' | 'tablet' | 'desktop';
  let viewport = $state<ViewportCategory>('desktop');
  let showHelix = $state(false);

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
    if (next !== 'desktop') showHelix = false;
  }

  // ── Setup gate ─────────────────────────────────────────────────────────────
  const setupDone = $derived($setupComplete && $step === 'done');

  $effect(() => {
    if (import.meta.env.DEV) (window as any).__e2e_ready = setupDone;
  });

  // ── Current route (replaces currentRoute writable + hashchange) ────────────
  // page.url.pathname is reactive — reads the SvelteKit routing state.
  const activeRoute = $derived(page.url.pathname);

  // ── Navigation history ─────────────────────────────────────────────────────
  const HOME_ROUTES = new Set(['/dashboard', '/dispatch', '/run', '/']);
  let navStack = $state<string[]>([]);
  const canGoBack = $derived(navStack.length > 1);

  function trackNav(path: string) {
    navStack = [...navStack.slice(-19), path];
  }

  function goBack() {
    if (navStack.length > 1) {
      navStack = navStack.slice(0, -1);
      window.history.back();
    }
  }

  function goHome() {
    navStack = [];
    goto('/dashboard');
  }

  // SvelteKit navigation hook — fires after every client-side navigation.
  // Replaces the hashchange event listener + handleHashChange() from App.svelte.
  // Also keeps currentRoute store in sync so snapshotContextForCopilot() has
  // the correct route when it stamps uiContext.route on copilot submissions.
  afterNavigate(({ to }) => {
    const path = to?.url.pathname ?? '/';
    trackNav(path);
    currentRoute.set(path);
    // Clear cockpit scope when navigating away from /cockpit/* — the 4 cockpit
    // +page.svelte files set scope on mount but never clear it on unmount.
    // Without this, any non-cockpit consumer of `scope` (e.g. copilot snapshot)
    // sees stale build context after the user leaves the cockpit surface.
    if (!path.startsWith('/cockpit')) scope.set(null);
  });

  onMount(() => {
    syncViewport();
    window.addEventListener('resize', syncViewport);
    currentRoute.set(window.location.pathname);

    loadSetupInfo();

    if (import.meta.env.DEV && window.location.hostname === 'localhost') {
      (window as any).__e2e = {
        setupComplete, step,
        builds, currentBuildId, findings, logEntries, artifacts,
        conductorTasks, arenaStatus, alerts, activePlan,
        latestScrumReport, hotMemory, coldMemory, activeHelixNode,
        selectedPillar, expandedFindings, supervisorAlerts,
        siblingHealth, copilotMessages, strategyHitl,
        authStatus, intakeFormDirty,
        commandPaletteOpen,
      };
      (window as any).__e2e_stores = () => ({
        builds: get(builds),
        siblingHealth: get(siblingHealth),
        copilotMessages: get(copilotMessages),
        ayinStatus: get(ayinStatus),
        alerts: get(alerts),
        currentBuildId: get(currentBuildId),
        selectedPillar: get(selectedPillar),
        authStatus: get(authStatus),
      });
    }

    startWaveTick();
    ayinStatus.set('reconnecting');

    const initializeStoresPromise = initializeStores();
    connectGlobalSSE();
    const stopLayoutSync = startLayoutSync();

    const e4SessionId = crypto.randomUUID();
    void fetch('/api/coordination/sessions/start', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json', ...authHeaders() },
      body: JSON.stringify({ build_codename: 'webshell', session_id: e4SessionId }),
    });

    // Track initial route
    trackNav(window.location.pathname);

    window.addEventListener('beforeunload', beforeUnloadGuard);

    const unregGlobalKeys = [
      registerHotkey({
        id: 'global-squad-dispatch', keys: ['⌘', 'K'], label: 'Open Dispatch',
        group: 'Navigation', scope: 'global',
        matches: e => (e.metaKey || e.ctrlKey) && e.key === 'k',
        handler: () => goto('/dispatch'),
      }),
      registerHotkey({
        id: 'global-keymap-legend', keys: ['⌘', '/'], label: 'Open keyboard shortcuts',
        group: 'Navigation', scope: 'global',
        matches: e => (e.metaKey || e.ctrlKey) && e.key === '/',
        handler: () => window.dispatchEvent(new CustomEvent('la:toggle-keymap-legend')),
      }),
      registerHotkey({
        id: 'global-keymap-legend-question', keys: ['?'], label: 'Open keyboard shortcuts (? alias)',
        group: 'Navigation', scope: 'global',
        matches: e => e.key === '?' && !e.metaKey && !e.ctrlKey && !e.altKey && !(e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement),
        handler: () => window.dispatchEvent(new CustomEvent('la:toggle-keymap-legend')),
      }),
      registerHotkey({
        id: 'global-tab-1', keys: ['1'], label: 'Dashboard: Overview',
        group: 'Navigation', scope: 'global',
        matches: e => !e.metaKey && !e.ctrlKey && !e.altKey && e.key === '1' && !(e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement),
        handler: () => goto('/dashboard'),
      }),
      registerHotkey({
        id: 'global-tab-2', keys: ['2'], label: 'Workspace: Build Studio',
        group: 'Navigation', scope: 'global',
        matches: e => !e.metaKey && !e.ctrlKey && !e.altKey && e.key === '2' && !(e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement),
        handler: () => goto('/builds'),
      }),
      registerHotkey({
        id: 'global-tab-3', keys: ['3'], label: 'Workspace: Dispatch',
        group: 'Navigation', scope: 'global',
        matches: e => !e.metaKey && !e.ctrlKey && !e.altKey && e.key === '3' && !(e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement),
        handler: () => goto('/dispatch'),
      }),
      registerHotkey({
        id: 'global-tab-4', keys: ['4'], label: 'Dashboard: Cockpit',
        group: 'Navigation', scope: 'global',
        matches: e => !e.metaKey && !e.ctrlKey && !e.altKey && e.key === '4' && !(e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement),
        handler: () => goto('/activity'),
      }),
      registerHotkey({
        id: 'global-tab-5', keys: ['5'], label: 'Knowledge: Helix',
        group: 'Navigation', scope: 'global',
        matches: e => !e.metaKey && !e.ctrlKey && !e.altKey && e.key === '5' && !(e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement),
        handler: () => goto('/helix'),
      }),
      registerHotkey({
        id: 'global-chip-builds', keys: ['⌥', '1'], label: 'Jump to All Builds',
        group: 'Status Chips', scope: 'global',
        matches: e => e.altKey && !e.metaKey && !e.ctrlKey && e.key === '1' && !(e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement),
        handler: () => goto('/builds'),
      }),
      registerHotkey({
        id: 'global-chip-active', keys: ['⌥', '2'], label: 'Jump to Active Builds',
        group: 'Status Chips', scope: 'global',
        matches: e => e.altKey && !e.metaKey && !e.ctrlKey && e.key === '2' && !(e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement),
        handler: () => goto('/builds?filter=active'),
      }),
      registerHotkey({
        id: 'global-chip-agents', keys: ['⌥', '3'], label: 'Jump to Agent Fleet',
        group: 'Status Chips', scope: 'global',
        matches: e => e.altKey && !e.metaKey && !e.ctrlKey && e.key === '3' && !(e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement),
        handler: () => goto('/dashboard'),
      }),
      registerHotkey({
        id: 'global-chip-gates', keys: ['⌥', '4'], label: 'Jump to Gate History',
        group: 'Status Chips', scope: 'global',
        matches: e => e.altKey && !e.metaKey && !e.ctrlKey && e.key === '4' && !(e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement),
        handler: () => goto('/builds?filter=gates'),
      }),
      registerHotkey({
        id: 'global-chip-approval', keys: ['⌥', '5'], label: 'Jump to Approval Queue',
        group: 'Status Chips', scope: 'global',
        matches: e => e.altKey && !e.metaKey && !e.ctrlKey && e.key === '5' && !(e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement),
        handler: () => goto('/run?tab=approval'),
      }),
      registerHotkey({
        id: 'global-chip-idle', keys: ['⌥', '6'], label: 'Jump to Idle/Stale View',
        group: 'Status Chips', scope: 'global',
        matches: e => e.altKey && !e.metaKey && !e.ctrlKey && e.key === '6' && !(e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement),
        handler: () => goto('/dashboard'),
      }),
      registerHotkey({
        id: 'global-copilot', keys: ['⌃', '`'], label: 'Toggle Copilot drawer',
        group: 'Drawers', scope: 'global',
        matches: e => e.ctrlKey && e.key === '`',
        handler: () => window.dispatchEvent(new CustomEvent('la:toggle-copilot')),
      }),
      registerHotkey({
        id: 'global-memory', keys: ['⌘', 'M'], label: 'Toggle Memory drawer',
        group: 'Drawers', scope: 'global',
        matches: e => (e.metaKey || e.ctrlKey) && e.key === 'm',
        handler: () => openStreamTab('memory'),
      }),
    ];

    function handleGlobalKey(e: KeyboardEvent) {
      dispatchHotkey(e, window.location.pathname);
    }
    window.addEventListener('keydown', handleGlobalKey);

    let initialized = false;
    const trigger = () => { if (initialized) saveSettingsDebounced(); };
    settingsUnsubs = [
      drawerHeightPx.subscribe(trigger),
      memoryDrawerOpen.subscribe(trigger),
      selectedBackend.subscribe(trigger),
      selectedModel.subscribe(trigger),
      selectedAgent.subscribe(trigger),
    ];
    initializeStoresPromise.then(() => { initialized = true; });

    return () => {
      stopWaveTick();
      stopLayoutSync();
      disconnectGlobalSSE();
      void fetch('/api/coordination/sessions/end', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json', ...authHeaders() },
        body: JSON.stringify({ session_id: e4SessionId }),
      });
      settingsUnsubs.forEach(fn => fn());
      unregGlobalKeys.forEach(fn => fn());
      window.removeEventListener('resize', syncViewport);
      window.removeEventListener('beforeunload', beforeUnloadGuard);
      window.removeEventListener('keydown', handleGlobalKey);
    };
  });

  function beforeUnloadGuard(event: BeforeUnloadEvent) {
    if (!get(intakeFormDirty)) return;
    event.preventDefault();
    event.returnValue = '';
  }
</script>

{#if !setupDone}
  <SetupFlow />
{:else}

<AuthBanner />
<DiffPreview />
<KeymapLegend />
<HelixLegend />
<CornerBrackets />
<ScanLines route={activeRoute} />

<div class="w-screen h-screen overflow-hidden bg-[var(--la-bg-void)] text-[var(--la-text-bright)]">
  <div class="flex flex-col md:flex-row" style="height: 100vh; padding-left: {$drawerWidthPx}px; transition: padding-left 0.18s ease;">
    <div
      class="flex-1 flex flex-col overflow-hidden relative"
      style="padding-right: {$streamDrawerWidthPx}px; transition: padding-right 260ms cubic-bezier(0.4,0,0.2,1);"
    >
      <AmbientParticles />

      <nav
        class="la-nav flex items-stretch border-b border-[var(--la-hair-base)] bg-[var(--la-bg-void)] shrink-0 overflow-visible"
        style="height: var(--la-header-height, 56px);"
        data-status={$ayinStatus}
      >
        <ProjectPicker />
        <NavDropdown />

        {#if canGoBack}
          <Tooltip content="Go back (Alt+←)" side="bottom">
            <button class="nav-cell nav-ctrl nav-ctrl--nav" onclick={goBack} aria-label="Navigate back" data-testid="nav-back">←</button>
          </Tooltip>
        {/if}
        {#if !HOME_ROUTES.has(activeRoute)}
          <Tooltip content="Go to Dashboard (home)" side="bottom">
            <button class="nav-cell nav-ctrl nav-ctrl--nav" onclick={goHome} aria-label="Go to dashboard" data-testid="nav-home">⌂</button>
          </Tooltip>
        {/if}

        <div class="ml-auto shrink-0 flex items-stretch">
          {#if $ayinStatus === 'reconnecting' || $ayinStatus === 'offline'}
            <Tooltip content="Gateway offline — click to reconnect" side="bottom">
              <button class="nav-offline-dot-btn nav-cell" onclick={connectGlobalSSE} aria-label="Gateway offline — click to reconnect"></button>
            </Tooltip>
          {/if}

          <ActiveBuildsChip />
          <AutoModeChip />

          <Tooltip content="Live events feed — opens in the right stream panel (E)" side="bottom">
            <button
              onclick={() => openStreamTab('events')}
              class="nav-cell nav-ctrl {$streamDrawerActiveTabs.includes('events') ? 'nav-ctrl--on' : ''}"
              data-testid="events-toggle"
            >Events</button>
          </Tooltip>

          <Tooltip content="Hot · Cold · Convergences — opens in the right stream panel (Cmd+M)" side="bottom">
            <button
              onclick={() => openStreamTab('memory')}
              class="nav-cell nav-ctrl {$streamDrawerActiveTabs.includes('memory') ? 'nav-ctrl--on' : ''}"
              title="Memory panel (Cmd+M)"
              data-testid="memory-toggle"
            >Memory</button>
          </Tooltip>

          <Tooltip content="Toggle the 3D knowledge graph — opens in the right stream panel" side="bottom">
            <button
              onclick={() => openStreamTab('3d')}
              class="nav-cell nav-ctrl {$streamDrawerActiveTabs.includes('3d') ? 'nav-ctrl--on' : ''}"
              data-testid="helix-toggle"
            >3D</button>
          </Tooltip>

          <Tooltip content="What is the Helix? — color map of agents and LASDLC quality gates" side="bottom">
            <button
              onclick={() => { window.dispatchEvent(new CustomEvent('la:toggle-helix-legend')); }}
              class="nav-cell nav-ctrl nav-ctrl--mute"
              aria-label="What is the Helix?"
              data-testid="helix-legend-trigger"
            >?</button>
          </Tooltip>
        </div>
      </nav>

      <StatsTopbar />

      <!-- SvelteKit renders the matched +page.svelte here (replaces the dynamic <ActiveScreen> -->
      {@render children()}

      <StatusBar />
      <div class="shrink-0" style="height: 56px;" aria-hidden="true"></div>
    </div>

    <!-- Desktop inline Helix3D panel -->
    {#if showHelix && viewport === 'desktop' && activeRoute !== '/helix'}
      <button
        class="w-1 shrink-0 cursor-col-resize hover:bg-[#FFD700]/30 active:bg-[#FFD700]/50 transition-colors border-l border-[#1e293b] p-0 bg-transparent {isResizing ? 'bg-[#FFD700]/40' : ''}"
        aria-label="Resize helix panel"
        onmousedown={startResize}
        data-testid="helix-resize-handle"
      ></button>
      <div class="relative shrink-0 overflow-hidden" style="width: {helixWidth}px" data-testid="helix-panel-inline">
        <Helix3D />
      </div>
    {/if}
  </div>

  <!-- Tablet/mobile Helix3D overlay -->
  {#if showHelix && viewport !== 'desktop' && activeRoute !== '/helix'}
    <div class="fixed inset-0 z-40 bg-[#0a0a0f]" data-testid="helix-panel-overlay">
      <button
        onclick={() => { showHelix = false; }}
        class="absolute top-3 right-3 z-50 px-3 py-1.5 text-[11px] rounded bg-[#1e293b] text-[#FFD700] hover:bg-[#FFD700]/15 border border-[#FFD700]/30 shadow-[0_0_8px_rgba(255,215,0,0.2)]"
        data-testid="helix-overlay-close"
      >Close 3D View</button>
      <Helix3D />
    </div>
  {/if}

  <GlobalEventsOverlay route={activeRoute} />
  <CommandPalette />
  <CopilotDrawer />
  <CopilotCornerButton />
  <PolytopeButton />
  <MemoryDrawer />
  <StreamDrawer />
  <StreamButton />
  <HelixTooltip />
  <HelixDetailPanel />
  <ScrumReport />
  <NotificationStack />
  {#if $settingsOpen}
    <SettingsOverlay />
  {/if}
  {#if $copilotSurfaceOpen}
    <CopilotSurface onclose={() => copilotSurfaceOpen.set(false)} />
  {/if}

  {#if $pendingQuestions.size > 0}
    <div
      class="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm"
      data-testid="question-overlay"
      role="presentation"
    >
      <div class="flex flex-col gap-3 w-full max-w-xl px-4">
        {#each [...$pendingQuestions] as [toolUseId, state] (toolUseId)}
          <QuestionCard
            {toolUseId}
            questions={state.questions}
            onAnswered={() => pendingQuestions.update(m => { const n = new Map(m); n.delete(toolUseId); return n; })}
          />
        {/each}
      </div>
    </div>
  {/if}
</div>

{/if}
