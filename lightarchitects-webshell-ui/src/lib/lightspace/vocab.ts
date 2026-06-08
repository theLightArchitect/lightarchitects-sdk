/**
 * Lightspace vocabulary — single source of truth for Rule 4 (domain names).
 * Codenames are internal identifiers. Domain names are the default display.
 * Technical mode appends the codename: "Engineering (CORSO)".
 *
 * Standard: arch/lightspace-plain-language-standard.md
 */

/** Lowercase sibling id → domain name */
export const AGENT_DOMAIN: Record<string, string> = {
  corso:   'Engineering',
  eva:     'DevOps',
  soul:    'Knowledge',
  quantum: 'Research',
  seraph:  'Security',
  ayin:    'Observability',
  laex:    'Standards',
  copilot: 'Copilot',
  system:  'System',
};

/** Uppercase codename → domain name (for API responses that return uppercase) */
export const AGENT_DOMAIN_UPPER: Record<string, string> = {
  CORSO:   'Engineering',
  EVA:     'DevOps',
  SOUL:    'Knowledge',
  QUANTUM: 'Research',
  SERAPH:  'Security',
  AYIN:    'Observability',
  'LÆX':   'Standards',
  COPILOT: 'Copilot',
  SYSTEM:  'System',
};

/** Status code → { label, color } — Rule 3 */
export const STATUS_DISPLAY: Record<string, { label: string; color: 'green' | 'yellow' | 'red' | 'blue' | 'gray' }> = {
  PASS:    { label: 'Pass',    color: 'green'  },
  OK:      { label: 'Pass',    color: 'green'  },
  FAIL:    { label: 'Fail',    color: 'red'    },
  WARN:    { label: 'Review',  color: 'yellow' },
  RUNNING: { label: 'Running', color: 'blue'   },
  ACTIVE:  { label: 'Active',  color: 'blue'   },
  PENDING: { label: 'Pending', color: 'gray'   },
  IDLE:    { label: 'Idle',    color: 'gray'   },
};

/** Evidence confidence tier → label — Rule 10 */
export const CONFIDENCE_LABEL: Record<string, string> = {
  VERIFIED:      'Verified',
  MULTI_SOURCE:  'Corroborated',
  SINGLE_SOURCE: 'Single source',
  INFERRED:      'Inferred',
};

/** Internal terms → plain vocabulary */
export const TERM: Record<string, string> = {
  LASDLC:    'build plan',
  Northstar:  'the goal',
  Strand:     'quality dimension',
  Sibling:    'agent',
  XEA:        'exam',
  LDB:        'benchmark',
  PBGC:       'post-build guarantees',
};

/**
 * Resolve a sibling id (any casing) to its domain name.
 * In technical mode returns "Domain (CODENAME)".
 */
export function agentDomain(id: string, technical = false): string {
  const lower = id.toLowerCase().replace('lÆx', 'laex').replace('læx', 'laex');
  const domain = AGENT_DOMAIN[lower] ?? AGENT_DOMAIN_UPPER[id.toUpperCase()] ?? id;
  if (technical) {
    const codename = id.toUpperCase();
    return domain === codename ? domain : `${domain} (${id.toUpperCase()})`;
  }
  return domain;
}
