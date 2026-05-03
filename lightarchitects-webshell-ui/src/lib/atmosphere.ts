/**
 * atmosphere.ts — shared visual atmosphere configuration
 *
 * Single source of truth for atmosphere-layer toggles and constants. Import
 * the stores here to read/write atmosphere state; import the constants for
 * rendering parameters in ScanLines, CodeRain, AmbientParticles, etc.
 */

import { writable } from 'svelte/store';

// ── Feature toggles (persisted via settings-persistence in a future wave) ──

/**
 * Scan-line overlay toggle. Off by default — user can enable in Settings.
 * Per Locked Decision #3: NOT rendered on /helix to keep that scene light.
 */
export const scanLinesEnabled = writable<boolean>(false);

// ── Visual constants ────────────────────────────────────────────────────────

/** Animation frame cap — WebGL scenes target this; atmosphere layers match. */
export const FPS_CAP = 60;

/**
 * Scan-line parameters. The scan-line overlay is a CSS repeating-gradient:
 * alternating fully-transparent and semi-opaque rows at `SCAN_PITCH` px spacing.
 */
export const SCAN_PITCH = 3;    // px between scan lines (3 = one line per 3px row)
export const SCAN_OPACITY = 0.06; // opacity of the dark scan rows (barely perceptible)

/** Ambient particle density multiplier. 1.0 = default (600 fine + 30 bokeh). */
export const PARTICLE_DENSITY = 1.0;

// ── Source color palette (design-token aligned) ─────────────────────────────
// These mirror SIBLING_COLORS from design-tokens.ts but are lower-case keyed
// for case-insensitive source matching in EventStream rows.

export const ATMOSPHERE_SOURCE_COLORS: Record<string, string> = {
  // Squad siblings
  soul:       '#f0c040',
  eva:        '#FF6B9D',
  corso:      '#00BFFF',
  quantum:    '#B44AFF',
  seraph:     '#FFEAA7',
  larc:       '#F59E0B',
  ayin:       '#FF6D00',
  // Domain agents (public-facing)
  engineer:   '#4d8eff',
  quality:    '#a874ff',
  security:   '#ff4d4d',
  ops:        '#ff8e3c',
  researcher: '#4dffe6',
  knowledge:  '#f5d440',
  testing:    '#4dff8e',
  squad:      '#ff7eb6',
  // System sources
  supervisor: '#f59e0b',
  pillar:     '#6366f1',
  copilot:    '#FF6B9D',
  system:     '#475569',
};

/** Resolve a source label to its display color, case-insensitive. */
export function sourceColor(source: string): string {
  return ATMOSPHERE_SOURCE_COLORS[source.toLowerCase()] ?? '#64748b';
}

// ── Severity colors ─────────────────────────────────────────────────────────

export type Severity = 'info' | 'ok' | 'warn' | 'err';

export const SEVERITY_COLORS: Record<Severity, string> = {
  info: '#94a3b8',
  ok:   '#22c55e',
  warn: '#f59e0b',
  err:  '#ef4444',
};
