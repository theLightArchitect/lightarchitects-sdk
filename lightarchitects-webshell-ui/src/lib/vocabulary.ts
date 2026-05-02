// ============================================================================
// Vocabulary canon — public-facing term mapping
// "Pillar" → "Quality Gate", "Sibling" → "Agent"
// Internal code (skills, Map keys, enums) preserves original names.
// ============================================================================

export const TERMS: Record<string, string> = {
  // Quality Gate (was "Pillar")
  Pillar:   'Quality Gate',
  Pillars:  'Quality Gates',
  PILLAR:   'QUALITY GATE',
  PILLARS:  'QUALITY GATES',
  pillar:   'quality gate',
  pillars:  'quality gates',

  // Agent (was "Sibling" on public surfaces)
  Sibling:  'Agent',
  Siblings: 'Agents',
  sibling:  'agent',
  siblings: 'agents',
};

// Navigation tab labels (used in App.svelte tab strip)
export const NAV_LABELS = {
  ops:      'OPS',
  dispatch: 'DISPATCH',
  builds:   'BUILDS',
  helix:    'HELIX',
} as const;

// Tooltip definitions for technical terms shown on public surfaces
export const TOOLTIPS: Record<string, string> = {
  MCP:    'Model Context Protocol — a standard for connecting AI to tools',
  Skill:  'A reusable capability that agents can invoke (similar to a tool or function)',
  Helix:  'The knowledge graph — stores decisions, context, and team memory across sessions',
  Wave:   'A unit of parallel agent work within a build phase',
  Phase:  'A stage in the build lifecycle (Plan → Research → Implement → Harden → Verify → Ship → Learn)',
  Rail:   "An agent's work stream within the operator console",
  LASDLC: 'Light Architects Software Development Lifecycle — the build execution framework',
  Arena:  'Training data factory — used internally to generate and evaluate model outputs',
};

/** Translate an internal term to its public-facing label. Falls through unchanged if no mapping. */
export function t(key: string): string {
  return TERMS[key] ?? key;
}

/** Tooltip text for a term, or undefined if none defined. */
export function tip(key: string): string | undefined {
  return TOOLTIPS[key];
}
