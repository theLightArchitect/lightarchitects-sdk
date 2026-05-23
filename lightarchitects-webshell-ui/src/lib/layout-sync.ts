// Outbound layout sync — push the current mosaic PanelTree to the gateway
// whenever it changes (debounced 500 ms). Also pushes on startup so the
// gateway has a snapshot immediately after the browser connects.
//
// The gateway stores it in AppState.layout_snapshot (POST /api/layout/snapshot)
// so any process with the auth token can retrieve the current layout via
// GET /api/layout/snapshot. Primary use cases:
//   1. Cross-tab sync: second tab starts up, GETs current layout instead of
//      reading a potentially stale localStorage value.
//   2. Session recovery: gateway-side tooling can inspect the operator's
//      current panel arrangement without a Playwright browser.

import { layoutTree } from './layout';
import { authHeaders } from './auth';

const ENDPOINT = '/api/layout/snapshot';
const DEBOUNCE_MS = 500;

let timer: ReturnType<typeof setTimeout> | null = null;

async function push(tree: unknown): Promise<void> {
  try {
    const headers = authHeaders();
    if (!headers['Authorization']) return; // not authenticated yet

    await fetch(ENDPOINT, {
      method: 'POST',
      headers: { ...headers, 'Content-Type': 'application/json' },
      body: JSON.stringify(tree),
    });
  } catch {
    // Network errors are expected when the gateway is offline; fail silently.
  }
}

/** Call once at app startup to subscribe + push the initial snapshot. */
export function startLayoutSync(): () => void {
  const unsubscribe = layoutTree.subscribe((tree) => {
    if (timer !== null) clearTimeout(timer);
    timer = setTimeout(() => {
      timer = null;
      push(tree);
    }, DEBOUNCE_MS);
  });

  return () => {
    if (timer !== null) clearTimeout(timer);
    unsubscribe();
  };
}
