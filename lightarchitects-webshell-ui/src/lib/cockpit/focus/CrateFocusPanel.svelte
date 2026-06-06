<script lang="ts">
  interface Props { name: string; }
  let { name }: Props = $props();

  const docsUrl = $derived(`https://docs.rs/${name}`);
</script>

<div class="focus-panel" data-focus-kind="crate">
  <header class="focus-hdr">
    <span class="focus-kind">CRATE</span>
    <span class="focus-name">{name}</span>
  </header>
  <section class="focus-body">
    <div class="field-row">
      <span class="field-label">NAME</span>
      <span class="field-value field-mono">{name}</span>
    </div>
    <div class="crate-actions">
      <a
        class="crate-link"
        href={docsUrl}
        target="_blank"
        rel="noopener noreferrer"
        aria-label="Open {name} on docs.rs"
      >DOCS.RS ↗</a>
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
  .focus-kind { font-size: 8px; font-weight: 700; letter-spacing: 0.14em; color: var(--la-semantic-ok, #4caf50); opacity: 0.7; }
  .focus-name { font-family: var(--font-mono, monospace); font-size: 10px; color: var(--la-text-dim, #888); }
  .focus-body { flex: 1; overflow-y: auto; padding: 8px 12px; display: flex; flex-direction: column; gap: 8px; }
  .field-row { display: flex; align-items: baseline; gap: 8px; }
  .field-label { font-size: 8px; font-weight: 700; letter-spacing: 0.1em; color: var(--la-text-mute, #555); min-width: 72px; flex-shrink: 0; }
  .field-value { font-size: 10px; color: var(--la-text-dim, #888); }
  .field-mono { font-family: var(--font-mono, monospace); font-size: 9px; }
  .crate-actions { margin-top: 4px; }
  .crate-link {
    font-size: 8px; font-weight: 700; letter-spacing: 0.1em;
    color: var(--la-semantic-ok, #4caf50); text-decoration: none;
    border-bottom: 1px solid var(--la-semantic-ok, #4caf50); opacity: 0.8;
  }
  .crate-link:hover { opacity: 1; }
</style>
