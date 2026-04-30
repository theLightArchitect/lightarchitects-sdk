<script lang="ts">
  import { SIBLING_COLORS, PILLAR_COLORS } from '../lib/design-tokens';

  let open = $state(false);

  $effect(() => {
    function show()   { open = true; }
    function hide()   { open = false; }
    function toggle() { open = !open; }
    window.addEventListener('la:open-helix-legend',   show);
    window.addEventListener('la:close-helix-legend',  hide);
    window.addEventListener('la:toggle-helix-legend', toggle);
    return () => {
      window.removeEventListener('la:open-helix-legend',   show);
      window.removeEventListener('la:close-helix-legend',  hide);
      window.removeEventListener('la:toggle-helix-legend', toggle);
    };
  });

  function onEsc(e: KeyboardEvent) {
    if (open && e.key === 'Escape') { e.preventDefault(); open = false; }
  }

  // Sibling strands visible in the 3D helix visualization.
  const AGENT_ROWS: { id: string; label: string; role: string }[] = [
    { id: 'soul',    label: 'SOUL',    role: 'Knowledge graph · memory vault · convergences' },
    { id: 'corso',   label: 'CORSO',   role: 'AppSec · quality · build orchestration' },
    { id: 'eva',     label: 'EVA',     role: 'Consciousness · DevOps · hook pipeline' },
    { id: 'quantum', label: 'QUANTUM', role: 'Forensic analyst · research · hypothesis testing' },
    { id: 'seraph',  label: 'SERAPH',  role: 'Red team · pentest · offensive security' },
    { id: 'ayin',    label: 'AYIN',    role: 'Observability · tracing · runtime dashboards' },
  ];

  // LASDLC quality gates — color-coded in plan views and build reports.
  const PILLAR_ROWS: { id: string; label: string; meaning: string }[] = [
    { id: 'ARCH', label: 'Architecture', meaning: 'Design decisions · ADRs · system boundaries' },
    { id: 'SEC',  label: 'Security',     meaning: 'Threat model · pen-test findings · hardening' },
    { id: 'QUAL', label: 'Quality',      meaning: 'Clippy pedantic · complexity · coverage' },
    { id: 'PERF', label: 'Performance',  meaning: 'Benchmarks · VRAM math · latency budgets' },
    { id: 'TEST', label: 'Testing',      meaning: 'Unit · integration · E2E · property tests' },
    { id: 'DOC',  label: 'Documentation','meaning': 'Public API docs · CLAUDE.md · ADRs' },
    { id: 'OPS',  label: 'Operations',   meaning: 'Deploy · drain escalation · port handling' },
  ];
</script>

<svelte:window onkeydown={onEsc} />

{#if open}
  <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
  <div
    class="legend-scrim"
    role="dialog"
    aria-modal="true"
    aria-labelledby="helix-legend-title"
    data-testid="helix-legend"
    tabindex={-1}
    onclick={() => { open = false; }}
  >
    <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
    <div class="legend-modal" onclick={(e) => e.stopPropagation()} role="presentation">

      <header class="legend-header">
        <h2 id="helix-legend-title">What is the Helix?</h2>
        <button class="legend-close" aria-label="Close" onclick={() => { open = false; }}>×</button>
      </header>

      <div class="legend-body">

        <p class="legend-blurb">
          The 3D helix is a live graph of every agent's memory of you — each strand is one
          agent's contributions to the knowledge graph. Nodes are memory entries; arcs are
          convergences between agents. Brighter nodes were accessed more recently.
        </p>

        <section class="legend-group">
          <h3>Agent strands</h3>
          <table>
            <tbody>
              {#each AGENT_ROWS as row (row.id)}
                {@const color = SIBLING_COLORS[row.id] ?? '#6b7280'}
                <tr>
                  <td class="legend-dot-cell">
                    <span class="legend-dot" style="background:{color}; box-shadow:0 0 5px {color}66;"></span>
                    <span class="legend-name" style="color:{color}">{row.label}</span>
                  </td>
                  <td class="legend-role">{row.role}</td>
                </tr>
              {/each}
            </tbody>
          </table>
        </section>

        <section class="legend-group">
          <h3>LASDLC quality gates</h3>
          <table>
            <tbody>
              {#each PILLAR_ROWS as row (row.id)}
                {@const color = PILLAR_COLORS[row.id] ?? '#6b7280'}
                <tr>
                  <td class="legend-dot-cell">
                    <span class="legend-dot" style="background:{color};"></span>
                    <span class="legend-name" style="color:{color}">{row.label}</span>
                  </td>
                  <td class="legend-role">{row.meaning}</td>
                </tr>
              {/each}
            </tbody>
          </table>
        </section>

      </div>

      <footer class="legend-footer">
        <span><kbd>Esc</kbd> to dismiss</span>
      </footer>
    </div>
  </div>
{/if}

<style>
  .legend-scrim {
    position: fixed;
    inset: 0;
    z-index: 80;
    background: var(--la-scrim-color);
    backdrop-filter: blur(var(--la-scrim-blur));
    display: grid;
    place-items: center;
    animation: helix-legend-fade var(--la-transition-fast) ease-out;
  }
  .legend-modal {
    width: min(560px, 94vw);
    max-height: 82vh;
    display: flex;
    flex-direction: column;
    background: var(--la-drawer-bg);
    border: 1px solid var(--la-drawer-border);
    border-radius: var(--la-radius-lg);
    box-shadow: var(--la-drawer-shadow);
    color: var(--la-text-body);
    font-family: var(--la-font-chrome);
  }
  .legend-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 12px 16px 8px;
    border-bottom: 1px solid var(--la-drawer-border);
  }
  .legend-header h2 {
    margin: 0;
    font-size: 13px;
    font-weight: 600;
    color: #f0c040;
    letter-spacing: 0.02em;
  }
  .legend-close {
    background: transparent;
    border: none;
    color: var(--la-text-mute);
    font-size: 20px;
    line-height: 1;
    cursor: pointer;
    padding: 0 4px;
    border-radius: var(--la-radius-sm);
    transition: color var(--la-transition-fast), background var(--la-transition-fast);
  }
  .legend-close:hover { color: var(--la-text-body); background: #1e293b; }

  .legend-body {
    flex: 1;
    overflow-y: auto;
    padding: 10px 16px 4px;
  }
  .legend-blurb {
    margin: 0 0 12px;
    font-size: 11px;
    color: var(--la-text-mute);
    line-height: 1.6;
  }
  .legend-group {
    padding: 8px 0;
    border-bottom: 1px solid var(--la-drawer-border);
  }
  .legend-group:last-child { border-bottom: none; }
  .legend-group h3 {
    margin: 0 0 6px;
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--la-text-mute);
  }
  .legend-group table { width: 100%; border-collapse: collapse; }
  .legend-group td { padding: 3px 0; vertical-align: middle; }

  .legend-dot-cell {
    width: 140px;
    white-space: nowrap;
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .legend-dot {
    display: inline-block;
    width: 7px;
    height: 7px;
    border-radius: 50%;
    flex-shrink: 0;
  }
  .legend-name {
    font-family: var(--la-font-mono);
    font-size: 10px;
    font-weight: 600;
    letter-spacing: 0.04em;
  }
  .legend-role {
    font-size: 11px;
    color: var(--la-text-body);
    padding-left: 8px;
  }

  .legend-footer {
    padding: 8px 16px;
    border-top: 1px solid var(--la-drawer-border);
    color: var(--la-text-mute);
    font-size: 10px;
    text-align: right;
  }
  .legend-footer kbd {
    background: #1e293b;
    padding: 1px 4px;
    border-radius: var(--la-radius-sm);
    font-family: var(--la-font-mono);
  }

  @keyframes helix-legend-fade {
    from { opacity: 0; }
    to   { opacity: 1; }
  }
</style>
