<script lang="ts">
  /**
   * SharedSlotBar — 7-slot agent pool view at panel header.
   *
   * Slots are ordered by assignment; unoccupied slots show as IDLE.
   * Model labels: SON = claude-sonnet-*, HAI = claude-haiku-*, OLL = ollama/*,
   * GATE = agent waiting at a quality gate, IDLE = no assignment.
   *
   * Phase 1 scaffold: static prop-driven. Phase 5/6 wires the `slotAssignments`
   * store and animates on assignment change events.
   */

  import type { WorktreeAssignment } from '$lib/gitforest';

  interface Props {
    /** Active worktree assignments to display. Max 7 rendered; extras are clipped. */
    assignments: WorktreeAssignment[];
    /** Total pool capacity (default: 7). */
    capacity?: number;
  }

  const { assignments, capacity = 7 }: Props = $props();

  /** Map model ID prefix → 3-char label. */
  function modelLabel(modelId: string): string {
    if (modelId.startsWith('claude-sonnet') || modelId === 'sonnet') return 'SON';
    if (modelId.startsWith('claude-haiku') || modelId === 'haiku') return 'HAI';
    if (modelId.startsWith('claude-opus') || modelId === 'opus') return 'OPS';
    if (modelId.startsWith('ollama') || modelId.startsWith('llama')) return 'OLL';
    return modelId.slice(0, 3).toUpperCase();
  }

  /** CSS class for the slot based on worktree state. */
  function stateClass(state: WorktreeAssignment['state']): string {
    switch (state) {
      case 'writing': return 'slot--writing';
      case 'gate':    return 'slot--gate';
      case 'done':    return 'slot--done';
      case 'failed':  return 'slot--failed';
    }
  }

  /** Domain abbreviation for the tooltip / inner label. */
  function domainAbbr(domain: string): string {
    const map: Record<string, string> = {
      engineer: 'ENG', quality: 'QUA', security: 'SEC',
      ops: 'OPS', researcher: 'RES', knowledge: 'KNW',
      testing: 'TST', squad: 'SQD',
    };
    return map[domain] ?? domain.slice(0, 3).toUpperCase();
  }

  const filled = $derived(assignments.slice(0, capacity));
  const idle   = $derived(Math.max(0, capacity - filled.length));
</script>

<div class="slot-bar" role="list" aria-label="Agent slot pool">
  {#each filled as wt (wt.agent_key)}
    <div
      class="slot {stateClass(wt.state)}"
      role="listitem"
      title="{wt.domain} · {wt.agent_key} · {wt.commits} commits"
    >
      <span class="slot-model">{modelLabel(wt.agent_key)}</span>
      <span class="slot-domain">{domainAbbr(wt.domain)}</span>
    </div>
  {/each}

  {#each { length: idle } as _, i (i)}
    <div class="slot slot--idle" role="listitem" aria-label="Idle slot">
      <span class="slot-model">IDL</span>
    </div>
  {/each}
</div>

<style>
  .slot-bar {
    display: flex;
    gap: 3px;
    align-items: center;
    padding: 4px 6px;
    background: rgba(2, 4, 8, 0.72);
    border: 1px solid rgba(0, 200, 255, 0.12);
    border-radius: 6px;
    width: fit-content;
  }

  .slot {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    width: 34px;
    height: 34px;
    border-radius: 4px;
    border: 1px solid transparent;
    transition: background 180ms ease, border-color 180ms ease;
    gap: 2px;
  }

  .slot-model {
    font: 700 8px/1 monospace;
    letter-spacing: 0.04em;
  }

  .slot-domain {
    font: 500 7px/1 monospace;
    opacity: 0.6;
  }

  /* State variants */
  .slot--idle {
    background: rgba(255, 255, 255, 0.03);
    border-color: rgba(255, 255, 255, 0.06);
  }
  .slot--idle .slot-model { color: #334155; }

  .slot--writing {
    background: rgba(0, 200, 255, 0.10);
    border-color: rgba(0, 200, 255, 0.30);
    animation: pulse-write 1.8s ease-in-out infinite;
  }
  .slot--writing .slot-model { color: #00c8ff; }
  .slot--writing .slot-domain { color: #7dd3fc; }

  .slot--gate {
    background: rgba(251, 191, 36, 0.10);
    border-color: rgba(251, 191, 36, 0.35);
  }
  .slot--gate .slot-model { color: #fbbf24; }
  .slot--gate .slot-domain { color: #fde68a; }

  .slot--done {
    background: rgba(34, 197, 94, 0.10);
    border-color: rgba(34, 197, 94, 0.30);
  }
  .slot--done .slot-model { color: #22c55e; }
  .slot--done .slot-domain { color: #86efac; }

  .slot--failed {
    background: rgba(239, 68, 68, 0.10);
    border-color: rgba(239, 68, 68, 0.30);
  }
  .slot--failed .slot-model { color: #ef4444; }
  .slot--failed .slot-domain { color: #fca5a5; }

  @keyframes pulse-write {
    0%, 100% { opacity: 1; }
    50% { opacity: 0.65; }
  }
</style>
