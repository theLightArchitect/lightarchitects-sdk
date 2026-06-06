<script lang="ts">
  import { writable } from 'svelte/store';
  import { onMount } from 'svelte';
  import Helix3D from './components/Helix3D.svelte';
  import CommandPalette from './components/CommandPalette.svelte';
  import SetupFlow from './screens/setup/SetupFlow.svelte';
  import CopilotDrawer from './components/CopilotDrawer.svelte';
  import CopilotSurface from './components/CopilotSurface.svelte';
  import MemoryDrawer from './components/MemoryDrawer.svelte';
  import PolytopeButton from './components/PolytopeButton.svelte';
  import CopilotCornerButton from './components/CopilotCornerButton.svelte';
  import StreamButton from './components/StreamButton.svelte';
  import StreamDrawer from './components/StreamDrawer.svelte';
  import NavDropdown from './components/NavDropdown.svelte';
  import AmbientParticles from './components/AmbientParticles.svelte';
  import HelixTooltip from './components/HelixTooltip.svelte';
  import HelixDetailPanel from './components/HelixDetailPanel.svelte';
  import ScrumReport from './components/ScrumReport.svelte';
  import NotificationStack from './components/notifications/NotificationStack.svelte';
  import Tooltip from './components/Tooltip.svelte';
  import AuthBanner from './components/AuthBanner.svelte';
  import DiffPreview from './components/DiffPreview.svelte';
  import KeymapLegend from './components/KeymapLegend.svelte';
  import HelixLegend from './components/HelixLegend.svelte';
  import CornerBrackets from '$lib/components/CornerBrackets.svelte';
  import QuestionCard from '$lib/components/QuestionCard.svelte';
  import ScanLines from './components/atmosphere/ScanLines.svelte';
  import ProjectPicker from './components/ProjectPicker.svelte';
  import ActiveBuildsChip from './components/ActiveBuildsChip.svelte';
  import AutoModeChip from './components/AutoModeChip.svelte';
  import GlobalEventsOverlay from './components/GlobalEventsOverlay.svelte';
  import StatsTopbar from './components/StatsTopbar.svelte';
  import StatusBar from './components/StatusBar.svelte';
  import {
    ayinStatus, startWaveTick, stopWaveTick, initializeStores, drawerHeightPx, drawerWidthPx, memoryDrawerOpen,
    builds, currentBuildId, findings, logEntries, artifacts, conductorTasks, arenaStatus, alerts,
    activePlan, latestScrumReport, hotMemory, coldMemory, activeHelixNode, selectedPillar,
    expandedFindings, supervisorAlerts, siblingHealth, copilotMessages, strategyHitl,
    intakeFormDirty, authStatus, commandPaletteOpen, eventsOverlayOpen, streamDrawerWidthPx,
    streamDrawerOpen, streamDrawerActiveTabs, type StreamDrawerTab,
    pendingQuestions, copilotSurfaceOpen,
  } from '$lib/stores';

  function openStreamTab(tab: StreamDrawerTab) {
    streamDrawerActiveTabs.update(tabs => tabs.includes(tab) ? tabs : [...tabs, tab]);
    streamDrawerOpen.set(true);
  }
  import { get } from 'svelte/store';
  import { setupComplete, step, loadSetupInfo, selectedBackend, selectedModel, selectedAgent, settingsOpen } from '$lib/setup';
  import SettingsOverlay from './components/SettingsOverlay.svelte';
  import { connectGlobalSSE, disconnectGlobalSSE } from '$lib/sse';
  import { authHeaders } from '$lib/auth';
  import { saveSettingsDebounced } from '$lib/settings-persistence';
  import { registerHotkey, dispatchHotkey } from '$lib/hotkeyRegistry';
  import { matchRoute, applyRedirects, navigate } from '$lib/routes';
  import { scope, scopeFromParams } from '$lib/cockpit/stores/scope';
  import { startLayoutSync } from '$lib/layout-sync';

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
    Dashboard:     () => import('./screens/Dashboard.svelte'),
    Dispatch:      () => import('./screens/Dispatch.svelte'),
    Builds:        () => import('./screens/Builds.svelte'),
    Intake:        () => import('./screens/Intake.svelte'),
    Helix:         () => import('./screens/Helix.svelte'),
    BuildDetail:   () => import('./screens/BuildDetail.svelte'),
    ProjectDetail: () => import('./screens/ProjectDetail.svelte'),
    Comms:         () => import('./screens/Comms.svelte'),
    Editor:        () => import('./screens/Editor.svelte'),
    Git:           () => import('./screens/Git.svelte'),
    PullRequest:   () => import('./screens/PullRequest.svelte'),
    Architecture:    () => import('./screens/Architecture.svelte'),
    DiagramLibrary:  () => import('./screens/DiagramLibrary.svelte'),
    Roadmap:         () => import('./lib/components/RoadmapPanel.svelte'),
    Program:         () => import('./screens/ProgramPanel.svelte'),
    Observability:   () => import('./lib/components/ObservabilityPanel.svelte'),
    Security:        () => import('./lib/components/ContainerPolicyPanel.svelte'),
    Tools:           () => import('./screens/Tools.svelte'),
    AutonomousBuilds: () => import('./screens/AutonomousBuilds.svelte'),
    Chat:            () => import('./screens/Chat.svelte'),
    Supervision:     () => import('./screens/ProgramScreen.svelte'),
    // ── Scope-keyed cockpit (scope-keyed-cockpit-routes) ──
    CockpitPlatform: () => import('./screens/CockpitPlatform.svelte'),
    CockpitProject:  () => import('./screens/CockpitProject.svelte'),
    CockpitBuild:    () => import('./screens/CockpitBuild.svelte'),
    CockpitFile:     () => import('./screens/CockpitFile.svelte'),
  };

  type ScreenModule = { default: any };

  let ActiveScreen = $state<any>(null);
  let screenParams = $state<Record<string, string>>({});
  let screenLoading = $state(true);
  let loadGen = 0; // monotonic counter — latest request wins, stale results dropped

  async function loadScreen(path: string) {
    const gen = ++loadGen;
    screenLoading = true;
    const { screen: key, params } = matchRoute(path);
    screenParams = params;
    scope.set(scopeFromParams(key, params));
    try {
      const mod: ScreenModule = await screenModules[key]();
      if (gen !== loadGen) return; // superseded by a newer navigation
      ActiveScreen = mod.default;
    } catch (err) {
      if (gen !== loadGen) return;
      console.error('Failed to load screen:', key, err);
      try {
        const mod: ScreenModule = await screenModules['Builds']();
        if (gen !== loadGen) return;
        ActiveScreen = mod.default;
      } catch {
        ActiveScreen = null;
      }
    } finally {
      if (gen === loadGen) screenLoading = false;
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

  // E2E readiness flag — set by reactive effect so Playwright polls a simple boolean
  // instead of subscribing to Svelte stores. Only present in DEV builds.
  $effect(() => {
    if (import.meta.env.DEV) (window as any).__e2e_ready = setupDone;
  });

  let activeRoute = $derived($currentRoute);

  // ── Navigation history — powers the back button ──────────────────────────
  const HOME_ROUTES = new Set(['/dashboard', '/dispatch', '/run', '/']);
  let navStack = $state<string[]>([]);
  let canGoBack = $derived(navStack.length > 1);

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
    navigate('/dashboard');
  }

  function handleHashChange() {
    applyRedirects();
    const path = window.location.hash.slice(1) || '/';
    currentRoute.set(path);
    loadScreen(path);
    trackNav(path);
  }

  onMount(() => {
    // Initialize responsive viewport before first render so layout
    // matches the current window width (avoids a desktop-default flash
    // on small screens). Listener stays for live resize tracking.
    syncViewport();
    window.addEventListener('resize', syncViewport);

    loadSetupInfo(); // check setup state before anything else
    // E2E hook — lets Playwright bypass setup flow by setting stores directly.
    // SECURITY: belt-and-suspenders — tree-shaken in prod builds AND restricted
    // to localhost at runtime (prevents staging exposure on dev-mode builds).
    if (import.meta.env.DEV && window.location.hostname === 'localhost') {
      (window as any).__e2e = {
        setupComplete, step,
        // Stores for E2E data injection (Workspace, ScrumReport, Helix, etc.)
        builds, currentBuildId, findings, logEntries, artifacts,
        conductorTasks, arenaStatus, alerts, activePlan,
        latestScrumReport, hotMemory, coldMemory, activeHelixNode,
        selectedPillar, expandedFindings, supervisorAlerts,
        siblingHealth, copilotMessages, strategyHitl,
        // Wave 3 P0s: AuthBanner status (#13), Intake dirty state (#15)
        authStatus, intakeFormDirty,
        // §11 command palette — Cmd+K conflicts with dispatch nav, use store for tests
        commandPaletteOpen,
      };
      // §57.9b — value snapshot for EvidenceCollector.captureStoreSnapshot().
      // Returns current store values (not reactive objects) so Playwright can
      // serialize the state at the moment of a test assertion.
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
    applyRedirects();
    loadScreen(window.location.hash.slice(1) || '/');
    const initializeStoresPromise = initializeStores(); // non-blocking; errors caught internally
    connectGlobalSSE(); // Phase 10.9 — global helix_entry / soul_promotion / strand_activation stream
    const stopLayoutSync = startLayoutSync(); // push mosaic tree to gateway on every change

    // E4 session lifecycle — materialise a soul-chat session for this webshell visit.
    const e4SessionId = crypto.randomUUID();
    void fetch('/api/coordination/sessions/start', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json', ...authHeaders() },
      body: JSON.stringify({ build_codename: 'webshell', session_id: e4SessionId }),
    });

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
        label: 'Open Dispatch',
        group: 'Navigation',
        scope: 'global',
        matches: e => (e.metaKey || e.ctrlKey) && e.key === 'k',
        handler: () => navigate('/dispatch'),
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
        label: 'Dashboard: Overview',
        group: 'Navigation',
        scope: 'global',
        matches: e => !e.metaKey && !e.ctrlKey && !e.altKey && e.key === '1' && !(e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement),
        handler: () => navigate('/dashboard'),
      }),
      registerHotkey({
        id: 'global-tab-2',
        keys: ['2'],
        label: 'Workspace: Build Studio',
        group: 'Navigation',
        scope: 'global',
        matches: e => !e.metaKey && !e.ctrlKey && !e.altKey && e.key === '2' && !(e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement),
        handler: () => navigate('/builds'),
      }),
      registerHotkey({
        id: 'global-tab-3',
        keys: ['3'],
        label: 'Workspace: Dispatch',
        group: 'Navigation',
        scope: 'global',
        matches: e => !e.metaKey && !e.ctrlKey && !e.altKey && e.key === '3' && !(e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement),
        handler: () => navigate('/dispatch'),
      }),
      registerHotkey({
        id: 'global-tab-4',
        keys: ['4'],
        label: 'Dashboard: Cockpit',
        group: 'Navigation',
        scope: 'global',
        matches: e => !e.metaKey && !e.ctrlKey && !e.altKey && e.key === '4' && !(e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement),
        handler: () => navigate('/activity'),
      }),
      registerHotkey({
        id: 'global-tab-5',
        keys: ['5'],
        label: 'Knowledge: Helix',
        group: 'Navigation',
        scope: 'global',
        matches: e => !e.metaKey && !e.ctrlKey && !e.altKey && e.key === '5' && !(e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement),
        handler: () => navigate('/helix'),
      }),
      registerHotkey({
        id: 'global-chip-builds',
        keys: ['⌥', '1'],
        label: 'Jump to All Builds',
        group: 'Status Chips',
        scope: 'global',
        matches: e => e.altKey && !e.metaKey && !e.ctrlKey && e.key === '1' && !(e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement),
        handler: () => navigate('/builds'),
      }),
      registerHotkey({
        id: 'global-chip-active',
        keys: ['⌥', '2'],
        label: 'Jump to Active Builds',
        group: 'Status Chips',
        scope: 'global',
        matches: e => e.altKey && !e.metaKey && !e.ctrlKey && e.key === '2' && !(e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement),
        handler: () => navigate('/builds?filter=active'),
      }),
      registerHotkey({
        id: 'global-chip-agents',
        keys: ['⌥', '3'],
        label: 'Jump to Agent Fleet',
        group: 'Status Chips',
        scope: 'global',
        matches: e => e.altKey && !e.metaKey && !e.ctrlKey && e.key === '3' && !(e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement),
        handler: () => navigate('/dashboard'),
      }),
      registerHotkey({
        id: 'global-chip-gates',
        keys: ['⌥', '4'],
        label: 'Jump to Gate History',
        group: 'Status Chips',
        scope: 'global',
        matches: e => e.altKey && !e.metaKey && !e.ctrlKey && e.key === '4' && !(e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement),
        handler: () => navigate('/builds?filter=gates'),
      }),
      registerHotkey({
        id: 'global-chip-approval',
        keys: ['⌥', '5'],
        label: 'Jump to Approval Queue',
        group: 'Status Chips',
        scope: 'global',
        matches: e => e.altKey && !e.metaKey && !e.ctrlKey && e.key === '5' && !(e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement),
        handler: () => navigate('/run?tab=approval'),
      }),
      registerHotkey({
        id: 'global-chip-idle',
        keys: ['⌥', '6'],
        label: 'Jump to Idle/Stale View',
        group: 'Status Chips',
        scope: 'global',
        matches: e => e.altKey && !e.metaKey && !e.ctrlKey && e.key === '6' && !(e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement),
        handler: () => navigate('/dashboard'),
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
        handler: () => openStreamTab('memory'),
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
      stopLayoutSync();
      disconnectGlobalSSE();
      void fetch('/api/coordination/sessions/end', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json', ...authHeaders() },
        body: JSON.stringify({ session_id: e4SessionId }),
      });
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
<!-- Corner brackets — fixed-position tactical frame.
     data-dispatching prop animates to researcher-green during active dispatch. -->
<CornerBrackets />
<ScanLines route={activeRoute} />
<div class="w-screen h-screen overflow-hidden bg-[var(--la-bg-void)] text-[var(--la-text-bright)]">
  <!-- Responsive container:
         <768  : flex-col       (vertical stack — single-column flow)
         >=768 : flex-row       (side-by-side) — at 768..1023 the helix panel
                                still hides; at >=1024 it renders inline. -->
  <div class="flex flex-col md:flex-row" style="height: 100vh; padding-left: {$drawerWidthPx}px; transition: padding-left 0.18s ease;">
    <!-- Main content area — padding-right when events overlay opens (push-not-occlude) -->
    <div
      class="flex-1 flex flex-col overflow-hidden relative"
      style="padding-right: {$streamDrawerWidthPx}px; transition: padding-right 260ms cubic-bezier(0.4,0,0.2,1);"
    >
      <!-- Ambient particles — drifting helix-palette dots behind content -->
      <AmbientParticles />
      <!-- Top navigation strip — dropdown screen picker, column-separated right controls -->
      <nav class="la-nav flex items-stretch border-b border-[var(--la-hair-base)] bg-[var(--la-bg-void)] shrink-0 overflow-visible" style="height: var(--la-header-height, 56px);" data-status={$ayinStatus}>
        <ProjectPicker />
        <NavDropdown />

        <!-- Back / Home nav affordance — appears when navigated away from root -->
        {#if canGoBack}
          <Tooltip content="Go back (Alt+←)" side="bottom">
            <button
              class="nav-cell nav-ctrl nav-ctrl--nav"
              onclick={goBack}
              aria-label="Navigate back"
              data-testid="nav-back"
            >←</button>
          </Tooltip>
        {/if}
        {#if !HOME_ROUTES.has(activeRoute)}
          <Tooltip content="Go to Dashboard (home)" side="bottom">
            <button
              class="nav-cell nav-ctrl nav-ctrl--nav"
              onclick={goHome}
              aria-label="Go to dashboard"
              data-testid="nav-home"
            >⌂</button>
          </Tooltip>
        {/if}

        <!-- Right-side controls — each in its own column cell separated by 1px hairlines -->
        <div class="ml-auto shrink-0 flex items-stretch">
          <!-- G-1: OFFLINE dot -->
          {#if $ayinStatus === 'reconnecting' || $ayinStatus === 'offline'}
            <Tooltip content="Gateway offline — click to reconnect" side="bottom">
              <button class="nav-offline-dot-btn nav-cell" onclick={connectGlobalSSE} aria-label="Gateway offline — click to reconnect"></button>
            </Tooltip>
          {/if}

          <ActiveBuildsChip />
          <AutoModeChip />

          <!-- Events → opens StreamDrawer on EVT tab -->
          <Tooltip content="Live events feed — opens in the right stream panel (E)" side="bottom">
            <button
              onclick={() => openStreamTab('events')}
              class="nav-cell nav-ctrl {$streamDrawerActiveTabs.includes('events') ? 'nav-ctrl--on' : ''}"
              data-testid="events-toggle"
            >Events</button>
          </Tooltip>

          <!-- Memory → opens StreamDrawer on MEM tab -->
          <Tooltip content="Hot · Cold · Convergences — opens in the right stream panel (Cmd+M)" side="bottom">
            <button
              onclick={() => openStreamTab('memory')}
              class="nav-cell nav-ctrl {$streamDrawerActiveTabs.includes('memory') ? 'nav-ctrl--on' : ''}"
              title="Memory panel (Cmd+M)"
              data-testid="memory-toggle"
            >Memory</button>
          </Tooltip>

          <!-- 3D → opens StreamDrawer on 3D tab -->
          <Tooltip content="Toggle the 3D knowledge graph — opens in the right stream panel" side="bottom">
            <button
              onclick={() => openStreamTab('3d')}
              class="nav-cell nav-ctrl {$streamDrawerActiveTabs.includes('3d') ? 'nav-ctrl--on' : ''}"
              data-testid="helix-toggle"
            >3D</button>
          </Tooltip>

          <!-- Helix legend -->
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

      <!-- Phase 6: StatsTopbar — persistent build fleet status ribbon -->
      <StatsTopbar />

      {#if screenLoading}
        <div class="flex-1 flex items-center justify-center">
          <div class="flex items-center gap-3">
            <div class="w-4 h-4 border-2 border-[#FFD700] border-t-transparent rounded-full animate-spin shadow-[0_0_6px_rgba(255,215,0,0.4)]"></div>
            <span class="text-xs text-[#64748b]">Loading...</span>
          </div>
        </div>
      {:else if ActiveScreen}
        {#key ActiveScreen}
          <ActiveScreen params={screenParams} />
        {/key}
      {/if}
      <!-- Status bar — PTY status · active backend chip · credential indicators -->
      <StatusBar />
      <!-- Bottom reserve: keeps content clear of the fixed corner elements (polytope + copilot) -->
      <div class="shrink-0" style="height: 56px;" aria-hidden="true"></div>
    </div>

    <!-- Desktop (>=1024): inline right-hand panel, user-resizable. -->
    {#if showHelix && viewport === 'desktop' && activeRoute !== '/helix'}
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
  {#if showHelix && viewport !== 'desktop' && activeRoute !== '/helix'}
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
  <!-- Global events overlay (Wave 1.5) — push-not-occlude, 320px right panel -->
  <GlobalEventsOverlay route={activeRoute} />
  <CommandPalette />
  <CopilotDrawer />
  <!-- Corner elements: fixed position, immune to drawer padding, never move -->
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
  <!-- Full-screen Squad Surface overlay — Three.js topology + signal canvas copilot. -->
  {#if $copilotSurfaceOpen}
    <CopilotSurface onclose={() => copilotSurfaceOpen.set(false)} />
  {/if}

  <!-- Question HITL overlay (webshell-hitl-bridge) — rendered above all content.
       Each card maps to one pending gateway `question` tool call identified by tool_use_id. -->
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
