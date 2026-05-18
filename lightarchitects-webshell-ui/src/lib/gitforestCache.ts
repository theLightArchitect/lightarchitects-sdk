/**
 * GitForest IndexedDB client-side cache.
 *
 * Implements stale-while-revalidate: returns the cached topology immediately
 * (first paint < 100ms cache-hit) while triggering a background refresh from
 * `GET /api/gitforest/topology` when the entry is stale.
 *
 * IDB schema (iter-7 impl-audit S4):
 *   DB name:   `la-gitforest`
 *   Version:   1
 *   Store:     `topology`
 *   Key:       repo name (string)
 *   Value:     `CachedTopology`
 */

import type { GitForestTopology } from './gitforest';

// ── Schema ────────────────────────────────────────────────────────────────────

/** One cached topology entry stored in IndexedDB. */
interface CachedTopology {
  repo: string;
  topology: GitForestTopology;
  /** Unix ms timestamp when this entry was fetched from the server. */
  cached_at: number;
}

const DB_NAME    = 'la-gitforest';
const DB_VERSION = 1;
const STORE      = 'topology';

/** Default staleness threshold: entries older than this are considered stale. */
const DEFAULT_TTL_MS = 30_000;   // 30 s — live-ops data refreshes frequently

// ── IDB helpers ───────────────────────────────────────────────────────────────

function openDB(): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    const req = indexedDB.open(DB_NAME, DB_VERSION);
    req.onupgradeneeded = () => {
      const db = req.result;
      if (!db.objectStoreNames.contains(STORE)) {
        db.createObjectStore(STORE, { keyPath: 'repo' });
      }
    };
    req.onsuccess = () => resolve(req.result);
    req.onerror   = () => reject(req.error);
  });
}

// ── Public API ────────────────────────────────────────────────────────────────

/**
 * Retrieve a cached topology entry for `repo`.
 *
 * Returns `null` if:
 * - IndexedDB is unavailable (sandboxed env)
 * - No entry exists for this repo
 * - The entry is older than `ttlMs` (stale)
 */
export async function getCached(
  repo: string,
  ttlMs = DEFAULT_TTL_MS,
): Promise<GitForestTopology | null> {
  try {
    const db    = await openDB();
    const entry = await new Promise<CachedTopology | undefined>((resolve, reject) => {
      const tx  = db.transaction(STORE, 'readonly');
      const req = tx.objectStore(STORE).get(repo);
      req.onsuccess = () => resolve(req.result as CachedTopology | undefined);
      req.onerror   = () => reject(req.error);
    });
    if (!entry) return null;
    if (Date.now() - entry.cached_at > ttlMs) return null;
    return entry.topology;
  } catch {
    return null;
  }
}

/**
 * Store a topology entry in the cache.
 * Silent no-op if IndexedDB is unavailable.
 */
export async function putCache(
  repo: string,
  topology: GitForestTopology,
): Promise<void> {
  try {
    const db = await openDB();
    await new Promise<void>((resolve, reject) => {
      const tx    = db.transaction(STORE, 'readwrite');
      const entry: CachedTopology = { repo, topology, cached_at: Date.now() };
      const req   = tx.objectStore(STORE).put(entry);
      req.onsuccess = () => resolve();
      req.onerror   = () => reject(req.error);
    });
  } catch {
    // Storage unavailable (private browsing, quota exceeded) — silent degradation
  }
}

/**
 * Invalidate the cache entry for a specific repo.
 * Called when `WebEvent::GitForestUpdate` arrives for that repo.
 */
export async function invalidate(repo: string): Promise<void> {
  try {
    const db = await openDB();
    await new Promise<void>((resolve, reject) => {
      const tx  = db.transaction(STORE, 'readwrite');
      const req = tx.objectStore(STORE).delete(repo);
      req.onsuccess = () => resolve();
      req.onerror   = () => reject(req.error);
    });
  } catch {
    // Silent degradation
  }
}

/**
 * Stale-while-revalidate fetch.
 *
 * 1. Returns the cached entry immediately if fresh (≤ `ttlMs`).
 * 2. If stale but present, returns the stale value AND fires a background
 *    revalidation that calls `onRevalidate` when the fresh data arrives.
 * 3. If absent, awaits the full fetch before returning.
 */
export async function getOrFetch(
  repo: string,
  fetcher: () => Promise<GitForestTopology>,
  onRevalidate?: (fresh: GitForestTopology) => void,
  ttlMs = DEFAULT_TTL_MS,
): Promise<GitForestTopology> {
  try {
    const db = await openDB();
    const entry = await new Promise<CachedTopology | undefined>((resolve, reject) => {
      const tx  = db.transaction(STORE, 'readonly');
      const req = tx.objectStore(STORE).get(repo);
      req.onsuccess = () => resolve(req.result as CachedTopology | undefined);
      req.onerror   = () => reject(req.error);
    });

    if (entry) {
      const isStale = Date.now() - entry.cached_at > ttlMs;
      if (!isStale) return entry.topology;

      // Stale — return immediately, revalidate in background
      void fetcher().then(fresh => {
        void putCache(repo, fresh);
        onRevalidate?.(fresh);
      });
      return entry.topology;
    }
  } catch {
    // IDB unavailable — fall through to network fetch
  }

  const fresh = await fetcher();
  void putCache(repo, fresh);
  return fresh;
}
