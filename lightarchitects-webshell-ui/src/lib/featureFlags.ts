// ============================================================================
// Feature flags — UI affordances that are scaffolded but backend-pending.
// Default: all false. Override via localStorage for local dev/preview.
// ============================================================================

export const FEATURE_FLAGS = {
  parallelismEnabled:  false,  // follow-up build: branching-coordinating-loom
  commPubSubEnabled:   false,  // follow-up build: humming-publishing-bee
  multiProjectGateway: false,  // follow-up build: wandering-orienting-harbor
} as const;

export type FeatureFlag = keyof typeof FEATURE_FLAGS;

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
};
