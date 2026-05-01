// ============================================================================
// Squad communication — typed pub/sub schema (12 message types)
// v1: TypeScript types + UI rendering. Backend pub/sub in follow-up build.
// Schema source: helix/inter_agent_communication (2026-04-30).
// ============================================================================

export type MessageType =
  | 'commit.completed'
  | 'decision.made'
  | 'gap.discovered'
  | 'blocker.raised'
  | 'assumption.flagged'
  | 'risk.surfaced'
  | 'convention.established'
  | 'review.requested'
  | 'finding.classified'
  | 'handoff.completed'
  | 'context.shared'
  | 'progress.updated';

export type DomainAgent =
  | 'engineer'
  | 'quality'
  | 'security'
  | 'ops'
  | 'researcher'
  | 'knowledge'
  | 'performance'
  | 'testing'
  | 'documentation';

export type MessageImportance = 'low' | 'normal' | 'high' | 'critical';
export type FindingSeverity = 'CRITICAL' | 'HIGH' | 'MEDIUM' | 'LOW';

// ── Payload types ─────────────────────────────────────────────────────────────

export interface CommitCompletedPayload {
  sha: string;
  branch: string;
  files: string[];
  message: string;
  lines_changed: number;
  agent_role: DomainAgent;
}

export interface DecisionMadePayload {
  decision: string;
  alternatives_rejected: string[];
  rationale: string;
  scope: string[];
}

export interface GapDiscoveredPayload {
  description: string;
  scope: string[];
  suggested_owner?: DomainAgent;
  blocks_what?: string;
}

export interface BlockerRaisedPayload {
  description: string;
  blocking_what: string;
  needs_from?: DomainAgent;
}

export interface AssumptionFlaggedPayload {
  assumption: string;
  would_invalidate_if: string[];
}

export interface RiskSurfacedPayload {
  risk: string;
  blast_radius: string;
  mitigation?: string;
}

export interface ConventionEstablishedPayload {
  rule: string;
  scope: string[];
  examples: string[];
}

export interface ReviewRequestedPayload {
  artifact: string;
  reviewer: DomainAgent;
  deadline?: string;
}

export interface FindingClassifiedPayload {
  finding_id: string;
  severity: FindingSeverity;
  fix_owner?: DomainAgent;
  evidence: string;
}

export interface HandoffCompletedPayload {
  summary: string;
  next_owner: DomainAgent;
  files_touched: string[];
}

export interface ContextSharedPayload {
  description: string;
  files_touched: string[];
  useful_for: DomainAgent[];
}

export interface ProgressUpdatedPayload {
  progress_pct: number;
  current_step: string;
  eta_ms?: number;
}

// ── Payload discriminated union ───────────────────────────────────────────────

export type PayloadByType = {
  'commit.completed':       CommitCompletedPayload;
  'decision.made':          DecisionMadePayload;
  'gap.discovered':         GapDiscoveredPayload;
  'blocker.raised':         BlockerRaisedPayload;
  'assumption.flagged':     AssumptionFlaggedPayload;
  'risk.surfaced':          RiskSurfacedPayload;
  'convention.established': ConventionEstablishedPayload;
  'review.requested':       ReviewRequestedPayload;
  'finding.classified':     FindingClassifiedPayload;
  'handoff.completed':      HandoffCompletedPayload;
  'context.shared':         ContextSharedPayload;
  'progress.updated':       ProgressUpdatedPayload;
};

export interface Message<T extends MessageType = MessageType> {
  id: string;
  type: T;
  from: DomainAgent | 'coordinator';
  to: DomainAgent | 'squad' | 'coordinator' | null;
  topic: string;
  timestamp: string;       // ISO 8601
  importance: MessageImportance;
  causal_ref?: string;     // message ID this responds to
  payload: PayloadByType[T];
}

// ── Default importance per type ───────────────────────────────────────────────

export const DEFAULT_IMPORTANCE: Record<MessageType, MessageImportance> = {
  'commit.completed':       'normal',
  'decision.made':          'high',
  'gap.discovered':         'high',
  'blocker.raised':         'critical',
  'assumption.flagged':     'normal',
  'risk.surfaced':          'high',
  'convention.established': 'high',
  'review.requested':       'normal',
  'finding.classified':     'normal',
  'handoff.completed':      'high',
  'context.shared':         'low',
  'progress.updated':       'low',
};

// ── Helpers ───────────────────────────────────────────────────────────────────

export function importanceForFinding(severity: FindingSeverity): MessageImportance {
  switch (severity) {
    case 'CRITICAL': return 'critical';
    case 'HIGH':     return 'high';
    case 'MEDIUM':   return 'normal';
    case 'LOW':      return 'low';
  }
}

/**
 * Wraps an untyped backend text message as a progress.updated message.
 * v1 backend sends plain text; this forward-compat wrapper makes it renderable.
 */
export function wrapAsProgressUpdate(text: string, progress_pct = 0): Message<'progress.updated'> {
  return {
    id: `${Date.now()}-${Math.random().toString(36).slice(2, 9)}`,
    type: 'progress.updated',
    from: 'coordinator',
    to: null,
    topic: 'build',
    timestamp: new Date().toISOString(),
    importance: 'low',
    payload: { progress_pct, current_step: text },
  };
}

// ── UI styling hints (consumed by MailboxStream.svelte) ───────────────────────

export const MESSAGE_STYLE: Record<MessageType, { edge: string; icon?: string }> = {
  'commit.completed':       { edge: '#22c55e',  icon: 'git-commit' },
  'decision.made':          { edge: '#f5d440',  icon: 'book-open' },
  'gap.discovered':         { edge: '#ef4444',  icon: 'alert-triangle' },
  'blocker.raised':         { edge: '#ef4444',  icon: 'alert-octagon' },
  'assumption.flagged':     { edge: '#8a929c',  icon: 'help-circle' },
  'risk.surfaced':          { edge: '#fb923c',  icon: 'shield-alert' },
  'convention.established': { edge: '#a874ff',  icon: 'bookmark' },
  'review.requested':       { edge: '#4dffe6',  icon: 'eye' },
  'finding.classified':     { edge: '#ef4444',  icon: 'flag' },
  'handoff.completed':      { edge: '#4dffe6',  icon: 'arrow-right-circle' },
  'context.shared':         { edge: '#3e434a'  },
  'progress.updated':       { edge: '#3e434a'  },
};
