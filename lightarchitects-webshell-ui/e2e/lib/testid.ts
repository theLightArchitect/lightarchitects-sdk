/**
 * Typed data-testid selectors (§57.5).
 *
 * All stable testid values in one place. Import getTestId() in specs
 * instead of hardcoding selector strings. Typos become compile errors.
 */
import type { Page, Locator } from '@playwright/test';

// Canonical testid registry — add entries as components gain data-testid attrs
export const TESTIDS = {
  // Dispatch
  dispatchInput:           'dispatch-input',
  agentBtn:                (agent: string) => `agent-btn-${agent}`,
  agentRail:               (agent: string) => `agent-rail-${agent}`,
  agentDetail:             (agent: string) => `agent-detail-${agent}`,

  // OPS
  squadHealthPanel:        'squad-health-panel',

  // Chrome
  navTab:                  (label: string) => `nav-tab-${label.toLowerCase()}`,
  activeBuildsChip:        'active-builds-chip',
  projectPicker:           'project-picker',

  // Copilot
  copilotDrawer:           'copilot-drawer',
  copilotInput:            'copilot-input',
  copilotSend:             'copilot-send',

  // Build lifecycle
  buildCard:               (id: string) => `build-card-${id}`,
  pillarRail:              (pillar: string) => `pillar-rail-${pillar}`,
} as const;

/** Get a locator by testid. */
export function byTestId(page: Page, testid: string): Locator {
  return page.locator(`[data-testid="${testid}"]`);
}
