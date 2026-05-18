// ============================================================================
// Feature flags — UI affordances that are scaffolded but backend-pending.
// Default: all false. Override via localStorage for local dev/preview.
// ============================================================================

export const FEATURE_FLAGS = {
  parallelismEnabled:  false,  // follow-up build: branching-coordinating-loom
  commPubSubEnabled:   false,  // follow-up build: humming-publishing-bee
  multiProjectGateway: false,  // follow-up build: wandering-orienting-harbor
  /** GitForest AYIN pulse overlay — `la_pulse_enabled` localStorage key. */
  pulseEnabled:        true,   // gitforest-live-ops: AYIN span→polytope pulse
  /** GitForest stats topbar — `la_stats_topbar_enabled` localStorage key. */
  statsTopbarEnabled:  true,   // gitforest-live-ops: Builds/Active/Agents/Gates
} as const;

export type FeatureFlag = keyof typeof FEATURE_FLAGS;

// localStorage key aliases used by GitForest components.
export const LA_PULSE_KEY        = 'la_pulse_enabled';
export const LA_STATS_TOPBAR_KEY = 'la_stats_topbar_enabled';

/**
 * Check whether a feature flag is enabled.
 * LocalStorage key `la.feature.<flag>` overrides the compile-time default.
 * Falls back to the default if localStorage is unavailable (sandboxed env).
 */
export function isEnabled(flag: FeatureFlag): boolean {
  try {
    const override = localStorage.getItem(`la.feature.${flag}`);
    if (override !== null) return override === 'true';
  } catch {
    // localStorage unavailable — sandboxed environment or SSR
  }
  return FEATURE_FLAGS[flag];
}

/** Tooltip shown on disabled UI controls guarded by a feature flag. */
export const FLAG_TOOLTIP: Record<FeatureFlag, string> = {
  parallelismEnabled:  'Custom-squad orchestrator pending — see follow-up build',
  commPubSubEnabled:   'Inter-agent pub/sub pending — see follow-up build',
  multiProjectGateway: 'Multi-project gateway pending — see follow-up build',
  pulseEnabled:        'AYIN pulse overlay — toggle off to reduce canvas updates',
  statsTopbarEnabled:  'GitForest stats topbar — toggle off for minimal header',
};

/**
 * `?safe=1` URL param forces `pulseEnabled` and `statsTopbarEnabled` to false,
 * useful when inspecting the static topology without live-update noise.
 */
export function isSafeMode(): boolean {
  try {
    return new URLSearchParams(window.location.search).get('safe') === '1';
  } catch {
    return false;
  }
}

/**
 * Check whether a GitForest-specific flag is enabled.
 * `?safe=1` forces `pulseEnabled` and `statsTopbarEnabled` off.
 */
export function isGitForestFlagEnabled(flag: 'pulseEnabled' | 'statsTopbarEnabled'): boolean {
  if (isSafeMode()) return false;
  return isEnabled(flag);
}
