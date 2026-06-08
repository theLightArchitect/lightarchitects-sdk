<!--
  @component HitlQueue — HITL escalation pills with reactive age labels.
  @reads hitlStore (Map<string, HitlItem>) from ../stores
         Age updates every 10s via setInterval inside $effect.
-->
<script lang="ts" module>
  /** Format elapsed seconds into a human-readable age label.
   *  Exported for direct testing (HitlQueue.test.ts). */
  export function formatAge(ageSeconds: number): string {
    if (ageSeconds < 60) return '< 1 min';
    const mins = Math.floor(ageSeconds / 60);
    if (mins < 60) return `${mins} min`;
    const hrs = Math.floor(mins / 60);
    return `${hrs} hr`;
  }
</script>

<script lang="ts">
  import { hitlStore } from '../stores';

  let tick = $state(Date.now());

  $effect(() => {
    const id = setInterval(() => { tick = Date.now(); }, 10_000);
    return () => clearInterval(id);
  });

  const items = $derived([...$hitlStore.values()]);
  const ageOf = (insertedAt: number) => formatAge(Math.floor((tick - insertedAt) / 1000));
</script>

{#if items.length > 0}
  <div class="ls-hitl-queue" aria-label="HITL escalation queue">
    {#each items as item (item.id)}
      <div class="ls-hitl-pill" role="status">
        <span class="ls-hitl-dot" aria-hidden="true"></span>
        <span class="ls-hitl-text">
          HITL{item.gate ? ` · gate [${item.gate}]` : ''} · {ageOf(item.inserted_at)}
        </span>
        <span class="ls-hitl-label">{item.label}</span>
      </div>
    {/each}
  </div>
{/if}

<style>
.ls-hitl-queue { display: flex; flex-direction: column; gap: 4px; padding: 6px 0; }
.ls-hitl-pill  {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 4px 8px;
  background: color-mix(in srgb, var(--ls-acc-amber) 12%, var(--ls-card));
  border: 1px solid var(--ls-acc-amber);
  border-radius: 3px;
  font-size: 9px;
}
.ls-hitl-dot   { width: 5px; height: 5px; border-radius: 50%; background: var(--ls-acc-amber); flex-shrink: 0; }
.ls-hitl-text  { color: var(--ls-acc-amber); font-family: var(--ls-font-code); white-space: nowrap; }
.ls-hitl-label { color: var(--ls-text-dim); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; flex: 1; }
</style>
