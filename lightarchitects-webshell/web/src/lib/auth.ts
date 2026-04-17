/**
 * Shared token resolution for SSE and WebSocket connections.
 *
 * Resolution order:
 *   1. URL hash fragment: #token=<value>  (injected by the Rust binary on launch)
 *   2. sessionStorage key: webshell_token (persisted after first resolution)
 *
 * After reading the hash, the token is saved to sessionStorage and the
 * fragment is stripped from the URL bar so it doesn't appear in history.
 */
export function resolveToken(): string {
  const hash = window.location.hash.slice(1);
  const params = new URLSearchParams(hash);
  const fromHash = params.get('token');
  if (fromHash) {
    sessionStorage.setItem('webshell_token', fromHash);
    window.history.replaceState(
      null,
      '',
      window.location.pathname + window.location.search,
    );
    return fromHash;
  }
  return sessionStorage.getItem('webshell_token') ?? '';
}
