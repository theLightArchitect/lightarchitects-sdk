// ============================================================================
// Build Plan Schema Validator — runtime validation for portfolio entries
// and per-build manifests. Enforces mandatory exit gates between phases,
// pre-flight/close-out completeness, and schema v1.1 rules.
// ============================================================================

import { META_SKILLS } from './types';
import type {
  BuildPlan, PhaseWithGates, ExitGate, GateCriterion,
  PreFlightCheck, CloseOutStep, GateType, DomainGateCategory,
} from './types';

export interface ValidationResult {
  valid: boolean;
  errors: ValidationError[];
  warnings: ValidationWarning[];
}

export interface ValidationError {
  path: string;     // e.g., 'phase_detail[2].exit_gate.criteria'
  message: string;
  rule: string;     // e.g., 'gate_criteria_non_empty'
}

export interface ValidationWarning {
  path: string;
  message: string;
}

const CODENAME_RE = /^[a-z]+-[a-z]+-[a-z]+$/;
const SEMVER_RE = /^\d+\.\d+\.\d+/;
const VALID_GATE_TYPES: GateType[] = ['quality', 'structural', 'testing', 'security', 'complexity', 'clean_room', 'custom'];
const VALID_DOMAIN_GATES: DomainGateCategory[] = ['security', 'ui_ux', 'dx', 'optimization', 'proofing', 'research', 'observability', 'memory', 'retrieval'];
const VALID_STATUSES = ['planned', 'in_progress', 'complete', 'failed', 'archived', 'production', 'active', 'experimental', 'prototype'];
const VALID_PRIORITIES = ['high', 'medium', 'low'];
const VALID_SOURCES = ['manual', 'github', 'audit', 'discovery'];

/** Validate a full BuildPlan object */
export function validateBuildPlan(plan: BuildPlan): ValidationResult {
  const errors: ValidationError[] = [];
  const warnings: ValidationWarning[] = [];

  // --- Identity ---
  if (!plan.name?.trim()) {
    errors.push({ path: 'name', message: 'Name is required', rule: 'required_field' });
  }
  if (!plan.codename || !CODENAME_RE.test(plan.codename)) {
    errors.push({ path: 'codename', message: 'Codename must match adjective-gerund-noun pattern (e.g., "keen-forging-hawk")', rule: 'codename_format' });
  }
  if (!plan.version || !SEMVER_RE.test(plan.version)) {
    errors.push({ path: 'version', message: 'Version must be valid semver (e.g., "0.3.0")', rule: 'semver_format' });
  }
  if (!plan.description?.trim()) {
    errors.push({ path: 'description', message: 'Description is required', rule: 'required_field' });
  }

  // --- Classification ---
  if (!plan.meta_skill || !META_SKILLS.includes(plan.meta_skill)) {
    errors.push({ path: 'meta_skill', message: `Meta-skill must be one of: ${META_SKILLS.join(', ')}`, rule: 'valid_meta_skill' });
  }
  if (!plan.priority || !VALID_PRIORITIES.includes(plan.priority)) {
    errors.push({ path: 'priority', message: 'Priority must be high, medium, or low', rule: 'valid_priority' });
  }
  if (!plan.source || !VALID_SOURCES.includes(plan.source)) {
    errors.push({ path: 'source', message: 'Source must be manual, github, audit, or discovery', rule: 'valid_source' });
  }
  if (!plan.status || !VALID_STATUSES.includes(plan.status)) {
    errors.push({ path: 'status', message: `Status must be one of: ${VALID_STATUSES.join(', ')}`, rule: 'valid_status' });
  }

  // --- Pre-flight ---
  if (!plan.pre_flight || plan.pre_flight.length === 0) {
    errors.push({ path: 'pre_flight', message: 'Pre-flight checks are required (Section 0)', rule: 'pre_flight_required' });
  } else {
    for (const check of plan.pre_flight) {
      if (!check.id || !check.label) {
        errors.push({ path: `pre_flight[${check.id}]`, message: 'Pre-flight check must have id and label', rule: 'pre_flight_fields' });
      }
    }
    // Blocking checks must be present
    const blockingIds = plan.pre_flight.filter(c => c.blocking).map(c => c.id);
    if (!blockingIds.includes('0a')) {
      warnings.push({ path: 'pre_flight', message: 'Missing blocking check 0a (spec validation)' });
    }
    if (!blockingIds.includes('0b')) {
      warnings.push({ path: 'pre_flight', message: 'Missing blocking check 0b (dependency audit)' });
    }
  }

  // --- Phase + Gate interleaving ---
  if (!plan.phase_detail || plan.phase_detail.length === 0) {
    errors.push({ path: 'phase_detail', message: 'At least one phase is required', rule: 'phases_required' });
  } else {
    for (let i = 0; i < plan.phase_detail.length; i++) {
      const phase = plan.phase_detail[i];
      const prefix = `phase_detail[${i}]`;

      if (!phase.title?.trim()) {
        errors.push({ path: `${prefix}.title`, message: 'Phase title is required', rule: 'required_field' });
      }

      // Every non-skipped phase MUST have an exit_gate
      if (phase.status !== 'skipped') {
        if (!phase.exit_gate) {
          errors.push({ path: `${prefix}.exit_gate`, message: 'Exit gate is mandatory for every non-skipped phase', rule: 'gate_required' });
        } else {
          // Validate the gate itself
          const gateErrors = validateExitGate(phase.exit_gate, `${prefix}.exit_gate`);
          errors.push(...gateErrors);
        }
      }

      // Gate ordering: Phase N+1 cannot be 'active' unless Phase N gate passed/waived
      if (i > 0 && phase.status === 'active') {
        const prevGate = plan.phase_detail[i - 1].exit_gate;
        if (prevGate && prevGate.status !== 'passed' && prevGate.status !== 'waived') {
          errors.push({
            path: `${prefix}.status`,
            message: `Phase ${phase.id} cannot be active — previous phase exit gate is ${prevGate.status}`,
            rule: 'gate_ordering',
          });
        }
      }
    }
  }

  // --- Domain gates ---
  if (plan.domain_gates) {
    for (const dg of plan.domain_gates) {
      if (!VALID_DOMAIN_GATES.includes(dg)) {
        errors.push({ path: 'domain_gates', message: `Invalid domain gate: ${dg}`, rule: 'valid_domain_gate' });
      }
    }
  }

  // --- Close-out ---
  if (!plan.close_out || plan.close_out.length === 0) {
    warnings.push({ path: 'close_out', message: 'Close-out steps recommended (Section 5)' });
  }

  // --- Siblings ---
  if (!plan.siblings || plan.siblings.length === 0) {
    warnings.push({ path: 'siblings', message: 'No siblings assigned — consider assigning at least one' });
  }

  return { valid: errors.length === 0, errors, warnings };
}

/** Validate a single exit gate */
function validateExitGate(gate: ExitGate, prefix: string): ValidationError[] {
  const errors: ValidationError[] = [];

  if (!gate.type || !VALID_GATE_TYPES.includes(gate.type)) {
    errors.push({ path: `${prefix}.type`, message: `Gate type must be one of: ${VALID_GATE_TYPES.join(', ')}`, rule: 'valid_gate_type' });
  }

  if (!gate.criteria || gate.criteria.length === 0) {
    errors.push({ path: `${prefix}.criteria`, message: 'Gate must have at least one criterion', rule: 'gate_criteria_non_empty' });
  } else {
    for (let j = 0; j < gate.criteria.length; j++) {
      const c = gate.criteria[j];
      if (!c.id || !c.label) {
        errors.push({ path: `${prefix}.criteria[${j}]`, message: 'Criterion must have id and label', rule: 'criterion_fields' });
      }
      if (c.type !== 'automated' && c.type !== 'manual') {
        errors.push({ path: `${prefix}.criteria[${j}].type`, message: 'Criterion type must be "automated" or "manual"', rule: 'criterion_type' });
      }
    }
  }

  return errors;
}

/** Quick check: does a phase_detail array have valid gate interleaving? */
export function hasValidGateInterleaving(phases: PhaseWithGates[]): boolean {
  return phases.every((phase, i) => {
    if (phase.status === 'skipped') return true;
    if (!phase.exit_gate || !phase.exit_gate.criteria?.length) return false;
    // Check ordering
    if (i > 0 && phase.status === 'active') {
      const prevGate = phases[i - 1].exit_gate;
      if (prevGate && prevGate.status !== 'passed' && prevGate.status !== 'waived') return false;
    }
    return true;
  });
}
