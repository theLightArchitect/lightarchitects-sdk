<script lang="ts">
  // SECURITY: escalation detail is display-only; resolution routes via /api/control
  // (nonce validated server-side). This panel never exposes or logs the nonce.
  type EscalationSource = 'pr' | 'conductor' | 'ironclaw';
  interface Props { source: EscalationSource; id: string; }
  let { source, id }: Props = $props();

  const sourceLabel: Record<EscalationSource, string> = {
    pr:         'Pull Request',
    conductor:  'Conductor',
    ironclaw:   'Ironclaw',
  };
  const sourceColor: Record<EscalationSource, string> = {
    pr:         'var(--scope-accent, #4da6ff)',
    conductor:  'var(--la-semantic-warn, #ff9800)',
    ironclaw:   'var(--la-semantic-err, #f44336)',
  };
</script>

<div class="focus-panel" data-focus-kind="escalation">
  <header class="focus-hdr">
    <span class="focus-kind" style="color: {sourceColor[source]}">ESCALATION</span>
    <span class="focus-source">{sourceLabel[source]}</span>
  </header>
  <section class="focus-body">
    <div class="field-row">
      <span class="field-label">SOURCE</span>
      <span class="field-value" style="color: {sourceColor[source]}">{source.toUpperCase()}</span>
    </div>
    <div class="field-row">
      <span class="field-label">ID</span>
      <span class="field-value field-mono">{id}</span>
    </div>
    <div class="focus-note">
      {#if source === 'ironclaw'}
        Ironclaw escalations resolve via HITL Escalations card → APPROVE/REJECT.
      {:else if source === 'conductor'}
        Conductor escalations require operator decision in the HITL Inbox.
      {:else}
        PR escalations are resolved by merging or closing the PR.
      {/if}
    </div>
  </section>
</div>

<style>
  .focus-panel { display: flex; flex-direction: column; height: 100%; overflow: hidden; }
  .focus-hdr {
    display: flex; align-items: center; gap: 8px;
    padding: 10px 12px; border-bottom: 1px solid var(--la-hair-base, rgba(255,255,255,0.06));
    flex-shrink: 0;
  }
  .focus-kind { font-size: 8px; font-weight: 700; letter-spacing: 0.14em; opacity: 0.8; }
  .focus-source { font-size: 10px; color: var(--la-text-dim, #888); }
  .focus-body { flex: 1; overflow-y: auto; padding: 8px 12px; display: flex; flex-direction: column; gap: 8px; }
  .field-row { display: flex; align-items: baseline; gap: 8px; }
  .field-label { font-size: 8px; font-weight: 700; letter-spacing: 0.1em; color: var(--la-text-mute, #555); min-width: 72px; flex-shrink: 0; }
  .field-value { font-size: 10px; color: var(--la-text-dim, #888); }
  .field-mono { font-family: var(--font-mono, monospace); font-size: 9px; }
  .focus-note { font-size: 9px; color: var(--la-text-mute, #555); opacity: 0.7; margin-top: 8px; line-height: 1.4; }
</style>
