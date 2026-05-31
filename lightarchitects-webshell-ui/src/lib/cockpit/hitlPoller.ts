/**
 * Shared HITL poller — combines GitHub PR + platform sources into a unified
 * HITLItem stream. Contract 4 (webshell-cockpit plan §1931).
 *
 * Sources:
 *  - GitHub:   GET /api/gitforest/hitl-search    (PRs awaiting review)
 *  - Platform: GET /api/conductor/hitl           (paused builds awaiting operator)
 *
 * Polls every 60s. Deduplicates by `id`. One subscriber starts the timer;
 * last unsubscriber stops it (Svelte readable cleanup contract).
 */
import { readable } from 'svelte/store';
import { authHeaders } from '$lib/auth';

/** Contract 4 — unified HITL item shape. */
export interface HITLItem {
  source: 'github_pr' | 'platform';
  id: string;
  title: string;
  url: string;
  age_seconds: number;
  severity: 'info' | 'warn' | 'block';
  /** Only for source === 'github_pr' */
  prNumber?: number;
  repo?: string;
  draft?: boolean;
}

interface GhHitlRaw {
  number: number;
  title: string;
  html_url: string;
  owner: string;
  repo: string;
  updated_at: string;
  draft: boolean;
}

interface PlatformHitlRaw {
  id: string;
  title: string;
  build_codename?: string;
  priority?: string;
  added?: string;
}

function mapGh(item: GhHitlRaw): HITLItem {
  const ageMs = Date.now() - new Date(item.updated_at).getTime();
  const ageH  = ageMs / 3_600_000;
  return {
    source:      'github_pr',
    id:          `${item.owner}/${item.repo}#${item.number}`,
    title:       item.title,
    url:         item.html_url,
    age_seconds: ageMs / 1000,
    severity:    ageH > 72 ? 'block' : ageH > 24 ? 'warn' : 'info',
    prNumber:    item.number,
    repo:        item.repo,
    draft:       item.draft,
  };
}

function mapPlatform(task: PlatformHitlRaw): HITLItem {
  const ageMs = task.added ? Date.now() - new Date(task.added).getTime() : 0;
  const p     = (task.priority ?? '').toUpperCase();
  return {
    source:      'platform',
    id:          task.id,
    title:       task.title,
    url:         task.build_codename ? `/builds/${task.build_codename}` : '/builds',
    age_seconds: ageMs / 1000,
    severity:    p === 'CRITICAL' ? 'block' : p === 'HIGH' ? 'warn' : 'info',
  };
}

async function fetchBoth(): Promise<HITLItem[]> {
  const [ghRes, platRes] = await Promise.allSettled([
    fetch('/api/gitforest/hitl-search', { headers: authHeaders() }),
    fetch('/api/conductor/hitl',        { headers: authHeaders() }),
  ]);

  const items: HITLItem[] = [];
  const seen  = new Set<string>();

  if (ghRes.status === 'fulfilled' && ghRes.value.ok) {
    const data: GhHitlRaw[] = await ghRes.value.json() as GhHitlRaw[];
    for (const item of data) {
      const mapped = mapGh(item);
      if (!seen.has(mapped.id)) { seen.add(mapped.id); items.push(mapped); }
    }
  }

  if (platRes.status === 'fulfilled' && platRes.value.ok) {
    const data: PlatformHitlRaw[] = await platRes.value.json() as PlatformHitlRaw[];
    for (const task of data) {
      const mapped = mapPlatform(task);
      if (!seen.has(mapped.id)) { seen.add(mapped.id); items.push(mapped); }
    }
  }

  return items.sort((a, b) => b.age_seconds - a.age_seconds);
}

export const hitlItems = readable<HITLItem[]>([], (set) => {
  void fetchBoth().then(set);
  const timer = setInterval(() => { void fetchBoth().then(set); }, 60_000);
  return () => clearInterval(timer);
});
