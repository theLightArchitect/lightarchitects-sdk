<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { activePanelContext } from '$lib/layout';
  import mermaid from 'mermaid';
  import DOMPurify from 'dompurify';
  import { sanitize, coerceDuration, buildSequenceDiagram, buildFlowDiagram, type TraceSpan } from '$lib/ayin-traces-utils';

  interface Props {
    compact?: boolean;
  }

  let { compact = false }: Props = $props();

  // ── State ─────────────────────────────────────────────────────────────────

  const AYIN_SSE_URL = 'http://localhost:3742/events';
  const MAX_SPANS = 40;
  const DIAGRAM_MAX = 20;
  const RENDER_DEBOUNCE_MS = 800;

  let spans = $state<TraceSpan[]>([]);
  let svgOutput = $state('');
  let connectionStatus = $state<'connecting' | 'connected' | 'offline'>('connecting');
  let renderError = $state<string | null>(null);
  let diagramMode = $state<'sequence' | 'flow'>('sequence');
  let mermaidReady = $state(false);
  let renderCount = 0;

  let es: EventSource | null = null;
  let renderTimer: ReturnType<typeof setTimeout> | null = null;

  // ── Mermaid init ──────────────────────────────────────────────────────────

  mermaid.initialize({
    startOnLoad: false,
    securityLevel: 'strict',
    theme: 'dark',
    themeVariables: {
      background: '#0a0a12',
      primaryColor: '#1a1f2e',
      primaryTextColor: '#c8d0e0',
      primaryBorderColor: '#2a3040',
      lineColor: '#00c8ff',
      secondaryColor: '#16181b',
      tertiaryColor: '#1a1f2e',
      edgeLabelBackground: '#16181b',
      fontSize: '11px',
    },
    sequence: {
      actorMargin: 32,
      boxMargin: 8,
      noteMargin: 8,
      messageMargin: 20,
      mirrorActors: false,
      bottomMarginAdj: 4,
    },
  });
  mermaidReady = true;

  async function renderDiagram() {
    if (!mermaidReady || spans.length === 0) { svgOutput = ''; return; }
    renderError = null;
    const window = spans.slice(-DIAGRAM_MAX);
    const definition = diagramMode === 'sequence'
      ? buildSequenceDiagram(window)
      : buildFlowDiagram(window);
    try {
      const id = `ayin-mermaid-${++renderCount}`;
      const { svg } = await mermaid.render(id, definition);
      svgOutput = DOMPurify.sanitize(svg, { USE_PROFILES: { svg: true, svgFilters: true } });
      activePanelContext.update(ctx =>
        ctx?.type === 'ayin-traces' ? { ...ctx, spanCount: spans.length } : ctx
      );
    } catch (e) {
      renderError = String(e);
    }
  }

  function scheduleRender() {
    if (renderTimer) clearTimeout(renderTimer);
    renderTimer = setTimeout(renderDiagram, RENDER_DEBOUNCE_MS);
  }

  // ── SSE connection ────────────────────────────────────────────────────────

  function connect() {
    if (es) { es.close(); es = null; }
    connectionStatus = 'connecting';
    try {
      es = new EventSource(AYIN_SSE_URL);
      es.onopen = () => { connectionStatus = 'connected'; };
      es.onmessage = (ev) => {
        try {
          const span: TraceSpan = JSON.parse(ev.data);
          spans = [...spans.slice(-(MAX_SPANS - 1)), span];
          scheduleRender();
        } catch { /* malformed event — skip */ }
      };
      es.onerror = () => {
        connectionStatus = 'offline';
        es?.close();
        es = null;
      };
    } catch {
      connectionStatus = 'offline';
    }
  }

  function reconnect() { connect(); }
  function clearSpans() { spans = []; svgOutput = ''; renderError = null; }

  // ── Lifecycle ─────────────────────────────────────────────────────────────

  onMount(() => {
    connect();
    return () => {
      es?.close();
      if (renderTimer) clearTimeout(renderTimer);
    };
  });

  onDestroy(() => {
    es?.close();
    if (renderTimer) clearTimeout(renderTimer);
  });

  // Re-render when mode switches
  $effect(() => {
    void diagramMode;
    scheduleRender();
  });
</script>

<div class="ayin-panel" class:ayin-panel--compact={compact}>
  <!-- Header bar -->
  <div class="ayin-header">
    <span class="ayin-title">◎ AYIN TRACES</span>
    <div class="ayin-controls">
      <button
        class="mode-btn"
        class:active={diagramMode === 'sequence'}
        onclick={() => diagramMode = 'sequence'}
        title="Sequence diagram — actor message flow"
      >SEQ</button>
      <button
        class="mode-btn"
        class:active={diagramMode === 'flow'}
        onclick={() => diagramMode = 'flow'}
        title="Dataflow graph — span dependency graph"
      >FLOW</button>
    </div>
    <div class="ayin-status-group">
      <span
        class="ayin-status"
        class:connected={connectionStatus === 'connected'}
        class:offline={connectionStatus === 'offline'}
        class:connecting={connectionStatus === 'connecting'}
      >
        {#if connectionStatus === 'connected'}● LIVE{:else if connectionStatus === 'connecting'}○ …{:else}✕ :3742{/if}
      </span>
      <span class="span-count">{spans.length} spans</span>
      {#if connectionStatus !== 'connected'}
        <button class="reconnect-btn" onclick={reconnect}>RECONNECT</button>
      {/if}
      {#if spans.length > 0}
        <button class="clear-btn" onclick={clearSpans}>CLEAR</button>
      {/if}
    </div>
  </div>

  <!-- Diagram view -->
  <div class="ayin-body">
    {#if connectionStatus === 'offline' && spans.length === 0}
      <div class="ayin-empty">
        <span class="empty-icon">◎</span>
        <span class="empty-label">AYIN OFFLINE</span>
        <span class="empty-note">Start AYIN: make deploy && launchctl kickstart -k gui/$(id -u)/io.lightarchitects.ayin</span>
        <button class="reconnect-large" onclick={reconnect}>RECONNECT</button>
      </div>

    {:else if spans.length === 0}
      <div class="ayin-empty">
        <span class="empty-icon">◎</span>
        <span class="empty-label">AWAITING TRACES</span>
        <span class="empty-note">Spans will appear as agents run tool calls through AYIN</span>
      </div>

    {:else if renderError}
      <div class="ayin-error">
        <span class="error-label">DIAGRAM ERROR</span>
        <pre class="error-pre">{renderError}</pre>
      </div>

    {:else if svgOutput}
      <div class="diagram-scroll">
          {@html svgOutput}
      </div>
    {:else}
      <div class="ayin-empty">
        <span class="empty-icon">◎</span>
        <span class="empty-label">RENDERING…</span>
      </div>
    {/if}
  </div>

  <!-- Span list (last 8, below diagram) -->
  {#if spans.length > 0 && !compact}
    <div class="span-log" role="log" aria-label="Recent AYIN spans">
      {#each spans.slice(-8).toReversed() as span (span.span_id)}
        <div class="span-row" class:finish={span.outcome === 'Finish'}>
          <span class="span-actor">{span.actor.slice(0, 12)}</span>
          <span class="span-action">{span.action.slice(0, 24)}</span>
          {#if span.tool}<span class="span-tool">[{span.tool.slice(0, 16)}]</span>{/if}
          <span class="span-duration">{span.duration_ms}ms</span>
          <span class="span-outcome" class:finish={span.outcome === 'Finish'}>
            {span.outcome === 'Finish' ? '✓' : '◌'}
          </span>
        </div>
      {/each}
    </div>
  {/if}
</div>

<style>
  .ayin-panel {
    display: flex;
    flex-direction: column;
    width: 100%;
    height: 100%;
    min-height: 0;
    background: var(--la-bg-base, #0a0a12);
    font-family: var(--la-font-mono, 'JetBrains Mono', monospace);
    overflow: hidden;
  }

  .ayin-panel--compact .ayin-header {
    padding: 4px 8px;
    gap: 6px;
    flex-wrap: nowrap;
  }

  .ayin-panel--compact .ayin-title,
  .ayin-panel--compact .ayin-status,
  .ayin-panel--compact .span-count,
  .ayin-panel--compact .reconnect-btn,
  .ayin-panel--compact .clear-btn {
    font-size: 8px;
  }

  .ayin-panel--compact .ayin-body {
    min-height: 120px;
  }

  .ayin-panel--compact .diagram-scroll {
    min-height: 120px;
  }

  .ayin-panel--compact .ayin-empty {
    padding: 12px;
  }

  /* ── Header ──────────────────────────────────────────────────────────────── */

  .ayin-header {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 5px 10px;
    border-bottom: 1px solid var(--la-hair-base, #1e2230);
    flex-shrink: 0;
    flex-wrap: wrap;
  }

  .ayin-title {
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.12em;
    color: var(--la-agent-ops, #f97316);
  }

  .ayin-controls {
    display: flex;
    gap: 2px;
    margin-right: auto;
  }

  .mode-btn {
    background: none;
    border: 1px solid var(--la-hair-strong, #2a3040);
    color: var(--la-text-dim, #6b7a90);
    font-size: 8px;
    font-family: var(--la-font-mono);
    font-weight: 700;
    letter-spacing: 0.08em;
    padding: 2px 6px;
    cursor: pointer;
    transition: color 80ms, border-color 80ms, background 80ms;
  }
  .mode-btn:hover { color: var(--la-text-bright); border-color: var(--la-text-dim); }
  .mode-btn.active {
    color: var(--la-agent-ops, #f97316);
    border-color: var(--la-agent-ops, #f97316);
    background: color-mix(in srgb, var(--la-agent-ops, #f97316) 10%, transparent);
  }

  .ayin-status-group {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .ayin-status {
    font-size: 8px;
    font-weight: 700;
    letter-spacing: 0.08em;
  }
  .ayin-status.connected { color: var(--la-semantic-ok, #22c55e); }
  .ayin-status.offline   { color: var(--la-semantic-error, #ef4444); }
  .ayin-status.connecting { color: var(--la-text-mute, #3a4450); animation: pulse 1.2s ease-in-out infinite; }

  @keyframes pulse { 0%, 100% { opacity: 1; } 50% { opacity: 0.4; } }

  .span-count {
    font-size: 8px;
    color: var(--la-text-mute);
    letter-spacing: 0.04em;
  }

  .reconnect-btn, .clear-btn {
    background: none;
    border: 1px solid var(--la-hair-strong);
    color: var(--la-text-mute);
    font-size: 7px;
    font-family: var(--la-font-mono);
    font-weight: 700;
    letter-spacing: 0.08em;
    padding: 2px 5px;
    cursor: pointer;
  }
  .reconnect-btn:hover { color: var(--la-struct-primary); border-color: var(--la-struct-primary); }
  .clear-btn:hover { color: var(--la-semantic-warn); border-color: var(--la-semantic-warn); }

  /* ── Body ────────────────────────────────────────────────────────────────── */

  .ayin-body {
    flex: 1;
    min-height: 0;
    overflow: hidden;
    display: flex;
    align-items: stretch;
  }

  .diagram-scroll {
    flex: 1;
    overflow: auto;
    padding: 12px;
    display: flex;
    align-items: flex-start;
    justify-content: center;
  }

  /* Mermaid SVG theming overrides */
  .diagram-scroll :global(svg) {
    max-width: 100%;
    height: auto;
    background: transparent;
  }
  .diagram-scroll :global(.label) { color: var(--la-text-bright); }

  /* ── Empty / error states ─────────────────────────────────────────────────── */

  .ayin-empty {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 6px;
    opacity: 0.45;
    padding: 24px;
    text-align: center;
  }

  .empty-icon  { font-size: 28px; color: var(--la-agent-ops, #f97316); opacity: 0.6; }
  .empty-label {
    font-size: 10px;
    font-weight: 700;
    letter-spacing: 0.12em;
    color: var(--la-text-dim);
  }
  .empty-note  { font-size: 8px; color: var(--la-text-mute); line-height: 1.5; max-width: 300px; }

  .reconnect-large {
    margin-top: 8px;
    background: none;
    border: 1px solid var(--la-struct-primary);
    color: var(--la-struct-primary);
    font-size: 9px;
    font-family: var(--la-font-mono);
    font-weight: 700;
    letter-spacing: 0.1em;
    padding: 4px 12px;
    cursor: pointer;
    opacity: 1;
  }
  .reconnect-large:hover {
    background: color-mix(in srgb, var(--la-struct-primary) 12%, transparent);
  }

  .ayin-error {
    flex: 1;
    padding: 16px;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .error-label {
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.1em;
    color: var(--la-semantic-error, #ef4444);
  }
  .error-pre {
    font-size: 8px;
    color: var(--la-text-mute);
    white-space: pre-wrap;
    word-break: break-all;
  }

  /* ── Span log ─────────────────────────────────────────────────────────────── */

  .span-log {
    flex-shrink: 0;
    border-top: 1px solid var(--la-hair-base);
    max-height: 140px;
    overflow-y: auto;
  }

  .span-row {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 3px 10px;
    border-bottom: 1px solid var(--la-hair-faint, #12141a);
    font-size: 8px;
    line-height: 1.2;
  }
  .span-row:hover { background: var(--la-bg-elev-1, #12141a); }
  .span-row.finish { background: color-mix(in srgb, var(--la-semantic-ok, #22c55e) 4%, transparent); }

  .span-actor  { color: var(--la-agent-ops, #f97316); font-weight: 700; flex-shrink: 0; width: 72px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .span-action { color: var(--la-text-bright); flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .span-tool   { color: var(--la-struct-primary); font-size: 7px; flex-shrink: 0; }
  .span-duration { color: var(--la-text-mute); flex-shrink: 0; font-size: 7px; }
  .span-outcome { flex-shrink: 0; font-size: 9px; color: var(--la-text-mute); }
  .span-outcome.finish { color: var(--la-semantic-ok, #22c55e); }
</style>
