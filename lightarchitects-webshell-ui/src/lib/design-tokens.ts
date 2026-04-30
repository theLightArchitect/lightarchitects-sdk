// ============================================================================
// Design tokens — LA Platform visual identity
// ============================================================================

import { type Polytope4DType } from './polytopes4d-canvas2d';

// --- Sibling names ---
export const SIBLINGS = ['soul', 'eva', 'corso', 'quantum', 'seraph', 'ayin', 'larc'] as const;
export type SiblingId = typeof SIBLINGS[number];

// --- Sibling colors (roadmap-content.html palette) ---
export const SIBLING_COLORS: Record<string, string> = {
  soul:    '#f0c040',  // gold
  eva:     '#FF6B9D',  // soft pink
  corso:   '#4ECDC4',  // teal
  quantum: '#96CEB4',  // sage
  seraph:  '#FFEAA7',  // cream
  larc:    '#F59E0B',  // amber
  ayin:    '#FF6D00',  // orange
};

// --- Tier colors ---
export const TIER_COLORS: Record<number | string, string> = {
  0:      '#ff4d6a',  // red — recon/hotfix
  1:      '#fb923c',  // orange — small
  2:      '#60a5fa',  // blue — medium
  3:      '#a78bfa',  // purple — large
  done:   '#4ade80',  // green — completed
};

// --- Roadmap visual constants (from roadmap-content.html) ---
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

// --- Pillar names ---
export const PILLARS = ['ARCH', 'SEC', 'QUAL', 'PERF', 'TEST', 'DOC', 'OPS'] as const;
export type Pillar = typeof PILLARS[number];

// --- Pillar colors ---
export const PILLAR_COLORS: Record<string, string> = {
  ARCH: '#8B5CF6',  // violet
  SEC:  '#EF4444',  // red
  QUAL: '#F59E0B',  // amber
  PERF: '#3B82F6',  // blue
  TEST: '#10B981',  // emerald
  DOC:  '#6366F1',  // indigo
  OPS:  '#EC4899',  // pink
};

// --- Status colors ---
export const STATUS_COLORS = {
  online:     '#22c55e',
  degraded:   '#f59e0b',
  offline:    '#ef4444',
  connected:  '#22c55e',
  reconnecting: '#f59e0b',
  passed:     '#22c55e',
  failed:     '#ef4444',
  in_progress: '#3b82f6',
  pending:    '#6b7280',
  blocked:    '#f59e0b',
} as const;

// --- Polytope → sibling mapping (from design spec) ---
export const SIBLING_POLYTOPES: Record<string, { type: Polytope4DType; label: string; vertices: number; edges: number }> = {
  soul:    { type: 'icositetrachoron', label: '24-cell', vertices: 24, edges: 96 },
  eva:     { type: 'rectified5cell',   label: 'Rectified 5-cell', vertices: 10, edges: 30 },
  corso:   { type: 'hexadecachoron',   label: '16-cell', vertices: 8, edges: 24 },
  quantum: { type: 'pentachoron',       label: '5-cell', vertices: 5, edges: 10 },
  seraph:  { type: 'duoprism64',       label: '(6,4)-duoprism', vertices: 24, edges: 48 },
  ayin:    { type: 'tesseract',         label: 'Tesseract', vertices: 16, edges: 32 },
};

// --- Layout constants ---
export const LAYOUT = {
  sidebarWidth: 260,
  headerHeight: 48,
  railWidth: 240,
  panelGap: 4,
  borderRadius: 8,
  terminalMinHeight: 200,
} as const;

// --- Responsive breakpoints (px) ---
// Aligned with Tailwind v4 defaults: tokens here exist so TypeScript / JS
// callers (matchMedia, resize observers) share one source of truth with the
// `md:` and `lg:` Tailwind utilities used in markup.
//   mobile (<768)        : single-column stack (no helix panel)
//   tablet (768..1023)   : two-row layout, helix hidden behind toggle
//   desktop (>=1024)     : side-by-side, helix panel visible (current default)
export const BREAKPOINTS = {
  mobile:  768,   // Tailwind `md`
  desktop: 1024,  // Tailwind `lg`
} as const;

// --- Typography ---
export const TYPO = {
  fontFamily: "'JetBrains Mono', monospace",
  mono: "'JetBrains Mono', monospace",
  sizeXs: '9px',
  sizeSm: '11px',
  sizeMd: '13px',
  sizeLg: '16px',
  sizeXl: '20px',
  size2xl: '24px',
} as const;

// --- Z-index layers ---
export const Z = {
  base: 0,
  panel: 10,
  overlay: 20,
  scope: 50,
  palette: 60,
  toast: 70,
} as const;

// --- MetaSkill → Sibling → Polytope mapping ---
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