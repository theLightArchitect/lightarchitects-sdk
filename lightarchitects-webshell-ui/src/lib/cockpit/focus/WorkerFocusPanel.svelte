<script lang="ts">
  interface Props { worker_id: string; build_codename: string; }
  let { worker_id, build_codename }: Props = $props();
</script>

<div class="focus-panel" data-focus-kind="worker">
  <header class="focus-hdr">
    <span class="focus-kind">WORKER</span>
    <span class="focus-id">{worker_id}</span>
  </header>
  <section class="focus-body">
    <div class="field-row">
      <span class="field-label">BUILD</span>
      <span class="field-value field-mono">{build_codename}</span>
    </div>
    <div class="field-row">
      <span class="field-label">WORKER ID</span>
      <span class="field-value field-mono">{worker_id}</span>
    </div>
    <div class="focus-note">Select a worker from the Worker Fleet card to see slot detail.</div>
  </section>
</div>

<style>
  .focus-panel { display: flex; flex-direction: column; height: 100%; overflow: hidden; }
  .focus-hdr {
    display: flex; align-items: center; gap: 8px;
    padding: 10px 12px; border-bottom: 1px solid var(--la-hair-base, rgba(255,255,255,0.06));
    flex-shrink: 0;
  }
  .focus-kind { font-size: 8px; font-weight: 700; letter-spacing: 0.14em; color: var(--scope-accent, var(--scope-d2)); opacity: 0.7; }
  .focus-id { font-family: var(--font-mono, monospace); font-size: 10px; color: var(--la-text-dim, #888); flex: 1; overflow: hidden; text-overflow: ellipsis; }
  .focus-body { flex: 1; overflow-y: auto; padding: 8px 12px; display: flex; flex-direction: column; gap: 8px; }
  .field-row { display: flex; align-items: baseline; gap: 8px; }
  .field-label { font-size: 8px; font-weight: 700; letter-spacing: 0.1em; color: var(--la-text-mute, #555); min-width: 72px; flex-shrink: 0; }
  .field-value { font-size: 10px; color: var(--la-text-dim, #888); }
  .field-mono { font-family: var(--font-mono, monospace); font-size: 9px; }
  .focus-note { font-size: 9px; color: var(--la-text-mute, #555); opacity: 0.6; margin-top: 8px; }
</style>
