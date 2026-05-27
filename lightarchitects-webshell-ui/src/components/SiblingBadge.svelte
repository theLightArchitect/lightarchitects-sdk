<script lang="ts">
  import { SIBLING_COLORS } from '$lib/design-tokens';

  interface Props {
    /** Sibling identifier, e.g. "eva", "corso". */
    sibling: string;
    /** Visual density: sm = compact pill, md = pill with larger dot. */
    size?: 'sm' | 'md';
  }

  let { sibling, size = 'sm' }: Props = $props();

  const color = $derived((SIBLING_COLORS as Record<string, string>)[sibling] ?? '#6b7280');
  const label = $derived(sibling.toUpperCase());
</script>

<!-- Sibling attribution badge used in multi-voice chatroom messages.
     data-testid is stable across re-renders for Playwright selectors. -->
<span
  class="sibling-badge sibling-badge--{size}"
  data-testid="sibling-badge-{sibling}"
  aria-label="{label} response"
  title="{label}"
>
  <span
    class="sibling-badge__dot"
    aria-hidden="true"
    style="background: {color}; box-shadow: 0 0 4px {color}55;"
  ></span>
  <span class="sibling-badge__name" style="color: {color};">{label}</span>
</span>

<style>
  .sibling-badge {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font-family: var(--la-font-mono, monospace);
    font-weight: 700;
    letter-spacing: 0.08em;
    flex-shrink: 0;
  }

  .sibling-badge--sm {
    font-size: 9px;
  }

  .sibling-badge--md {
    font-size: 11px;
  }

  .sibling-badge__dot {
    display: inline-block;
    border-radius: 50%;
    flex-shrink: 0;
  }

  .sibling-badge--sm .sibling-badge__dot {
    width: 5px;
    height: 5px;
  }

  .sibling-badge--md .sibling-badge__dot {
    width: 7px;
    height: 7px;
  }

  .sibling-badge__name {
    /* color set inline — driven by SIBLING_COLORS token */
  }
</style>
