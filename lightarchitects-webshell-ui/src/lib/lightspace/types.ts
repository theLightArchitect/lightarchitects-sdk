// Lightspace workspace type definitions.
// Shared across the Lightspace screen, subcomponents, and the backend SSE contract.

export type CardKind =
  | 'monitor'    // Tier 1 — compact KPI grid
  | 'instrument' // Tier 1 — pub/sub topology
  | 'bash'       // Tier 1 — shell output
  | 'agentspawn' // Tier 2 — agent type/status/progress
  | 'trace'      // Tier 2 — decision feed / ReAct steps
  | 'thinking'   // Tier 2 — collapsible reasoning block
  | 'toolcall'   // Tier 2 — tool invocation + result
  | 'artifact'   // Tier 3 — graduated files / plan summary
  | 'research'   // Tier 3 — context7 / arXiv / helix citations
  | 'diff'       // Tier 3 — unified diff view
  | 'archgallery'// Tier 3 — diagram thumbnail grid
  | 'branchlane';// Tier 4 — parallel agent exploration lane

export type ViewPreset = 'all' | 'agents' | 'research' | 'diffs';

export type LasdlcPhaseId =
  | 'phase-0-discover'
  | 'phase-1-plan'
  | 'phase-2-design'
  | 'phase-3-build'
  | 'phase-4-verify'
  | 'phase-5-deploy'
  | 'phase-6-enrich';

export type MatPhaseId =
  | 'begin'
  | 'rail_collapsed'
  | 'grid_revealed'
  | 'drawer_revealed'
  | 'cards_streaming'
  | 'complete';

export interface CardProv {
  agent: string;
  src?: string;
  spanId?: string;
}

export interface CardConf {
  value: number; // 0.0 – 1.0
  tier: 'VERIFIED' | 'MULTI_SOURCE' | 'SINGLE_SOURCE' | 'UNVERIFIED';
}

export interface Card {
  id: string;
  kind: CardKind;
  title: string;
  body: string;           // rendered HTML — use {@html body} in templates
  tier?: number;          // override tier bucketing (1–4)
  span?: string;          // override column span (span-3..span-12)
  prov?: CardProv;
  conf?: CardConf;
  contradicts?: boolean;
  pinned?: boolean;       // never auto-evict
}

export interface ConvMsg {
  who: string;            // 'operator' | 'copilot' | sibling id
  text: string;
  time?: string;
  isTool?: boolean;
  agentClass?: string;    // overrides CSS sibling colour class
  cardLink?: string;      // card id to highlight on hover
  cardLinkLabel?: string;
}

export interface LightspaceFile {
  id: string;
  name: string;
  mime: string;           // 'md' | 'rs' | 'ts' | 'svg' | 'yaml' | 'txt'
  meta: string;           // e.g. "copilot · LASDLC LARGE plan draft"
  prov: CardProv;
  path?: string;
}

export interface RecentSession {
  id: string;             // short git sha
  summary: string;
  ago: string;
}

export interface BranchLane {
  ag: string;             // agent id
  state: 'pending' | 'running' | 'committed' | 'rolled_back';
  task: string;
  prog: number;           // 0–100
  pulse?: boolean;
}

export interface LasdlcPhase {
  id: LasdlcPhaseId;
  name: string;
  gates: string[];
}

export const LASDLC_PHASES: LasdlcPhase[] = [
  { id: 'phase-0-discover', name: 'Discover', gates: ['R'] },
  { id: 'phase-1-plan',     name: 'Plan',     gates: ['A', 'S', 'C'] },
  { id: 'phase-2-design',   name: 'Design',   gates: ['A', 'Q', 'D'] },
  { id: 'phase-3-build',    name: 'Build',    gates: ['A', 'S', 'Q', 'T'] },
  { id: 'phase-4-verify',   name: 'Verify',   gates: ['T', 'Q', 'S'] },
  { id: 'phase-5-deploy',   name: 'Deploy',   gates: ['O', 'P'] },
  { id: 'phase-6-enrich',   name: 'Enrich',   gates: ['K', 'D'] },
];

/** Default column span per card kind. Override via `Card.span`. */
export const KIND_SPAN: Record<CardKind, string> = {
  monitor:    'span-4',
  instrument: 'span-4',
  bash:       'span-4',
  agentspawn: 'span-4',
  trace:      'span-6',
  thinking:   'span-6',
  toolcall:   'span-6',
  artifact:   'span-6',
  research:   'span-6',
  diff:       'span-6',
  archgallery:'span-6',
  branchlane: 'span-12',
};

/** Default tier per card kind. Override via `Card.tier`. */
export const KIND_TO_TIER: Record<CardKind, number> = {
  monitor:    1,
  instrument: 1,
  bash:       1,
  agentspawn: 2,
  trace:      2,
  thinking:   2,
  toolcall:   2,
  artifact:   3,
  research:   3,
  diff:       3,
  archgallery:3,
  branchlane: 4,
};

/** CSS custom property value per card kind for `--kind-color`. */
export const KIND_COLOR: Record<CardKind, string> = {
  monitor:    'var(--la-info)',
  instrument: 'var(--la-warn)',
  bash:       'var(--la-info)',
  agentspawn: 'var(--la-acc)',
  trace:      'var(--la-ok)',
  thinking:   'var(--la-acc2)',
  toolcall:   'var(--la-text-dim)',
  artifact:   'var(--la-acc)',
  research:   'var(--la-acc2)',
  diff:       'var(--la-warn)',
  archgallery:'var(--la-acc2)',
  branchlane: 'var(--la-acc3)',
};

/** Human-readable kind label shown in the card header. Fallback: kind.toUpperCase(). */
export const KIND_DISPLAY: Partial<Record<CardKind, string>> = {
  monitor:    'STATUS',
  instrument: 'METRICS',
  trace:      'ACTIVITY',
  archgallery:'DIAGRAMS',
  agentspawn: 'AGENT',
  branchlane: 'PHASES',
};

/** Cards visible under each view preset (null = no filter). */
export const PRESET_KINDS: Record<ViewPreset, Set<CardKind> | null> = {
  all:      null,
  agents:   new Set(['agentspawn', 'monitor', 'trace', 'instrument']),
  research: new Set(['research', 'archgallery', 'artifact']),
  diffs:    new Set(['diff', 'trace']),
};

export const TIER_LABEL: Record<number, string> = {
  1: 'Glance',
  2: 'Stream',
  3: 'Focus',
  4: 'Lane',
};
