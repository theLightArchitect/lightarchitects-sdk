/**
 * Mosaic panel-role taxonomy — Dashboard screen panels declare `data-card-role`
 * on their root element, keyed by this registry.
 *
 * Semantically distinct from Cockpit card roles (lib/cockpit/cardRoles.ts):
 *   • Cockpit roles  — fixed-layout screen cards for build orchestration
 *   • Mosaic roles   — user-composable Dashboard panels, drag-split layout
 *
 * Roles follow the same four-category vocabulary:
 *   action     — operator takes immediate actions
 *   stream     — live data flowing from the backend
 *   status     — current system state, not live-streaming
 *   navigation — changes the active target or view
 *
 * Northstar alignment:
 *   P3 (MoE Platform) — operator sees adaptive retrieval mode (KW/BALANCED/GRAPH)
 *                        and signal counts directly in the webshell panel.
 *   P5 (Persistent Knowledge) — cache-stats surfaces TinyLFU entry count so the
 *                        operator can reason about helix knowledge coverage.
 */

/** Union of all valid panel roles on the Dashboard mosaic. */
export type MosaicPanelRole =
  | 'retrieval-metrics'
  | 'cache-stats';

/** Human-readable description for each mosaic panel role. */
export const MOSAIC_PANEL_ROLES: Record<MosaicPanelRole, string> = {
  'retrieval-metrics': 'stream — adaptive helix retrieval; shows mode (KW/BALANCED/GRAPH), result count, and cache-hit ratio; advances P3 operator legibility',
  'cache-stats':       'status — TinyLFU helix cache counters (entry count, weighted size); surfaces P5 knowledge coverage at a glance',
};

/** All panel role keys, derived from the registry for exhaustiveness checks. */
export const ALL_MOSAIC_PANEL_ROLES = Object.keys(
  MOSAIC_PANEL_ROLES,
) as MosaicPanelRole[];
