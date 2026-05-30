<script lang="ts">
  /** ObservabilityPanel — /#/observability — embeds the AYIN dashboard at :3742. */
  import { onMount } from 'svelte';

  const AYIN_URL = 'http://127.0.0.1:3742';

  let status: 'loading' | 'ready' | 'error' | 'blocked' = $state('loading');
  let frameEl: HTMLIFrameElement | undefined = $state();

  function onLoad() {
    // For cross-origin iframes, contentDocument access throws SecurityError.
    // A blocked frame (X-Frame-Options) fires load but renders nothing; we
    // detect this by probing the document. Any exception means same-origin
    // access was denied — frame blocked.
    try {
      const doc = frameEl?.contentDocument;
      if (doc && doc.body && doc.body.innerHTML.trim().length > 0) {
        status = 'ready';
      } else {
        status = 'blocked';
      }
    } catch {
      // SecurityError — cross-origin frame that loaded but is blocked.
      status = 'blocked';
    }
  }

  function onError() { status = 'error'; }

  function openInTab() { window.open(AYIN_URL, '_blank', 'noopener'); }

  // Probe AYIN reachability on mount. `no-cors` mode succeeds on any HTTP
  // response (including 200/401/500) and fails only on network errors.
  // This distinguishes "offline" from "running but blocks framing".
  onMount(() => {
    const controller = new AbortController();
    const timeout = setTimeout(() => controller.abort(), 4000);

    fetch(AYIN_URL, { mode: 'no-cors', signal: controller.signal })
      .then(() => {
        // AYIN is reachable. If the iframe hasn't fired onLoad yet, set a
        // secondary timeout: if still 'loading' after 2s, assume framing block.
        setTimeout(() => {
          if (status === 'loading') status = 'blocked';
        }, 2000);
      })
      .catch(() => {
        if (status === 'loading') status = 'error';
      })
      .finally(() => clearTimeout(timeout));

    return () => controller.abort();
  });
</script>

<div class="obs-root">
  {#if status === 'error'}
    <div class="obs-error">
      <span class="obs-icon">⬡</span>
      <p>AYIN is not running.</p>
      <p class="obs-hint">Start it with <code>make deploy && launchctl kickstart -k gui/$(id -u)/io.lightarchitects.ayin</code></p>
    </div>
  {:else if status === 'blocked'}
    <div class="obs-error">
      <span class="obs-icon">⬡</span>
      <p>AYIN is running but blocks frame embedding.</p>
      <p class="obs-hint">The AYIN server sends an <code>X-Frame-Options</code> header that prevents inline display.</p>
      <button class="obs-open-btn" onclick={openInTab}>Open AYIN in new tab →</button>
    </div>
  {/if}

  <iframe
    bind:this={frameEl}
    src={AYIN_URL}
    title="AYIN Observability Dashboard"
    class="obs-frame"
    class:obs-frame--hidden={status === 'error' || status === 'blocked'}
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

  .obs-open-btn {
    margin-top: 0.5rem;
    padding: 0.4rem 1rem;
    background: transparent;
    border: 1px solid #1ecbe1;
    border-radius: 4px;
    color: #1ecbe1;
    font-family: 'Berkeley Mono', monospace;
    font-size: 0.75rem;
    cursor: pointer;
    transition: background 0.15s, color 0.15s;
  }

  .obs-open-btn:hover {
    background: #1ecbe122;
  }
</style>
