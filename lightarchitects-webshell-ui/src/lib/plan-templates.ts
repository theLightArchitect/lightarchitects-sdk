// ============================================================================
// Plan Templates — LASDLC v1.0 (Light Architects Software Development Lifecycle)
//
// Generates tier-telescoped phases with mandatory interleaved exit gates.
// Three axes: Execution Phases × Quality Gates × Agent Topology.
// Spec: helix/user/standards/lasdlc-spec.md
// Template: helix/corso/builds/LASDLC-TEMPLATE-v1.yaml
// ============================================================================

import { QUALITY_GATE_LABELS, TIER_PHASES } from './types';
import type {
  MetaSkill, GateType, GateCriterion, ExitGate,
  PhaseWithGates, PreFlightCheck, CloseOutStep,
  DomainGateCategory, AgenticConfig, BuildTier, ExecutionPhase,
} from './types';

// ─── Default Gate Criteria (template v2 Section 3) ────────────────────────────

export const DEFAULT_GATE_CRITERIA: Record<GateType, GateCriterion[]> = {
  quality: [
    { id: 'fmt_clean',    label: 'Code formatting clean (cargo fmt / prettier)',   type: 'automated', passed: false },
    { id: 'lint_zero',    label: 'Zero lint warnings (clippy / eslint -D)',        type: 'automated', passed: false },
    { id: 'tests_pass',   label: 'All tests passing',                             type: 'automated', passed: false },
    { id: 'test_ratchet', label: 'Test count >= previous phase (ratchet)',         type: 'automated', passed: false },
  ],
  structural: [
    { id: 'ownership_drift',  label: 'No file ownership drift',                   type: 'automated', passed: false },
    { id: 'contract_drift',   label: 'API contracts match typed interfaces',      type: 'automated', passed: false },
    { id: 'boundary_drift',   label: 'SDK/crate boundary integrity',             type: 'automated', passed: false },
    { id: 'verify_drift',     label: 'Verification coverage maintained',          type: 'automated', passed: false },
    { id: 'test_realism',     label: 'Tests exercise real behavior (no mocking internals)', type: 'manual', passed: false },
  ],
  testing: [
    { id: 'unit_tests',        label: 'Unit tests for new code paths',             type: 'automated', passed: false },
    { id: 'contract_tests',    label: 'Contract tests for new interfaces',         type: 'automated', passed: false },
    { id: 'integration_tests', label: 'Integration tests if cross-module',         type: 'automated', passed: false },
    { id: 'roundtrip_test',    label: 'Serialization roundtrip test',              type: 'automated', passed: false },
    { id: 'real_world_inputs', label: 'Tested with real-world inputs',             type: 'manual',    passed: false },
  ],
  security: [
    { id: 'injection_test',    label: 'No injection vulnerabilities (SQL/XSS/cmd)', type: 'automated', passed: false },
    { id: 'permission_bypass', label: 'No permission bypass paths',                type: 'automated', passed: false },
    { id: 'scope_escape',      label: 'No scope escape vectors',                   type: 'automated', passed: false },
    { id: 'redaction_test',    label: 'Secrets redacted from output/logs',         type: 'automated', passed: false },
  ],
  complexity: [
    { id: 'no_n_squared',     label: 'No O(n^2) in new hot paths',                type: 'automated', passed: false },
    { id: 'complexity_bound',  label: 'Cyclomatic complexity <= 10',               type: 'automated', passed: false },
    { id: 'function_length',   label: 'Functions <= 60 lines',                     type: 'automated', passed: false },
  ],
  clean_room: [
    { id: 'no_code_copy',     label: 'No code copied from references',             type: 'manual', passed: false },
    { id: 'attribution',      label: 'All sources properly attributed',            type: 'manual', passed: false },
  ],
  custom: [],
};

// ─── Domain-Specific Gate Criteria (template v2 Section 4) ────────────────────

export const DOMAIN_GATE_CRITERIA: Record<DomainGateCategory, GateCriterion[]> = {
  security: [
    { id: 'injection_test',       label: 'Injection attack prevention',          type: 'automated', passed: false },
    { id: 'permission_bypass',    label: 'Permission bypass test',               type: 'automated', passed: false },
    { id: 'scope_escape',         label: 'Scope escape test',                    type: 'automated', passed: false },
    { id: 'redaction_test',       label: 'Secret redaction test',                type: 'automated', passed: false },
  ],
  ui_ux: [
    { id: 'input_handling',       label: 'Input handling tested',                type: 'automated', passed: false },
    { id: 'render_sizes',         label: 'Renders at standard viewport sizes',   type: 'manual',    passed: false },
    { id: 'snapshot_regression',  label: 'Visual snapshot regression check',     type: 'automated', passed: false },
    { id: 'graceful_degradation', label: 'Graceful degradation without JS/GPU',  type: 'manual',    passed: false },
  ],
  dx: [
    { id: 'builder_ergonomics',   label: 'Builder pattern ergonomic',            type: 'manual',    passed: false },
    { id: 'error_messages',       label: 'Error messages are helpful',           type: 'manual',    passed: false },
    { id: 'doc_coverage',         label: 'Public API documented',                type: 'automated', passed: false },
    { id: 'api_consistency',      label: 'API naming consistent with project',   type: 'manual',    passed: false },
  ],
  optimization: [
    { id: 'complexity_documented', label: 'Complexity bounds documented',        type: 'manual',    passed: false },
    { id: 'hot_path_benchmark',   label: 'Hot path benchmarked',                 type: 'automated', passed: false },
    { id: 'no_unbounded_alloc',   label: 'No unbounded allocations',             type: 'automated', passed: false },
  ],
  proofing: [
    { id: 'determinism_test',     label: 'Determinism test (same input = same output)', type: 'automated', passed: false },
    { id: 'serialization_rt',    label: 'Serialization roundtrip',               type: 'automated', passed: false },
    { id: 'idempotency_test',    label: 'Idempotency test',                      type: 'automated', passed: false },
    { id: 'property_based',      label: 'Property-based test (proptest/quickcheck)', type: 'automated', passed: false },
  ],
  research: [
    { id: 'context7_consulted',   label: 'Context7 docs consulted for libraries', type: 'manual',   passed: false },
    { id: 'reference_documented', label: 'Reference study documented',            type: 'manual',   passed: false },
    { id: 'research_vetted',      label: 'Research vetted before applied',        type: 'manual',   passed: false },
    { id: 'clean_room_verified',  label: 'Clean room implementation verified',   type: 'manual',   passed: false },
  ],
  observability: [
    { id: 'span_coverage',       label: 'AYIN spans cover new operations',       type: 'automated', passed: false },
    { id: 'trace_doubles',       label: 'Trace test doubles work',               type: 'automated', passed: false },
    { id: 'event_emission',      label: 'Events emitted for state changes',      type: 'automated', passed: false },
  ],
  memory: [
    { id: 'event_roundtrip',     label: 'Event → store → UI roundtrip',          type: 'automated', passed: false },
    { id: 'projection_derivation', label: 'Projections derive correctly',        type: 'automated', passed: false },
    { id: 'retrieval_assembly',  label: 'Retrieval context assembly correct',     type: 'automated', passed: false },
  ],
  retrieval: [
    { id: 'per_signal_hits',     label: 'Per-signal hit counts validated',        type: 'automated', passed: false },
    { id: 'index_existence',     label: 'Required indexes exist',                 type: 'automated', passed: false },
    { id: 'multi_signal_valid',  label: 'Multi-signal fusion produces ranked results', type: 'automated', passed: false },
    { id: 'weight_calibration',  label: 'RRF weight calibration test',           type: 'automated', passed: false },
  ],
};

// ─── Gate type assignment per CORSO pillar ─────────────────────────────────────

/** Maps each pillar action to its natural gate type */
const PILLAR_GATE_MAP: Record<string, GateType> = {
  // /BUILD cycle
  SCOUT: 'structural',   // architecture → check structure before proceeding
  FETCH: 'quality',      // research → quality gate after gathering
  SNIFF: 'quality',      // analysis → quality gate
  GUARD: 'security',     // security hardening → security gate
  CHASE: 'testing',      // testing → testing gate
  HUNT:  'quality',      // implementation → quality gate (fmt + clippy + tests)
  SCRUM: 'clean_room',   // review → clean room check
  // /RESEARCH cycle
  SCAN:      'structural',
  SWEEP:     'quality',
  TRACE:     'quality',
  PROBE:     'quality',
  THEORIZE:  'quality',
  VERIFY:    'testing',
  CLOSE:     'clean_room',
  // /SECURE cycle
  RECON:     'structural',
  SURVEY:    'security',
  EXAMINE:   'security',
  STRIKE:    'security',
  REPORT:    'quality',
  REMEDIATE: 'testing',
  // Shared
  TEAM: 'structural',
  AUTH: 'security',
  CHECK: 'quality',
  REVIEW: 'quality',
  TEST: 'testing',
  DOC: 'clean_room',
};

// ─── Pre-flight Checks (template v2 Section 0) ───────────────────────────────

export function generatePreFlight(): PreFlightCheck[] {
  return [
    { id: '0a', label: 'Spec validation — approved plan exists',                 blocking: true,  status: 'pending', skill: '/PLAN' },
    { id: '0b', label: 'Dependency audit — cargo deny / npm audit clean',        blocking: true,  status: 'pending', fallback_command: 'cargo deny check' },
    { id: '0c', label: 'Sibling impact — which siblings consume changed crates', blocking: true,  status: 'pending' },
    { id: '0d', label: 'Architecture decisions — cloud/local, OSS/paid, DB',     blocking: false, status: 'pending' },
    { id: '0e', label: 'Dependency planning — crates, APIs, services needed',    blocking: false, status: 'pending', skill: '/RESEARCH' },
    { id: '0f', label: 'Agent composition — file-ownership partitioning',        blocking: false, status: 'pending' },
    { id: '0g', label: 'Context budget — token estimate per phase',              blocking: false, status: 'pending' },
    { id: '0h', label: 'HITL protocol — human vs autonomous boundaries',         blocking: false, status: 'pending' },
    { id: '0i', label: 'Fallback chains — LLM retry, MCP down, overflow',       blocking: false, status: 'pending' },
    { id: '0j', label: 'Risk analysis — top failure modes identified',           blocking: true,  status: 'pending', skill: '/RISK-ANALYSIS' },
    { id: '0k', label: 'Project board — tracking artifacts generated',           blocking: false, status: 'pending' },
  ];
}

// ─── Close-out Steps (template v2 Section 5) ──────────────────────────────────

export function generateCloseOut(): CloseOutStep[] {
  return [
    { id: '5a', label: 'Cross-build learning — most expensive mistake, what to change',  status: 'pending', skill: '/REFLECT' },
    { id: '5b', label: 'Training data capture — traces worth preserving for Arena',       status: 'pending', skill: '/ENRICH' },
    { id: '5c', label: 'Cross-build memory — SOUL significance score + helix write',      status: 'pending' },
    { id: '5d', label: 'Spec audit — compliant/partial/deviates matrix',                  status: 'pending', skill: '/GATE' },
    { id: '5e', label: 'SQUAD review — multi-sibling plan review',                        status: 'pending', skill: '/SCRUM' },
    { id: '5f', label: 'Deploy — binary built, codesigned, installed, MCP reconnect',     status: 'pending', skill: '/DEPLOY' },
  ];
}

// ─── Default Agentic Config (template v2 Section 6) ──────────────────────────

export function generateAgenticConfig(): AgenticConfig {
  return {
    agent_composition: 'File-ownership partitioning (Canon XXIII). DAG-ordered batching.',
    context_budget: 'Estimate tokens per phase. Compact at 70% capacity.',
    tool_permissions: 'Read-only for research phases. Write+Edit for implementation. HITL for destructive.',
    fallback_chains: 'LLM: retry 3x → HITL. MCP: filesystem fallback → warn. Context: compact → split phase.',
    hitl_protocol: 'Destructive ops, scope violations, architecture divergence, rollback, cost > $10.',
  };
}

// ─── Phase + Gate Generation ──────────────────────────────────────────────────

/** Create a default exit gate for a given action/gate type */
function makeGate(gateType: GateType, skill?: string, fallback?: string): ExitGate {
  return {
    type: gateType,
    criteria: DEFAULT_GATE_CRITERIA[gateType].map(c => ({ ...c })),
    status: 'pending',
    hitl_required: gateType === 'clean_room', // clean room always needs human approval
    skill: skill ?? gateSkillMap[gateType],
    fallback_command: fallback ?? gateFallbackMap[gateType],
  };
}

const gateSkillMap: Record<GateType, string | undefined> = {
  quality: '/GATE',
  structural: '/WIRING',
  testing: '/TESTING',
  security: '/SECURE',
  complexity: undefined,
  clean_room: undefined,
  custom: undefined,
};

const gateFallbackMap: Record<GateType, string | undefined> = {
  quality: 'cargo fmt --check && cargo clippy -D warnings && cargo test',
  structural: undefined,
  testing: 'cargo test --all-features',
  security: undefined,
  complexity: undefined,
  clean_room: undefined,
  custom: undefined,
};

/**
 * Generate a tier-telescoped LASDLC plan with mandatory exit gates.
 * Default tier is MEDIUM (6 phases). Override with explicit tier parameter.
 */
export function generateDefaultPlan(metaSkill: MetaSkill, tier: BuildTier = 'MEDIUM'): PhaseWithGates[] {
  const phaseNames = TIER_PHASES[tier];
  const phases: PhaseWithGates[] = [];

  for (let i = 0; i < phaseNames.length; i++) {
    const phaseName = phaseNames[i];
    const config = LASDLC_PHASE_CONFIG[phaseName];

    phases.push({
      id: i + 1,
      title: `${phaseName} — ${config.subtitle}`,
      status: i === 0 ? 'active' : 'pending',
      description: config.description,
      items: [],
      files_expected: [],
      deliverables: config.deliverables,
      assigned_sibling: config.defaultSibling,
      exit_gate: makeGate(config.gateType),
    });
  }

  return phases;
}


/** Get phases for a specific tier */
export function getPhasesForTier(tier: BuildTier): ExecutionPhase[] {
  return TIER_PHASES[tier];
}

// ─── LASDLC Phase Configuration ──────────────────────────────────────────────

interface PhaseConfig {
  subtitle: string;
  description: string;
  deliverables: string[];
  gateType: GateType;
  defaultSibling?: string;
}

const LASDLC_PHASE_CONFIG: Record<ExecutionPhase, PhaseConfig> = {
  Plan: {
    subtitle: 'Requirements & Architecture',
    description: 'Define requirements, architecture, interfaces, scope, and file-function ownership map',
    deliverables: ['Specification', 'Architecture plan', 'File-function map'],
    gateType: 'structural',
    defaultSibling: 'corso',
  },
  Research: {
    subtitle: 'Dependencies & Prior Art',
    description: 'Audit dependencies, gather prior art, consult library docs, model threats',
    deliverables: ['Dependency list', 'Advisory results', 'Research notes'],
    gateType: 'quality',
    defaultSibling: 'quantum',
  },
  Implement: {
    subtitle: 'Code & Integration',
    description: 'Write code — types, logic, modules, wiring, integration',
    deliverables: ['Source code', 'Inline documentation', 'Integration tests'],
    gateType: 'quality',
    defaultSibling: 'corso',
  },
  Harden: {
    subtitle: 'Security & Performance',
    description: 'Security scanning, performance profiling, observability instrumentation',
    deliverables: ['Vulnerability report', 'Performance baseline', 'AYIN spans'],
    gateType: 'security',
    defaultSibling: 'seraph',
  },
  Verify: {
    subtitle: 'Testing & Validation',
    description: 'Testing — unit, integration, contract, property-based, E2E',
    deliverables: ['Test suite', 'Coverage report', 'Regression check'],
    gateType: 'testing',
    defaultSibling: 'corso',
  },
  Ship: {
    subtitle: 'Build & Deploy',
    description: 'Build, codesign, deploy, reconnect, smoke test',
    deliverables: ['Release binary', 'Deploy receipt', 'Health check'],
    gateType: 'quality',
    defaultSibling: 'ayin',
  },
  Learn: {
    subtitle: 'Retrospective & Enrichment',
    description: 'Retrospective, helix enrichment, training data capture, team review',
    deliverables: ['SCRUM report', 'Helix entry', 'Arena traces'],
    gateType: 'clean_room',
    defaultSibling: 'soul',
  },
};

/** Pillar → human-readable description */
const pillarDescriptions: Record<string, string> = {
  ARCH: 'Architecture & Design',
  SEC:  'Security & Dependencies',
  QUAL: 'Quality & Analysis',
  PERF: 'Performance & Hardening',
  TEST: 'Testing & Validation',
  DOC:  'Documentation & Implementation',
  OPS:  'Operations & Review',
};

// ─── SDLC Display Names ──────────────────────────────────────────────────────
// Primary labels for the UI. CORSO codenames shown as subtitles for squad members.

/** CORSO action → universally recognizable SDLC phase name */
export const SDLC_NAMES: Record<string, string> = {
  // /BUILD cycle
  SCOUT:     'Plan',
  FETCH:     'Research',
  SNIFF:     'Analyze',
  GUARD:     'Secure',
  CHASE:     'Test',
  HUNT:      'Build',
  SCRUM:     'Review',
  // /RESEARCH cycle
  SCAN:      'Survey',
  SWEEP:     'Collect',
  TRACE:     'Trace',
  PROBE:     'Investigate',
  THEORIZE:  'Hypothesize',
  VERIFY:    'Validate',
  CLOSE:     'Conclude',
  // /SECURE cycle
  RECON:     'Discover',
  SURVEY:    'Enumerate',
  EXAMINE:   'Assess',
  STRIKE:    'Exploit',
  REPORT:    'Report',
  REMEDIATE: 'Remediate',
  // Shared (/SQUAD, /DEPLOY)
  TEAM:      'Assemble',
  AUTH:      'Authorize',
  CHECK:     'Check',
  REVIEW:    'Review',
  TEST:      'Test',
  DOC:       'Document',
};

/** Action → human-readable phase description */
const phaseDescriptions: Record<string, string> = {
  SCOUT:     'Design component hierarchy, define interfaces, plan architecture',
  FETCH:     'Audit dependencies, check advisories, gather context from docs',
  SNIFF:     'Review existing patterns, identify quality issues and refactoring targets',
  GUARD:     'Scan for vulnerabilities, audit permissions, test for injection',
  CHASE:     'Generate test plan, write test scaffolds, verify coverage targets',
  HUNT:      'Implement features, wire modules, integrate components',
  SCRUM:     'Multi-sibling assessment, synthesize findings, deploy decision',
  SCAN:      'Gather initial evidence, map the landscape and scope',
  SWEEP:     'Systematic evidence gathering across all available sources',
  TRACE:     'Follow execution paths and dependency chains to root causes',
  PROBE:     'Deep targeted analysis of specific findings and anomalies',
  THEORIZE:  'Propose explanations, rank hypotheses by evidence strength',
  VERIFY:    'Confirm or deny hypotheses with tests and reproduction',
  CLOSE:     'Document conclusions, archive evidence, write final report',
  RECON:     'Passive target enumeration, surface area mapping, fingerprinting',
  SURVEY:    'Active service discovery, port scanning, technology identification',
  EXAMINE:   'CVE matching, configuration review, dependency vulnerability audit',
  STRIKE:    'Controlled proof-of-concept exploitation, evidence capture',
  REPORT:    'Severity classification, impact analysis, remediation recommendations',
  REMEDIATE: 'Apply patches, re-test attack vectors, confirm remediation success',
};

/** Action → expected deliverables */
const phaseDeliverables: Record<string, string[]> = {
  SCOUT:  ['Architecture plan', 'Interface definitions', 'File ownership map'],
  FETCH:  ['Dependency list', 'Advisory check results', 'Research notes'],
  SNIFF:  ['Code review findings', 'Refactoring targets', 'Quality metrics'],
  GUARD:  ['Security scan report', 'Vulnerability list', 'Permission audit'],
  CHASE:  ['Test plan', 'Test scaffolds', 'Coverage report'],
  HUNT:   ['Implementation code', 'Integration tests', 'Updated docs'],
  SCRUM:  ['SQUAD review report', 'Deploy decision', 'Helix entry'],
};

/** Action → default sibling assignment */
const actionSiblingMap: Record<string, string | undefined> = {
  SCOUT: 'corso', FETCH: 'quantum', SNIFF: 'corso', GUARD: 'seraph',
  CHASE: 'corso', HUNT: 'corso', SCRUM: 'corso',
  SCAN: 'quantum', SWEEP: 'quantum', TRACE: 'quantum', PROBE: 'quantum',
  THEORIZE: 'quantum', VERIFY: 'quantum', CLOSE: 'quantum',
  RECON: 'seraph', SURVEY: 'seraph', EXAMINE: 'seraph', STRIKE: 'seraph',
  REPORT: 'seraph', REMEDIATE: 'seraph',
};

// ─── Auto-detect Domain Gates ─────────────────────────────────────────────────

/** Suggest domain gates based on what the build touches */
export function suggestDomainGates(
  language: string,
  path: string,
  description: string,
): DomainGateCategory[] {
  const gates: DomainGateCategory[] = [];
  const lower = `${language} ${path} ${description}`.toLowerCase();

  if (lower.includes('auth') || lower.includes('token') || lower.includes('permission') || lower.includes('secure'))
    gates.push('security');
  if (lower.includes('svelte') || lower.includes('react') || lower.includes('ui') || lower.includes('css') || lower.includes('frontend'))
    gates.push('ui_ux');
  if (lower.includes('sdk') || lower.includes('api') || lower.includes('client') || lower.includes('library'))
    gates.push('dx');
  if (lower.includes('perf') || lower.includes('optim') || lower.includes('cache') || lower.includes('hot path'))
    gates.push('optimization');
  if (lower.includes('serial') || lower.includes('deterministic') || lower.includes('idempoten'))
    gates.push('proofing');
  if (lower.includes('research') || lower.includes('library') || lower.includes('new dep'))
    gates.push('research');
  if (lower.includes('trace') || lower.includes('ayin') || lower.includes('observ') || lower.includes('log'))
    gates.push('observability');
  if (lower.includes('soul') || lower.includes('helix') || lower.includes('vault') || lower.includes('memory'))
    gates.push('memory');
  if (lower.includes('search') || lower.includes('index') || lower.includes('retriev') || lower.includes('rrf'))
    gates.push('retrieval');

  return [...new Set(gates)];
}
