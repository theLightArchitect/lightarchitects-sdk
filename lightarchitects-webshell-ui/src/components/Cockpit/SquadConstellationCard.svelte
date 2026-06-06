<!--
  @component
  SquadConstellationCard — 7 siblings rendered as a hex constellation.
  Status snapshot: `GET /api/siblings` (real_data::get_squad_status).
  Live A2A link edges: `GET (SSE) /api/squad/a2a` (§2.54).
  SOUL anchored at center; the 6 other siblings circle in canonical positions.
-->
<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { api, type SiblingStatus } from '$lib/api';
  import { authHeaders } from '$lib/auth';

  const STATUS_POLL_MS = 60_000;
  const LINK_TTL_MS = 8_000;

  /** Canonical sibling layout (hex around SOUL at centre). */
  interface NodeLayout { name: string; x: number; y: number; sub: string; hue: string; }
  const LAYOUT: NodeLayout[] = [
    { name: 'SOUL',    x: 150, y: 120, sub: 'vault',   hue: '#7a6cff' },
    { name: 'EVA',     x: 150, y: 38,  sub: 'consc',   hue: '#d44df0' },
    { name: 'CORSO',   x: 236, y: 78,  sub: 'build',   hue: '#ffad2e' },
    { name: 'QUANTUM', x: 236, y: 162, sub: 'invest',  hue: '#39d8ff' },
    { name: 'SERAPH',  x: 150, y: 202, sub: 'redtm',   hue: '#ff4d6a' },
    { name: 'AYIN',    x: 64,  y: 162, sub: 'obs',     hue: '#5f6e84' },
    { name: 'LÆX',    x: 64,  y: 78,  sub: 'canon',   hue: '#f0c14d' },
  ];

  interface ActiveLink { id: string; from: string; to: string; expires: number; }

  let status: SiblingStatus[] = $state([]);
  let activeLinks: ActiveLink[] = $state([]);
  let connected: boolean = $state(false);
  let loading: boolean = $state(true);
  let error: string | null = $state(null);
  let statusTimer: ReturnType<typeof setInterval> | null = null;
  let eventSource: EventSource | null = null;
  let cleanupTimer: ReturnType<typeof setInterval> | null = null;

  async function refreshStatus() {
    try {
      status = await api.getSquadStatus();
      error = null;
    } catch (err) {
      error = err instanceof Error ? err.message : 'squad fetch failed';
    } finally {
      loading = false;
    }
  }

  function attachSse() {
    // EventSource uses the la_session cookie; no header injection possible.
    const url = '/api/squad/a2a';
    eventSource = new EventSource(url);
    eventSource.addEventListener('snapshot', () => { connected = true; });
    eventSource.addEventListener('link', (ev) => {
      try {
        const data = JSON.parse((ev as MessageEvent).data) as {
          from: string; to: string; ts?: string;
        };
        const id = `${data.from}->${data.to}-${Date.now()}`;
        activeLinks = [
          ...activeLinks.filter((l) => l.expires > Date.now()),
          { id, from: data.from, to: data.to, expires: Date.now() + LINK_TTL_MS },
        ].slice(-32);
      } catch {
        // ignore malformed link payload — don't pollute store
      }
    });
    eventSource.onerror = () => {
      connected = false;
      // EventSource auto-reconnects with backoff; nothing to do here.
    };
  }

  function teardownSse() {
    if (eventSource) {
      eventSource.close();
      eventSource = null;
    }
  }

  onMount(() => {
    // authHeaders is referenced to ensure auth.ts is imported in the bundle
    // (cookie-based auth means we don't pass headers to EventSource explicitly,
    // but keeping the import preserves the wiring contract for non-cookie modes).
    void authHeaders;
    refreshStatus();
    statusTimer = setInterval(refreshStatus, STATUS_POLL_MS);
    attachSse();
    cleanupTimer = setInterval(() => {
      const now = Date.now();
      activeLinks = activeLinks.filter((l) => l.expires > now);
    }, 1_000);
  });

  onDestroy(() => {
    if (statusTimer !== null) clearInterval(statusTimer);
    if (cleanupTimer !== null) clearInterval(cleanupTimer);
    teardownSse();
  });

  function byName(n: string): NodeLayout | undefined {
    return LAYOUT.find((l) => l.name === n);
  }

  const siblingStatus = $derived.by(() => {
    const map = new Map<string, string>();
    for (const s of status) map.set(s.id.toUpperCase(), s.status);
    return map;
  });

  function nodeStatus(name: string): 'active' | 'idle' | 'offline' {
    const raw = siblingStatus.get(name) ?? 'offline';
    if (raw === 'online' || raw === 'active') return 'active';
    if (raw === 'offline') return 'offline';
    return 'idle';
  }

  const linkPaths = $derived.by(() => {
    return activeLinks
      .map((l) => {
        const from = byName(l.from);
        const to = byName(l.to);
        if (!from || !to) return null;
        return { id: l.id, d: `M${from.x},${from.y} L${to.x},${to.y}` };
      })
      .filter((p): p is { id: string; d: string } => p !== null);
  });

  const counts = $derived.by(() => ({
    active: LAYOUT.filter((l) => nodeStatus(l.name) === 'active').length,
    idle:   LAYOUT.filter((l) => nodeStatus(l.name) === 'idle').length,
    links:  activeLinks.length,
  }));
</script>

<div class="sc-card">
  {#if loading && status.length === 0}
    <div class="sc-empty">loading squad…</div>
  {:else if error && status.length === 0}
    <div class="sc-error">squad unavailable — {error}</div>
  {:else}
    <div class="sc-canvas">
      <svg viewBox="0 0 300 240" preserveAspectRatio="xMidYMid meet" role="img"
           aria-label="Squad constellation — 7 siblings with live A2A links">
        <g class="sc-links">
          {#each linkPaths as path (path.id)}
            <path class="sc-link" d={path.d} />
          {/each}
        </g>
        <g class="sc-nodes">
          {#each LAYOUT as node (node.name)}
            <g class="sc-node sc-{nodeStatus(node.name)}" style="--node-hue: {node.hue};" data-name={node.name}>
              {#if nodeStatus(node.name) === 'active'}
                <circle class="sc-pulse" cx={node.x} cy={node.y} r="16" />
              {/if}
              <circle class="sc-node-bg" cx={node.x} cy={node.y} r="14" />
              <text class="sc-node-label" x={node.x} y={node.y + 3.5}
                    style="font-size: {node.name.length > 6 ? 7.5 : node.name.length > 4 ? 8.5 : 9}px;">
                {node.name}
              </text>
              <text class="sc-node-sub" x={node.x} y={node.y + 30}>{node.sub}</text>
            </g>
          {/each}
        </g>
      </svg>
    </div>

    <div class="sc-foot">
      <span>A2A links <strong>{counts.links}</strong></span>
      <span>Idle <strong>{counts.idle}</strong></span>
      <span>Active <strong>{counts.active}</strong></span>
      <span class="sc-conn-pill" class:sc-conn-on={connected}>{connected ? 'LIVE' : 'SNAPSHOT'}</span>
    </div>
  {/if}
</div>

<style>
  .sc-card {
    display: flex;
    flex-direction: column;
    gap: 6px;
    font-family: var(--la-font-mono, monospace);
    min-height: 0;
  }

  .sc-empty, .sc-error {
    font-size: 9px;
    color: var(--la-text-mute);
    font-style: italic;
    padding: 14px 0;
    text-align: center;
  }
  .sc-error { color: var(--la-err, #ff4d6a); }

  .sc-canvas {
    flex: 1;
    min-height: 200px;
    position: relative;
  }
  .sc-canvas svg { width: 100%; height: 100%; display: block; }

  .sc-link {
    stroke: var(--la-struct-primary, #4d8eff);
    stroke-width: 1;
    opacity: 0.42;
    stroke-dasharray: 2 3;
    fill: none;
    animation: sc-dash 18s linear infinite;
  }
  @keyframes sc-dash {
    to { stroke-dashoffset: -200; }
  }

  .sc-node-bg {
    fill: var(--la-bg-card, #111420);
    stroke: var(--node-hue);
    stroke-width: 1.5;
  }
  .sc-active .sc-node-bg {
    stroke-width: 2;
    filter: drop-shadow(0 0 6px var(--node-hue));
  }
  .sc-idle .sc-node-bg { opacity: 0.42; }
  .sc-offline .sc-node-bg { opacity: 0.22; stroke-dasharray: 2 2; }

  .sc-node-label {
    font-family: var(--la-font-mono, monospace);
    font-weight: 700;
    letter-spacing: 0.04em;
    fill: var(--la-text-bright, rgba(255,255,255,0.95));
    text-anchor: middle;
    pointer-events: none;
  }
  .sc-idle .sc-node-label,
  .sc-offline .sc-node-label { fill: var(--la-text-mute, rgba(255,255,255,0.28)); }

  .sc-node-sub {
    font-family: var(--la-font-mono, monospace);
    font-size: 7px;
    font-weight: 500;
    letter-spacing: 0.1em;
    fill: var(--la-text-mute, rgba(255,255,255,0.28));
    text-anchor: middle;
    pointer-events: none;
    text-transform: uppercase;
  }

  .sc-pulse {
    fill: var(--node-hue);
    opacity: 0.4;
    animation: sc-ring 2.4s ease-out infinite;
    pointer-events: none;
  }
  @keyframes sc-ring {
    0%   { r: 16; opacity: 0.5; }
    100% { r: 32; opacity: 0; }
  }

  .sc-foot {
    border-top: 1px solid var(--la-hair-faint, rgba(255,255,255,0.04));
    padding-top: 6px;
    font-size: 9px;
    color: var(--la-text-mute, rgba(255,255,255,0.28));
    display: flex;
    justify-content: space-between;
    align-items: center;
    flex-wrap: wrap;
    gap: 6px;
  }
  .sc-foot strong {
    color: var(--la-text-bright, rgba(255,255,255,0.95));
    font-weight: 700;
  }

  .sc-conn-pill {
    font-size: 7px;
    font-weight: 700;
    letter-spacing: 0.1em;
    padding: 1px 5px;
    color: var(--la-text-mute, rgba(255,255,255,0.28));
    border: 1px solid var(--la-text-mute, rgba(255,255,255,0.28));
  }
  .sc-conn-on {
    color: var(--la-ok, #39ff8a);
    border-color: var(--la-ok, #39ff8a);
  }
</style>
