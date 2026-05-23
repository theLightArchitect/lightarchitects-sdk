<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import {
    mailboxMessages, mailboxUnread,
    streamDrawerOpen, streamDrawerWidthPx, streamDrawerMode, streamDrawerActiveTabs,
    type StreamDrawerTab,
    builds, slotAssignments,
    drawerWidthPx,
  } from '$lib/stores';
  import Helix3D from './Helix3D.svelte';

  // ── Constants ────────────────────────────────────────────
  const DEFAULT_WIDTH  = 300;
  const MIN_WIDTH      = 220;
  const MAX_WIDTH      = 560;
  const DEFAULT_HEIGHT = 280;
  const MIN_HEIGHT     = 160;
  const MAX_HEIGHT     = 560;
  const STATS_H        = 24; // px — must match StatsTopbar height

  const AGENT_COLORS: Record<string, string> = {
    ENG: '#3B82F6', QLT: '#8B5CF6', SEC: '#EF4444', OPS: '#10B981',
    RES: '#17C3B2', KNW: '#F59E0B', TST: '#EC4899', SQD: '#6366F1',
  };
  const BUILD_PALETTE = ['#3B82F6', '#17C3B2', '#8B5CF6', '#F59E0B', '#10B981', '#EC4899'];

  // ── Local state ──────────────────────────────────────────
  let widthPx   = $state(DEFAULT_WIDTH);
  let heightPx  = $state(DEFAULT_HEIGHT);
  let pinned    = $state(false);
  let autoscroll = $state(true);
  let searchQ   = $state('');

  // Filter inclusion maps — true = shown. Populated reactively as new agents/builds appear.
  let agentFilter = $state<Record<string, boolean>>({});
  let buildFilter = $state<Record<string, boolean>>({});

  let streamEl: HTMLDivElement | undefined = $state();
  let prevActive = $state(0);

  // ── Derived ──────────────────────────────────────────────
  // Alias prevents TypeScript from narrowing streamDrawerMode inside {#if} blocks
  let mode = $derived<'right' | 'top'>($streamDrawerMode);

  let activeCount = $derived(
    [...$slotAssignments.values()].flat().filter(w => w.state === 'writing' || w.state === 'gate').length,
  );

  // Chronological order (store is newest-first)
  let ordered = $derived([...$mailboxMessages].reverse());

  // Unique agent codes seen across all messages
  let knownAgents = $derived([...new Set(ordered.map(m => normalizeAgent(m.agent)))]);

  // Unique build IDs seen — mapped to build names from the builds store
  let knownBuildIds = $derived([...new Set(ordered.map(m => m.dispatchId).filter((id): id is string => !!id))]);

  let buildLabel = $derived((id: string): string => {
    const b = $builds.find(bld => bld.id === id);
    return b?.name ?? id.slice(0, 12);
  });

  // Apply filters
  let filtered = $derived(
    ordered.filter(m => {
      const code = normalizeAgent(m.agent);
      const bid  = m.dispatchId ?? '';
      return (agentFilter[code] !== false)
        && (!m.dispatchId || buildFilter[bid] !== false)
        && (!searchQ || m.text.toLowerCase().includes(searchQ));
    }),
  );

  // ── Auto-populate filter maps when new agents/builds appear ──
  $effect(() => {
    for (const code of knownAgents) {
      if (!(code in agentFilter)) agentFilter[code] = true;
    }
  });

  $effect(() => {
    for (const id of knownBuildIds) {
      if (!(id in buildFilter)) buildFilter[id] = true;
    }
  });

  // ── Auto-pop on 0→N agent transition ────────────────────
  $effect(() => {
    const cur = activeCount;
    if (cur > 0 && prevActive === 0 && !$streamDrawerOpen && !pinned) {
      streamDrawerOpen.set(true);
    }
    prevActive = cur;
  });

  // ── Publish width to layout ──────────────────────────────
  $effect(() => {
    streamDrawerWidthPx.set($streamDrawerOpen && $streamDrawerMode === 'right' ? widthPx : 0);
  });

  // ── Clear unread when drawer opens ───────────────────────
  $effect(() => {
    if ($streamDrawerOpen) mailboxUnread.set(0);
  });

  // ── Autoscroll when new messages arrive ──────────────────
  $effect(() => {
    // Track filtered.length for reactivity
    const _len = filtered.length;
    if (autoscroll && streamEl) {
      requestAnimationFrame(() => {
        if (streamEl) streamEl.scrollTop = streamEl.scrollHeight;
      });
    }
  });

  // ── Helpers ──────────────────────────────────────────────
  function normalizeAgent(raw: string): string {
    const upper = (raw ?? 'UNK').toUpperCase().trim();
    const MAP: Record<string, string> = {
      ENGINEER: 'ENG', QUALITY: 'QLT', SECURITY: 'SEC',
      OPERATIONS: 'OPS', RESEARCHER: 'RES', RESEARCH: 'RES',
      KNOWLEDGE: 'KNW', TESTING: 'TST', SQUAD: 'SQD',
    };
    return MAP[upper] ?? upper.slice(0, 3);
  }

  function agentColor(code: string): string {
    return AGENT_COLORS[code] ?? '#6e7681';
  }

  function buildColor(id: string): string {
    const idx = knownBuildIds.indexOf(id);
    return BUILD_PALETTE[idx % BUILD_PALETTE.length];
  }

  function formatTs(ts: number): string {
    const d = new Date(ts);
    return [d.getHours(), d.getMinutes(), d.getSeconds()]
      .map(n => String(n).padStart(2, '0'))
      .join(':');
  }

  function toggleAgent(code: string) {
    agentFilter[code] = agentFilter[code] === false ? true : false;
  }

  function toggleBuild(id: string) {
    buildFilter[id] = buildFilter[id] === false ? true : false;
  }

  function close() {
    if (!pinned) streamDrawerOpen.set(false);
  }

  function setMode(m: 'right' | 'top') {
    streamDrawerMode.set(m);
  }

  function onScroll() {
    if (!streamEl) return;
    autoscroll = Math.abs(streamEl.scrollHeight - streamEl.scrollTop - streamEl.clientHeight) < 12;
  }

  // ── Resize — right drawer (drag left edge) ────────────────
  let resizingW = false;
  let resizeStartX = 0;
  let resizeStartW = 0;

  function onResizeWMouseDown(e: MouseEvent) {
    resizingW = true;
    resizeStartX = e.clientX;
    resizeStartW = widthPx;
    document.body.style.cursor = 'col-resize';
    document.body.style.userSelect = 'none';
  }

  // ── Resize — top drawer (drag bottom edge) ────────────────
  let resizingH = false;
  let resizeStartY = 0;
  let resizeStartH = 0;

  function onResizeHMouseDown(e: MouseEvent) {
    resizingH = true;
    resizeStartY = e.clientY;
    resizeStartH = heightPx;
    document.body.style.cursor = 'row-resize';
    document.body.style.userSelect = 'none';
  }

  function onMouseMove(e: MouseEvent) {
    if (resizingW) {
      widthPx = Math.max(MIN_WIDTH, Math.min(MAX_WIDTH, resizeStartW + (resizeStartX - e.clientX)));
    }
    if (resizingH) {
      heightPx = Math.max(MIN_HEIGHT, Math.min(MAX_HEIGHT, resizeStartH + (e.clientY - resizeStartY)));
    }
  }

  function onMouseUp() {
    if (resizingW || resizingH) {
      resizingW = false;
      resizingH = false;
      document.body.style.cursor = '';
      document.body.style.userSelect = '';
    }
  }

  onMount(() => {
    window.addEventListener('mousemove', onMouseMove);
    window.addEventListener('mouseup',   onMouseUp);
  });

  onDestroy(() => {
    window.removeEventListener('mousemove', onMouseMove);
    window.removeEventListener('mouseup',   onMouseUp);
    streamDrawerWidthPx.set(0);
  });

  // ── Tab panel system ─────────────────────────────────────
  const TAB_ORDER: StreamDrawerTab[] = ['stream', 'events', 'memory', '3d'];
  const TAB_SHORT: Record<StreamDrawerTab, string> = { stream: 'STRM', events: 'EVT', memory: 'MEM', '3d': '3D' };
  const TAB_FULL:  Record<StreamDrawerTab, string> = { stream: 'Agent Stream', events: 'Events Feed', memory: 'Memory', '3d': 'Helix 3D' };
  const TAB_COLOR: Record<StreamDrawerTab, string> = { stream: '#FFD700', events: '#17C3B2', memory: '#8B5CF6', '3d': '#3B82F6' };

  function toggleTab(tab: StreamDrawerTab) {
    streamDrawerActiveTabs.update(tabs =>
      tabs.includes(tab) ? tabs.filter(t => t !== tab) : [...tabs, tab],
    );
    if (!$streamDrawerOpen) streamDrawerOpen.set(true);
  }

  function deactivateTab(tab: StreamDrawerTab) {
    streamDrawerActiveTabs.update(tabs => tabs.filter(t => t !== tab));
  }
</script>

<!-- ── RIGHT drawer ─────────────────────────────────────── -->
{#if $streamDrawerMode === 'right'}
  <div
    class="drawer drawer--right"
    class:drawer--open={$streamDrawerOpen}
    style="width: {widthPx}px"
    aria-label="Agent output stream"
    aria-hidden={!$streamDrawerOpen}
  >
    <!-- Resize handle — drag left edge to resize width -->
    <div
      class="resize-h resize-h--w"
      role="separator"
      aria-orientation="vertical"
      aria-label="Resize stream drawer"
      onmousedown={onResizeWMouseDown}
    ></div>

    <div class="seam" class:seam--active={activeCount > 0}></div>

    <!-- Header -->
    <div class="header">
      <div class="header-title">
        <div class="live-dot" class:live-dot--idle={activeCount === 0}></div>
        {#if activeCount > 0}
          <span class="agent-count">{activeCount}</span>
        {/if}
      </div>

      <!-- Tab rail: STRM · EVT · MEM · 3D -->
      <div class="tab-rail" role="group" aria-label="Panel tabs">
        {#each TAB_ORDER as tab (tab)}
          {@const active = $streamDrawerActiveTabs.includes(tab)}
          <button
            class="tab-btn"
            class:tab-btn--active={active}
            style="--tab-color: {TAB_COLOR[tab]}"
            onclick={() => toggleTab(tab)}
            title="{TAB_FULL[tab]} — click to {active ? 'hide' : 'show'}"
            aria-pressed={active}
          >{TAB_SHORT[tab]}</button>
        {/each}
      </div>

      <div class="agent-dots" aria-hidden="true">
        {#each knownAgents as code (code)}
          <div class="agent-dot" style="background: {agentColor(code)}" title={code}></div>
        {/each}
      </div>

      <div class="spacer"></div>

      <button class="ctrl" class:ctrl--active={mode === 'right'} onclick={() => setMode('right')} title="Right drawer">R</button>
      <button class="ctrl" class:ctrl--active={mode === 'top'}   onclick={() => setMode('top')}   title="Top drawer">T</button>
      <button class="ctrl" class:ctrl--active={pinned}           onclick={() => { pinned = !pinned; }} title="Pin open">PIN</button>
      <button class="ctrl ctrl--close" onclick={close} title="Collapse">▸</button>
    </div>

    <!-- Multi-panel body — each active tab gets flex: 1 of the available height -->
    <div class="panels">
      {#each $streamDrawerActiveTabs as tab (tab)}
        <div class="panel" style="--tab-color: {TAB_COLOR[tab]}">

          <!-- Panel mini-header -->
          <div class="panel-head">
            <span class="panel-dot" style="background: {TAB_COLOR[tab]}"></span>
            <span class="panel-label">{TAB_FULL[tab]}</span>
            <button class="panel-close" onclick={() => deactivateTab(tab)} title="Close panel" aria-label="Close {TAB_FULL[tab]}">✕</button>
          </div>

          <!-- Panel content -->
          <div class="panel-body">
            {#if tab === 'stream'}
              <!-- Filters -->
              <div class="filters">
                {#if knownBuildIds.length > 0}
                  <div class="filter-row">
                    <span class="filter-label">BUILD</span>
                    <div class="chips">
                      {#each knownBuildIds as id (id)}
                        <button
                          class="chip"
                          class:chip--off={buildFilter[id] === false}
                          style="color: {buildColor(id)}; border-color: {buildFilter[id] === false ? 'var(--la-hair-faint, #1c2028)' : buildColor(id) + '55'}"
                          onclick={() => toggleBuild(id)}
                          title={buildLabel(id)}
                        >{buildLabel(id)}</button>
                      {/each}
                    </div>
                  </div>
                {/if}
                <div class="filter-row">
                  <span class="filter-label">AGENT</span>
                  <div class="chips">
                    {#each knownAgents as code (code)}
                      <button
                        class="chip"
                        class:chip--off={agentFilter[code] === false}
                        style="color: {agentColor(code)}; border-color: {agentFilter[code] === false ? 'var(--la-hair-faint, #1c2028)' : agentColor(code) + '44'}"
                        onclick={() => toggleAgent(code)}
                      >{code}</button>
                    {/each}
                  </div>
                  <input class="search" type="text" placeholder="search…" bind:value={searchQ}
                    oninput={() => { searchQ = searchQ.toLowerCase(); }}
                    aria-label="Filter stream output" />
                </div>
              </div>
              <!-- Stream log -->
              <div class="stream" bind:this={streamEl} onscroll={onScroll} role="log" aria-live="polite" aria-label="Agent output">
                {#each filtered as msg (msg.id)}
                  {@const code = normalizeAgent(msg.agent)}
                  <div class="row" style="border-left-color: {agentColor(code)}55">
                    <span class="row-ts">{formatTs(msg.ts)}</span>
                    {#if msg.dispatchId}
                      <span class="row-bld" style="color: {buildColor(msg.dispatchId)}">{buildLabel(msg.dispatchId)}</span>
                    {/if}
                    <span class="row-ag" style="color: {agentColor(code)}">{code}</span>
                    <span class="row-bar" aria-hidden="true">│</span>
                    <span class="row-tx">{msg.text}</span>
                  </div>
                {/each}
                {#if filtered.length === 0}
                  <div class="empty">{$mailboxMessages.length === 0 ? 'Waiting for agent output…' : 'No messages match filters'}</div>
                {/if}
              </div>
              <!-- Footer -->
              <div class="footer">
                <span class="foot-item foot-item--live" class:foot-item--idle={activeCount === 0}>{activeCount > 0 ? '● LIVE' : '○ IDLE'}</span>
                <span class="foot-item">{filtered.length} / {$mailboxMessages.length} ROWS</span>
                <span class="foot-spacer"></span>
                <span class="foot-item" class:foot-item--warn={!autoscroll}>AUTO-SCROLL {autoscroll ? '✓' : '✗'}</span>
              </div>

            {:else if tab === 'events'}
              <div class="panel-stub">
                <span class="stub-icon">⚡</span>
                <span class="stub-label">EVENTS FEED</span>
                <span class="stub-hint">Live activity, AYIN spans, gate verdicts</span>
                <span class="stub-note">Content integration in progress</span>
              </div>

            {:else if tab === 'memory'}
              <div class="panel-stub">
                <span class="stub-icon">◈</span>
                <span class="stub-label">MEMORY</span>
                <span class="stub-hint">Hot · Cold · Convergences</span>
                <span class="stub-note">Content integration in progress</span>
              </div>

            {:else if tab === '3d'}
              <div class="helix-wrap">
                <Helix3D />
              </div>
            {/if}
          </div>

        </div>
      {/each}

      {#if $streamDrawerActiveTabs.length === 0}
        <div class="no-panels">
          <span class="no-panels-hint">Select a panel above</span>
        </div>
      {/if}
    </div>
  </div>
{/if}

<!-- ── TOP drawer ──────────────────────────────────────── -->
{#if $streamDrawerMode === 'top'}
  <div
    class="drawer drawer--top"
    class:drawer--open={$streamDrawerOpen}
    style="
      top: {STATS_H}px;
      left: {$drawerWidthPx}px;
      height: {heightPx}px;
    "
    aria-label="Agent output stream"
    aria-hidden={!$streamDrawerOpen}
  >
    <!-- Resize handle — drag bottom edge to resize height -->
    <div
      class="resize-h resize-h--h"
      role="separator"
      aria-orientation="horizontal"
      aria-label="Resize stream drawer height"
      onmousedown={onResizeHMouseDown}
    ></div>

    <div class="seam seam--top" class:seam--active={activeCount > 0}></div>

    <!-- Header — shared tab rail -->
    <div class="header">
      <div class="header-title">
        <div class="live-dot" class:live-dot--idle={activeCount === 0}></div>
        {#if activeCount > 0}
          <span class="agent-count">{activeCount}</span>
        {/if}
      </div>

      <div class="tab-rail" role="group" aria-label="Panel tabs">
        {#each TAB_ORDER as tab (tab)}
          {@const active = $streamDrawerActiveTabs.includes(tab)}
          <button
            class="tab-btn"
            class:tab-btn--active={active}
            style="--tab-color: {TAB_COLOR[tab]}"
            onclick={() => toggleTab(tab)}
            title="{TAB_FULL[tab]}"
            aria-pressed={active}
          >{TAB_SHORT[tab]}</button>
        {/each}
      </div>

      <div class="agent-dots" aria-hidden="true">
        {#each knownAgents as code (code)}
          <div class="agent-dot" style="background: {agentColor(code)}" title={code}></div>
        {/each}
      </div>

      <div class="spacer"></div>
      <button class="ctrl" class:ctrl--active={mode === 'right'} onclick={() => setMode('right')} title="Right drawer">R</button>
      <button class="ctrl" class:ctrl--active={mode === 'top'}   onclick={() => setMode('top')}   title="Top drawer">T</button>
      <button class="ctrl ctrl--close" onclick={close} title="Collapse">▾</button>
    </div>

    <!-- Multi-panel body -->
    <div class="panels">
      {#each $streamDrawerActiveTabs as tab (tab)}
        <div class="panel" style="--tab-color: {TAB_COLOR[tab]}">
          <div class="panel-head">
            <span class="panel-dot" style="background: {TAB_COLOR[tab]}"></span>
            <span class="panel-label">{TAB_FULL[tab]}</span>
            <button class="panel-close" onclick={() => deactivateTab(tab)} title="Close panel" aria-label="Close {TAB_FULL[tab]}">✕</button>
          </div>
          <div class="panel-body">
            {#if tab === 'stream'}
              <div class="filters">
                {#if knownBuildIds.length > 0}
                  <div class="filter-row">
                    <span class="filter-label">BUILD</span>
                    <div class="chips">
                      {#each knownBuildIds as id (id)}
                        <button class="chip" class:chip--off={buildFilter[id] === false}
                          style="color: {buildColor(id)}; border-color: {buildFilter[id] === false ? 'var(--la-hair-faint, #1c2028)' : buildColor(id) + '55'}"
                          onclick={() => toggleBuild(id)}>{buildLabel(id)}</button>
                      {/each}
                    </div>
                  </div>
                {/if}
                <div class="filter-row">
                  <span class="filter-label">AGENT</span>
                  <div class="chips">
                    {#each knownAgents as code (code)}
                      <button class="chip" class:chip--off={agentFilter[code] === false}
                        style="color: {agentColor(code)}; border-color: {agentFilter[code] === false ? 'var(--la-hair-faint, #1c2028)' : agentColor(code) + '44'}"
                        onclick={() => toggleAgent(code)}>{code}</button>
                    {/each}
                  </div>
                  <input class="search" type="text" placeholder="search…" bind:value={searchQ}
                    oninput={() => { searchQ = searchQ.toLowerCase(); }} aria-label="Filter stream output" />
                </div>
              </div>
              <div class="stream" bind:this={streamEl} onscroll={onScroll} role="log" aria-live="polite" aria-label="Agent output">
                {#each filtered as msg (msg.id)}
                  {@const code = normalizeAgent(msg.agent)}
                  <div class="row" style="border-left-color: {agentColor(code)}55">
                    <span class="row-ts">{formatTs(msg.ts)}</span>
                    {#if msg.dispatchId}<span class="row-bld" style="color: {buildColor(msg.dispatchId)}">{buildLabel(msg.dispatchId)}</span>{/if}
                    <span class="row-ag" style="color: {agentColor(code)}">{code}</span>
                    <span class="row-bar" aria-hidden="true">│</span>
                    <span class="row-tx">{msg.text}</span>
                  </div>
                {/each}
                {#if filtered.length === 0}
                  <div class="empty">{$mailboxMessages.length === 0 ? 'Waiting for agent output…' : 'No messages match filters'}</div>
                {/if}
              </div>
              <div class="footer">
                <span class="foot-item foot-item--live" class:foot-item--idle={activeCount === 0}>{activeCount > 0 ? '● LIVE' : '○ IDLE'}</span>
                <span class="foot-item">{filtered.length} / {$mailboxMessages.length} ROWS</span>
                <span class="foot-spacer"></span>
                <span class="foot-item" class:foot-item--warn={!autoscroll}>AUTO-SCROLL {autoscroll ? '✓' : '✗'}</span>
              </div>
            {:else if tab === 'events'}
              <div class="panel-stub">
                <span class="stub-icon">⚡</span>
                <span class="stub-label">EVENTS FEED</span>
                <span class="stub-hint">Live activity, AYIN spans, gate verdicts</span>
                <span class="stub-note">Content integration in progress</span>
              </div>
            {:else if tab === 'memory'}
              <div class="panel-stub">
                <span class="stub-icon">◈</span>
                <span class="stub-label">MEMORY</span>
                <span class="stub-hint">Hot · Cold · Convergences</span>
                <span class="stub-note">Content integration in progress</span>
              </div>
            {:else if tab === '3d'}
              <div class="helix-wrap"><Helix3D /></div>
            {/if}
          </div>
        </div>
      {/each}
      {#if $streamDrawerActiveTabs.length === 0}
        <div class="no-panels"><span class="no-panels-hint">Select a panel above</span></div>
      {/if}
    </div>
  </div>
{/if}

<style>
  /* ── Shared drawer base ────────────────────────────────── */
  .drawer {
    position: fixed;
    background: var(--la-bg-elev-1, #111214);
    display: flex;
    flex-direction: column;
    z-index: 38;
    overflow: hidden;
  }

  /* ── Right mode ────────────────────────────────────────── */
  .drawer--right {
    top: 0;
    bottom: 0;
    right: 0;
    border-left: 1px solid var(--la-hair-base, #2c3140);
    transform: translateX(100%);
    transition: transform 0.18s ease;
  }
  .drawer--right.drawer--open {
    transform: translateX(0);
  }

  /* ── Top mode ──────────────────────────────────────────── */
  .drawer--top {
    right: 0;
    border-bottom: 1px solid var(--la-hair-base, #2c3140);
    transform: translateY(-100%);
    transition: transform 0.18s ease, left 0.18s ease;
  }
  .drawer--top.drawer--open {
    transform: translateY(0);
  }

  /* ── Resize handles ────────────────────────────────────── */
  .resize-h {
    position: absolute;
    z-index: 10;
  }
  .resize-h--w {
    left: -3px;
    top: 0;
    bottom: 0;
    width: 6px;
    cursor: col-resize;
  }
  .resize-h--h {
    bottom: -3px;
    left: 0;
    right: 0;
    height: 6px;
    cursor: row-resize;
  }
  .resize-h--w::after {
    content: '';
    position: absolute;
    left: 2px;
    top: 0;
    bottom: 0;
    width: 1px;
    background: var(--la-hair-base, #2c3140);
    transition: background 80ms;
  }
  .resize-h--h::after {
    content: '';
    position: absolute;
    left: 0;
    right: 0;
    top: 2px;
    height: 1px;
    background: var(--la-hair-base, #2c3140);
    transition: background 80ms;
  }
  .resize-h--w:hover::after,
  .resize-h--h:hover::after {
    background: var(--la-focus-ring, #FFD700);
  }

  /* ── Gold seam ─────────────────────────────────────────── */
  .seam {
    height: 1px;
    flex-shrink: 0;
    background: linear-gradient(90deg, transparent, var(--la-focus-ring, #FFD700) 30%, var(--la-focus-ring, #FFD700) 70%, transparent);
    opacity: 0.25;
    transition: opacity 200ms;
  }
  .seam--top {
    height: 1px;
    background: linear-gradient(90deg, transparent, var(--la-focus-ring, #FFD700) 30%, var(--la-focus-ring, #FFD700) 70%, transparent);
  }
  .seam--active {
    opacity: 0.8;
    animation: seam-pulse 2s ease-in-out infinite;
  }
  @keyframes seam-pulse {
    0%, 100% { opacity: 0.5; }
    50%       { opacity: 1;   }
  }

  /* ── Header ────────────────────────────────────────────── */
  .header {
    flex-shrink: 0;
    height: 32px;
    display: flex;
    align-items: center;
    gap: 0;
    border-bottom: 1px solid var(--la-hair-faint, #1c2028);
    background: var(--la-bg-void, #08090a);
  }

  .header-title {
    display: flex;
    align-items: center;
    gap: 5px;
    padding: 0 9px;
    height: 100%;
    border-right: 1px solid var(--la-hair-faint, #1c2028);
  }

  .label {
    font-family: var(--la-font-mono, monospace);
    font-size: 7.5px;
    font-weight: 700;
    letter-spacing: 0.14em;
    color: var(--la-text-mute, #6e7681);
    text-transform: uppercase;
  }

  .agent-count {
    font-family: var(--la-font-mono, monospace);
    font-size: 7px;
    font-weight: 700;
    letter-spacing: 0.08em;
    color: var(--la-focus-ring, #FFD700);
    background: rgba(255, 215, 0, 0.1);
    padding: 0 4px;
    height: 14px;
    line-height: 14px;
  }

  .live-dot {
    width: 5px;
    height: 5px;
    border-radius: 50%;
    background: var(--la-focus-ring, #FFD700);
    flex-shrink: 0;
    animation: dot-pulse 1.8s ease-in-out infinite;
  }
  .live-dot--idle {
    background: var(--la-text-mute, #6e7681);
    animation: none;
  }
  @keyframes dot-pulse {
    0%, 100% { opacity: 0.4; }
    50%       { opacity: 1; box-shadow: 0 0 4px var(--la-focus-ring, #FFD700); }
  }

  .agent-dots {
    display: flex;
    align-items: center;
    gap: 3px;
    padding: 0 8px;
    height: 100%;
    border-right: 1px solid var(--la-hair-faint, #1c2028);
  }

  .agent-dot {
    width: 5px;
    height: 5px;
    border-radius: 50%;
    opacity: 0.85;
  }

  .spacer { flex: 1; }

  .ctrl {
    background: none;
    border: none;
    cursor: pointer;
    color: var(--la-text-mute, #6e7681);
    font-family: var(--la-font-mono, monospace);
    font-size: 7px;
    font-weight: 700;
    letter-spacing: 0.1em;
    text-transform: uppercase;
    padding: 0 6px;
    height: 100%;
    display: flex;
    align-items: center;
    transition: color 80ms;
  }
  .ctrl:hover { color: var(--la-text-base, #c9d1d9); }
  .ctrl--active { color: var(--la-focus-ring, #FFD700); }
  .ctrl--close {
    border-left: 1px solid var(--la-hair-faint, #1c2028);
  }
  .ctrl--close:hover { color: #ef4444; }

  /* ── Filters ────────────────────────────────────────────── */
  .filters {
    flex-shrink: 0;
    display: flex;
    flex-direction: column;
    border-bottom: 1px solid var(--la-hair-faint, #1c2028);
    background: var(--la-bg-void, #08090a);
  }

  .filter-row {
    height: 24px;
    display: flex;
    align-items: center;
    border-bottom: 1px solid var(--la-hair-faint, #1c2028);
  }
  .filter-row:last-child { border-bottom: none; }

  .filter-label {
    font-family: var(--la-font-mono, monospace);
    font-size: 7px;
    font-weight: 700;
    letter-spacing: 0.1em;
    color: var(--la-text-mute, #6e7681);
    text-transform: uppercase;
    padding: 0 6px;
    height: 100%;
    display: flex;
    align-items: center;
    border-right: 1px solid var(--la-hair-faint, #1c2028);
    flex-shrink: 0;
    min-width: 42px;
    opacity: 0.6;
  }

  .chips {
    display: flex;
    align-items: center;
    gap: 2px;
    padding: 0 5px;
    overflow-x: auto;
    flex: 1;
    height: 100%;
    scrollbar-width: none;
  }
  .chips::-webkit-scrollbar { display: none; }

  .chip {
    flex-shrink: 0;
    height: 13px;
    padding: 0 5px;
    font-family: var(--la-font-mono, monospace);
    font-size: 6.5px;
    font-weight: 700;
    letter-spacing: 0.1em;
    text-transform: uppercase;
    border: 1px solid;
    background: none;
    cursor: pointer;
    transition: background 80ms, opacity 80ms;
    opacity: 1;
  }
  .chip--off { opacity: 0.3; }
  .chip:hover { background: rgba(255, 255, 255, 0.04); }

  .search {
    background: none;
    border: none;
    border-left: 1px solid var(--la-hair-faint, #1c2028);
    color: var(--la-text-base, #c9d1d9);
    font-family: var(--la-font-mono, monospace);
    font-size: 8.5px;
    padding: 0 7px;
    height: 100%;
    width: 110px;
    outline: none;
    flex-shrink: 0;
  }
  .search::placeholder { color: var(--la-text-mute, #6e7681); }

  /* ── Stream area ────────────────────────────────────────── */
  .stream {
    flex: 1;
    overflow-y: auto;
    overflow-x: hidden;
    background: var(--la-bg-void, #08090a);
    background-image: repeating-linear-gradient(
      0deg, transparent 0px, transparent 3px,
      rgba(255, 255, 255, 0.007) 3px, rgba(255, 255, 255, 0.007) 4px
    );
    scrollbar-width: thin;
    scrollbar-color: var(--la-hair-base, #2c3140) transparent;
  }
  .stream::-webkit-scrollbar { width: 3px; }
  .stream::-webkit-scrollbar-thumb { background: var(--la-hair-base, #2c3140); }

  .row {
    display: flex;
    align-items: baseline;
    padding: 0 6px;
    min-height: 18px;
    border-left: 2px solid transparent;
    font-family: var(--la-font-mono, monospace);
    font-size: 9px;
    line-height: 1.55;
    transition: background 40ms;
  }
  .row:hover { background: rgba(255, 255, 255, 0.022); }

  .row-ts {
    color: var(--la-text-mute, #6e7681);
    margin-right: 5px;
    font-size: 8px;
    flex-shrink: 0;
    min-width: 52px;
    opacity: 0.6;
  }

  .row-bld {
    font-size: 7px;
    font-weight: 700;
    letter-spacing: 0.06em;
    margin-right: 5px;
    flex-shrink: 0;
    background: var(--la-bg-elev-1, #111214);
    border: 1px solid var(--la-hair-faint, #1c2028);
    padding: 0 3px;
  }

  .row-ag {
    font-weight: 700;
    font-size: 7.5px;
    letter-spacing: 0.08em;
    margin-right: 5px;
    flex-shrink: 0;
    min-width: 24px;
  }

  .row-bar {
    color: var(--la-hair-base, #2c3140);
    margin-right: 5px;
    flex-shrink: 0;
  }

  .row-tx {
    color: var(--la-text-base, #c9d1d9);
    white-space: pre-wrap;
    word-break: break-all;
    flex: 1;
    opacity: 0.85;
  }

  .empty {
    padding: 24px 12px;
    font-family: var(--la-font-mono, monospace);
    font-size: 9px;
    color: var(--la-text-mute, #6e7681);
    text-align: center;
    letter-spacing: 0.08em;
  }

  /* ── Footer ─────────────────────────────────────────────── */
  .footer {
    flex-shrink: 0;
    height: 17px;
    display: flex;
    align-items: center;
    border-top: 1px solid var(--la-hair-faint, #1c2028);
    background: var(--la-bg-elev-1, #111214);
    padding: 0 4px;
    gap: 0;
  }

  .foot-item {
    font-family: var(--la-font-mono, monospace);
    font-size: 7px;
    letter-spacing: 0.08em;
    color: var(--la-text-mute, #6e7681);
    text-transform: uppercase;
    padding: 0 5px;
    border-right: 1px solid var(--la-hair-faint, #1c2028);
    height: 100%;
    display: flex;
    align-items: center;
  }
  .foot-item:last-child { border-right: none; }
  .foot-item--live { color: #10B981; }
  .foot-item--idle { color: var(--la-text-mute, #6e7681); }
  .foot-item--warn { color: #f59e0b; }

  .foot-spacer { flex: 1; }

  /* ── Tab rail ───────────────────────────────────────────── */
  .tab-rail {
    display: flex;
    align-items: stretch;
    height: 100%;
    border-right: 1px solid var(--la-hair-faint, #1c2028);
  }

  .tab-btn {
    position: relative;
    background: none;
    border: none;
    border-right: 1px solid var(--la-hair-faint, #1c2028);
    cursor: pointer;
    color: var(--la-text-mute, #6e7681);
    font-family: var(--la-font-mono, monospace);
    font-size: 6.5px;
    font-weight: 700;
    letter-spacing: 0.12em;
    padding: 0 7px;
    height: 100%;
    display: flex;
    align-items: center;
    transition: color 80ms, background 80ms;
  }
  .tab-btn:last-child { border-right: none; }
  .tab-btn:hover { color: var(--la-text-base, #c9d1d9); background: rgba(255,255,255,0.025); }
  .tab-btn--active { color: var(--tab-color, #FFD700); }
  .tab-btn--active::after {
    content: '';
    position: absolute;
    bottom: 0;
    left: 4px;
    right: 4px;
    height: 1.5px;
    background: var(--tab-color, #FFD700);
    border-radius: 1px;
  }

  /* ── Multi-panel container ──────────────────────────────── */
  .panels {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  .panel {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    overflow: hidden;
    border-top: 1px solid var(--tab-color, #2c3140);
    border-top-color: color-mix(in srgb, var(--tab-color, #2c3140) 30%, transparent);
  }
  .panel:first-child { border-top: none; }

  .panel-head {
    flex-shrink: 0;
    height: 20px;
    display: flex;
    align-items: center;
    gap: 5px;
    padding: 0 7px;
    background: color-mix(in srgb, var(--tab-color, #111214) 6%, var(--la-bg-void, #08090a));
    border-bottom: 1px solid var(--la-hair-faint, #1c2028);
  }

  .panel-dot {
    width: 4px;
    height: 4px;
    border-radius: 50%;
    flex-shrink: 0;
    opacity: 0.8;
  }

  .panel-label {
    flex: 1;
    font-family: var(--la-font-mono, monospace);
    font-size: 6.5px;
    font-weight: 700;
    letter-spacing: 0.14em;
    color: var(--la-text-mute, #6e7681);
    text-transform: uppercase;
  }

  .panel-close {
    background: none;
    border: none;
    cursor: pointer;
    color: var(--la-text-mute, #6e7681);
    font-size: 7px;
    padding: 0 2px;
    line-height: 1;
    transition: color 80ms;
    opacity: 0.5;
  }
  .panel-close:hover { color: #ef4444; opacity: 1; }

  .panel-body {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  /* ── Stub panels (EVT / MEM pending integration) ────────── */
  .panel-stub {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 5px;
    background: var(--la-bg-void, #08090a);
    background-image: repeating-linear-gradient(
      0deg, transparent 0px, transparent 3px,
      rgba(255,255,255,0.006) 3px, rgba(255,255,255,0.006) 4px
    );
  }

  .stub-icon {
    font-size: 18px;
    opacity: 0.15;
    margin-bottom: 4px;
  }

  .stub-label {
    font-family: var(--la-font-mono, monospace);
    font-size: 8px;
    font-weight: 700;
    letter-spacing: 0.16em;
    color: var(--la-text-mute, #6e7681);
    text-transform: uppercase;
  }

  .stub-hint {
    font-family: var(--la-font-mono, monospace);
    font-size: 7px;
    color: var(--la-text-dim, #3e4551);
    letter-spacing: 0.06em;
  }

  .stub-note {
    font-family: var(--la-font-mono, monospace);
    font-size: 6.5px;
    color: var(--la-text-dim, #3e4551);
    letter-spacing: 0.04em;
    opacity: 0.6;
    font-style: italic;
  }

  /* ── Helix 3D inline wrapper ────────────────────────────── */
  .helix-wrap {
    flex: 1;
    min-height: 0;
    overflow: hidden;
    display: flex;
    flex-direction: column;
  }
  .helix-wrap :global(canvas) { flex: 1; min-height: 0; }

  /* ── Empty state (no tabs selected) ────────────────────── */
  .no-panels {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    background: var(--la-bg-void, #08090a);
  }
  .no-panels-hint {
    font-family: var(--la-font-mono, monospace);
    font-size: 8px;
    color: var(--la-text-dim, #3e4551);
    letter-spacing: 0.1em;
  }
</style>
