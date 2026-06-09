import { writable } from 'svelte/store';

/**
 * URL scope keyed by depth — single source of truth for the active cockpit
 * screen mount and breadcrumb. Set from page.params in each cockpit +page.svelte
 * via scopeFromParams(); cleared to null when navigating away from /cockpit/*.
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
    case 'platform': return '/command-center/platform';
    case 'project':  return `/command-center/project/${encodeURIComponent(s.project_id)}`;
    case 'build':    return `/command-center/build/${encodeURIComponent(s.codename)}`;
    case 'file': {
      const segments = s.file_path.split('/').map(encodeURIComponent).join('/');
      return `/command-center/file/${encodeURIComponent(s.codename)}/${segments}`;
    }
  }
}

/** Valid cockpit screen keys for scopeFromParams. Narrowed union restores the
 *  compile-time safety that ScreenKey previously provided before it was removed
 *  in the SvelteKit migration. Callers in cockpit +page.svelte files are checked
 *  exhaustively at compile time; a typo now produces a type error, not a null scope. */
export type CockpitScreenKey = 'CockpitPlatform' | 'CockpitProject' | 'CockpitBuild' | 'CockpitFile';

/** Parse SvelteKit page.params into a RouteScope for the active cockpit depth. */
export function scopeFromParams(
  screenKey: CockpitScreenKey,
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
