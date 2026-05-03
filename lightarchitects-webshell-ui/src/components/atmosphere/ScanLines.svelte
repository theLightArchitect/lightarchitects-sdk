<script lang="ts">
  import { scanLinesEnabled } from '$lib/atmosphere';

  /**
   * Optional route prop — the parent passes activeRoute so ScanLines can
   * skip rendering on /helix (Locked Decision #3: no scan lines on /helix).
   */
  let { route = '/' }: { route?: string } = $props();

  let isHelix = $derived((route || '/').startsWith('/helix'));
  let show = $derived($scanLinesEnabled && !isHelix);
</script>

{#if show}
  <!--
    Pure CSS scan-line overlay. Uses a repeating linear gradient of alternating
    transparent / semi-opaque rows. pointer-events: none so the overlay never
    intercepts clicks. z-index: 9 sits above screen content but below all drawers
    and overlays (z-20+).
  -->
  <div
    class="fixed inset-0 pointer-events-none"
    style="
      z-index: 9;
      background: repeating-linear-gradient(
        to bottom,
        transparent 0px,
        transparent 2px,
        rgba(0,0,0,0.06) 2px,
        rgba(0,0,0,0.06) 3px
      );
    "
    aria-hidden="true"
    data-testid="scanlines"
  ></div>
{/if}
