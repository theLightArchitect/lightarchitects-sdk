<script lang="ts">
  import { gitforestTree } from '$lib/stores';
  import type { BranchNode, WorktreeAssignment } from '$lib/gitforest';

  interface TreeRow {
    depth: number;
    kind: 'repo' | 'program' | 'build' | 'wave' | 'worktree';
    label: string;
    node?: BranchNode;
    wt?: WorktreeAssignment;
  }

  function shortPath(p: string): string {
    return p
      .replace(/^\/Users\/[^/]+\/lightarchitects\/worktrees\//, '~/wt/')
      .replace(/^\/Users\/[^/]+/, '~');
  }

  function stateColor(s: string): string {
    switch (s) {
      case 'writing': return '#f5a623';
      case 'gate':    return '#a78bfa';
      case 'done':    return '#22c55e';
      case 'failed':  return '#f87171';
      default:        return '#334155';
    }
  }

  function buildRows(nodes: Record<string, BranchNode>, rootId: string): TreeRow[] {
    const root = nodes[rootId];
    if (!root) return [];
    const rows: TreeRow[] = [];

    rows.push({ depth: 0, kind: 'repo', label: root.name || root.id });

    function walk(nodeId: string, depth: number) {
      const node = nodes[nodeId];
      if (!node || node.kind === 'main') return;

      const rowKind: TreeRow['kind'] =
        node.kind === 'program'      ? 'program'
        : node.kind === 'build'      ? 'build'
        : node.kind === 'wave_cluster' ? 'wave'
        : 'wave';

      rows.push({ depth, kind: rowKind, label: node.name.replace(/^feat\//, ''), node });

      if (node.worktrees.length) {
        for (const wt of node.worktrees) {
          rows.push({ depth: depth + 1, kind: 'worktree', label: wt.agent_key, wt });
        }
      }

      for (const childId of node.children) {
        walk(childId, depth + 1);
      }
    }

    for (const childId of root.children) {
      walk(childId, 1);
    }

    return rows;
  }

  let rows = $derived.by((): TreeRow[] => {
    const t = $gitforestTree;
    if (!t) return [];
    return buildRows(t.nodes, t.root_id);
  });

  let wtCount = $derived(rows.filter(r => r.kind === 'worktree').length);
</script>

<div class="wt-panel">
  {#if rows.length === 0}
    <div class="wt-empty">
      <span class="wt-empty-icon">⌥</span>
      <span class="wt-empty-label">No active worktrees</span>
    </div>
  {:else}
    <div class="wt-tree">
      {#each rows as row}
        {#if row.kind === 'repo'}
          <div class="wt-row wt-repo">
            <span class="wt-icon">▸</span>
            <span class="wt-repo-name">{row.label}</span>
            {#if wtCount > 0}
              <span class="wt-badge">{wtCount}</span>
            {/if}
          </div>

        {:else if row.kind === 'program'}
          <div class="wt-row wt-program" style="padding-left:{row.depth * 10 + 6}px">
            <span class="wt-connector">├─</span>
            <span class="wt-kind-tag">prog</span>
            <span class="wt-node-name">{row.label}</span>
          </div>

        {:else if row.kind === 'build'}
          <div class="wt-row wt-build" style="padding-left:{row.depth * 10 + 6}px">
            <span class="wt-connector">├─</span>
            <span class="wt-kind-tag build">build</span>
            <span class="wt-node-name build">{row.label}</span>
            {#if row.node?.overlay.lifecycle}
              <span class="wt-lc" class:active={row.node.overlay.lifecycle === 'live_active'}>
                {row.node.overlay.lifecycle.replace('live_', '')}
              </span>
            {/if}
          </div>

        {:else if row.kind === 'wave'}
          <div class="wt-row wt-wave" style="padding-left:{row.depth * 10 + 6}px">
            <span class="wt-connector">└─</span>
            <span class="wt-kind-tag wave">wc</span>
            <span class="wt-node-name dim">{row.label}</span>
          </div>

        {:else if row.kind === 'worktree' && row.wt}
          <div class="wt-row wt-worktree" style="padding-left:{row.depth * 10 + 6}px" title={row.wt.worktree_path}>
            <span class="wt-connector dim">└╴</span>
            <span class="wt-state-dot" style="background:{stateColor(row.wt.state)}"></span>
            <span class="wt-domain">{row.wt.domain.slice(0, 3).toUpperCase()}</span>
            <span class="wt-path">{shortPath(row.wt.worktree_path)}</span>
            {#if row.wt.commits > 0}
              <span class="wt-commits">+{row.wt.commits}</span>
            {/if}
          </div>
        {/if}
      {/each}
    </div>
  {/if}
</div>

<style>
  .wt-panel {
    width: 100%;
    height: 100%;
    overflow-y: auto;
    scrollbar-width: none;
    font-family: var(--la-font-mono, 'JetBrains Mono', monospace);
    font-size: 9px;
  }
  .wt-panel::-webkit-scrollbar { display: none; }

  .wt-empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    height: 100%;
    gap: 6px;
    color: var(--la-text-mute);
  }
  .wt-empty-icon { font-size: 20px; opacity: 0.3; }
  .wt-empty-label { font-size: 9px; letter-spacing: 0.08em; }

  .wt-tree { padding: 4px 0; }

  .wt-row {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 2px 8px 2px 6px;
    min-height: 18px;
    transition: background 80ms;
  }
  .wt-row:hover { background: rgba(255,255,255,0.025); }

  .wt-repo {
    padding: 4px 8px;
    border-bottom: 1px solid var(--la-hair-base);
    margin-bottom: 2px;
  }
  .wt-icon { color: var(--la-struct-primary, #00c8ff); font-size: 8px; flex-shrink: 0; }
  .wt-repo-name {
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.08em;
    color: var(--la-text-stark);
    flex: 1;
  }
  .wt-badge {
    font-size: 7px;
    color: var(--la-struct-primary, #00c8ff);
    background: rgba(0, 200, 255, 0.1);
    border: 1px solid rgba(0, 200, 255, 0.25);
    padding: 0 4px;
    border-radius: 2px;
  }

  .wt-connector {
    font-size: 8px;
    color: var(--la-hair-strong, #1e3a52);
    flex-shrink: 0;
    width: 14px;
  }
  .wt-connector.dim { color: #1a2a38; }

  .wt-kind-tag {
    font-size: 6px;
    font-weight: 700;
    letter-spacing: 0.08em;
    color: var(--la-text-mute);
    background: var(--la-bg-elev-1, #111214);
    border: 1px solid var(--la-hair-base);
    padding: 0 3px;
    border-radius: 1px;
    flex-shrink: 0;
  }
  .wt-kind-tag.build {
    color: #38bdf8;
    border-color: rgba(56, 189, 248, 0.25);
    background: rgba(56, 189, 248, 0.06);
  }
  .wt-kind-tag.wave {
    color: var(--la-text-mute);
  }

  .wt-node-name {
    font-size: 9px;
    color: var(--la-text-base);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    flex: 1;
    min-width: 0;
  }
  .wt-node-name.build { color: var(--la-text-stark); font-weight: 600; }
  .wt-node-name.dim   { color: var(--la-text-mute); font-size: 8px; }

  .wt-lc {
    font-size: 6px;
    color: var(--la-text-mute);
    letter-spacing: 0.06em;
    flex-shrink: 0;
  }
  .wt-lc.active { color: #f5a623; }

  .wt-state-dot {
    width: 5px;
    height: 5px;
    border-radius: 50%;
    flex-shrink: 0;
  }

  .wt-domain {
    font-size: 7px;
    font-weight: 700;
    color: var(--la-text-dim);
    background: var(--la-bg-elev-2, #0a1520);
    border: 1px solid var(--la-hair-base);
    padding: 0 3px;
    border-radius: 1px;
    flex-shrink: 0;
    letter-spacing: 0.04em;
  }

  .wt-path {
    font-size: 7px;
    color: var(--la-text-mute);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    flex: 1;
    min-width: 0;
  }

  .wt-commits {
    font-size: 7px;
    color: #22c55e;
    flex-shrink: 0;
  }
</style>
