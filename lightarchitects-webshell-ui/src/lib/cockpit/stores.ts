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
