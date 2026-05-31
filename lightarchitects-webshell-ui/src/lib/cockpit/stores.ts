import { writable } from 'svelte/store';

/** Domain preset keys — match DOMAIN_AGENT_COLORS keys in design-tokens.ts */
export type CockpitPreset =
  | 'engineer'
  | 'security'
  | 'ops'
  | 'quality'
  | 'knowledge'
  | 'researcher'
  | 'testing';

/** Human-readable labels for each preset (displayed in chips). */
export const PRESET_DISPLAY: Record<CockpitPreset, string> = {
  engineer:   'Engineer',
  security:   'Security',
  ops:        'Ops',
  quality:    'Quality',
  knowledge:  'Knowledge',
  researcher: 'Research',
  testing:    'Testing',
};

/** Target entity type within the LASDLC hierarchy. */
export type TargetType = 'project' | 'build' | 'phase' | 'wave' | 'file' | 'commit' | 'branch' | 'pr';

/** The currently selected target scope for the cockpit. */
export interface CockpitTarget {
  type: TargetType;
  id: string;
  label: string;
}

/** Active domain preset. Default: Engineer. */
export const selectedPreset = writable<CockpitPreset>('engineer');

/** Active target scope. Null = no target selected. */
export const selectedTarget = writable<CockpitTarget | null>(null);

/** Controls QuickPickPalette visibility. */
export const quickPickOpen = writable<boolean>(false);

// ── Wave Composer stores (cockpit-wave-composer) ──────────────────────────────

/** One agent assignment row in the wave composer. */
export interface AgentTaskRow {
  /** Domain preset for this agent. */
  preset: CockpitPreset;
  /** Skill key, e.g. `"lightarchitects:engineer"`. */
  skill: string;
  /** Operator-supplied task description, injected into the worker prompt. */
  taskDescription: string;
  /** Worktree-relative file paths this agent may write. Empty = no enforcement. */
  fileOwnership: string[];
}

/** Presets selected for the current wave (multi-select). */
export const selectedAgents = writable<Set<CockpitPreset>>(new Set());

/** Per-agent task rows — one entry per selected preset. */
export const agentTaskRows = writable<AgentTaskRow[]>([]);

/** Controls WaveComposer card expand/collapse. */
export const waveComposerOpen = writable<boolean>(false);

/** True while a wave dispatch request is in-flight. */
export const waveDispatchPending = writable<boolean>(false);

/** UUID of the last successfully dispatched wave. Null until first dispatch. */
export const lastWaveId = writable<string | null>(null);

/** Number of agents in the last successfully dispatched wave. */
export const lastWaveAgentCount = writable<number>(0);
