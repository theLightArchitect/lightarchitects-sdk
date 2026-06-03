<!--
@component
Auto-Mode toggle chip — enables autonomous program dispatch without per-step HITL.

G6 operator-intent gate: requires explicit confirmation on first activation per
session and after 1 hour of idle. Keyboard: Alt-A.
-->
<script lang="ts">
  import { autoModeActive, autoModeConfirmedAt } from '$lib/stores';

  const RECONFIRM_MS = 3_600_000; // 1 hour

  let showConfirm = $state(false);

  function needsConfirm(): boolean {
    if ($autoModeConfirmedAt === null) return true;
    return Date.now() - $autoModeConfirmedAt > RECONFIRM_MS;
  }

  function handleToggle() {
    if ($autoModeActive) {
      autoModeActive.set(false);
    } else if (needsConfirm()) {
      showConfirm = true;
    } else {
      autoModeActive.set(true);
    }
  }

  function handleConfirm() {
    autoModeConfirmedAt.set(Date.now());
    autoModeActive.set(true);
    showConfirm = false;
  }

  function handleCancel() {
    showConfirm = false;
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.altKey && e.key === 'a') {
      e.preventDefault();
      handleToggle();
    } else if (showConfirm && e.key === 'Escape') {
      handleCancel();
    }
  }
</script>

<svelte:window onkeydown={handleKeydown} />

<button
  class="chip"
  class:chip--active={$autoModeActive}
  onclick={handleToggle}
  title="{$autoModeActive ? 'Auto Mode on — Alt-A or click to disable' : 'Auto Mode off — Alt-A or click to enable'}"
  aria-pressed={$autoModeActive}
  data-testid="auto-mode-chip"
>
  <span class="chip-dot" class:chip-dot--pulse={$autoModeActive} aria-hidden="true"></span>
  <span class="chip-label">AUTO</span>
</button>

{#if showConfirm}
  <div
    class="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm"
    role="presentation"
  >
    <div class="confirm-card" role="dialog" aria-modal="true" aria-labelledby="auto-mode-dlg-title">
      <p id="auto-mode-dlg-title" class="confirm-title">Enable Auto Mode?</p>
      <p class="confirm-body">
        Autonomous program dispatch will proceed without per-step confirmation.
        You will be asked again after 1 hour of idle.
      </p>
      <div class="confirm-actions">
        <button class="btn-cancel" onclick={handleCancel}>Cancel</button>
        <button class="btn-confirm" onclick={handleConfirm}>Enable</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .chip {
    display: flex;
    align-items: center;
    gap: 5px;
    padding: 2px 8px;
    background: color-mix(in srgb, #6b7280 8%, transparent);
    border: 1px solid color-mix(in srgb, #6b7280 25%, transparent);
    color: #6b7280;
    font-family: var(--la-font-mono);
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.1em;
    cursor: pointer;
    transition: background 80ms, border-color 80ms, color 80ms;
    flex-shrink: 0;
  }

  .chip:hover {
    background: color-mix(in srgb, #6b7280 15%, transparent);
    border-color: color-mix(in srgb, #6b7280 40%, transparent);
  }

  .chip--active {
    background: color-mix(in srgb, #F59E0B 10%, transparent);
    border-color: color-mix(in srgb, #F59E0B 30%, transparent);
    color: #F59E0B;
  }

  .chip--active:hover {
    background: color-mix(in srgb, #F59E0B 18%, transparent);
    border-color: color-mix(in srgb, #F59E0B 50%, transparent);
  }

  .chip-dot {
    display: inline-block;
    width: 5px;
    height: 5px;
    border-radius: 50%;
    background: currentColor;
    flex-shrink: 0;
  }

  .chip-dot--pulse {
    box-shadow: 0 0 4px #F59E0B;
    animation: pulse 2s ease-in-out infinite;
  }

  .chip-label {
    font-weight: 400;
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }

  @keyframes pulse {
    0%, 100% { opacity: 1; }
    50%       { opacity: 0.5; }
  }

  /* ── Confirm modal ── */

  .confirm-card {
    background: var(--la-bg-elevated);
    border: 1px solid color-mix(in srgb, #F59E0B 40%, transparent);
    border-radius: 6px;
    padding: 24px 28px;
    max-width: 380px;
    width: 90vw;
    display: flex;
    flex-direction: column;
    gap: 12px;
    box-shadow: 0 0 24px color-mix(in srgb, #F59E0B 15%, transparent);
  }

  .confirm-title {
    font-family: var(--la-font-mono);
    font-size: 13px;
    font-weight: 700;
    color: #F59E0B;
    letter-spacing: 0.08em;
    margin: 0;
  }

  .confirm-body {
    font-size: 12px;
    color: var(--la-text-mute);
    line-height: 1.6;
    margin: 0;
  }

  .confirm-actions {
    display: flex;
    gap: 8px;
    justify-content: flex-end;
    margin-top: 4px;
  }

  .btn-cancel,
  .btn-confirm {
    font-family: var(--la-font-mono);
    font-size: 10px;
    font-weight: 700;
    letter-spacing: 0.08em;
    padding: 5px 14px;
    border-radius: 3px;
    cursor: pointer;
    transition: background 80ms;
  }

  .btn-cancel {
    background: transparent;
    border: 1px solid #374151;
    color: var(--la-text-mute);
  }

  .btn-cancel:hover {
    background: var(--la-bg-elev-1);
    border-color: #4b5563;
  }

  .btn-confirm {
    background: color-mix(in srgb, #F59E0B 15%, transparent);
    border: 1px solid color-mix(in srgb, #F59E0B 50%, transparent);
    color: #F59E0B;
  }

  .btn-confirm:hover {
    background: color-mix(in srgb, #F59E0B 25%, transparent);
  }

  /* ── color-mix fallback ── */
  @supports not (color: color-mix(in srgb, red 50%, blue)) {
    .chip         { background: rgba(107, 114, 128, 0.08); border-color: rgba(107, 114, 128, 0.25); }
    .chip:hover   { background: rgba(107, 114, 128, 0.15); }
    .chip--active { background: rgba(245, 158, 11, 0.10); border-color: rgba(245, 158, 11, 0.30); color: #F59E0B; }
    .chip--active:hover { background: rgba(245, 158, 11, 0.18); }
    .confirm-card { border-color: rgba(245, 158, 11, 0.40); }
    .btn-confirm  { background: rgba(245, 158, 11, 0.15); border-color: rgba(245, 158, 11, 0.50); }
    .btn-confirm:hover { background: rgba(245, 158, 11, 0.25); }
  }
</style>
