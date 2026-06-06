import { writable } from 'svelte/store';

/**
 * URL scope keyed by depth — single source of truth for the active cockpit
 * screen mount and breadcrumb. Synced bidirectionally with window.location.hash.
 *
 * d0 platform  → /cockpit/platform
 * d1 project   → /cockpit/project/:project_id
 * d2 build     → /cockpit/build/:codename
 * d3 file      → /cockpit/file/:codename/:file_path*
 */
export type RouteScope =
  | { depth: 0; kind: 'platform' }
  | { depth: 1; kind: 'project'; project_id: string }
  | { depth: 2; kind: 'build';   codename: string }
  | { depth: 3; kind: 'file';    codename: string; file_path: string };

/** Active cockpit scope — set by CockpitShell on mount + hash change. */
export const scope = writable<RouteScope | null>(null);

/** Convert a RouteScope to its canonical hash path. */
export function toScopeUrl(s: RouteScope): string {
  switch (s.kind) {
    case 'platform': return '/cockpit/platform';
    case 'project':  return `/cockpit/project/${encodeURIComponent(s.project_id)}`;
    case 'build':    return `/cockpit/build/${encodeURIComponent(s.codename)}`;
    case 'file': {
      const segments = s.file_path.split('/').map(encodeURIComponent).join('/');
      return `/cockpit/file/${encodeURIComponent(s.codename)}/${segments}`;
    }
  }
}

/** Parse route params from routes.ts matchRoute into a RouteScope. */
export function scopeFromParams(
  screenKey: string,
  params: Record<string, string>,
): RouteScope | null {
  switch (screenKey) {
    case 'CockpitPlatform':
      return { depth: 0, kind: 'platform' };
    case 'CockpitProject':
      return { depth: 1, kind: 'project', project_id: params['projectId'] ?? '' };
    case 'CockpitBuild':
      return { depth: 2, kind: 'build', codename: params['codename'] ?? '' };
    case 'CockpitFile':
      return {
        depth: 3, kind: 'file',
        codename: params['codename'] ?? '',
        file_path: decodeURIComponent(params['filePath'] ?? ''),
      };
    default:
      return null;
  }
}
