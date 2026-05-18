/**
 * WavePipelineView — shared prop contract between gitforest-live-ops and ironclaw-spine.
 *
 * This file is the **single source of truth** for the `WavePipelineView` component
 * interface.  Both builds import from here; any prop change must land here first,
 * then flow downstream to the component implementations.
 *
 * Origin: Phase 1 item 8 (CORSO CO-R2-3 + QUANTUM QU-R2-4).
 * Cross-build pact: gitforest-live-ops owns this file; ironclaw-spine reads it
 * via the shared worktree-read ownership model agreed 2026-05-18.
 */

// ── Phase / Wave / Task schema (matches webshell manifest.yaml) ──────────────

/** LASDLC quality gate identifiers. */
export type GateLabel = 'A' | 'S' | 'Q' | 'C' | 'O' | 'P' | 'K' | 'D' | 'T' | 'R';

/** Gate evaluation result for a single dimension. */
export interface GateResult {
  label: GateLabel;
  passed: boolean;
  /** 0..1 confidence score from the sibling evaluator. */
  score: number | null;
  /** Blocking issue description; `null` if passed. */
  blocker: string | null;
}

/** LASDLC gate verdict for a phase or wave boundary. */
export interface GateVerdictSummary {
  overall: 'pass' | 'hitl' | 'fail';
  results: GateResult[];
  /** ISO-8601 timestamp when the gate ran. */
  evaluated_at: string;
}

/** A single task within a wave. */
export interface WaveTask {
  id: string;
  title: string;
  status: 'pending' | 'in_progress' | 'completed' | 'failed';
  /** Agent key that owns this task. */
  agent_key: string | null;
  /** ISO-8601 start time; `null` if not yet started. */
  started_at: string | null;
  /** ISO-8601 completion time; `null` if not yet done. */
  completed_at: string | null;
}

/** A wave within a LASDLC phase. */
export interface Wave {
  id: string;
  /** Display label, e.g. `"Wave 1"`. */
  label: string;
  status: 'pending' | 'in_progress' | 'completed' | 'failed';
  tasks: WaveTask[];
  gate_verdict: GateVerdictSummary | null;
}

/** A LASDLC phase as rendered by WavePipelineView. */
export interface Phase {
  id: string;
  /** Display label, e.g. `"Phase 1 — Foundation"`. */
  label: string;
  status: 'pending' | 'in_progress' | 'completed' | 'failed';
  waves: Wave[];
  gate_verdict: GateVerdictSummary | null;
}

// ── Component prop contract ───────────────────────────────────────────────────

/**
 * Props for `WavePipelineView`.
 *
 * **`mode`** determines the visual layout:
 * - `'full'` — ironclaw-spine view-mode-6: full-width timeline with all waves expanded
 * - `'split'` — gitforest-live-ops L2 drill: half-width panel alongside the forest canvas
 */
export interface WavePipelineViewProps {
  mode: 'full' | 'split';
  phases: Phase[];
  /** When set, the matching wave is highlighted and scrolled into view. */
  selectedWaveId?: string;
  /** Bubbles to parent for L3 task-log navigation. */
  onTaskClick?: (taskId: string) => void;
  /** Bubbles to parent for gate detail panel. */
  onGateClick?: (phaseId: string, waveId: string | null) => void;
}

// ── Context-tier schema (Phase 1 item 4) ─────────────────────────────────────

/**
 * Task context-tier classification for `manifest.yaml`.
 *
 * Tiers reflect the token-budget tier of a task:
 * - `T1` (◈) — full context: entire codebase, all prior conversation
 * - `T2` (◇) — scoped context: file-set + immediate ancestors
 * - `T3` (○) — minimal context: single file + direct dependencies
 */
export interface TaskContextTier {
  tier: 'T1' | 'T2' | 'T3';
  label: string;
  token_count: number;
  /** Indicator glyph shown in the WavePipelineView task row. */
  icon: '◈' | '◇' | '○';
}

export const CONTEXT_TIER_DEFAULTS: Record<'T1' | 'T2' | 'T3', TaskContextTier> = {
  T1: { tier: 'T1', label: 'Full context',    token_count: 200_000, icon: '◈' },
  T2: { tier: 'T2', label: 'Scoped context',  token_count: 50_000,  icon: '◇' },
  T3: { tier: 'T3', label: 'Minimal context', token_count: 8_000,   icon: '○' },
};
