<script lang="ts">
  // Git screen — wraps GitOpsPanel with a project-directory picker.
  // Route: /git
  // The `cwd` state is initialized from the `cwd` URL param if present,
  // falling back to a sensible default (the server's working directory).
  import GitOpsPanel from '../components/git/GitOpsPanel.svelte';
  import { page } from '$app/state';

  // Working directory — read from ?cwd= query param or default to '.'.
  let cwd = $state(page.url.searchParams.get('cwd') ?? '.');

  // Tracks the live value in the input before confirming.
  let cwdInput = $state(cwd);

  function applyDirectory() {
    const trimmed = cwdInput.trim();
    if (trimmed) cwd = trimmed;
  }
</script>

<div class="git-screen" data-testid="git-screen">
  <div class="screen-header">
    <span class="screen-title">Git</span>
    <div class="cwd-row">
      <label class="cwd-label" for="git-cwd">Directory</label>
      <input
        id="git-cwd"
        class="cwd-input"
        type="text"
        aria-label="Working directory"
        data-testid="git-cwd-input"
        bind:value={cwdInput}
        onkeydown={(e) => { if (e.key === 'Enter') applyDirectory(); }}
      />
      <button
        class="cwd-btn"
        type="button"
        onclick={applyDirectory}
        aria-label="Apply directory"
      >Go</button>
    </div>
  </div>

  <div class="panel-area">
    <GitOpsPanel {cwd} />
  </div>
</div>

<style>
  .git-screen {
    display: flex;
    flex-direction: column;
    height: 100%;
    background: var(--la-bg, #0a0a0f);
    font-family: var(--la-font-mono, monospace);
  }

  .screen-header {
    display: flex;
    align-items: center;
    gap: 16px;
    padding: 8px 14px;
    border-bottom: 1px solid var(--la-hair-base, #25282d);
    flex-shrink: 0;
    background: var(--la-bg-panel, #0f1117);
  }

  .screen-title {
    font-size: 11px;
    letter-spacing: 0.18em;
    text-transform: uppercase;
    color: var(--la-text-mute, #5a6472);
    user-select: none;
    flex-shrink: 0;
  }

  .cwd-row {
    display: flex;
    align-items: center;
    gap: 6px;
    flex: 1;
  }

  .cwd-label {
    font-size: 10px;
    letter-spacing: 0.14em;
    text-transform: uppercase;
    color: var(--la-text-mute, #5a6472);
    white-space: nowrap;
    user-select: none;
  }

  .cwd-input {
    flex: 1;
    background: var(--la-bg-elevated, #1a2030);
    border: 1px solid var(--la-hair-base, #25282d);
    color: var(--la-text-bright, #f1f5f9);
    font-family: var(--la-font-mono, monospace);
    font-size: 12px;
    padding: 4px 8px;
    outline: none;
    min-width: 0;
  }

  .cwd-input::placeholder {
    color: var(--la-text-mute, #5a6472);
  }

  .cwd-input:focus-visible {
    outline: 2px solid var(--la-focus-ring, #FFD700);
    outline-offset: 2px;
  }

  .cwd-btn {
    padding: 4px 12px;
    background: var(--la-bg-elevated, #1a2030);
    border: 1px solid var(--la-hair-strong, #3a3f47);
    color: var(--la-text-bright, #f1f5f9);
    font-family: var(--la-font-mono, monospace);
    font-size: 11px;
    letter-spacing: 0.08em;
    cursor: pointer;
    transition: background 120ms ease;
    white-space: nowrap;
    flex-shrink: 0;
  }

  .cwd-btn:hover {
    background: var(--la-struct-primary, #00c8ff);
    color: var(--la-bg-base, #0a0a0f);
    border-color: var(--la-struct-primary, #00c8ff);
  }

  .cwd-btn:focus-visible {
    outline: 2px solid var(--la-focus-ring, #FFD700);
    outline-offset: 2px;
  }

  .panel-area {
    flex: 1;
    min-height: 0;
    overflow: hidden;
  }
</style>
