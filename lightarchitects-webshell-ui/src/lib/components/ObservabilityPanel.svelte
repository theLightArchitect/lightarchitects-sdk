<script lang="ts">
  /** ObservabilityPanel — /#/observability — embeds the AYIN dashboard at :3742. */

  const AYIN_URL = 'http://127.0.0.1:3742';

  let status: 'loading' | 'ready' | 'error' = $state('loading');

  function onLoad() { status = 'ready'; }
  function onError() { status = 'error'; }
</script>

<div class="obs-root">
  {#if status === 'error'}
    <div class="obs-error">
      <span class="obs-icon">⬡</span>
      <p>AYIN is not running.</p>
      <p class="obs-hint">Start it with <code>make deploy && launchctl kickstart -k gui/$(id -u)/io.lightarchitects.ayin</code></p>
    </div>
  {/if}

  <iframe
    src={AYIN_URL}
    title="AYIN Observability Dashboard"
    class="obs-frame"
    class:obs-frame--hidden={status === 'error'}
    onload={onLoad}
    onerror={onError}
    sandbox="allow-scripts allow-same-origin allow-forms"
  ></iframe>
</div>

<style>
  .obs-root {
    position: relative;
    width: 100%;
    height: 100%;
    display: flex;
    flex-direction: column;
    background: #0b1014;
  }

  .obs-frame {
    flex: 1;
    width: 100%;
    height: 100%;
    border: none;
  }

  .obs-frame--hidden {
    display: none;
  }

  .obs-error {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 0.5rem;
    color: #7a8ea0;
    font-family: 'Berkeley Mono', monospace;
  }

  .obs-icon {
    font-size: 2.5rem;
    color: #1ecbe133;
  }

  .obs-error p { margin: 0; font-size: 0.9rem; }

  .obs-hint {
    font-size: 0.75rem !important;
    color: #4a5a6a;
    text-align: center;
    max-width: 42ch;
  }

  .obs-hint code {
    color: #1ecbe1;
    font-size: 0.7rem;
  }
</style>
