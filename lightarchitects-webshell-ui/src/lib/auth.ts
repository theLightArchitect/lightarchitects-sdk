// ============================================================================
// Auth — token resolution from URL hash + sessionStorage, cookie upgrade
// ============================================================================

const SESSION_KEY = 'la_webshell_token';

// When true, the browser holds an HttpOnly session cookie; no token lives in JS
// memory and authHeaders() returns {} so fetch() sends the cookie automatically.
let cookieMode = false;
let refreshTimer: ReturnType<typeof setInterval> | null = null;

/**
 * Resolves the webshell Bearer token on page load.
 * Priority: window.location.hash#token=<hex> → sessionStorage fallback.
 * Strips the hash after reading so the token does not appear in bookmarks,
 * history entries, or Referer headers on outbound clicks.
 */
export function resolveToken(): string | null {
  const hash = window.location.hash.slice(1);
  const params = new URLSearchParams(hash);
  const fromHash = params.get('token');
  if (fromHash) {
    sessionStorage.setItem(SESSION_KEY, fromHash);
    history.replaceState(null, '', window.location.pathname + window.location.search);
    return fromHash;
  }
  return sessionStorage.getItem(SESSION_KEY);
}

/** Returns the stored token, or null when unauthenticated or in cookie mode. */
export function getToken(): string | null {
  if (cookieMode) return null;
  return sessionStorage.getItem(SESSION_KEY);
}

/** Returns an Authorization header object, or {} when in cookie mode or unauthenticated. */
export function authHeaders(): { Authorization: string } | Record<string, never> {
  if (cookieMode) return {};
  const token = getToken();
  return token ? { Authorization: `Bearer ${token}` } : {};
}

/**
 * Exchanges a Bearer token for an HttpOnly session cookie (v0.4.0).
 *
 * Idempotent — returns immediately if already in cookie mode.
 * On success: sets cookieMode=true, clears sessionStorage, starts the
 * sliding-TTL refresh timer, and registers a pagehide cleanup listener.
 * On failure: silently leaves the existing Bearer flow intact.
 */
export async function initCookieSession(token: string): Promise<void> {
  if (cookieMode) return;
  try {
    const res = await fetch('/api/auth/exchange', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      credentials: 'same-origin',
      body: JSON.stringify({ token }),
    });
    if (!res.ok) return;
    cookieMode = true;
    sessionStorage.removeItem(SESSION_KEY);
    scheduleRefresh();
    window.addEventListener('pagehide', stopRefresh, { once: true });
  } catch (e) {
    console.warn('[la-auth] Cookie exchange failed — staying in bearer mode', e);
  }
}

/**
 * Starts (or resets) the 30-minute sliding-TTL refresh timer.
 *
 * Each tick calls GET /api/auth/status, which validates the cookie and returns
 * a refreshed Set-Cookie extending the TTL.  On 401, cookie mode is disabled;
 * the user must re-authenticate on next page load.
 */
function scheduleRefresh(): void {
  if (refreshTimer !== null) clearInterval(refreshTimer);
  // Capture id so the callback never needs a non-null assertion on refreshTimer.
  const id = setInterval(async () => {
    try {
      const res = await fetch('/api/auth/status', { credentials: 'same-origin' });
      if (!res.ok) {
        cookieMode = false;
        clearInterval(id);
        refreshTimer = null;
      }
    } catch {
      // Network failure — keep trying next tick
    }
  }, 30 * 60 * 1000);
  refreshTimer = id;
}

/** Clears the refresh timer — called on pagehide to avoid timer leaks. */
function stopRefresh(): void {
  if (refreshTimer !== null) {
    clearInterval(refreshTimer);
    refreshTimer = null;
  }
}
