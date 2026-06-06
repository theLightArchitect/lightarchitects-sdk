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
  | 'copilot-drawer'
  | 'strategy-catalogue'
  | 'wave-composer'
  | 'northstar-pulse'
  | 'strand-mosaic'
  | 'smart-dispatch'
  | 'squad-constellation';

/** Scope depths at which a card role is valid (0=platform, 1=project, 2=build, 3=file). */
export type CardScopeDepth = 0 | 1 | 2 | 3;

export interface CockpitCardRoleMeta {
  description: string;
  /** Scope depths where this card is valid. Empty = all scopes. */
  scope: CardScopeDepth[];
}

/** Human-readable description + scope constraints for each card role. */
export const COCKPIT_CARD_ROLES: Record<CockpitCardRole, CockpitCardRoleMeta> = {
  'preset-chips':        { description: 'navigation — switches the active domain preset (engineer/security/ops/…)',                             scope: [] },
  'target-breadcrumb':   { description: 'navigation — displays current PR/build target; one-click to change',                                   scope: [] },
  'quick-pick-palette':  { description: 'navigation — keyboard-first overlay for selecting any target type',                                     scope: [] },
  'build-health':        { description: 'status+stream — sparkline and aggregate build stats (done/active/failed/queued)',                        scope: [1, 2] },
  'hitl-escalations':    { description: 'action — approve or deny pending agent permission requests with countdown timer',                       scope: [1, 2] },
  'worker-fleet':        { description: 'stream — live agent slot occupancy, conductor queue depth, running task context',                       scope: [2] },
  'decision-feed':       { description: 'stream — L1–L4 architectural decisions from the active build, sortable by severity',                    scope: [2] },
  'git-state':           { description: 'status — branch name, staged/modified/untracked counts, active worktree topology',                     scope: [2, 3] },
  'builds-rail':         { description: 'navigation+status — paginated build list with status dots; click navigates to build',                  scope: [1] },
  'hitl-inbox':          { description: 'action — pending PR reviews and agent tasks surfaced for operator triage',                              scope: [0, 1] },
  'pr-detail-panel':     { description: 'action — PRMetadataBlock + verb surface (approve/request-changes/comment) for selected PR',            scope: [1, 2] },
  'engineer-zones':      { description: 'action+stream — NeedsAction / InFlight / QuickActions / Insights panels for engineer preset',          scope: [2] },
  'copilot-drawer':      { description: 'action+stream — AI assistant; context-aware to cockpit preset and selected target',                     scope: [] },
  'strategy-catalogue':  { description: 'status — all 10 loop strategies (6 L2 registered + 4 L0 executor-backed); L2 tiles are selectable',   scope: [0, 1] },
  'wave-composer':       { description: 'action — compose and dispatch a multi-agent wave from domain preset chips and per-agent task descriptions', scope: [2] },
  'northstar-pulse':     { description: 'status — P1–P7 health bars composed from per-build supervisor state (§2.51)',                          scope: [0] },
  'strand-mosaic':       { description: 'status — project × gatekeeper risk matrix (Canon XXX strand mosaic — §2.52)',                          scope: [0, 1] },
  'smart-dispatch':      { description: 'action — reasons-aware dispatch suggestions composed from observable signals (§2.53)',                  scope: [0] },
  'squad-constellation': { description: 'status+stream — 7-sibling constellation with live A2A link edges (§2.54)',                             scope: [0] },
};

/** All card role keys, derived from the registry for exhaustiveness checks. */
export const ALL_COCKPIT_CARD_ROLES = Object.keys(
  COCKPIT_CARD_ROLES,
) as CockpitCardRole[];
