// ============================================================================
// Auth — token resolution, cookie upgrade, session refresh, resync
// ============================================================================

const SESSION_KEY = 'la_webshell_token';

// When true, the browser holds an HttpOnly session cookie; no token lives in
// JS memory and authHeaders() returns {} so fetch() sends the cookie automatically.
let cookieMode = false;
let refreshTimer: ReturnType<typeof setInterval> | null = null;

// ── Reliability utilities ────────────────────────────────────────────────────

/** Rejects after `ms` milliseconds. Wraps any Promise to enforce a deadline. */
function withTimeout<T>(ms: number, promise: Promise<T>): Promise<T> {
  return new Promise((resolve, reject) => {
    const tid = setTimeout(() => reject(new Error(`auth timeout after ${ms}ms`)), ms);
    promise.then(
      (v) => { clearTimeout(tid); resolve(v); },
      (e) => { clearTimeout(tid); reject(e); },
    );
  });
}

/** Retry `fn` up to `maxAttempts` times with exponential back-off (base 250ms). */
async function withRetry<T>(fn: () => Promise<T>, maxAttempts = 3): Promise<T> {
  let lastErr: unknown;
  for (let i = 0; i < maxAttempts; i++) {
    try {
      return await fn();
    } catch (e) {
      lastErr = e;
      if (i < maxAttempts - 1) {
        await new Promise(r => setTimeout(r, 250 * 2 ** i));
      }
    }
  }
  throw lastErr;
}

// ── Token resolution ─────────────────────────────────────────────────────────

/**
 * Resolves the webshell Bearer token on page load.
 * Priority: window.location.hash#token=<hex> → sessionStorage fallback.
 * Strips the hash so the token does not appear in bookmarks, history, or Referer.
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

// ── Cookie exchange ──────────────────────────────────────────────────────────

/**
 * Redeems a one-time nonce UUID for an HttpOnly session cookie.
 * Retries up to 3 times with 5 s timeout per attempt.
 */
export async function initNonceSession(nonce: string): Promise<void> {
  if (cookieMode) return;
  try {
    const res = await withRetry(() =>
      withTimeout(5000, fetch('/api/auth/nonce-exchange', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        credentials: 'same-origin',
        body: JSON.stringify({ nonce }),
      }))
    );
    if (!res.ok) return;
    // Guard against Vite SPA fallback (text/html 200 is not a real session cookie).
    if ((res.headers.get('content-type') ?? '').includes('text/html')) return;
    cookieMode = true;
    sessionStorage.removeItem(SESSION_KEY);
    scheduleRefresh();
    window.addEventListener('pagehide', stopRefresh, { once: true });
  } catch (e) {
    console.warn('[la-auth] Nonce exchange failed', e);
  }
}

/**
 * Exchanges a Bearer token for an HttpOnly session cookie.
 * Idempotent — returns immediately if already in cookie mode.
 */
export async function initCookieSession(token: string): Promise<void> {
  if (cookieMode) return;
  try {
    const res = await withRetry(() =>
      withTimeout(5000, fetch('/api/auth/exchange', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        credentials: 'same-origin',
        body: JSON.stringify({ token }),
      }))
    );
    if (!res.ok) return;
    if (!(res.headers.get('content-type') ?? '').includes('application/json')) return;
    cookieMode = true;
    sessionStorage.removeItem(SESSION_KEY);
    scheduleRefresh();
    window.addEventListener('pagehide', stopRefresh, { once: true });
  } catch (e) {
    console.warn('[la-auth] Cookie exchange failed — staying in bearer mode', e);
  }
}

// ── Session refresh ──────────────────────────────────────────────────────────

/**
 * Re-validates the current session (cookie or Bearer) against the server and
 * refreshes the sliding TTL. Fires `la:auth-resynced` on success so components
 * can re-fetch state without a page reload.
 *
 * Distinguishes transient network errors (degraded — keeps cookie mode) from
 * definitive 401s (expired — clears cookie mode).
 */
export async function resyncAuth(): Promise<boolean> {
  try {
    const res = await withTimeout(5000, fetch('/api/auth/status', {
      credentials: 'same-origin',
      headers: authHeaders(),
    }));
    if (res.ok) {
      window.dispatchEvent(new CustomEvent('la:auth-resynced'));
      return true;
    }
    if (res.status === 401) {
      // Definitive rejection — clear auth state.
      cookieMode = false;
      clearInterval(refreshTimer!);
      refreshTimer = null;
    }
    // 5xx or other: treat as transient, keep current state.
    return false;
  } catch {
    // Network error: degraded but not expired.
    return false;
  }
}

/**
 * Starts (or resets) the 30-minute sliding-TTL refresh timer.
 * Clears any existing timer first to prevent accumulation across re-auth cycles.
 *
 * Transient network failures keep the session alive (degraded mode).
 * A definitive 401 clears cookie mode and the timer.
 */
function scheduleRefresh(): void {
  if (refreshTimer !== null) {
    clearInterval(refreshTimer);
    refreshTimer = null;
  }
  const id = setInterval(async () => {
    try {
      const res = await withTimeout(5000, fetch('/api/auth/status', { credentials: 'same-origin' }));
      if (res.status === 401) {
        cookieMode = false;
        clearInterval(id);
        refreshTimer = null;
      }
      // 5xx / network error: keep timer alive, session is degraded not expired.
    } catch {
      // Transient network failure — keep trying next tick.
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
