// ============================================================================
// Design tokens — LA Platform visual identity
// ============================================================================

import { type Polytope4DType } from './polytopes4d-canvas2d';

// --- Squad member names (internal canonical; vocabulary canon: preserve here) ---
export const SIBLINGS = ['soul', 'eva', 'corso', 'quantum', 'seraph', 'ayin', 'larc'] as const;
export type SiblingId = typeof SIBLINGS[number];

// --- Squad member colors (roadmap-content.html palette) ---
export const SIBLING_COLORS: Record<string, string> = {
  soul:    '#f0c040',
  eva:     '#FF6B9D',
  corso:   '#00BFFF',
  quantum: '#B44AFF',
  seraph:  '#FFEAA7',
  larc:    '#F59E0B',
  ayin:    '#FF6D00',
};

// --- Domain agent colors (public-facing dispatch surfaces) ---
export const DOMAIN_AGENT_COLORS: Record<string, string> = {
  engineer:   '#4d8eff',  // blue   — LASDLC A (architecture)
  quality:    '#a874ff',  // purple — LASDLC Q (quality)
  security:   '#ff4d4d',  // red    — LASDLC S (security risk)
  ops:        '#ff8e3c',  // orange — LASDLC O (ops + performance heat)
  researcher: '#4dffe6',  // cyan   — research/recall
  knowledge:  '#f5d440',  // yellow — knowledge/caution
  testing:    '#4dff8e',  // green  — LASDLC T (testing/go)
  squad:      '#ff7eb6',  // pink   — squad consultation
};

// --- Tier colors ---
export const TIER_COLORS: Record<number | string, string> = {
  0:    '#ff4d6a',
  1:    '#fb923c',
  2:    '#60a5fa',
  3:    '#a78bfa',
  done: '#4ade80',
};

// --- Roadmap visual constants ---
export const ROADMAP = {
  bg:          '#050508',
  glass:       'rgba(18, 18, 30, 0.55)',
  glassBorder: 'rgba(42, 42, 58, 0.6)',
  accent:      '#f0c040',
  grid:        '#f0c040',
  gridSize:    60,
  blurCard:    'blur(20px) saturate(1.2)',
  blurPanel:   'blur(32px) saturate(1.3)',
  blurHeader:  'blur(12px)',
} as const;

// --- Quality Gate names (vocabulary canon: "Pillar" → "Quality Gate") ---
export const QUALITY_GATES = ['ARCH', 'SEC', 'QUAL', 'PERF', 'TEST', 'DOC', 'OPS'] as const;
export type QualityGate = typeof QUALITY_GATES[number];

/** @deprecated Use QUALITY_GATES */
export const PILLARS = QUALITY_GATES;
/** @deprecated Use QualityGate */
export type Pillar = QualityGate;

// --- Quality Gate colors ---
export const QUALITY_GATE_COLORS: Record<string, string> = {
  ARCH: '#8B5CF6',
  SEC:  '#EF4444',
  QUAL: '#F59E0B',
  PERF: '#3B82F6',
  TEST: '#10B981',
  DOC:  '#6366F1',
  OPS:  '#EC4899',
};

/** @deprecated Use QUALITY_GATE_COLORS */
export const PILLAR_COLORS = QUALITY_GATE_COLORS;

// --- Status colors ---
export const STATUS_COLORS = {
  online:       '#22c55e',
  degraded:     '#f59e0b',
  offline:      '#ef4444',
  connected:    '#22c55e',
  reconnecting: '#f59e0b',
  passed:       '#22c55e',
  failed:       '#ef4444',
  in_progress:  '#3b82f6',
  pending:      '#6b7280',
  blocked:      '#f59e0b',
} as const;

// --- Polytope → squad member mapping ---
export const SIBLING_POLYTOPES: Record<string, { type: Polytope4DType; label: string; vertices: number; edges: number }> = {
  soul:    { type: 'icositetrachoron', label: '24-cell',           vertices: 24, edges: 96 },
  eva:     { type: 'rectified5cell',   label: 'Rectified 5-cell',  vertices: 10, edges: 30 },
  corso:   { type: 'hexadecachoron',   label: '16-cell',           vertices: 8,  edges: 24 },
  quantum: { type: 'pentachoron',      label: '5-cell',            vertices: 5,  edges: 10 },
  seraph:  { type: 'duoprism64',       label: '(6,4)-duoprism',    vertices: 24, edges: 48 },
  ayin:    { type: 'tesseract',        label: 'Tesseract',         vertices: 16, edges: 32 },
};

// --- Layout constants ---
export const LAYOUT = {
  sidebarWidth:     260,
  headerHeight:     48,
  railWidth:        240,
  panelGap:         4,
  borderRadius:     0,   // squad-dispatch: zero-radius everywhere
  terminalMinHeight: 200,
} as const;

// --- Responsive breakpoints (px) ---
export const BREAKPOINTS = {
  mobile:  768,
  desktop: 1024,
} as const;

// --- Typography ---
export const TYPO = {
  fontFamily: "'JetBrains Mono Variable', 'JetBrains Mono', monospace",
  mono:       "'JetBrains Mono Variable', 'JetBrains Mono', monospace",
  sizeXs:  '9px',
  sizeSm:  '11px',
  sizeMd:  '13px',
  sizeLg:  '16px',
  sizeXl:  '20px',
  size2xl: '24px',
} as const;

// --- Z-index layers (mirrors CSS --z-* ladder in tokens.css) ---
export const Z = {
  // Canonical ladder
  grid:       0,
  vignette:   1,
  content:   10,
  panel:     20,
  drawer:    30,
  bracket:   70,
  modalScrim: 90,
  modal:     100,
  tooltip:   110,
  overlay:   200,
  // @deprecated aliases — kept for backward compat with existing components
  base:    0,
  scope:  50,
  palette: 60,
  toast:  70,
} as const;

// --- Motion tokens (bridges CSS --la-t-* / --la-ease-*) ---
export const MOTION = {
  snap: 80,
  base: 200,
  slow: 400,
  ease: 'cubic-bezier(0.2, 0, 0.4, 1)',
} as const;

// --- Letter-spacing (bridges CSS --la-tk-*) ---
export const LETTER_SPACING = {
  loose: '0.18em',
  mid:   '0.08em',
  tight: '0.02em',
} as const;

// --- Elevation / background scale (bridges CSS --la-bg-*) ---
export const ELEVATION = {
  void:  '#08090a',
  frame: '#0c0d0e',
  elev1: '#111214',
  elev2: '#16181b',
} as const;

// --- Hairline / border scale (bridges CSS --la-hair-*) ---
export const HAIRLINE = {
  faint:  '#16181b',
  base:   '#25282d',
  strong: '#3a3f47',
} as const;

// --- Text scale (bridges CSS --la-text-*) ---
export const TEXT = {
  mute:   '#3e434a',
  dim:    '#5d646e',
  base:   '#8a929c',
  bright: '#d8dde4',
  stark:  '#f6f7f8',
} as const;

// --- MetaSkill → Squad member → Polytope mapping ---
export const META_SKILL_TO_SIBLING: Record<string, SiblingId> = {
  '/BUILD':    'corso',
  '/RESEARCH': 'quantum',
  '/SECURE':   'seraph',
  '/SQUAD':    'soul',
  '/PLAN':     'quantum',
  '/DEPLOY':   'ayin',
  '/REVIEW':   'quantum',
  '/OBSERVE':  'ayin',
  '/ONBOARD':  'soul',
  '/OPTIMIZE': 'corso',
  '/REFLECT':  'eva',
  '/ENRICH':   'eva',
};

export function getMetaSkillPolytope(metaSkill: string): Polytope4DType {
  const sib = META_SKILL_TO_SIBLING[metaSkill];
  if (!sib) return 'icositetrachoron';
  return SIBLING_POLYTOPES[sib]?.type ?? 'icositetrachoron';
}

export function getMetaSkillColor(metaSkill: string): string {
  const sib = META_SKILL_TO_SIBLING[metaSkill];
  return sib ? SIBLING_COLORS[sib] ?? '#8B5CF6' : '#8B5CF6';
}
