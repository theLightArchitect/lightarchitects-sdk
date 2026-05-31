// ============================================================================
// Design tokens — LA Platform visual identity
// ============================================================================

import { type Polytope4DType } from './polytopes4d-canvas2d';

// --- Squad member names (internal canonical; vocabulary canon: preserve here) ---
export const SIBLINGS = ['soul', 'eva', 'corso', 'quantum', 'seraph', 'ayin', 'laex'] as const;
export type SiblingId = typeof SIBLINGS[number];

// --- Squad member colors (roadmap-content.html palette) ---
export const SIBLING_COLORS: Record<string, string> = {
  soul:    '#f0c040',
  eva:     '#FF6B9D',
  corso:   '#00BFFF',
  quantum: '#B44AFF',
  seraph:  '#FFEAA7',
  laex:    '#F59E0B',
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
  // Canon keeper — a polytope and its dual together, evokes the keeper of
  // standards that bind opposing positions into one figure.
  laex:    { type: 'dualCompound',     label: 'Dual Compound',     vertices: 13, edges: 36 },
};

// --- Chat actor polytopes — for assistant / user message avatars when
// no specific sibling identity is present. Distinct from SIBLING_POLYTOPES
// so the squad set stays intact for the dispatch surfaces. ---
export const CHAT_ACTOR_POLYTOPES: Record<string, { type: Polytope4DType; color: string; label: string }> = {
  // Operator's voice — the most complex regular 4-polytope. Architect-grade
  // density, ordered into one figure.
  user:      { type: 'hexacosichoron', color: '#FFD700', label: '600-cell' },
  // Default assistant identity — two strands twisting, suggests reasoning
  // and articulation moving together.
  assistant: { type: 'doubleHelix4D',  color: '#9ed59e', label: 'Double Helix' },
};

/**
 * Resolve a polytope + color for a chat message author. If the message
 * carries a sibling identity, the sibling's polytope wins (e.g. EVA
 * speaking gets the rectified 5-cell). Otherwise falls back to the
 * actor default (assistant or user).
 */
export function getChatActorPolytope(
  role: 'user' | 'assistant' | 'system',
  sibling?: string | null,
): { type: Polytope4DType; color: string } {
  if (sibling) {
    const sib = sibling.toLowerCase();
    const sibPoly = SIBLING_POLYTOPES[sib];
    const sibColor = SIBLING_COLORS[sib];
    if (sibPoly && sibColor) {
      return { type: sibPoly.type, color: sibColor };
    }
  }
  const actor = role === 'user' ? CHAT_ACTOR_POLYTOPES.user : CHAT_ACTOR_POLYTOPES.assistant;
  return { type: actor.type, color: actor.color };
}

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

// ── GitForest visual tokens ────────────────────────────────────────────────────
// Centralised here so GitForest.svelte, overlay canvases, and Three.js layers
// all share the same colour vocabulary.  Previously inline in GitForest.svelte.

import type { BranchKind, BranchLifecycle } from './gitforest';

/**
 * Gate state → hex colour.  Drives branch stroke + Three.js tube material.
 * Matches the old `GateState` union in GitForest.svelte.
 */
export const GATE_COLORS: Record<string, number> = {
  clean:        0x22c55e,
  hitl_pending: 0xf59e0b,
  merge_ready:  0xffd700,
  failed:       0xef4444,
  writing:      0x00c8ff,
  ghost:        0x334155,
};

/**
 * Branch lifecycle → opacity factor applied on top of the base fade_level.
 * `live_active` = full opacity; `abandoned` = 30%.
 */
export const LIFECYCLE_OPACITY: Record<BranchLifecycle, number> = {
  live_active: 1.0,
  live_idle:   1.0,
  merged:      0.75,  // further modulated by fade_level
  abandoned:   0.30,
};

/**
 * Branch kind → hex accent colour used for the branch stroke.
 *
 * Main trunk colour comes from `REPO_TRUNK_COLORS`; these accents apply
 * to the 3 non-trunk levels so the hierarchy is visually legible.
 */
export const BRANCH_KIND_COLORS: Record<BranchKind, number> = {
  main:         0x38bdf8,   // sky blue — trunk (overridden by REPO_TRUNK_COLORS)
  program:      0xa78bfa,   // violet   — program stubs
  build:        0x60a5fa,   // blue     — build branches
  wave_cluster: 0x34d399,   // emerald  — wave twigs
};

/**
 * Per-repo trunk and wire edge colours.  Index matches the order repos appear
 * in the forest layout (sdk=0, soul=1, corso=2, …).
 *
 * `[trunk: number, wire: number]` — both as hex integers for Three.js + `#rrggbb`.
 */
export const REPO_TRUNK_COLORS: [trunk: number, wire: number][] = [
  [0x0ea5e9, 0x38bdf8],   // sdk   — sky blue
  [0xf5d440, 0xfde68a],   // soul  — gold
  [0x8b5cf6, 0xa78bfa],   // corso — violet
];

/**
 * Model identifier → accent colour for `model_attribution` badges.
 * Keys match the internal model IDs used by the squad.
 */
export const MODEL_COLORS: Record<string, string> = {
  'claude-opus-4-7':          '#f5d440',   // gold   — Opus
  'claude-sonnet-4-6':        '#60a5fa',   // blue   — Sonnet
  'claude-haiku-4-5':         '#34d399',   // emerald — Haiku
  'mistral-vibe':              '#ff7eb6',   // pink   — Mistral
  'ollama':                    '#fb923c',   // orange — local Ollama
  'unknown':                   '#475569',   // slate  — unattributed
};

/** Converts a hex integer colour (e.g. `0x0ea5e9`) to a CSS hex string. */
export function hexToCSS(hex: number): string {
  return `#${hex.toString(16).padStart(6, '0')}`;
}
