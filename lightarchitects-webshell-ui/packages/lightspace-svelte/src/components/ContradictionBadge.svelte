<!--
  @component ContradictionBadge — amber warning indicator for cards with pending contradictions.
  @props count  Number of contradicting targets (renders nothing when 0)
-->
<script lang="ts">
  let { count = 0 }: { count?: number } = $props();
</script>
{#if count > 0}
  <span class="ls-contra-badge" aria-label="{count} contradiction{count > 1 ? 's' : ''} pending">
    ⚠ {count}
  </span>
{/if}
<style>
.ls-contra-badge {
  display: inline-flex;
  align-items: center;
  gap: 2px;
  font-size: 8px;
  padding: 1px 5px;
  background: color-mix(in srgb, var(--ls-acc-amber) 15%, transparent);
  border: 1px solid var(--ls-acc-amber);
  color: var(--ls-acc-amber);
  border-radius: 10px;
  font-family: var(--ls-font-code);
}
</style>
