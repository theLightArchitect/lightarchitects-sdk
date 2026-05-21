/**
 * Cockpit card-role taxonomy — extends the §1.7 screen-level view roles to
 * card granularity. Every load-bearing surface on the Cockpit screen declares
 * `data-card-role` on its root element, keyed by this registry.
 *
 * Roles fall into four functional categories:
 *   action     — operator takes immediate actions (approve/deny, verb buttons)
 *   stream     — live data flowing from the backend
 *   status     — current system state, not live-streaming
 *   navigation — changes the active target or preset
 *
 * Cockpit is the first hybrid (action + stream) card witness in the platform.
 */

/** Union of all valid card roles on the Cockpit screen. */
export type CockpitCardRole =
  | 'preset-chips'
  | 'target-breadcrumb'
  | 'quick-pick-palette'
  | 'build-health'
  | 'hitl-escalations'
  | 'worker-fleet'
  | 'decision-feed'
  | 'git-state'
  | 'builds-rail'
  | 'hitl-inbox'
  | 'pr-detail-panel'
  | 'engineer-zones'
  | 'copilot-drawer';

/** Human-readable description for each card role. */
export const COCKPIT_CARD_ROLES: Record<CockpitCardRole, string> = {
  'preset-chips':        'navigation — switches the active domain preset (engineer/security/ops/…)',
  'target-breadcrumb':   'navigation — displays current PR/build target; one-click to change',
  'quick-pick-palette':  'navigation — keyboard-first overlay for selecting any target type',
  'build-health':        'status+stream — sparkline and aggregate build stats (done/active/failed/queued)',
  'hitl-escalations':    'action — approve or deny pending agent permission requests with countdown timer',
  'worker-fleet':        'stream — live agent slot occupancy, conductor queue depth, running task context',
  'decision-feed':       'stream — L1–L4 architectural decisions from the active build, sortable by severity',
  'git-state':           'status — branch name, staged/modified/untracked counts, active worktree topology',
  'builds-rail':         'navigation+status — paginated build list with status dots; click navigates to build',
  'hitl-inbox':          'action — pending PR reviews and agent tasks surfaced for operator triage',
  'pr-detail-panel':     'action — PRMetadataBlock + verb surface (approve/request-changes/comment) for selected PR',
  'engineer-zones':      'action+stream — NeedsAction / InFlight / QuickActions / Insights panels for engineer preset',
  'copilot-drawer':      'action+stream — AI assistant; context-aware to cockpit preset and selected target',
};

/** All card role keys, derived from the registry for exhaustiveness checks. */
export const ALL_COCKPIT_CARD_ROLES = Object.keys(
  COCKPIT_CARD_ROLES,
) as CockpitCardRole[];
