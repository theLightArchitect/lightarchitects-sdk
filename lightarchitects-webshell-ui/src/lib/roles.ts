// ============================================================================
// Agent role classification — maps sibling + action to squad role badges
// ============================================================================

/** Squad operational roles visible in the Activity panel. */
export type AgentRole = 'Doer' | 'Planner' | 'Critic' | 'Supervisor' | 'Learner' | 'Presenter';

/** Role badge color (Tailwind-compatible hex). Tuned for dark bg-[#0a0a0f]. */
export const ROLE_COLORS: Record<AgentRole, string> = {
  Doer:       '#22c55e', // green — execution
  Planner:    '#3b82f6', // blue — planning
  Critic:     '#ef4444', // red — review/security
  Supervisor: '#f59e0b', // amber — orchestration
  Learner:    '#a78bfa', // violet — research
  Presenter:  '#ec4899', // pink — output/memory
};

/** Background tint at ~12% opacity for subtle badge fill. */
export const ROLE_BG: Record<AgentRole, string> = {
  Doer:       'rgba(34,197,94,0.12)',
  Planner:    'rgba(59,130,246,0.12)',
  Critic:     'rgba(239,68,68,0.12)',
  Supervisor: 'rgba(245,158,11,0.12)',
  Learner:    'rgba(167,139,250,0.12)',
  Presenter:  'rgba(236,72,153,0.12)',
};

// --- Action → role lookup tables (lowercase) ---

const CORSO_PLANNER: ReadonlySet<string> = new Set([
  'sniff', 'scout', 'fetch',
]);

const CORSO_DOER: ReadonlySet<string> = new Set([
  'hunt', 'generate_code', 'code_review', 'chase', 'scrum',
]);

const CORSO_CRITIC: ReadonlySet<string> = new Set([
  'guard', 'chow', 'alpha',
]);

const QUANTUM_LEARNER: ReadonlySet<string> = new Set([
  'research', 'sweep', 'trace', 'scan', 'probe',
]);

const QUANTUM_CRITIC: ReadonlySet<string> = new Set([
  'verify', 'theorize', 'close',
]);

const EVA_DOER: ReadonlySet<string> = new Set([
  'deploy', 'build', 'hook', 'pipeline',
]);

const EVA_PRESENTER: ReadonlySet<string> = new Set([
  'remember', 'crystallize', 'enrich', 'recall', 'reflect',
]);

const SOUL_LEARNER: ReadonlySet<string> = new Set([
  'search', 'helix', 'query', 'retrieve', 'consolidate',
]);

const SOUL_PRESENTER: ReadonlySet<string> = new Set([
  'write_note', 'voice', 'promote', 'write',
]);

/** Default fallback role per sibling when action is unknown. */
const SIBLING_DEFAULT: Record<string, AgentRole> = {
  corso:   'Doer',
  quantum: 'Learner',
  seraph:  'Critic',
  eva:     'Presenter',
  soul:    'Learner',
  ayin:    'Supervisor',
  laex:    'Supervisor',
};

/**
 * Derive the squad role for a sibling + action pair.
 *
 * Action matching is case-insensitive and uses substring containment
 * so compound actions like "corso_guard_scan" still match the right set.
 */
export function getAgentRole(sibling: string, action?: string): AgentRole {
  const sib = sibling.toLowerCase();
  const act = action?.toLowerCase() ?? '';

  switch (sib) {
    case 'corso':
      if (matchesAny(act, CORSO_PLANNER)) return 'Planner';
      if (matchesAny(act, CORSO_CRITIC))  return 'Critic';
      if (matchesAny(act, CORSO_DOER))    return 'Doer';
      break;

    case 'quantum':
      if (matchesAny(act, QUANTUM_CRITIC))  return 'Critic';
      if (matchesAny(act, QUANTUM_LEARNER)) return 'Learner';
      break;

    case 'seraph':
      return 'Critic'; // all SERAPH actions are security review

    case 'eva':
      if (matchesAny(act, EVA_DOER))      return 'Doer';
      if (matchesAny(act, EVA_PRESENTER)) return 'Presenter';
      break;

    case 'soul':
      if (matchesAny(act, SOUL_PRESENTER)) return 'Presenter';
      if (matchesAny(act, SOUL_LEARNER))   return 'Learner';
      break;

    case 'ayin':
    case 'laex':
      return 'Supervisor';
  }

  return SIBLING_DEFAULT[sib] ?? 'Doer';
}

/** Check if the action string contains any keyword from the set. */
function matchesAny(action: string, keywords: ReadonlySet<string>): boolean {
  if (!action) return false;
  for (const kw of keywords) {
    if (action.includes(kw)) return true;
  }
  return false;
}
