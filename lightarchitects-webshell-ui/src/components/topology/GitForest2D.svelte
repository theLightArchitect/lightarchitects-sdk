<script lang="ts">
  import { gitforestTree } from '$lib/stores';
  import type { BranchNode, WorktreeAssignment } from '$lib/gitforest';
  import { goto } from '$app/navigation';

  // ── Derived data ────────────────────────────────────────────────────────────

  let builds = $derived.by((): BranchNode[] => {
    const t = $gitforestTree;
    if (!t) return [];
    return Object.values(t.nodes)
      .filter(n => n.kind === 'build')
      .sort((a, b) => {
        const rank = (n: BranchNode) =>
          n.overlay.lifecycle === 'live_active' ? 0
          : n.overlay.lifecycle === 'live_idle' ? 1
          : n.overlay.lifecycle === 'merged' ? 2
          : 3;
        return rank(a) - rank(b);
      });
  });

  let activeWorktrees = $derived.by((): (WorktreeAssignment & { buildName: string })[] => {
    const t = $gitforestTree;
    if (!t) return [];
    const out: (WorktreeAssignment & { buildName: string })[] = [];
    for (const node of Object.values(t.nodes)) {
      if (node.worktrees.length) {
        const build = node.parent_id ? (t.nodes[node.parent_id] ?? node) : node;
        for (const wt of node.worktrees) {
          out.push({ ...wt, buildName: build.name });
        }
      }
    }
    return out;
  });

  // ── Stuck-build detection ────────────────────────────────────────────────────
  // Tracks last time waves_done changed per build node. Flags as stuck after 5 min.

  let _progressLog = new Map<string, { done: number; ts: number }>();
  let stuckNodes = $state(new Set<string>());

  $effect(() => {
    const t = $gitforestTree;
    if (!t) return;
    const now = Date.now();
    const next = new Set<string>();
    for (const node of Object.values(t.nodes)) {
      if (node.kind !== 'build' || node.overlay.lifecycle !== 'live_active') continue;
      const done = node.build_progress?.waves_done ?? 0;
      const prev = _progressLog.get(node.id);
      if (!prev || prev.done !== done) {
        _progressLog.set(node.id, { done, ts: now });
      } else if (now - prev.ts > 5 * 60_000) {
        next.add(node.id);
      }
    }
    stuckNodes = next;
  });

  // ── HITL resolve state ───────────────────────────────────────────────────────

  let hitlOpen     = $state<string | null>(null);
  let hitlDecision = $state<'approve' | 'reject' | null>(null);
  let hitlRationale = $state('');
  let hitlPending  = $state(false);
  let hitlError    = $state<string | null>(null);

  function openHitl(nodeId: string, e: Event) {
    e.stopPropagation();
    hitlOpen = nodeId;
    hitlDecision = null;
    hitlRationale = '';
    hitlError = null;
  }

  function closeHitl(e: Event) {
    e.stopPropagation();
    hitlOpen = null;
  }

  async function submitHitl(node: BranchNode, e: Event) {
    e.stopPropagation();
    if (!hitlDecision) return;
    hitlPending = true;
    hitlError = null;
    try {
      const res = await fetch('/api/gitforest/hitl-resolve', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          build_id: node.id,
          decision: hitlDecision,
          rationale: hitlRationale.trim() || null,
        }),
      });
      if (!res.ok && res.status !== 404) {
        hitlError = `Server error ${res.status}`;
      } else {
        hitlOpen = null;
      }
    } catch {
      hitlError = 'Network error';
    } finally {
      hitlPending = false;
    }
  }

  // ── Visual helpers ───────────────────────────────────────────────────────────

  function cardClass(node: BranchNode): string {
    const lc = node.overlay.lifecycle;
    if (lc === 'live_active') return 'tc-active';
    if (lc === 'merged')      return 'tc-done';
    return 'tc-pending';
  }

  function ciDot(node: BranchNode): string {
    switch (node.overlay.ci_status) {
      case 'success': return '#22c55e';
      case 'failure': return '#f87171';
      case 'pending': return '#f59e0b';
      default:        return '#334155';
    }
  }

  function gateColor(score: number | null): string {
    if (score === null)  return '#334155';
    if (score >= 0.95)   return '#22c55e';
    if (score >= 0.80)   return '#f59e0b';
    return '#f87171';
  }

  function wavePct(node: BranchNode): number {
    const bp = node.build_progress;
    if (!bp || !bp.waves_total) return 0;
    return Math.round((bp.waves_done / bp.waves_total) * 100);
  }

  /** Array of wave dot states for a build node. */
  function waveDots(node: BranchNode): Array<'done' | 'active' | 'pending'> {
    const bp = node.build_progress;
    if (!bp || !bp.waves_total) return [];
    const isActive = node.overlay.lifecycle === 'live_active';
    return Array.from({ length: bp.waves_total }, (_, i) => {
      if (i < bp.waves_done) return 'done';
      if (i === bp.waves_done && isActive) return 'active';
      return 'pending';
    });
  }

  /** SVG conifer tree whose canopy layers reflect wave progress. */
  function treeSvg(node: BranchNode): string {
    const lc = node.overlay.lifecycle;
    const pct = wavePct(node);
    const trunkColor  = lc === 'live_active' ? '#f5a623' : lc === 'merged' ? '#22c55e' : '#1e3a52';
    const canopyBase  = lc === 'live_active' ? '#c8862a' : lc === 'merged' ? '#16a34a' : '#1a3048';
    const canopyTip   = lc === 'live_active' ? '#ffd580' : lc === 'merged' ? '#4ade80' : '#223d58';
    const layers = Math.max(2, Math.min(5, Math.ceil((pct / 100) * 4) + 1));
    const canopyPaths = Array.from({ length: layers }, (_, i) => {
      const y = 20 + i * 18;
      const w = 28 + i * 14;
      const x = 50 - w / 2;
      return `<polygon points="${x},${y + 18} ${50},${y} ${x + w},${y + 18}" fill="${i === 0 ? canopyTip : canopyBase}" opacity="${0.6 + i * 0.08}"/>`;
    }).join('');
    return `<svg viewBox="0 0 100 120" xmlns="http://www.w3.org/2000/svg">
      ${canopyPaths}
      <rect x="44" y="${20 + layers * 18}" width="12" height="${120 - 20 - layers * 18 - 4}" fill="${trunkColor}" opacity="0.8" rx="2"/>
    </svg>`;
  }

  function shortPath(p: string): string {
    return p.replace(/^\/Users\/[^/]+/, '~').replace(/\/lightarchitects\/worktrees\//, '/wt/');
  }

  function handleCardClick(node: BranchNode): void {
    if (hitlOpen === node.id) return;
    const id = node.name.replace(/^feat\//, '');
    goto(`/builds/${id}`);
  }
</script>

<div class="forest2d">
  {#if builds.length === 0}
    <div class="forest2d-empty">
      <div class="empty-icon">🌲</div>
      <div class="empty-label">No active builds</div>
      <div class="empty-sub">Builds will appear here when a lightsquad is running</div>
    </div>
  {:else}
    <div class="tree-grid">
      {#each builds as node (node.id)}
        {@const pct = wavePct(node)}
        {@const hasHitl = node.overlay.hitl_state === 'pending'}
        {@const isStuck = stuckNodes.has(node.id)}
        {@const isCiFail = node.overlay.ci_status === 'failure' && node.overlay.lifecycle === 'live_active'}
        {@const dots = waveDots(node)}
        {@const isHitlOpen = hitlOpen === node.id}

        <div class="tree-card-wrap">
          <!-- Badge row sits on the wrapper, not inside the button (no nested interactive elements) -->
          {#if hasHitl || isStuck || isCiFail}
            <div class="badge-row" aria-label="Build alerts">
              {#if hasHitl}
                <button
                  class="hitl-badge"
                  onclick={(e) => openHitl(node.id, e)}
                  aria-label="Resolve HITL for {node.name}"
                  title="Human-in-the-loop gate pending — click to resolve"
                >⚠ HITL</button>
              {/if}
              {#if isStuck}
                <span class="stuck-badge" title="No wave progress in 5+ minutes">STUCK?</span>
              {/if}
              {#if isCiFail}
                <span class="ci-fail-badge">CI FAIL</span>
              {/if}
            </div>
          {/if}

          <button
            class="tree-card {cardClass(node)}"
            class:hitl-glow={hasHitl}
            onclick={() => handleCardClick(node)}
            aria-label="Build: {node.name}"
            title={node.id}
          >
            <!-- SVG canopy -->
            <div class="tree-canopy">
              <div class="canopy-glow"></div>
              <!-- eslint-disable-next-line svelte/no-at-html-tags -->
              {@html treeSvg(node)}
            </div>

            <div class="tree-info">
              <div class="tree-name">{node.name.replace(/^feat\//, '').replace(/^main\//, '')}</div>
              <div class="tree-branch">{node.id}</div>

              <!-- Wave dots (replaces progress bar) -->
              {#if dots.length > 0}
                <div class="wave-dots-row" title="Wave progress: {node.build_progress?.waves_done}/{node.build_progress?.waves_total}">
                  {#each dots.slice(0, 20) as dotState}
                    <span class="wave-dot wave-dot--{dotState}"></span>
                  {/each}
                  {#if dots.length > 20}
                    <span class="wave-dots-overflow">+{dots.length - 20}</span>
                  {/if}
                  <span class="wave-dots-pct">{pct}%</span>
                </div>
              {/if}

              <div class="tree-footer">
                <div class="tree-meta-left">
                  <span class="ci-dot" style="background:{ciDot(node)}"></span>
                  {#if node.overlay.gate_score !== null}
                    <span class="gate-score" style="color:{gateColor(node.overlay.gate_score)}">
                      {Math.round(node.overlay.gate_score * 100)}
                    </span>
                  {/if}
                  {#if node.overlay.phase}
                    <span class="phase-tag">P{node.overlay.phase}</span>
                  {/if}
                </div>
                <span class="tree-status {node.overlay.lifecycle === 'merged' ? 'done' : node.overlay.lifecycle === 'live_active' ? 'run' : 'pend'}">
                  {node.overlay.lifecycle.replace('_', ' ')}
                </span>
              </div>

              <!-- Agent pips -->
              {#if activeWorktrees.filter(w => w.buildName === node.name).length > 0}
                <div class="agent-row">
                  {#each activeWorktrees.filter(w => w.buildName === node.name).slice(0, 4) as wt}
                    <span
                      class="agent-pip"
                      class:pip-writing={wt.state === 'writing'}
                      class:pip-gate={wt.state === 'gate'}
                      class:pip-done={wt.state === 'done'}
                      title="{wt.agent_key} · {shortPath(wt.worktree_path)}"
                    >{wt.domain.slice(0, 3).toUpperCase()}</span>
                  {/each}
                </div>
              {/if}
            </div>
          </button>

          <!-- HITL resolve drawer — renders outside the button to avoid nesting issues -->
          {#if isHitlOpen}
            <div class="hitl-drawer" role="dialog" aria-label="Resolve HITL for {node.name}">
              <div class="hitl-drawer-head">
                <span class="hitl-drawer-title">Resolve HITL</span>
                <button class="hitl-close" onclick={closeHitl} aria-label="Cancel">✕</button>
              </div>
              <div class="hitl-decision-row">
                <button
                  class="hitl-btn approve"
                  class:selected={hitlDecision === 'approve'}
                  onclick={(e) => { e.stopPropagation(); hitlDecision = 'approve'; }}
                >✓ Approve</button>
                <button
                  class="hitl-btn reject"
                  class:selected={hitlDecision === 'reject'}
                  onclick={(e) => { e.stopPropagation(); hitlDecision = 'reject'; }}
                >✕ Reject</button>
              </div>
              <textarea
                class="hitl-rationale"
                placeholder="Rationale (optional)"
                bind:value={hitlRationale}
                rows={2}
                onclick={(e) => e.stopPropagation()}
              ></textarea>
              {#if hitlError}
                <div class="hitl-error">{hitlError}</div>
              {/if}
              <button
                class="hitl-confirm"
                disabled={!hitlDecision || hitlPending}
                onclick={(e) => submitHitl(node, e)}
              >{hitlPending ? 'Sending…' : 'Confirm'}</button>
            </div>
          {/if}
        </div>
      {/each}
    </div>
  {/if}
</div>

<style>
  .forest2d {
    width: 100%;
    height: 100%;
    overflow-y: auto;
    padding: 12px;
    scrollbar-width: none;
  }
  .forest2d::-webkit-scrollbar { display: none; }

  .forest2d-empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    height: 100%;
    gap: 8px;
    color: var(--la-text-mute);
  }
  .empty-icon  { font-size: 32px; opacity: 0.4; }
  .empty-label { font-size: 11px; font-weight: 600; letter-spacing: 0.08em; color: var(--la-text-dim); }
  .empty-sub   { font-size: 9px; text-align: center; max-width: 200px; line-height: 1.5; }

  .tree-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(160px, 1fr));
    gap: 10px;
  }

  /* Wrap so the HITL drawer can sit outside the <button> */
  .tree-card-wrap {
    position: relative;
    display: flex;
    flex-direction: column;
  }

  .tree-card {
    background: var(--la-bg-raised, #0a1520);
    border: 1px solid var(--la-hair-base, #1a3048);
    border-radius: 8px;
    overflow: hidden;
    cursor: pointer;
    transition: border-color 0.15s, transform 0.15s, box-shadow 0.15s;
    position: relative;
    text-align: left;
    padding: 0;
    font-family: var(--la-font-chrome, 'JetBrains Mono', monospace);
    font-size: 11px;
    width: 100%;
  }
  .tree-card:hover {
    border-color: var(--la-hair-strong, #223d58);
    transform: translateY(-2px);
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.4);
  }
  .tree-card:focus-visible {
    outline: 2px solid var(--la-focus-ring, #00c8ff);
    outline-offset: 2px;
  }
  .tc-done    { border-top: 3px solid #22c55e; }
  .tc-active  { border-top: 3px solid #f5a623; }
  .tc-pending { border-top: 3px solid #1e3a52; opacity: 0.75; }

  .hitl-glow {
    box-shadow: 0 0 0 1px rgba(248, 113, 113, 0.5), 0 4px 16px rgba(248, 113, 113, 0.15);
  }

  /* ── Badge row ── */
  .badge-row {
    position: absolute;
    top: 5px;
    right: 5px;
    display: flex;
    gap: 3px;
    z-index: 2;
  }

  .hitl-badge {
    background: rgba(248, 113, 113, 0.9);
    color: #fff;
    font-size: 7px;
    font-weight: 700;
    padding: 2px 5px;
    border-radius: 3px;
    border: none;
    cursor: pointer;
    animation: badge-flash 1s infinite;
    font-family: var(--la-font-chrome, 'JetBrains Mono', monospace);
    letter-spacing: 0.04em;
    transition: background 80ms;
  }
  .hitl-badge:hover { background: rgba(248, 113, 113, 1); }

  .stuck-badge {
    background: rgba(245, 158, 11, 0.85);
    color: #fff;
    font-size: 7px;
    font-weight: 700;
    padding: 2px 5px;
    border-radius: 3px;
    letter-spacing: 0.04em;
    animation: badge-flash 2s infinite;
  }

  .ci-fail-badge {
    background: rgba(239, 68, 68, 0.85);
    color: #fff;
    font-size: 7px;
    font-weight: 700;
    padding: 2px 5px;
    border-radius: 3px;
    letter-spacing: 0.04em;
  }

  @keyframes badge-flash { 0%, 100% { opacity: 1; } 50% { opacity: 0.55; } }

  /* ── Canopy ── */
  .tree-canopy {
    width: 100%;
    height: 90px;
    display: flex;
    align-items: flex-end;
    justify-content: center;
    padding: 6px 6px 0;
    position: relative;
    overflow: hidden;
  }
  .tree-canopy :global(svg) { width: 100%; height: 100%; }
  .canopy-glow {
    position: absolute;
    inset: 0;
    border-radius: 50%;
    filter: blur(18px);
    opacity: 0.12;
    pointer-events: none;
  }
  .tc-done   .canopy-glow { background: #22c55e; }
  .tc-active .canopy-glow { background: #f5a623; animation: cg-pulse 3s infinite; }
  @keyframes cg-pulse { 0%, 100% { opacity: 0.12; } 50% { opacity: 0.25; } }

  /* ── Card info section ── */
  .tree-info { padding: 8px 10px 10px; }
  .tree-name {
    font-size: 10px;
    font-weight: 700;
    color: var(--la-text-stark, #c0d4e4);
    margin-bottom: 2px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .tree-branch {
    font-size: 8px;
    color: #38bdf8;
    margin-bottom: 6px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  /* ── Wave dots ── */
  .wave-dots-row {
    display: flex;
    align-items: center;
    gap: 2px;
    margin-bottom: 6px;
    flex-wrap: wrap;
  }
  .wave-dot {
    display: inline-block;
    width: 6px;
    height: 6px;
    border-radius: 2px;
    flex-shrink: 0;
    transition: background 0.3s;
  }
  .wave-dot--done    { background: #22c55e; }
  .wave-dot--pending { background: #1e3a52; }
  .wave-dot--active  {
    background: #f5a623;
    animation: dot-pulse 1.2s ease-in-out infinite;
  }
  @keyframes dot-pulse {
    0%, 100% { opacity: 1; box-shadow: 0 0 4px #f5a623; }
    50%       { opacity: 0.6; box-shadow: 0 0 2px #f5a623; }
  }
  .wave-dots-overflow {
    font-size: 7px;
    color: var(--la-text-mute);
    margin-left: 1px;
  }
  .wave-dots-pct {
    margin-left: auto;
    font-size: 7px;
    color: var(--la-text-dim);
    font-variant-numeric: tabular-nums;
  }

  /* ── Footer ── */
  .tree-footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding-top: 6px;
    border-top: 1px solid var(--la-hair-base, #1a3048);
  }
  .tree-meta-left { display: flex; align-items: center; gap: 5px; }
  .ci-dot  { display: inline-block; width: 6px; height: 6px; border-radius: 50%; flex-shrink: 0; }
  .gate-score { font-size: 9px; font-weight: 600; }
  .phase-tag  { font-size: 7px; color: var(--la-text-mute, #4e7090); }
  .tree-status { font-size: 8px; }
  .tree-status.done { color: #22c55e; }
  .tree-status.run  { color: #f5a623; }
  .tree-status.pend { color: #4e7090; }

  /* ── Agent pips ── */
  .agent-row { display: flex; gap: 3px; margin-top: 5px; flex-wrap: wrap; }
  .agent-pip {
    font-size: 7px;
    font-weight: 700;
    padding: 1px 4px;
    border-radius: 2px;
    border: 1px solid #1a3048;
    background: #0a1520;
    color: #4e7090;
  }
  .pip-writing { border-color: rgba(245,166,35,0.5); color: #f5a623; background: rgba(245,166,35,0.08); }
  .pip-gate    { border-color: rgba(167,139,250,0.5); color: #a78bfa; background: rgba(167,139,250,0.08); }
  .pip-done    { border-color: rgba(34,197,94,0.4);  color: #22c55e; background: rgba(34,197,94,0.06);  }

  /* ── HITL resolve drawer ── */
  .hitl-drawer {
    background: #0d1e2e;
    border: 1px solid rgba(248, 113, 113, 0.4);
    border-top: none;
    border-radius: 0 0 8px 8px;
    padding: 8px 10px 10px;
    display: flex;
    flex-direction: column;
    gap: 6px;
    box-shadow: 0 4px 16px rgba(248, 113, 113, 0.12);
    animation: drawer-slide-in 0.15s ease-out;
  }
  @keyframes drawer-slide-in {
    from { opacity: 0; transform: translateY(-4px); }
    to   { opacity: 1; transform: translateY(0); }
  }

  .hitl-drawer-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  .hitl-drawer-title {
    font-size: 8px;
    font-weight: 700;
    letter-spacing: 0.1em;
    color: #f87171;
    text-transform: uppercase;
  }
  .hitl-close {
    background: none;
    border: none;
    color: var(--la-text-mute);
    font-size: 10px;
    cursor: pointer;
    padding: 0 2px;
    line-height: 1;
    transition: color 80ms;
  }
  .hitl-close:hover { color: var(--la-text-base); }

  .hitl-decision-row {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 4px;
  }
  .hitl-btn {
    font-size: 8px;
    font-weight: 700;
    font-family: var(--la-font-chrome, 'JetBrains Mono', monospace);
    letter-spacing: 0.06em;
    padding: 4px 0;
    border-radius: 4px;
    cursor: pointer;
    border: 1px solid var(--la-hair-base);
    background: transparent;
    color: var(--la-text-dim);
    transition: background 80ms, border-color 80ms, color 80ms;
  }
  .hitl-btn.approve.selected {
    background: rgba(34, 197, 94, 0.15);
    border-color: #22c55e;
    color: #22c55e;
  }
  .hitl-btn.reject.selected {
    background: rgba(248, 113, 113, 0.15);
    border-color: #f87171;
    color: #f87171;
  }
  .hitl-btn.approve:hover:not(.selected) { border-color: #22c55e55; color: #22c55e; }
  .hitl-btn.reject:hover:not(.selected)  { border-color: #f8717155; color: #f87171; }

  .hitl-rationale {
    font-family: var(--la-font-chrome, 'JetBrains Mono', monospace);
    font-size: 8px;
    background: rgba(255,255,255,0.04);
    border: 1px solid var(--la-hair-base);
    border-radius: 3px;
    color: var(--la-text-base);
    padding: 4px 6px;
    resize: none;
    outline: none;
    width: 100%;
    box-sizing: border-box;
    line-height: 1.5;
  }
  .hitl-rationale:focus { border-color: var(--la-hair-strong); }

  .hitl-error {
    font-size: 7px;
    color: #f87171;
    letter-spacing: 0.04em;
  }

  .hitl-confirm {
    font-size: 8px;
    font-weight: 700;
    font-family: var(--la-font-chrome, 'JetBrains Mono', monospace);
    letter-spacing: 0.08em;
    padding: 5px 0;
    border-radius: 4px;
    cursor: pointer;
    border: 1px solid var(--la-struct-primary, #00c8ff);
    background: rgba(0, 200, 255, 0.1);
    color: var(--la-struct-primary, #00c8ff);
    transition: background 80ms;
    width: 100%;
  }
  .hitl-confirm:hover:not(:disabled) { background: rgba(0, 200, 255, 0.2); }
  .hitl-confirm:disabled { opacity: 0.35; cursor: not-allowed; }
</style>
