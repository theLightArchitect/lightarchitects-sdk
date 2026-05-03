<script lang="ts">
  // Wave 1 of the Recursive Rainbow build: thin label strip that sits above the
  // primary nav and answers "where am I?" at a glance. Slash-separated segments,
  // matches the prototype's `LIGHT ARCHITECTS / {SCREEN} / {SUB} / LIVE` pattern.
  //
  // Takes `route` as a prop rather than reading currentRoute directly — this
  // keeps the component a pure function of its input and lets the parent decide
  // which store to subscribe to (app.svelte has its own currentRoute writable
  // distinct from the one in $lib/stores; see app.svelte:44).

  let { route = '/' }: { route?: string } = $props();

  // Strip query and fragment for label derivation; keep the path for matching.
  let path = $derived((route || '/').split('?')[0].split('#')[0] || '/');

  // Static label table for the primary screens. Keep in sync with NAV_ITEMS in
  // app.svelte:150-155 — the breadcrumb is a chrome layer, not a router.
  const SCREEN_LABELS: Record<string, [string, string]> = {
    ops:      ['OPS',      'KNOWLEDGE GRAPH'],
    dispatch: ['DISPATCH', 'EXECUTION CONSOLE'],
    builds:   ['BUILDS',   'PIPELINE'],
    intake:   ['INTAKE',   'NEW MISSION'],
    helix:    ['HELIX',    'KNOWLEDGE GRAPH'],
    project:  ['PROJECT',  'DETAIL'],
    workspace: ['WORKSPACE','LEGACY'], // matched only briefly before redirect fires
  };

  // Derive [screen, sub] from the path's first non-empty segment.
  // Deeper levels (e.g. /builds/:id/:view) will be surfaced in Wave 6 once
  // BuildDetail wires the URL :view param; for now /builds and /builds/:id
  // both show the same "BUILDS / PIPELINE" label.
  let labels = $derived.by(() => {
    if (path === '/' || path === '') return SCREEN_LABELS.builds; // default landing
    const seg = path.split('/').filter(Boolean)[0]?.toLowerCase() ?? '';
    return SCREEN_LABELS[seg] ?? ['—', '—'];
  });

  let screen = $derived(labels[0]);
  let sub    = $derived(labels[1]);
</script>

<!--
  44px chrome strip. Same border + bg treatment as the nav directly below it
  (app.svelte:374) so the two strips read as a unified header. Letter-spacing
  matches the tactical-monospace aesthetic from the prototype.
-->
<div
  class="flex items-center gap-2 px-4 h-[28px] border-b border-[#1e293b] bg-[#0a0a0f] shrink-0 text-[10px] font-mono tracking-[0.18em] text-[#475569]"
  data-testid="breadcrumb"
  aria-label="breadcrumb"
>
  <span class="text-[#94a3b8] font-bold">LIGHT ARCHITECTS</span>
  <span class="text-[#1e293b]">/</span>
  <span class="text-[#cbd5e1]" data-testid="breadcrumb-screen">{screen}</span>
  <span class="text-[#1e293b]">/</span>
  <span class="text-[#64748b]" data-testid="breadcrumb-sub">{sub}</span>
  <span class="text-[#1e293b]">/</span>
  <!-- LIVE indicator — pulses when SSE is connected; static when offline.
       For Wave 1 we render unconditionally; Wave 1.5 will wire to ayinStatus. -->
  <span class="text-[#FFD700] font-bold flex items-center gap-1.5">
    <span class="w-[5px] h-[5px] rounded-full bg-[#FFD700] shadow-[0_0_4px_#FFD700]"></span>
    LIVE
  </span>
</div>
