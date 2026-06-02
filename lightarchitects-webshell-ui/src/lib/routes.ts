// Hash-based SPA router. Custom implementation (no SvelteKit).
// Per project_webshell_drill_hierarchy memory: BuildDetail owns view modes;
// Dispatch owns orphan runs; Helix owns strand/entry drilldown.

export type ScreenKey =
  | 'Dashboard'
  | 'Dispatch'
  | 'Builds'
  | 'Intake'
  | 'Helix'
  | 'BuildDetail'
  | 'ProjectDetail'
  | 'Comms'
  | 'Editor'
  | 'Git'
  | 'PullRequest'
  | 'Architecture'
  | 'DiagramLibrary'
  | 'Roadmap'
  | 'Observability'
  | 'Tools'           // §O Tool Surface Parity
  | 'AutonomousBuilds' // ironclaw autonomous build panel
  | 'Chat'            // Polished LiteLLM streaming chat panel
  | 'Security'        // Container spawn policy + isolation controls
  | 'Program';        // Alpha/public readiness program manifest

export interface RouteMatch {
  screen: ScreenKey;
  params: Record<string, string>;
}

// Legacy path → replacement path rewrites (history.replaceState; preserves deep links per H-quantum-2)
// /workspace* → /builds added 2026-05-02 (Wave 1) — fully replaced by /builds/:id
const REDIRECTS: [string, string][] = [
  ['/squad-dispatch', '/run'],
  ['/sitrep',         '/dashboard#health'],
  ['/workspace',      '/builds'],
  ['/ops',            '/dashboard'],
  ['/monitor',        '/dashboard'],
  ['/comms',          '/activity'],
  ['/helix',          '/knowledge'],
  ['/memory',         '/knowledge'],
  ['/arch',           '/diagrams'],
  ['/architecture',   '/diagrams'],
  ['/manage',         '/builds'],
];

/**
 * View modes for the /builds/:buildId/:view URL pattern.
 * Wave 1 adds the route shape; Wave 6 wires BuildDetail.svelte to read it.
 */
export type BuildViewMode = 'kanban' | 'list' | 'operator' | 'manifest' | 'plan' | 'comms' | 'pipeline' | 'autonomous' | 'decisions' | 'fleet';
const BUILD_VIEW_PATTERN = '(?:kanban|list|operator|manifest|plan|comms|pipeline|autonomous|decisions|fleet)';

type RouteEntry = [RegExp, ScreenKey, string[]];

// Ordered most-specific first — prevents /builds/:buildId from matching before deeper patterns
const ROUTES: RouteEntry[] = [
  // L3 task drill-down (Phase 5) — must precede the agent pattern
  [/^\/builds\/([^/]+)\/phase\/([^/]+)\/wave\/([^/]+)\/agent\/([^/]+)\/task\/([^/]+)$/, 'BuildDetail', ['buildId', 'phaseId', 'waveId', 'agentKey', 'taskId']],
  [/^\/builds\/([^/]+)\/phase\/([^/]+)\/wave\/([^/]+)\/agent\/([^/]+)$/,    'BuildDetail',   ['buildId', 'phaseId', 'waveId', 'agentKey']],
  [/^\/builds\/([^/]+)\/phase\/([^/]+)\/wave\/([^/]+)$/,                    'BuildDetail',   ['buildId', 'phaseId', 'waveId']],
  [/^\/builds\/([^/]+)\/phase\/([^/]+)$/,                                   'BuildDetail',   ['buildId', 'phaseId']],
  // View-encoded pattern (Wave 1) — view is enum-validated in regex so /builds/abc/phase
  // can never match here (would have hit one of the /phase/... routes above first)
  [new RegExp(`^/builds/([^/]+)/(${BUILD_VIEW_PATTERN})$`),                 'BuildDetail',   ['buildId', 'view']],
  [/^\/builds\/([^/]+)$/,                                                   'BuildDetail',   ['buildId']],
  [/^\/dispatch\/run\/([^/]+)\/agent\/([^/]+)$/,                            'Dispatch',      ['runId', 'agentKey']],
  [/^\/dispatch\/run\/([^/]+)$/,                                            'Dispatch',      ['runId']],
  [/^\/helix\/strand\/([^/]+)$/,                                            'Helix',         ['siblingKey']],
  [/^\/helix\/entry\/([^/]+)$/,                                             'Helix',         ['entryId']],
  [/^\/project\/([^/]+)$/,                                                  'ProjectDetail', ['projectId']],
  [/^\/?$/,                                                                 'Dispatch',      []],
  [/^\/dashboard(#.*)?$/,                                                   'Dashboard',     []],
  [/^\/monitor(#.*)?$/,                                                     'Dashboard',     []],
  [/^\/ops(#.*)?$/,                                                         'Dashboard',     []],
  [/^\/run$/,                                                               'Dispatch',      []],
  [/^\/dispatch$/,                                                          'Dispatch',      []],
  [/^\/builds$/,                                                            'Builds',        []],
  [/^\/manage$/,                                                            'Builds',        []],
  [/^\/intake$/,                                                            'Intake',        []],
  [/^\/knowledge$/,                                                         'Helix',         []],
  [/^\/knowledge\/strand\/([^/]+)$/,                                        'Helix',         ['siblingKey']],
  [/^\/knowledge\/entry\/([^/]+)$/,                                         'Helix',         ['entryId']],
  [/^\/memory$/,                                                            'Helix',         []],
  [/^\/memory\/strand\/([^/]+)$/,                                           'Helix',         ['siblingKey']],
  [/^\/memory\/entry\/([^/]+)$/,                                            'Helix',         ['entryId']],
  [/^\/helix$/,                                                             'Helix',         []],
  [/^\/activity$/,                                                          'Comms',         []],
  [/^\/comms$/,                                                             'Comms',         []],
  // Diagram library — must precede the generic /diagrams/:project pattern
  [/^\/diagrams\/library$/,                                                 'DiagramLibrary', []],
  [/^\/library$/,                                                           'DiagramLibrary', []],
  [/^\/diagrams\/(.+)$/,                                                    'Architecture',  ['project']],
  [/^\/diagrams$/,                                                          'Architecture',  []],
  [/^\/editor\/(.+)$/,                                                      'Editor',        ['filepath']],
  [/^\/editor$/,                                                            'Editor',        []],
  [/^\/git$/,                                                               'Git',           []],
  [/^\/pr\/new$/,                                                           'PullRequest',   []],
  [/^\/pr\/(\d+)$/,                                                         'PullRequest',   ['number']],
  [/^\/arch\/(.+)$/,                                                        'Architecture',  ['project']],
  [/^\/arch$/,                                                              'Architecture',  []],
  [/^\/roadmap$/,                                                           'Roadmap',       []],
  [/^\/program$/,                                                           'Program',       []],
  [/^\/observability$/,                                                     'Observability', []],
  [/^\/security$/,                                                          'Security',      []],
  [/^\/ayin$/,                                                              'Observability', []],
  [/^\/tools$/,                                                             'Tools',         []],
  [/^\/autonomous$/,                                                        'AutonomousBuilds', []],
  [/^\/chat$/,                                                              'Chat',          []],
];

/** Matches a hash-fragment path (with or without leading #) to a screen + params. */
export function matchRoute(hash: string): RouteMatch {
  const path = hash.replace(/^#/, '').split('?')[0] || '/';
  for (const [pattern, screen, paramKeys] of ROUTES) {
    const m = path.match(pattern);
    if (m) {
      const params: Record<string, string> = {};
      paramKeys.forEach((k, i) => { params[k] = m[i + 1] ?? ''; });
      return { screen, params };
    }
  }
  return { screen: 'Dashboard', params: {} };
}

/** Checks current hash for legacy paths and rewrites via history.replaceState. */
export function applyRedirects(): void {
  const hash = window.location.hash.slice(1).split('?')[0];
  for (const [from, to] of REDIRECTS) {
    if (hash === from || hash.startsWith(`${from}/`)) {
      const suffix = hash.slice(from.length);
      history.replaceState(null, '', `#${to}${suffix}`);
      window.dispatchEvent(new HashChangeEvent('hashchange'));
      return;
    }
  }
}

/**
 * Navigate to a hash path with optional param interpolation.
 * @example navigate('/builds/:buildId', { buildId: 'abc' })
 */
export function navigate(path: string, params?: Record<string, string>): void {
  let resolved = path;
  if (params) {
    for (const [k, v] of Object.entries(params)) {
      resolved = resolved.replace(`:${k}`, encodeURIComponent(v));
    }
  }
  window.location.hash = resolved;
}
