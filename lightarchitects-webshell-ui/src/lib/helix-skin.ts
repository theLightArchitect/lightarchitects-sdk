/**
 * Helix Skin — dynamic theme system for the 3D helix visualization.
 *
 * Sibling colors are a `Record<string, string>` (not a fixed 6-key object)
 * so the schema supports custom siblings registered by the user. The editor
 * reads the active sibling list from the SOUL vault health response and
 * renders a color picker for each.
 *
 * Built-in presets provide defaults for the canonical 6 siblings; any
 * sibling not in the skin's color map falls back to a deterministic hash
 * color derived from the sibling name.
 */

export interface HelixSkin {
  // ── Metadata ──────────────────────────────────────────────────────────────
  id: string;
  name: string;
  author: string;
  version: 1;
  createdAt: string;

  // ── 1. Sibling colors (dynamic — any number of siblings) ──────────────────
  /** Map of sibling ID → hex color string. Unknown siblings get a hash-derived color. */
  colors: Record<string, string>;

  // ── 2. Glow parameters ────────────────────────────────────────────────────
  glow: {
    bloomStrength: number;
    bloomRadius: number;
    bloomThreshold: number;
    dustOpacity: number;
    bokehOpacity: number;
    bokehSize: number;
    strandOpacity: number;
    nodeGlow: number;
    fogDensity: number;
  };

  // ── 3. Background & atmosphere ────────────────────────────────────────────
  atmosphere: {
    backgroundColor: string;
    ambientLightColor: string;
    ambientLightIntensity: number;
    dustPalette: string[];
    bokehPalette: string[];
  };

  // ── 4. Rail geometry ──────────────────────────────────────────────────────
  rails: {
    railOpacity: number;
    railColor: string;
    crossRungOpacity: number;
    crossRungColor: string;
    strandBrightness: number;
    nodeSizeScale: number;
    haloOpacity: number;
  };
}

/** Canonical sibling defaults — used when a skin doesn't specify a color for a sibling. */
const CANONICAL_COLORS: Record<string, string> = {
  eva: '#FF1493',
  corso: '#00BFFF',
  quantum: '#B44AFF',
  seraph: '#FF0040',
  larc: '#F59E0B',
  ayin: '#FF6D00',
};

/** Default color for unknown/new siblings — neutral light grey.
 *  Clearly signals "unconfigured" until the user picks a color via the skin editor. */
export const NEW_SIBLING_COLOR = '#C0C0C0';

/** Fallback color for unknown siblings. Returns light grey (not a random hue). */
export function hashColor(_name: string): string {
  return NEW_SIBLING_COLOR;
}

/** Resolve a sibling's color from a skin, with canonical + hash fallbacks. */
export function resolveSiblingColor(skin: HelixSkin, siblingId: string): string {
  return skin.colors[siblingId]
    ?? CANONICAL_COLORS[siblingId]
    ?? hashColor(siblingId);
}

/** Convert hex string to 0xRRGGBB number for Three.js. */
export function hexToNum(hex: string): number {
  if (hex.startsWith('#')) return parseInt(hex.slice(1), 16);
  if (hex.startsWith('hsl')) {
    // Parse HSL and convert — simple approximation for Three.js
    const match = hex.match(/hsl\((\d+),\s*(\d+)%,\s*(\d+)%\)/);
    if (match) {
      const [, h, s, l] = match.map(Number);
      return hslToHex(h, s / 100, l / 100);
    }
  }
  return parseInt(hex, 16);
}

function hslToHex(h: number, s: number, l: number): number {
  const a = s * Math.min(l, 1 - l);
  const f = (n: number) => {
    const k = (n + h / 30) % 12;
    const color = l - a * Math.max(Math.min(k - 3, 9 - k, 1), -1);
    return Math.round(255 * color);
  };
  return (f(0) << 16) | (f(8) << 8) | f(4);
}

// ── Default skin (matches current webshell exactly) ─────────────────────────

export const DEFAULT_SKIN: HelixSkin = {
  id: 'default',
  name: 'Default',
  author: 'Light Architects',
  version: 1,
  createdAt: '2025-09-30T00:00:00Z',
  colors: { ...CANONICAL_COLORS },
  glow: {
    bloomStrength: 1.1,
    bloomRadius: 0.6,
    bloomThreshold: 0.25,
    dustOpacity: 0.25,
    bokehOpacity: 0.05,
    bokehSize: 0.12,
    strandOpacity: 0.8,
    nodeGlow: 0.95,
    fogDensity: 0.06,
  },
  atmosphere: {
    backgroundColor: '#03030a',
    ambientLightColor: '#ffffff',
    ambientLightIntensity: 0.15,
    dustPalette: ['#FF1493', '#00BFFF', '#B44AFF', '#FFD700', '#FF6D00', '#ffffff'],
    bokehPalette: ['#FF1493', '#00BFFF', '#B44AFF'],
  },
  rails: {
    railOpacity: 0.6,
    railColor: '#808080',
    crossRungOpacity: 0.05,
    crossRungColor: '#FFD700',
    strandBrightness: 1.0,
    nodeSizeScale: 1.0,
    haloOpacity: 0.35,
  },
};

// ── Built-in preset skins ───────────────────────────────────────────────────

export const PRESET_SKINS: HelixSkin[] = [
  DEFAULT_SKIN,
  {
    ...DEFAULT_SKIN,
    id: 'midnight',
    name: 'Midnight',
    colors: { eva: '#4A90D9', corso: '#2E86C1', quantum: '#5B2C6F', seraph: '#1A5276', larc: '#2E4053', ayin: '#1B4F72' },
    glow: { ...DEFAULT_SKIN.glow, bloomStrength: 0.7, bloomRadius: 0.4, dustOpacity: 0.15, fogDensity: 0.08 },
    atmosphere: { ...DEFAULT_SKIN.atmosphere, backgroundColor: '#020208', dustPalette: ['#4A90D9', '#2E86C1', '#5B2C6F', '#85929E', '#AEB6BF', '#ffffff'], bokehPalette: ['#4A90D9', '#85929E', '#AEB6BF'] },
    rails: { ...DEFAULT_SKIN.rails, crossRungColor: '#2E86C1', strandBrightness: 0.8 },
  },
  {
    ...DEFAULT_SKIN,
    id: 'ember',
    name: 'Ember',
    colors: { eva: '#FF4500', corso: '#FF6347', quantum: '#FF8C00', seraph: '#DC143C', larc: '#FFD700', ayin: '#FF7F50' },
    glow: { ...DEFAULT_SKIN.glow, bloomStrength: 1.6, bloomRadius: 0.8, dustOpacity: 0.3, bokehOpacity: 0.08, fogDensity: 0.04 },
    atmosphere: { ...DEFAULT_SKIN.atmosphere, backgroundColor: '#0a0502', ambientLightColor: '#ff8844', ambientLightIntensity: 0.1, dustPalette: ['#FF4500', '#FF6347', '#FF8C00', '#FFD700', '#FF7F50', '#FFDAB9'], bokehPalette: ['#FF4500', '#FFD700', '#FF8C00'] },
    rails: { ...DEFAULT_SKIN.rails, crossRungColor: '#FF8C00', strandBrightness: 1.3 },
  },
  {
    ...DEFAULT_SKIN,
    id: 'arctic',
    name: 'Arctic',
    colors: { eva: '#E0F7FA', corso: '#80DEEA', quantum: '#4DD0E1', seraph: '#00BCD4', larc: '#B2EBF2', ayin: '#84FFFF' },
    glow: { ...DEFAULT_SKIN.glow, bloomStrength: 0.9, bloomRadius: 0.3, bloomThreshold: 0.3, dustOpacity: 0.1, bokehOpacity: 0.03, fogDensity: 0.0 },
    atmosphere: { ...DEFAULT_SKIN.atmosphere, backgroundColor: '#040810', dustPalette: ['#E0F7FA', '#80DEEA', '#4DD0E1', '#B2EBF2', '#84FFFF', '#ffffff'], bokehPalette: ['#E0F7FA', '#84FFFF', '#ffffff'] },
    rails: { ...DEFAULT_SKIN.rails, railColor: '#4DD0E1', crossRungColor: '#80DEEA', strandBrightness: 0.9 },
  },
  {
    ...DEFAULT_SKIN,
    id: 'neon',
    name: 'Neon',
    colors: { eva: '#FF00FF', corso: '#00FF00', quantum: '#FFFF00', seraph: '#FF0000', larc: '#00FFFF', ayin: '#FF8800' },
    glow: { ...DEFAULT_SKIN.glow, bloomStrength: 2.2, bloomRadius: 1.0, bloomThreshold: 0.15, dustOpacity: 0.35, bokehOpacity: 0.1, bokehSize: 0.15, nodeGlow: 1.0, fogDensity: 0.03 },
    atmosphere: { ...DEFAULT_SKIN.atmosphere, backgroundColor: '#000000', dustPalette: ['#FF00FF', '#00FF00', '#FFFF00', '#FF0000', '#00FFFF', '#FF8800'], bokehPalette: ['#FF00FF', '#00FF00', '#00FFFF'] },
    rails: { ...DEFAULT_SKIN.rails, crossRungColor: '#FFFF00', strandBrightness: 1.5, nodeSizeScale: 1.3 },
  },
];

// ── Skin export/import ──────────────────────────────────────────────────────

export function exportSkin(skin: HelixSkin): string {
  return JSON.stringify(skin, null, 2);
}

export function importSkin(json: string): HelixSkin | null {
  try {
    const obj = JSON.parse(json);
    if (typeof obj !== 'object' || obj.version !== 1) return null;
    if (typeof obj.colors !== 'object') return null;
    if (typeof obj.glow !== 'object') return null;
    // Merge with defaults to fill missing fields
    return {
      ...DEFAULT_SKIN,
      ...obj,
      colors: { ...DEFAULT_SKIN.colors, ...obj.colors },
      glow: { ...DEFAULT_SKIN.glow, ...obj.glow },
      atmosphere: { ...DEFAULT_SKIN.atmosphere, ...obj.atmosphere },
      rails: { ...DEFAULT_SKIN.rails, ...obj.rails },
    };
  } catch {
    return null;
  }
}
