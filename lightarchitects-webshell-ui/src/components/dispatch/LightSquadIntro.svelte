<script lang="ts">
  import { onMount } from 'svelte';

  const SEEN_KEY = 'la_lasdlc_intro_seen';

  let visible = $state(false);

  onMount(() => {
    if (!localStorage.getItem(SEEN_KEY)) visible = true;
  });

  function dismiss() {
    localStorage.setItem(SEEN_KEY, '1');
    visible = false;
  }
</script>

{#if visible}
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <div
    class="intro-backdrop"
    role="dialog"
    aria-modal="true"
    aria-label="LightSquad introduction"
    onkeydown={(e) => { if (e.key === 'Escape') dismiss(); }}
    onclick={dismiss}
  >
    <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
    <div class="intro-panel" data-testid="lasdlc-intro-panel" onclick={(e) => e.stopPropagation()}>
      <header class="intro-head">
        <span class="intro-logo">[ LIGHTSQUAD ]</span>
        <span class="intro-sub">LASDLC — Light Architects Software Development Lifecycle</span>
      </header>

      <p class="intro-lead">
        Every dispatch runs through <strong>7 quality gates</strong>.
        Pick agents — they handle the gates. Follow the framework, get consistent results.
      </p>

      <ul class="gate-grid" aria-label="LASDLC quality gates">
        <li class="gate-item" data-gate="A">
          <span class="gate-badge">[A]</span>
          <span class="gate-name">ARCH</span>
          <span class="gate-desc">Architecture, correctness, API design</span>
        </li>
        <li class="gate-item" data-gate="S">
          <span class="gate-badge">[S]</span>
          <span class="gate-name">SEC</span>
          <span class="gate-desc">Threat surface, vulns, supply chain</span>
        </li>
        <li class="gate-item" data-gate="Q">
          <span class="gate-badge">[Q]</span>
          <span class="gate-name">QUAL</span>
          <span class="gate-desc">Standards, linting, complexity ≤10</span>
        </li>
        <li class="gate-item" data-gate="T">
          <span class="gate-badge">[T]</span>
          <span class="gate-name">TEST</span>
          <span class="gate-desc">6-suite pyramid, ≥90% coverage</span>
        </li>
        <li class="gate-item" data-gate="P">
          <span class="gate-badge">[P]</span>
          <span class="gate-name">PERF</span>
          <span class="gate-desc">Latency, throughput, O(n) bounds</span>
        </li>
        <li class="gate-item" data-gate="K">
          <span class="gate-badge">[K]</span>
          <span class="gate-name">KNOW</span>
          <span class="gate-desc">Docs, enrichment, prior decisions</span>
        </li>
        <li class="gate-item" data-gate="O">
          <span class="gate-badge">[O]</span>
          <span class="gate-name">OPS</span>
          <span class="gate-desc">Deploy pipeline, CI/CD, rollback</span>
        </li>
      </ul>

      <p class="intro-tip">
        <strong>Try it:</strong> type <code>"Plan a new Rust API endpoint"</code> and hit <kbd>⌘↵</kbd>
      </p>

      <footer class="intro-foot">
        <button class="btn-skip" onclick={dismiss}>Skip</button>
        <!-- autofocus: lands keyboard focus here so Tab cycles within the dialog -->
        <button class="btn-got-it" onclick={dismiss} autofocus>Got it →</button>
      </footer>
    </div>
  </div>
{/if}

<style>
  .intro-backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.75);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 500;
    padding: 1rem;
  }

  .intro-panel {
    background: var(--la-bg-elev-1, #111);
    border: 1px solid var(--la-hair-strong, #444);
    max-width: 520px;
    width: 100%;
    display: flex;
    flex-direction: column;
    gap: 16px;
    padding: 24px;
    font-family: var(--la-font-chrome, monospace);
  }

  .intro-head {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .intro-logo {
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.16em;
    color: var(--la-agent-researcher, #4dffe6);
  }

  .intro-sub {
    font-size: 9px;
    letter-spacing: 0.1em;
    color: var(--la-text-mute, #666);
    text-transform: uppercase;
  }

  .intro-lead {
    font-size: 11px;
    color: var(--la-text-base, #ccc);
    line-height: 1.6;
    margin: 0;
  }

  .intro-lead strong { color: var(--la-text-bright, #fff); }

  .gate-grid {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(140px, 1fr));
    gap: 6px;
  }

  .gate-item {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 7px 9px;
    border: 1px solid var(--la-hair-base, #333);
    background: var(--la-bg-void, #000);
  }

  .gate-badge {
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.1em;
    color: var(--la-text-mute, #666);
  }

  .gate-item[data-gate="A"] .gate-badge { color: var(--la-agent-engineer, #4d8eff); }
  .gate-item[data-gate="S"] .gate-badge { color: var(--la-agent-security, #ff4d4d); }
  .gate-item[data-gate="Q"] .gate-badge { color: var(--la-agent-quality, #a874ff); }
  .gate-item[data-gate="T"] .gate-badge { color: var(--la-agent-testing, #4dff8e); }
  .gate-item[data-gate="P"] .gate-badge { color: var(--la-agent-ops, #ff8e3c); }
  .gate-item[data-gate="K"] .gate-badge { color: var(--la-agent-knowledge, #f5d440); }
  .gate-item[data-gate="O"] .gate-badge { color: var(--la-agent-ops, #ff8e3c); }

  .gate-name {
    font-size: 10px;
    font-weight: 700;
    letter-spacing: 0.08em;
    color: var(--la-text-bright, #fff);
    text-transform: uppercase;
  }

  .gate-desc {
    font-size: 8px;
    color: var(--la-text-mute, #666);
    line-height: 1.4;
  }

  .intro-tip {
    font-size: 10px;
    color: var(--la-text-dim, #888);
    line-height: 1.5;
    margin: 0;
    padding: 8px 10px;
    border: 1px solid var(--la-hair-base, #333);
    background: var(--la-bg-void, #000);
  }

  .intro-tip strong { color: var(--la-text-base, #ccc); }

  .intro-tip code {
    font-family: inherit;
    color: var(--la-agent-researcher, #4dffe6);
    background: transparent;
  }

  .intro-tip kbd {
    font-family: inherit;
    font-size: 9px;
    padding: 1px 4px;
    border: 1px solid var(--la-hair-strong, #555);
    color: var(--la-text-base, #ccc);
  }

  .intro-foot {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    padding-top: 4px;
    border-top: 1px solid var(--la-hair-base, #333);
  }

  .btn-skip, .btn-got-it {
    font-family: inherit;
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.12em;
    text-transform: uppercase;
    padding: 5px 14px;
    cursor: pointer;
    transition: background 80ms, color 80ms;
  }

  .btn-skip {
    background: transparent;
    border: 1px solid var(--la-hair-base, #333);
    color: var(--la-text-mute, #666);
  }
  .btn-skip:hover {
    border-color: var(--la-hair-strong, #555);
    color: var(--la-text-base, #ccc);
  }

  .btn-got-it {
    background: transparent;
    border: 1px solid var(--la-agent-researcher, #4dffe6);
    color: var(--la-agent-researcher, #4dffe6);
  }
  .btn-got-it:hover {
    background: color-mix(in srgb, var(--la-agent-researcher, #4dffe6) 12%, transparent);
  }

  @supports not (color: color-mix(in srgb, red 50%, blue)) {
    .btn-got-it:hover { background: rgba(77, 255, 230, 0.08); }
  }
</style>
