<script lang="ts">
  // Status rail — bottom 30px footer.
  // Shows live telemetry: spans, last event, throughput sparkline, tokens.
  import { ls } from '$lib/lightspace/state.svelte';

  const maxThroughput = $derived(Math.max(...ls.throughputHistory, 1));
</script>

<footer class="la-statusrail">
  <div class="la-sr-section">
    <span class="lab">spans</span>
    <span class="val ok">{ls.spans}</span>
  </div>
  <div class="la-sr-divider"></div>
  <div class="la-sr-section">
    <span class="lab">last event</span>
    <span class="val">{ls.lastEvent}</span>
  </div>
  <div class="la-sr-divider"></div>
  <div class="la-sr-section">
    <span class="lab">throughput</span>
    <span class="val">{ls.throughput.toFixed(1)} evt/s</span>
    <span class="la-sparkline" aria-hidden="true">
      {#each ls.throughputHistory as v, i}
        <span
          class="bar"
          class:tip={i === ls.throughputHistory.length - 1}
          style="height:{Math.round((v / maxThroughput) * 12)}px"
        ></span>
      {/each}
    </span>
  </div>
  <div class="la-sr-divider"></div>
  <div class="la-sr-section">
    <span class="lab">contradictions</span>
    <span class="val" class:warn={ls.contradictions > 0}>{ls.contradictions}</span>
  </div>
  <div class="la-sr-divider"></div>
  <div class="la-sr-section">
    <span class="lab">replay</span>
    <span class="val acc">recording</span>
  </div>
  <div class="la-sr-section la-sr-spend">
    <span class="lab">tokens</span>
    <span class="val">{(ls.tokens / 1000).toFixed(0)}k / 1m</span>
  </div>
</footer>

<style>
.la-statusrail {
  grid-area: status;
  display: flex; align-items: center;
  padding: 0 14px;
  border-top: 1px solid var(--la-hair-base);
  background: var(--la-bg-sunken);
  font-size: 9px; letter-spacing: var(--la-tk-mid);
  transition: opacity var(--la-mid);
}
.la-sr-section { display: flex; align-items: center; gap: 5px; padding: 0 12px 0 0; }
.la-sr-divider { width: 1px; height: 14px; background: var(--la-hair-base); margin: 0 12px 0 0; }
.la-sr-section .lab { color: var(--la-text-ghost); text-transform: uppercase; font-size: 8px; }
.la-sr-section .val { color: var(--la-text-dim); font-variant-numeric: tabular-nums; }
.la-sr-section .val.ok   { color: var(--la-ok); }
.la-sr-section .val.acc  { color: var(--la-acc); }
.la-sr-section .val.warn { color: var(--la-warn); }
.la-sr-spend { margin-left: auto; }

.la-sparkline { display: inline-flex; gap: 1px; height: 12px; align-items: flex-end; margin-left: 6px; }
.la-sparkline .bar { width: 2px; min-height: 1px; background: var(--la-acc); opacity: 0.45; transition: height 0.3s ease; }
.la-sparkline .bar.tip { opacity: 1; background: var(--la-ok); box-shadow: 0 0 4px var(--la-ok); }
</style>
