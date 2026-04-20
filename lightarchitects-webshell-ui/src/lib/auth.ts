// ============================================================================
// Auth — token resolution from URL hash + sessionStorage
// ============================================================================

const SESSION_KEY = 'la_webshell_token';

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

/** Returns the stored token, or null if the user has no session. */
export function getToken(): string | null {
  return sessionStorage.getItem(SESSION_KEY);
}

/** Returns an Authorization header object, or an empty object when unauthenticated. */
export function authHeaders(): { Authorization: string } | Record<string, never> {
  const token = getToken();
  return token ? { Authorization: `Bearer ${token}` } : {};
}
