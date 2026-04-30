<script lang="ts">
  /**
   * AuthBanner — top-of-screen affordance that fires when the SSE stream
   * receives 401 (unauthorized) or 403 (forbidden). #13.
   *
   * The banner is the operator's recovery path when the bearer token is
   * missing, expired, or rejected — without it, the app silently sits in
   * "reconnecting" forever. Clicking "Reset" routes back to SetupFlow,
   * which clears localStorage and re-prompts for a token.
   */
  import { authStatus } from '$lib/stores';
  import { setupComplete, step } from '$lib/setup';

  let s = $derived($authStatus);
  let dismissed = $state(false);

  // Re-show the banner whenever a NEW failure arrives, even if the user
  // dismissed a prior one. We watch for transitions ok→unauthorized/forbidden.
  let prev: typeof s = 'ok';
  $effect(() => {
    if (prev === 'ok' && s !== 'ok') dismissed = false;
    prev = s;
  });

  function resetAuth() {
    // Clear local credentials and bounce back to setup. Setup writes a fresh
    // token + cookie pair on completion.
    try {
      localStorage.removeItem('la.bearer');
      localStorage.removeItem('la.session');
    } catch {
      /* localStorage may be disabled — setup will recover regardless */
    }
    setupComplete.set(false);
    // 'auth' is the SetupStep that prompts for backend credentials.
    step.set('auth');
  }
</script>

{#if s !== 'ok' && !dismissed}
  <div
    class="auth-banner"
    role="alert"
    aria-live="assertive"
    data-testid="auth-banner"
  >
    <div class="auth-banner-text">
      <span class="auth-banner-icon" aria-hidden="true">⚠</span>
      <strong>
        {s === 'unauthorized' ? 'Session expired' : 'Access denied'}
      </strong>
      <span class="auth-banner-detail">
        {s === 'unauthorized'
          ? 'The bearer token is missing or no longer valid. Reset to re-authenticate.'
          : 'Your token is valid but the server rejected this request. Check server logs or reset.'}
      </span>
    </div>
    <div class="auth-banner-actions">
      <button class="auth-banner-btn primary" onclick={resetAuth}>Reset auth</button>
      <button class="auth-banner-btn" onclick={() => { dismissed = true; }}>Dismiss</button>
    </div>
  </div>
{/if}

<style>
  .auth-banner {
    position: fixed;
    top: 0;
    left: 0;
    right: 0;
    z-index: 80;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    padding: 8px 16px;
    background: var(--la-danger-bg);
    border-bottom: 1px solid var(--la-danger-stroke);
    color: var(--la-danger-text);
    font-family: var(--la-font-chrome);
    font-size: 12px;
    box-shadow: 0 4px 12px var(--la-danger-glow);
    animation: auth-banner-slide var(--la-transition-med) ease-out;
  }
  .auth-banner-text {
    display: flex;
    align-items: center;
    gap: 8px;
    flex: 1;
    min-width: 0;
  }
  .auth-banner-icon {
    font-size: 14px;
  }
  .auth-banner-detail {
    color: #FECACA;
    opacity: 0.85;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .auth-banner-actions {
    display: flex;
    gap: 8px;
    flex-shrink: 0;
  }
  .auth-banner-btn {
    padding: 4px 10px;
    font-size: 11px;
    font-family: inherit;
    border-radius: var(--la-radius-md);
    border: 1px solid var(--la-danger-stroke);
    background: transparent;
    color: var(--la-danger-text);
    cursor: pointer;
    transition: background var(--la-transition-fast), color var(--la-transition-fast);
  }
  .auth-banner-btn:hover {
    background: rgba(220, 38, 38, 0.15);
  }
  .auth-banner-btn.primary {
    background: var(--la-danger-stroke);
    color: #FFFFFF;
  }
  .auth-banner-btn.primary:hover {
    background: #B91C1C;
  }
  @keyframes auth-banner-slide {
    from { transform: translateY(-100%); opacity: 0; }
    to   { transform: translateY(0);     opacity: 1; }
  }
</style>
