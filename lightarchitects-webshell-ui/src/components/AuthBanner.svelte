<script lang="ts">
  /**
   * AuthBanner — top-of-screen affordance that fires when the SSE stream or any
   * polled endpoint receives 401 (unauthorized) or 403 (forbidden). #13.
   *
   * Recovery path:
   *   1. "Relaunch in CLI" — copies the canonical relaunch command so the
   *      operator can paste it into Claude Code (`/webshell`) or their
   *      terminal (`~/.lightarchitects/bin/lightarchitects launch-webshell`).
   *      This is the load-bearing recovery action: once they relaunch, the
   *      gateway mints a fresh nonce, opens a new URL, and the session
   *      reconnects cleanly.
   *   2. "Reset auth" — clears local + server-side auth state and reloads.
   *      Useful when the session is stale and a fresh handshake is wanted,
   *      but does NOT re-authenticate by itself — the operator still has
   *      to relaunch (path #1).
   *   3. "Dismiss" — hides the banner. Re-fires on next 401/403.
   *
   * Until a `/webshell` re-invocation, the only path to a working session is
   * keeping the existing cookie (refresh the original URL with `#nonce=…`
   * still in it) or restarting the webshell process.
   */
  import { authStatus } from '$lib/stores';
  import { setupComplete, step } from '$lib/setup';
  import { authHeaders } from '$lib/auth';

  let s = $derived($authStatus);
  let dismissed = $state(false);
  let copyState = $state<'idle' | 'copied' | 'failed'>('idle');

  // Re-show the banner whenever a NEW failure arrives, even if the user
  // dismissed a prior one. We watch for transitions ok→unauthorized/forbidden.
  let prev: typeof s = 'ok';
  $effect(() => {
    if (prev === 'ok' && s !== 'ok') dismissed = false;
    prev = s;
  });

  // The canonical relaunch command. In Claude Code / Codex CLI / Cursor the
  // operator types `/webshell` to invoke the lightarchitects skill which
  // spawns a fresh webshell with a session-pre-seeded nonce. The bash
  // fallback is for operators outside an MCP-aware coding agent.
  const RELAUNCH_SLASH = '/webshell';
  const RELAUNCH_SHELL = 'lightarchitects launch_webshell';

  async function copyRelaunch() {
    const cmd = RELAUNCH_SLASH;
    try {
      await navigator.clipboard.writeText(cmd);
      copyState = 'copied';
      setTimeout(() => { copyState = 'idle'; }, 2000);
    } catch {
      copyState = 'failed';
      setTimeout(() => { copyState = 'idle'; }, 2000);
    }
  }

  async function resetAuth() {
    // BUG FIX (2026-06-03): prior implementation cleared `localStorage['la.bearer']`
    // and `localStorage['la.session']` — NEITHER KEY EXISTS. The real session state is:
    //   - sessionStorage['la_webshell_token']  (see auth.ts SESSION_KEY)
    //   - HttpOnly cookie `la_session`        (cleared via DELETE /api/auth/session)
    // Old code was a no-op for actual session state; cookie persisted; UI ping-ponged.

    // 1. Clear the client-side bearer token (correct key).
    try { sessionStorage.removeItem('la_webshell_token'); } catch { /* SSR-safe */ }

    // 2. Defensive: clear any stale localStorage keys from older versions.
    try {
      localStorage.removeItem('la.bearer');
      localStorage.removeItem('la.session');
    } catch { /* localStorage may be disabled */ }

    // 3. Clear the server-side HttpOnly cookie. We pass authHeaders() so this
    //    works whether we still hold a bearer or only the cookie. The server
    //    accepts either form per auth_logout handler in server/mod.rs.
    try {
      await fetch('/api/auth/session', {
        method: 'DELETE',
        credentials: 'same-origin',
        headers: authHeaders(),
      });
    } catch {
      // Server unreachable — proceed with client-side cleanup; reload will
      // surface the failure as a fresh auth prompt.
    }

    // 4. Route to setup flow + force a full reload so main.ts re-runs the
    //    bootstrap (nonce-or-token discovery) with a clean slate.
    setupComplete.set(false);
    step.set('auth');
    window.location.reload();
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
          ? 'Re-launch the webshell to get a fresh nonce — copy the command, then run it in your coding-agent or terminal.'
          : 'Your token is valid but the server rejected this request. Check server logs, or copy the relaunch command for a fresh handshake.'}
      </span>
      <code class="auth-banner-cmd">{RELAUNCH_SLASH}</code>
    </div>
    <div class="auth-banner-actions">
      <button class="auth-banner-btn primary" onclick={copyRelaunch}>
        {copyState === 'copied' ? '✓ Copied' : copyState === 'failed' ? '✗ Copy failed' : 'Copy /webshell'}
      </button>
      <button
        class="auth-banner-btn"
        onclick={resetAuth}
        title="Clears local + server-side auth state and reloads. You still need to relaunch the webshell to re-authenticate."
      >
        Reset auth
      </button>
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
    color: var(--la-danger-text);
    opacity: 0.85;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .auth-banner-cmd {
    font-family: var(--la-font-mono, monospace);
    font-size: 11px;
    padding: 2px 6px;
    background: color-mix(in srgb, var(--la-danger-stroke) 25%, transparent);
    border: 1px solid color-mix(in srgb, var(--la-danger-stroke) 40%, transparent);
    border-radius: 2px;
    color: var(--la-danger-text);
    white-space: nowrap;
    flex-shrink: 0;
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
    border-radius: 0;
    border: 1px solid var(--la-danger-stroke);
    background: transparent;
    color: var(--la-danger-text);
    cursor: pointer;
    transition: background var(--la-transition-fast), color var(--la-transition-fast);
  }
  .auth-banner-btn:hover {
    background: color-mix(in srgb, var(--la-danger-stroke) 15%, transparent);
  }
  .auth-banner-btn.primary {
    background: var(--la-danger-stroke);
    color: #FFFFFF;
  }
  .auth-banner-btn.primary:hover {
    background: var(--la-danger-stroke);
  }
  @keyframes auth-banner-slide {
    from { transform: translateY(-100%); opacity: 0; }
    to   { transform: translateY(0);     opacity: 1; }
  }
</style>
