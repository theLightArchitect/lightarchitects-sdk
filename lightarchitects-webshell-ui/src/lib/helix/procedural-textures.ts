// Procedural Canvas2D texture generators for Helix3D polytope faces.
// Each mode returns a flat RGBA Uint8Array of size*size*4 bytes.
// Designed to feed into THREE.DataTexture or Canvas ImageData directly.

export type TextureMode = 'noise' | 'plasma' | 'circuit' | 'flow';

export const TEXTURE_MODES: TextureMode[] = ['noise', 'plasma', 'circuit', 'flow'];

export const TEXTURE_LABELS: Record<TextureMode, string> = {
  noise:   'Raw memories',
  plasma:  'Patterns',
  circuit: 'Decisions',
  flow:    'Flows',
};

// Deterministic pseudo-random in [0,1) from an integer seed.
function rand(s: number): number {
  let x = Math.sin(s) * 43758.5453123;
  return x - Math.floor(x);
}

// Smooth noise via bilinear interpolation over a random lattice.
function smoothNoise(x: number, y: number): number {
  const ix = Math.floor(x), iy = Math.floor(y);
  const fx = x - ix, fy = y - iy;
  const ux = fx * fx * (3 - 2 * fx);
  const uy = fy * fy * (3 - 2 * fy);
  const a = rand(ix     + iy     * 317);
  const b = rand(ix + 1 + iy     * 317);
  const c = rand(ix     + (iy+1) * 317);
  const d = rand(ix + 1 + (iy+1) * 317);
  return a + (b-a)*ux + (c-a)*uy + (d-b-c+a)*ux*uy;
}

// Fractional Brownian motion (4 octaves).
function fbm(x: number, y: number): number {
  let v = 0, amp = 0.5, freq = 1;
  for (let i = 0; i < 4; i++) {
    v += smoothNoise(x * freq, y * freq) * amp;
    amp *= 0.5; freq *= 2;
  }
  return v;
}

function noiseGenerator(size: number): Uint8Array {
  const buf = new Uint8Array(size * size * 4);
  for (let y = 0; y < size; y++) {
    for (let x = 0; x < size; x++) {
      const v = fbm(x / size * 4, y / size * 4);
      const i = (y * size + x) * 4;
      // Dark blue-green grain
      buf[i]   = Math.round(v * 30);
      buf[i+1] = Math.round(v * 80);
      buf[i+2] = Math.round(v * 120 + 30);
      buf[i+3] = Math.round(v * 200 + 55);
    }
  }
  return buf;
}

function plasmaGenerator(size: number): Uint8Array {
  const buf = new Uint8Array(size * size * 4);
  for (let y = 0; y < size; y++) {
    for (let x = 0; x < size; x++) {
      const nx = x / size, ny = y / size;
      // Sinusoidal interference (static phase — no time dependency for static textures)
      const v = 0.5 + 0.5 * Math.sin(nx * 10 + Math.sin(ny * 8) * 2)
                    * Math.cos(ny * 9 - Math.cos(nx * 7) * 1.5);
      const i = (y * size + x) * 4;
      // Cyan-magenta gradient
      buf[i]   = Math.round(v * 180);
      buf[i+1] = Math.round((1 - v) * 100 + v * 40);
      buf[i+2] = Math.round((1 - v) * 200 + v * 80);
      buf[i+3] = Math.round(v * 180 + 55);
    }
  }
  return buf;
}

function circuitGenerator(size: number): Uint8Array {
  const buf = new Uint8Array(size * size * 4);
  const scale = Math.round(size / 8);
  for (let y = 0; y < size; y++) {
    for (let x = 0; x < size; x++) {
      const cx = x % scale, cy = y % scale;
      const onH = cy === 0 && rand(Math.floor(y / scale) * 53 + Math.floor(x / scale)) > 0.4;
      const onV = cx === 0 && rand(Math.floor(x / scale) * 71 + Math.floor(y / scale)) > 0.4;
      const bright = onH || onV ? 1 : 0;
      const i = (y * size + x) * 4;
      // Green-on-dark circuit trace
      buf[i]   = bright ? 20 : 5;
      buf[i+1] = bright ? 255 : 15;
      buf[i+2] = bright ? 80 : 20;
      buf[i+3] = bright ? 230 : 80;
    }
  }
  return buf;
}

function flowGenerator(size: number): Uint8Array {
  const buf = new Uint8Array(size * size * 4);
  for (let y = 0; y < size; y++) {
    for (let x = 0; x < size; x++) {
      const nx = x / size, ny = y / size;
      // Diagonal flow lines via modulo of fbm-distorted coordinates
      const distX = nx + 0.3 * fbm(nx * 3 + 1, ny * 3);
      const distY = ny + 0.3 * fbm(nx * 3,     ny * 3 + 1);
      const v = 0.5 + 0.5 * Math.sin((distX + distY) * Math.PI * 8);
      const i = (y * size + x) * 4;
      // Gold-on-dark flow streaks
      buf[i]   = Math.round(v * 200 + 30);
      buf[i+1] = Math.round(v * 140 + 20);
      buf[i+2] = Math.round(v * 20);
      buf[i+3] = Math.round(v * 180 + 55);
    }
  }
  return buf;
}

/**
 * Generate a flat RGBA Uint8Array of `size × size` pixels for the given mode.
 * Thread-safe (pure function, no globals). Suitable for THREE.DataTexture.
 */
export function generateTexture(mode: TextureMode, size = 64): Uint8Array {
  switch (mode) {
    case 'noise':   return noiseGenerator(size);
    case 'plasma':  return plasmaGenerator(size);
    case 'circuit': return circuitGenerator(size);
    case 'flow':    return flowGenerator(size);
  }
}
