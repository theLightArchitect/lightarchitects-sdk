<!--
  DevModeToggle — inline switch for enabling/disabling dev-mode features
  (CDP bridge, Playwright inspection, etc.).
-->
<script lang="ts">
  import { devModeEnabled } from '$lib/stores';
</script>

<label
  class="flex items-center gap-2 cursor-pointer select-none text-xs text-[var(--la-text-dim)]"
  data-testid="dev-mode-toggle"
  data-card-role="dev-mode-toggle"
>
  <input
    type="checkbox"
    bind:checked={$devModeEnabled}
    class="accent-[var(--la-focus-ring)] w-3.5 h-3.5"
  />
  <span>Dev mode</span>
</label>