<script lang="ts">
  import { scope } from '$lib/cockpit/stores/scope';
  import type { RouteScope } from '$lib/cockpit/stores/scope';
  import CockpitShell from '$lib/cockpit/shell/CockpitShell.svelte';
  import { authHeaders } from '$lib/auth';

  // d3 — /cockpit/file/:codename/:filePath*
  const currentScope = $derived($scope as Extract<RouteScope, { kind: 'file' }> | null);
  const filePath = $derived(currentScope?.file_path ?? '');
  const codename = $derived(currentScope?.codename ?? '');
  const fileName = $derived(filePath.split('/').pop() ?? filePath);
  const fileDir  = $derived(filePath.includes('/') ? filePath.slice(0, filePath.lastIndexOf('/') + 1) : '');

  // ── Tab + panel state ──────────────────────────────────────────────────────
  type Tab = 'raw' | 'diff' | 'blame';
  type PanelState = 'hidden' | 'half' | 'full';
  let activeTab    = $state<Tab>('raw');
  let panelState   = $state<PanelState>('half');
  let snapping     = $state(false);
  let cmdOpen      = $state(false);
  let cmdQuery     = $state('');

  // ── File content ───────────────────────────────────────────────────────────
  let fileLines    = $state<string[]>([]);
  let loading      = $state(false);
  let loadError    = $state('');

  type BlameLine = { sha: string; author: string; date: string; text: string };
  let blameLines   = $state<BlameLine[]>([]);
  let blameLoading = $state(false);

  // ── Scroll / minimap ──────────────────────────────────────────────────────
  let codeBodyEl      = $state<HTMLElement | null>(null);
  let minimapEl       = $state<HTMLElement | null>(null);
  let viewportTop     = $state(0);
  let viewportHeight  = $state(40);
  let highlightedLine = $state<number | null>(null);

  // ── Resize drag ───────────────────────────────────────────────────────────
  let leftEl       = $state<HTMLElement | null>(null);
  let isResizing   = $state(false);
  let leftWidthPct = $state(52);
  let dragStartX   = 0;
  let dragStartW   = 0;

  // ── Symbol extraction (regex, no LSP) ────────────────────────────────────
  type SymKind = 'fn' | 'struct' | 'trait' | 'enum' | 'impl' | 'mod';
  interface Sym { kind: SymKind; name: string; line: number }

  const KIND_COLOR: Record<SymKind, string> = {
    fn:     '#4d8eff',
    struct: '#39ff8a',
    trait:  '#ffad2e',
    enum:   '#ff5c8a',
    impl:   '#b478ff',
    mod:    '#c8cfff',
  };

  const PATTERNS: [SymKind, RegExp][] = [
    ['fn',     /^(?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?fn\s+(\w+)/],
    ['struct', /^(?:pub(?:\([^)]*\))?\s+)?struct\s+(\w+)/],
    ['trait',  /^(?:pub(?:\([^)]*\))?\s+)?trait\s+(\w+)/],
    ['enum',   /^(?:pub(?:\([^)]*\))?\s+)?enum\s+(\w+)/],
    ['impl',   /^impl(?:<[^>]*>)?\s+(?:\w+::)*(\w+)/],
    ['mod',    /^(?:pub(?:\([^)]*\))?\s+)?mod\s+(\w+)/],
  ];

  const symbols = $derived.by((): Sym[] => {
    const out: Sym[] = [];
    for (let i = 0; i < fileLines.length; i++) {
      const line = fileLines[i].trim();
      for (const [kind, re] of PATTERNS) {
        const m = line.match(re);
        if (m) { out.push({ kind, name: m[1], line: i + 1 }); break; }
      }
    }
    return out.slice(0, 40);
  });

  const symCounts = $derived({
    fn:     symbols.filter(s => s.kind === 'fn').length,
    struct: symbols.filter(s => s.kind === 'struct').length,
    trait:  symbols.filter(s => s.kind === 'trait').length,
    enum:   symbols.filter(s => s.kind === 'enum').length,
  });

  // ── Data loading ──────────────────────────────────────────────────────────
  async function loadRaw() {
    if (!filePath) return;
    loading = true; loadError = '';
    try {
      const res = await fetch(`/api/code/read?${new URLSearchParams({ path: filePath })}`, { headers: authHeaders() });
      if (!res.ok) throw new Error(`${res.status} ${res.statusText}`);
      const data = await res.json() as { content: string };
      fileLines = data.content.split('\n');
    } catch (e) { loadError = e instanceof Error ? e.message : 'load failed'; }
    finally { loading = false; }
  }

  async function loadBlame() {
    if (!filePath || blameLines.length || blameLoading) return;
    blameLoading = true;
    try {
      const res = await fetch(`/api/git/blame?${new URLSearchParams({ path: filePath })}`, { headers: authHeaders() });
      if (!res.ok) return;
      const data = await res.json() as { lines: BlameLine[] };
      blameLines = data.lines ?? [];
    } catch { /* non-fatal */ }
    finally { blameLoading = false; }
  }

  $effect(() => { if (filePath) { fileLines = []; blameLines = []; loadRaw(); } });
  $effect(() => { if (activeTab === 'blame') loadBlame(); });

  // ── Minimap sync ──────────────────────────────────────────────────────────
  function onCodeScroll(e: Event) {
    const el = e.target as HTMLElement;
    if (!el.scrollHeight || el.scrollHeight <= el.clientHeight) return;
    const ratio = el.scrollTop / (el.scrollHeight - el.clientHeight);
    viewportHeight = (el.clientHeight / el.scrollHeight) * 100;
    viewportTop    = ratio * (100 - viewportHeight);
  }

  function onMinimapClick(e: MouseEvent) {
    if (!minimapEl || !codeBodyEl) return;
    const rect = minimapEl.getBoundingClientRect();
    const ratio = (e.clientY - rect.top) / rect.height;
    codeBodyEl.scrollTop = ratio * (codeBodyEl.scrollHeight - codeBodyEl.clientHeight);
  }

  function jumpToLine(line: number) {
    highlightedLine = line;
    if (!codeBodyEl) return;
    const el = codeBodyEl.querySelector<HTMLElement>(`[data-line="${line}"]`);
    el?.scrollIntoView({ block: 'center' });
  }

  // ── Panel window controls ─────────────────────────────────────────────────
  function setPanelState(s: PanelState) {
    snapping = true;
    panelState = s;
    setTimeout(() => { snapping = false; }, 250);
  }

  // ── Command bar ───────────────────────────────────────────────────────────
  function toggleCmd() {
    cmdOpen = !cmdOpen;
    if (cmdOpen) setTimeout(() => document.querySelector<HTMLInputElement>('.cp-cmd-input')?.focus(), 50);
  }

  function onCmdKey(e: KeyboardEvent) {
    if (e.key === 'Escape') { cmdOpen = false; cmdQuery = ''; }
  }

  // ── Drag resize ───────────────────────────────────────────────────────────
  function onResizeStart(e: MouseEvent | TouchEvent) {
    isResizing = true;
    dragStartX = 'touches' in e ? e.touches[0].clientX : e.clientX;
    dragStartW = leftWidthPct;

    function onMove(ev: MouseEvent | TouchEvent) {
      const x = 'touches' in ev ? ev.touches[0].clientX : ev.clientX;
      const total = leftEl?.parentElement?.clientWidth ?? 1;
      leftWidthPct = Math.min(80, Math.max(20, dragStartW + ((x - dragStartX) / total) * 100));
    }
    function onUp() {
      isResizing = false;
      window.removeEventListener('mousemove', onMove);
      window.removeEventListener('mouseup', onUp);
      window.removeEventListener('touchmove', onMove as EventListener);
      window.removeEventListener('touchend', onUp);
    }
    window.addEventListener('mousemove', onMove);
    window.addEventListener('mouseup', onUp);
    window.addEventListener('touchmove', onMove as EventListener);
    window.addEventListener('touchend', onUp);
  }

  function onGlobalKey(e: KeyboardEvent) {
    if ((e.metaKey || e.ctrlKey) && e.key === 'k') { e.preventDefault(); toggleCmd(); }
  }

  // Left panel inline style — responds to panel state + drag
  const leftStyle = $derived(
    panelState === 'full'   ? 'flex: 0 0 0%; min-width:0; overflow:hidden;' :
    panelState === 'hidden' ? 'flex: 1;' :
    `flex: 0 0 ${leftWidthPct}%;`
  );

  const panelFlex = $derived(
    panelState === 'hidden' ? 'flex: 0 0 0%; opacity:0; overflow:hidden;' :
    panelState === 'full'   ? 'flex: 1; opacity:1;' :
    `flex: 0 0 ${100 - leftWidthPct}%; opacity:1;`
  );
</script>

<svelte:window onkeydown={onGlobalKey} />

<CockpitShell>
  <div class="cockpit-body" class:is-resizing={isResizing}>

    <!-- ── Left panel: d3 bento ────────────────────────────────────── -->
    <div class="cockpit-left" bind:this={leftEl} style={leftStyle}>
      <div class="bento-d3">

        <!-- Symbol browser portal -->
        <div class="card card-portal scoped" data-card-role="d3-portal">
          <div class="card-hdr">
            <span class="card-lbl">SYMBOLS</span>
            <span class="portal-kind">FILE SCOPE</span>
          </div>
          <div class="card-body">
            {#if loading}
              <div class="empty-state">loading symbols…</div>
            {:else if symbols.length === 0 && !loadError}
              <div class="empty-state">{filePath ? 'no symbols detected' : 'no file selected'}</div>
            {:else if loadError}
              <div class="empty-state err">{loadError}</div>
            {:else}
              <div class="portal-list">
                {#each symbols as sym}
                  <button
                    class="portal-item"
                    class:item-hl={highlightedLine === sym.line}
                    onclick={() => jumpToLine(sym.line)}
                    title="Jump to line {sym.line}"
                  >
                    <span class="pi-dot active"></span>
                    <span class="sym-kind-pill" style="background: {KIND_COLOR[sym.kind]}22; color: {KIND_COLOR[sym.kind]}; border: 1px solid {KIND_COLOR[sym.kind]}44;">{sym.kind}</span>
                    <span class="pi-info">
                      <span class="pi-name">{sym.name}</span>
                      <span class="pi-meta">:{sym.line}</span>
                    </span>
                    <span class="pi-drill" aria-hidden="true">→</span>
                  </button>
                {/each}
              </div>
            {/if}
          </div>
        </div>

        <!-- File metrics strat card -->
        <div class="card card-strat" data-card-role="d3-strat">
          <div class="card-hdr"><span class="card-lbl">FILE METRICS</span></div>
          <div class="card-body">
            {#if fileLines.length > 0}
              <div class="metric-row"><span class="m-key">lines</span><span class="m-val">{fileLines.length}</span></div>
              <div class="metric-row"><span class="m-key">symbols</span><span class="m-val acc">{symbols.length}</span></div>
              <div class="metric-row"><span class="m-key">fn</span><span class="m-val" style="color:var(--sym-fn)">{symCounts.fn}</span></div>
              <div class="metric-row"><span class="m-key">struct</span><span class="m-val" style="color:var(--sym-struct)">{symCounts.struct}</span></div>
              <div class="metric-row"><span class="m-key">trait</span><span class="m-val" style="color:var(--sym-trait)">{symCounts.trait}</span></div>
              <div class="metric-row"><span class="m-key">enum</span><span class="m-val" style="color:var(--sym-enum)">{symCounts.enum}</span></div>
            {:else}
              <div class="empty-state">{loading ? 'loading…' : '—'}</div>
            {/if}
          </div>
        </div>

        <!-- Domain gate context card -->
        <div class="card card-context" data-card-role="d3-context">
          <div class="card-hdr"><span class="card-lbl">DOMAIN GATES</span></div>
          <div class="card-body">
            {#each (['A','S','Q','T','P','D','K','O'] as const) as gate}
              <div class="scan-row">
                <span class="gate-badge">[{gate}]</span>
                <span class="gate-status dim">awaiting</span>
              </div>
            {/each}
          </div>
        </div>

      </div><!-- /bento-d3 -->
    </div><!-- /cockpit-left -->

    <!-- ── Resize handle ─────────────────────────────────────────────── -->
    <div
      class="cp-resize-bar"
      class:dragging={isResizing}
      class:panel-hidden={panelState === 'hidden'}
      role="separator"
      aria-orientation="vertical"
      aria-label="Resize code panel"
      onmousedown={onResizeStart}
      ontouchstart={onResizeStart}
    ></div>

    <!-- ── Restore pill (panel minimized) ───────────────────────────── -->
    {#if panelState === 'hidden'}
      <button class="cp-minimized-pill" onclick={() => setPanelState('half')}>◈ CODE</button>
    {/if}

    <!-- ── Code panel ────────────────────────────────────────────────── -->
    <div
      class="code-panel"
      class:cp-snapping={snapping}
      style={panelFlex}
    >
      <!-- Header bar -->
      <div class="cp-hdr">
        <div class="cp-file-info">
          <span class="cp-file-icon">◈</span>
          <span class="cp-file-path" title={filePath}>{fileDir}<b>{fileName}</b></span>
        </div>
        <div class="cp-tabs">
          {#each (['raw', 'diff', 'blame'] as Tab[]) as tab}
            <button class="cp-tab" class:active={activeTab === tab} onclick={() => { activeTab = tab; }}>{tab.toUpperCase()}</button>
          {/each}
        </div>
        <div class="cp-status">
          <button class="cp-cmd-trigger" class:active={cmdOpen} onclick={toggleCmd}>⌘K</button>
          <div class="cp-live"><div class="cp-live-dot"></div>LIVE</div>
        </div>
        <div class="cp-wc-group">
          <button class="cp-wc" class:active={panelState === 'hidden'} title="Minimize" onclick={() => setPanelState('hidden')}>⇐</button>
          <button class="cp-wc" class:active={panelState === 'half'}   title="Half screen" onclick={() => setPanelState('half')}>⊟</button>
          <button class="cp-wc" class:active={panelState === 'full'}   title="Maximize" onclick={() => setPanelState('full')}>⊡</button>
        </div>
      </div>

      <!-- Command bar (⌘K) -->
      {#if cmdOpen}
        <div class="cp-cmd-bar">
          <span class="cp-cmd-mode m-idle">⌘K</span>
          <input
            class="cp-cmd-input"
            type="text"
            spellcheck="false"
            autocomplete="off"
            placeholder="/ regex  ·  : line  ·  ? fuzzy"
            bind:value={cmdQuery}
            onkeydown={onCmdKey}
          />
          <button class="cp-cmd-close" onclick={() => { cmdOpen = false; cmdQuery = ''; }}>ESC</button>
        </div>
      {/if}

      <!-- Body row: code + minimap -->
      <div class="cp-body-row">
        <div class="cp-body" bind:this={codeBodyEl} onscroll={onCodeScroll}>
          {#if loading}
            <div class="cp-loading">loading {fileName}…</div>
          {:else if loadError}
            <div class="cp-error">{loadError}</div>
          {:else if activeTab === 'blame' && blameLines.length > 0}
            {#each blameLines as bl, i}
              <div class="cp-line bl-line" class:hl={highlightedLine === i + 1} data-line={i + 1}>
                <span class="cp-ln">{i + 1}</span>
                <span class="bl-sha">{bl.sha.slice(0, 7)}</span>
                <span class="bl-msg">{bl.author}</span>
                <span class="cp-src">{bl.text}</span>
              </div>
            {/each}
          {:else if activeTab === 'blame' && blameLoading}
            <div class="cp-loading">loading blame…</div>
          {:else}
            {#each fileLines as line, i}
              <div class="cp-line raw-line" class:hl={highlightedLine === i + 1} data-line={i + 1}>
                <span class="cp-ln">{i + 1}</span>
                <span class="cp-mark"></span>
                <span class="cp-src">{line}</span>
              </div>
            {/each}
          {/if}
        </div>

        <!-- Minimap -->
        <!-- WHY: click-to-jump uses ratio of click Y within minimap element to total scrollable height -->
        <div
          class="cp-minimap"
          bind:this={minimapEl}
          onclick={onMinimapClick}
          role="scrollbar"
          aria-label="Minimap — click to jump"
          aria-valuenow={viewportTop}
          aria-valuemin={0}
          aria-valuemax={100}
        >
          <div class="cp-mm-content">
            {#each fileLines as _, i}
              <div class="cp-mm-row context" class:hl={highlightedLine === i + 1}></div>
            {/each}
          </div>
          <div class="cp-mm-viewport" style="top: {viewportTop}%; height: {viewportHeight}%"></div>
        </div>
      </div>

      <!-- Status footer -->
      <div class="cp-footer">
        <span class="cp-ft"><span class="v acc">{codename || '—'}</span></span>
        <span class="cp-ft"><span class="v">{fileLines.length} lines</span></span>
        <span class="cp-ft"><span class="v">{symbols.length} symbols</span></span>
        {#if highlightedLine}<span class="cp-ft"><span class="acc">:{highlightedLine}</span></span>{/if}
        <span class="cp-ft-spacer"></span>
        <span class="cp-ft"><span class="v">{activeTab.toUpperCase()}</span></span>
      </div>
    </div><!-- /code-panel -->

  </div><!-- /cockpit-body -->
</CockpitShell>

<style>
  /* ── Symbol color tokens ──────────────────────────────────────── */
  :global(.cockpit-body) {
    --sym-fn:     #4d8eff;
    --sym-struct: #39ff8a;
    --sym-trait:  #ffad2e;
    --sym-enum:   #ff5c8a;
    --sym-impl:   #b478ff;
    --sym-mod:    #c8cfff;
  }

  /* ── Layout ───────────────────────────────────────────────────── */
  .cockpit-body {
    display: flex;
    flex-direction: row;
    height: 100%;
    overflow: hidden;
  }
  .cockpit-body.is-resizing { user-select: none; cursor: col-resize; }

  .cockpit-left {
    min-width: 0;
    overflow: hidden;
    display: flex;
    flex-direction: column;
    padding: 8px;
    transition: flex var(--depth-shift, 0.4s);
  }

  /* ── D3 bento grid ────────────────────────────────────────────── */
  .bento-d3 {
    display: grid;
    height: 100%;
    grid-template-columns: 1fr;
    grid-template-rows: 1fr auto auto;
    grid-template-areas: "portal" "strat" "context";
    gap: 8px;
  }

  .card {
    background: var(--la-bg-panel);
    border: 1px solid var(--la-hair-base);
    display: flex;
    flex-direction: column;
    overflow: hidden;
    position: relative;
    transition: border-color var(--la-fast, 0.12s);
  }
  .card-portal  { grid-area: portal; }
  .card-strat   { grid-area: strat; }
  .card-context { grid-area: context; }

  /* Accent left-edge stripe on scoped portal */
  .card.scoped {
    border-color: color-mix(in srgb, var(--scope-accent, #4d8eff) 35%, var(--la-hair-base));
  }
  .card.scoped::before {
    content: '';
    position: absolute;
    left: 0; top: 0; bottom: 0; width: 2px;
    background: var(--scope-accent, #4d8eff);
  }

  .card-hdr {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 8px 10px 0;
    flex-shrink: 0;
  }
  .card-lbl {
    font-size: 8px; font-weight: 700;
    letter-spacing: 0.08em;
    color: var(--la-text-mute);
    text-transform: uppercase;
    flex: 1;
    transition: color var(--depth-shift, 0.4s);
  }
  .card.scoped .card-lbl { color: var(--scope-accent, #4d8eff); }

  .card-body { flex: 1; overflow-y: auto; padding: 6px 10px 8px; }
  .card-body::-webkit-scrollbar { width: 3px; }
  .card-body::-webkit-scrollbar-thumb { background: var(--la-hair-base); }

  .portal-kind {
    font-size: 8px; letter-spacing: 0.06em;
    color: color-mix(in srgb, var(--scope-accent, #4d8eff) 70%, transparent);
    background: var(--acc-tint, rgba(77,142,255,0.07));
    padding: 2px 6px;
    border: 1px solid color-mix(in srgb, var(--scope-accent, #4d8eff) 25%, transparent);
    flex-shrink: 0;
  }

  .empty-state { color: var(--la-text-mute); font-size: 10px; }
  .empty-state.err { color: var(--la-semantic-error, #ff4d4d); }

  /* ── Portal item list ─────────────────────────────────────────── */
  .portal-list { display: flex; flex-direction: column; gap: 3px; }

  .portal-item {
    display: flex;
    align-items: center;
    gap: 7px;
    padding: 5px 8px;
    cursor: pointer;
    border: 1px solid transparent;
    background: none;
    text-align: left;
    width: 100%;
    transition: background var(--la-fast, 0.12s), border-color var(--la-fast, 0.12s);
    position: relative;
  }
  .portal-item:hover {
    background: var(--acc-tint, rgba(77,142,255,0.07));
    border-color: color-mix(in srgb, var(--scope-accent, #4d8eff) 20%, transparent);
  }
  .portal-item.item-hl {
    background: var(--acc-tint, rgba(77,142,255,0.07));
    border-color: color-mix(in srgb, var(--scope-accent, #4d8eff) 30%, transparent);
  }

  .pi-dot {
    width: 5px; height: 5px; border-radius: 50%;
    flex-shrink: 0; margin-top: 1px;
  }
  .pi-dot.active {
    background: var(--scope-accent, #4d8eff);
    box-shadow: 0 0 5px var(--scope-accent, #4d8eff);
    transition: all var(--depth-shift, 0.4s);
  }

  .sym-kind-pill {
    font-size: 7px; font-weight: 700;
    letter-spacing: 0.05em;
    padding: 1px 4px;
    border-radius: 2px;
    flex-shrink: 0;
  }

  .pi-info { flex: 1; min-width: 0; }
  .pi-name { font-size: 10px; font-weight: 500; color: var(--la-text-bright); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .pi-meta { font-size: 8px; color: var(--la-text-mute); }

  .pi-drill {
    font-size: 9px; color: var(--la-text-mute); flex-shrink: 0;
    opacity: 0; transform: translateX(-4px);
    transition: all var(--la-fast, 0.12s);
  }
  .portal-item:hover .pi-drill { opacity: 1; transform: translateX(0); color: var(--scope-accent, #4d8eff); }

  /* ── Metrics ──────────────────────────────────────────────────── */
  .metric-row { display: flex; justify-content: space-between; font-size: 9px; padding: 2px 0; }
  .m-key { color: var(--la-text-mute); }
  .m-val { color: var(--la-text-dim); font-weight: 600; }
  .m-val.acc { color: var(--scope-accent, #4d8eff); }

  /* ── Domain gate scan rows ────────────────────────────────────── */
  .scan-row { display: flex; gap: 8px; align-items: center; font-size: 9px; padding: 2px 0; }
  .gate-badge { font-size: 8px; font-weight: 700; color: var(--la-text-mute); letter-spacing: 0.05em; min-width: 22px; }
  .gate-status { color: var(--la-text-mute); }
  .gate-status.dim { opacity: 0.5; }

  /* ── Resize handle ────────────────────────────────────────────── */
  .cp-resize-bar {
    flex: 0 0 5px;
    display: flex;
    cursor: col-resize;
    background: transparent;
    position: relative;
    z-index: 20;
    align-items: center;
    justify-content: center;
  }
  .cp-resize-bar::after {
    content: '';
    position: absolute;
    top: 20%; bottom: 20%; left: 2px;
    width: 1px;
    background: var(--la-hair-base);
    transition: background 0.1s, width 0.1s, left 0.1s;
  }
  .cp-resize-bar:hover::after,
  .cp-resize-bar.dragging::after {
    background: var(--scope-accent, #4d8eff);
    width: 2px; left: 1.5px;
    transition: none;
  }
  .cp-resize-bar.panel-hidden { display: none; }

  /* ── Minimized restore pill ───────────────────────────────────── */
  .cp-minimized-pill {
    flex: 0 0 18px;
    background: #0a0c16;
    border-left: 1px solid var(--la-hair-base);
    padding: 10px 0;
    cursor: pointer;
    z-index: 30;
    writing-mode: vertical-rl;
    text-orientation: mixed;
    font-family: var(--la-font-mono, monospace);
    font-size: 8px; font-weight: 700;
    letter-spacing: 0.1em;
    color: var(--la-text-mute);
    display: flex;
    align-items: center;
    justify-content: center;
    border: none;
    transition: color 0.12s, border-color 0.12s;
  }
  .cp-minimized-pill:hover { color: var(--scope-accent, #ff5c8a); border-color: var(--scope-accent, #ff5c8a); }

  /* ── Code panel ───────────────────────────────────────────────── */
  .code-panel {
    min-width: 0;
    overflow: hidden;
    display: flex;
    flex-direction: column;
    background: #090b14;
    border-left: 1px solid var(--la-hair-base);
    transition: flex var(--depth-shift, 0.4s), opacity var(--depth-shift, 0.4s);
  }
  .code-panel.cp-snapping {
    transition: flex 0.22s cubic-bezier(0.4,0,0.2,1), opacity 0.22s ease !important;
  }

  /* ── Code panel header ────────────────────────────────────────── */
  .cp-hdr {
    display: flex; align-items: center; gap: 8px;
    padding: 0 10px; height: 34px;
    border-bottom: 1px solid var(--la-hair-base);
    background: #0a0c16; flex-shrink: 0;
  }
  .cp-file-info { display: flex; align-items: center; gap: 5px; flex: 1; min-width: 0; }
  .cp-file-icon { color: var(--scope-accent, #ff5c8a); font-size: 10px; flex-shrink: 0; transition: color var(--depth-shift, 0.4s); }
  .cp-file-path { font-size: 9px; color: var(--la-text-dim); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .cp-file-path :global(b) { color: var(--la-text-bright); font-weight: 600; }

  .cp-tabs { display: flex; flex-shrink: 0; }
  .cp-tab {
    font-family: var(--la-font-mono, monospace);
    font-size: 8px; font-weight: 700; letter-spacing: 0.08em;
    padding: 4px 8px; background: none; border: none;
    border-bottom: 2px solid transparent;
    color: var(--la-text-mute); cursor: pointer;
    transition: all 0.12s;
  }
  .cp-tab:hover { color: var(--la-text-dim); }
  .cp-tab.active { color: var(--la-text-bright); border-bottom-color: var(--scope-accent, #ff5c8a); }

  .cp-status { display: flex; align-items: center; gap: 8px; flex-shrink: 0; }
  .cp-live { display: flex; align-items: center; gap: 4px; font-size: 8px; font-weight: 700; letter-spacing: 0.08em; color: var(--la-semantic-ok, #39ff8a); }
  .cp-live-dot {
    width: 5px; height: 5px; border-radius: 50%;
    background: var(--la-semantic-ok, #39ff8a);
    animation: livePulse 2s ease-in-out infinite;
  }
  @keyframes livePulse {
    0%, 100% { opacity: 1; box-shadow: 0 0 0 0 rgba(57,255,138,0.5); }
    50%       { opacity: 0.9; box-shadow: 0 0 0 5px rgba(57,255,138,0); }
  }

  .cp-cmd-trigger {
    height: 20px; padding: 0 6px;
    border: 1px solid var(--la-hair-base); border-radius: 3px;
    background: transparent; color: var(--la-text-mute);
    font-family: var(--la-font-mono, monospace);
    font-size: 8px; font-weight: 700; letter-spacing: 0.05em;
    cursor: pointer; display: flex; align-items: center;
    transition: all 0.12s;
  }
  .cp-cmd-trigger:hover,
  .cp-cmd-trigger.active { color: var(--scope-accent, #ff5c8a); border-color: var(--scope-accent, #ff5c8a); background: var(--acc-tint, rgba(255,92,138,0.06)); }

  .cp-wc-group { display: flex; align-items: center; gap: 2px; flex-shrink: 0; margin-left: 4px; }
  .cp-wc {
    width: 20px; height: 20px;
    border: 1px solid var(--la-hair-base); border-radius: 3px;
    background: transparent; color: var(--la-text-mute);
    font-size: 8px; cursor: pointer;
    display: flex; align-items: center; justify-content: center;
    padding: 0; line-height: 1;
    transition: all 0.12s;
  }
  .cp-wc:hover { background: var(--la-hair-base); color: var(--la-text-base); }
  .cp-wc.active { background: var(--acc-tint, rgba(255,92,138,0.06)); color: var(--scope-accent, #ff5c8a); border-color: var(--scope-accent, #ff5c8a); }

  /* ── Command bar ──────────────────────────────────────────────── */
  .cp-cmd-bar {
    display: flex; flex-shrink: 0; height: 30px;
    align-items: center; gap: 0;
    background: #070a14;
    border-bottom: 1px solid var(--scope-accent, #ff5c8a);
    box-shadow: 0 4px 20px rgba(0,0,0,0.55);
  }
  .cp-cmd-mode {
    flex-shrink: 0; width: 32px; height: 100%;
    display: flex; align-items: center; justify-content: center;
    font-size: 8px; font-weight: 700; letter-spacing: 0.06em;
    border-right: 1px solid var(--la-hair-base);
  }
  .cp-cmd-mode.m-idle { color: var(--la-text-mute); }
  .cp-cmd-input {
    flex: 1; height: 100%; padding: 0 10px;
    background: none; border: none; outline: none;
    font-family: var(--la-font-mono, monospace);
    font-size: 10px; color: var(--la-text-bright);
  }
  .cp-cmd-close {
    flex-shrink: 0; height: 100%; padding: 0 8px;
    background: none; border-left: 1px solid var(--la-hair-base);
    color: var(--la-text-mute); font-size: 8px; font-weight: 700;
    cursor: pointer;
    transition: background 0.1s;
  }
  .cp-cmd-close:hover { background: var(--la-hair-faint); color: var(--la-text-dim); }

  /* ── Code body ────────────────────────────────────────────────── */
  .cp-body-row { flex: 1; display: flex; overflow: hidden; min-height: 0; }
  .cp-body     { flex: 1; overflow-y: auto; overflow-x: hidden; }
  .cp-body::-webkit-scrollbar { width: 4px; }
  .cp-body::-webkit-scrollbar-thumb { background: var(--la-hair-base); }

  .cp-loading, .cp-error {
    padding: 20px 16px;
    font-size: 10px; color: var(--la-text-mute);
    font-family: var(--la-font-mono, monospace);
  }
  .cp-error { color: var(--la-semantic-error, #ff4d4d); }

  .cp-line {
    display: flex;
    align-items: flex-start;
    min-height: 18px;
    font-family: var(--la-font-mono, monospace);
    font-size: 11px;
    line-height: 18px;
    white-space: pre;
  }
  .cp-line.hl { background: color-mix(in srgb, var(--scope-accent, #ff5c8a) 8%, transparent); }

  .cp-ln {
    width: 42px; min-width: 42px; padding: 0 8px 0 4px;
    font-size: 9px; color: rgba(255,255,255,0.22);
    text-align: right; user-select: none;
    border-right: 1px solid var(--la-hair-faint, rgba(255,255,255,0.06));
    flex-shrink: 0;
  }
  .cp-mark {
    width: 16px; flex-shrink: 0;
    display: flex; align-items: center; justify-content: center;
    font-size: 9px; color: var(--la-text-mute);
  }
  .cp-src { flex: 1; padding: 0 12px 0 8px; white-space: pre; overflow-x: visible; }

  /* Blame line extras */
  .bl-sha  { width: 46px; min-width: 46px; padding: 0 6px; font-size: 9px; color: var(--scope-accent, #ff5c8a); flex-shrink: 0; user-select: none; }
  .bl-msg  { width: 110px; min-width: 0; flex-shrink: 1; padding: 0 6px 0 0; font-size: 9px; color: rgba(255,255,255,0.22); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; user-select: none; font-style: italic; }

  /* ── Minimap ──────────────────────────────────────────────────── */
  .cp-minimap {
    width: 52px; flex-shrink: 0;
    background: #060810;
    border-left: 1px solid var(--la-hair-faint, rgba(255,255,255,0.06));
    overflow: hidden; position: relative;
    cursor: ns-resize;
  }
  .cp-mm-content { position: absolute; top: 0; left: 0; right: 0; padding: 4px 7px; }
  .cp-mm-row { height: 2px; margin-bottom: 0.5px; border-radius: 1px; }
  .cp-mm-row.context { background: rgba(255,255,255,0.09); }
  .cp-mm-row.hl { background: var(--scope-accent, #ff5c8a); opacity: 0.9; }
  .cp-mm-viewport {
    position: absolute; left: 0; right: 0;
    background: rgba(255,255,255,0.04);
    border-top: 1px solid rgba(255,255,255,0.12);
    border-bottom: 1px solid rgba(255,255,255,0.12);
    pointer-events: none;
  }

  /* ── Status footer ────────────────────────────────────────────── */
  .cp-footer {
    flex-shrink: 0; height: 22px;
    display: flex; align-items: center; gap: 0;
    padding: 0 10px;
    background: #060810;
    border-top: 1px solid var(--la-hair-faint, rgba(255,255,255,0.06));
    font-family: var(--la-font-mono, monospace); font-size: 9px;
    color: var(--la-text-mute); overflow: hidden;
  }
  .cp-ft { display: flex; align-items: center; gap: 3px; padding: 0 8px 0 0; white-space: nowrap; }
  .cp-ft + .cp-ft { border-left: 1px solid var(--la-hair-faint, rgba(255,255,255,0.06)); padding-left: 8px; }
  .cp-ft .v   { color: var(--la-text-dim); }
  .cp-ft .acc { color: var(--scope-accent, #ff5c8a); transition: color var(--depth-shift, 0.4s); }
  .cp-ft-spacer { flex: 1; }
</style>
