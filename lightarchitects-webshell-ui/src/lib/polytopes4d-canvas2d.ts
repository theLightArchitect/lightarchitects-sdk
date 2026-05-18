/**
 * 4D polytope vertex and edge definitions.
 * Vertices are normalized to the unit 4-sphere.
 * Used by PolytopeIcon and PolytopeDecor for stereographic projection rendering.
 *
 * Ported from lightarchitects-next — pure math, no framework dependency.
 */

export type Vec4 = [number, number, number, number];

export interface Polytope4D {
  vertices: Vec4[];
  edges: [number, number][];
}

export type Polytope4DType =
  | 'pentachoron'
  | 'tesseract'
  | 'hexadecachoron'
  | 'icositetrachoron'
  | 'hexacosichoron'
  | 'doubleHelix4D'
  | 'dualCompound'
  | 'rectified5cell'
  | 'duoprism55'
  | 'duoprism34'
  | 'duoprism64'
  | 'duoprism83'
  | 'duoprism53';

function normalize(verts: Vec4[]): Vec4[] {
  return verts.map(v => {
    const len = Math.sqrt(v[0] ** 2 + v[1] ** 2 + v[2] ** 2 + v[3] ** 2);
    if (len < 1e-10) return v;
    return [v[0] / len, v[1] / len, v[2] / len, v[3] / len] as Vec4;
  });
}

/** 5-cell: 5 vertices, 10 edges — simplest regular 4D polytope (complete graph K5) */
function pentachoron(): Polytope4D {
  const s5 = Math.sqrt(5);
  const vertices = normalize([
    [1, 1, 1, -1 / s5],
    [1, -1, -1, -1 / s5],
    [-1, 1, -1, -1 / s5],
    [-1, -1, 1, -1 / s5],
    [0, 0, 0, 4 / s5],
  ]);
  const edges: [number, number][] = [];
  for (let i = 0; i < 5; i++)
    for (let j = i + 1; j < 5; j++)
      edges.push([i, j]);
  return { vertices, edges };
}

/** Tesseract (8-cell): 16 vertices, 32 edges — the 4D hypercube */
function tesseract(): Polytope4D {
  const vertices: Vec4[] = [];
  for (const x of [-1, 1])
    for (const y of [-1, 1])
      for (const z of [-1, 1])
        for (const w of [-1, 1])
          vertices.push([x, y, z, w]);

  const edges: [number, number][] = [];
  for (let i = 0; i < 16; i++)
    for (let j = i + 1; j < 16; j++) {
      let diff = 0;
      for (let k = 0; k < 4; k++)
        if (vertices[i][k] !== vertices[j][k]) diff++;
      if (diff === 1) edges.push([i, j]);
    }
  return { vertices: normalize(vertices), edges };
}

/** 16-cell: 8 vertices, 24 edges — dual of the tesseract, star-like crystalline */
function hexadecachoron(): Polytope4D {
  const vertices: Vec4[] = [];
  for (let d = 0; d < 4; d++)
    for (const s of [-1, 1]) {
      const v: Vec4 = [0, 0, 0, 0];
      v[d] = s;
      vertices.push(v);
    }

  const edges: [number, number][] = [];
  for (let i = 0; i < 8; i++)
    for (let j = i + 1; j < 8; j++) {
      let dot = 0;
      for (let k = 0; k < 4; k++) dot += vertices[i][k] * vertices[j][k];
      // Connected unless antipodal (dot = -1)
      if (Math.abs(dot + 1) > 0.01) edges.push([i, j]);
    }
  return { vertices, edges };
}

/** 24-cell: 24 vertices, 96 edges — unique to 4D, self-dual, no 3D analogue */
function icositetrachoron(): Polytope4D {
  const vertices: Vec4[] = [];
  for (let i = 0; i < 4; i++)
    for (let j = i + 1; j < 4; j++)
      for (const si of [-1, 1])
        for (const sj of [-1, 1]) {
          const v: Vec4 = [0, 0, 0, 0];
          v[i] = si;
          v[j] = sj;
          vertices.push(v);
        }

  const edges: [number, number][] = [];
  for (let i = 0; i < vertices.length; i++)
    for (let j = i + 1; j < vertices.length; j++) {
      let d2 = 0;
      for (let k = 0; k < 4; k++) d2 += (vertices[i][k] - vertices[j][k]) ** 2;
      if (Math.abs(d2 - 2) < 0.01) edges.push([i, j]);
    }
  return { vertices: normalize(vertices), edges };
}

/** 600-cell: 120 vertices, 720 edges — most complex regular 4D polytope, golden ratio geometry */
function hexacosichoron(): Polytope4D {
  const phi = (1 + Math.sqrt(5)) / 2;
  const iphi = 1 / phi;
  const vertices: Vec4[] = [];

  // Group 1: permutations of (±1, 0, 0, 0) — 8 vertices
  for (let d = 0; d < 4; d++)
    for (const s of [-1, 1]) {
      const v: Vec4 = [0, 0, 0, 0];
      v[d] = s;
      vertices.push(v);
    }

  // Group 2: (±½, ±½, ±½, ±½) — 16 vertices
  for (let mask = 0; mask < 16; mask++)
    vertices.push([
      (mask & 1) ? -0.5 : 0.5,
      (mask & 2) ? -0.5 : 0.5,
      (mask & 4) ? -0.5 : 0.5,
      (mask & 8) ? -0.5 : 0.5,
    ]);

  // Group 3: even permutations of (0, ±1/(2φ), ±½, ±φ/2) — 96 vertices
  const baseMags = [0, iphi / 2, 0.5, phi / 2];
  const evenPerms = [
    [0,1,2,3], [1,2,0,3], [2,0,1,3],
    [1,3,2,0], [3,0,2,1], [0,2,3,1],
    [0,3,1,2], [2,1,3,0], [3,1,0,2],
    [2,3,0,1], [1,0,3,2], [3,2,1,0],
  ];
  for (const perm of evenPerms) {
    const mags = perm.map(i => baseMags[i]);
    const nonzero: number[] = [];
    for (let i = 0; i < 4; i++) if (mags[i] !== 0) nonzero.push(i);
    for (let sm = 0; sm < 8; sm++) {
      const v: Vec4 = [0, 0, 0, 0];
      for (let s = 0; s < nonzero.length; s++)
        v[nonzero[s]] = ((sm >> s) & 1) ? -mags[nonzero[s]] : mags[nonzero[s]];
      vertices.push(v);
    }
  }

  // Edges: vertices on unit sphere connected when dot product ≈ φ/2
  const targetDot = phi / 2;
  const edges: [number, number][] = [];
  for (let i = 0; i < vertices.length; i++)
    for (let j = i + 1; j < vertices.length; j++) {
      let dot = 0;
      for (let k = 0; k < 4; k++) dot += vertices[i][k] * vertices[j][k];
      if (Math.abs(dot - targetDot) < 0.01) edges.push([i, j]);
    }
  return { vertices, edges };
}

/** Dual compound: tesseract + 16-cell interpenetrating — 24 vertices, 56 edges */
function dualCompound(): Polytope4D {
  const t = tesseract();
  const h = hexadecachoron();
  const offset = t.vertices.length;
  const vertices: Vec4[] = [...t.vertices, ...h.vertices];
  const edges: [number, number][] = [
    ...t.edges,
    ...h.edges.map(([a, b]) => [a + offset, b + offset] as [number, number]),
  ];
  return { vertices, edges };
}

/** 4D Double Helix: two strands spiraling on the Clifford torus, connected by rungs.
 *  48 vertices, 72 edges. Morphs between interlocking rings and DNA spiral under 4D rotation. */
function doubleHelix4D(): Polytope4D {
  const N = 24;  // samples per strand
  const k = 3;   // winding number (3 helical turns)
  const r = 1 / Math.sqrt(2); // radius for unit 3-sphere

  const vertices: Vec4[] = [];

  // Strand 1: helix winding k times around the Clifford torus
  for (let i = 0; i < N; i++) {
    const t = (2 * Math.PI * i) / N;
    vertices.push([r * Math.cos(t), r * Math.sin(t), r * Math.cos(k * t), r * Math.sin(k * t)]);
  }

  // Strand 2: offset by π in the second circle (opposite side, like DNA base pair offset)
  for (let i = 0; i < N; i++) {
    const t = (2 * Math.PI * i) / N;
    vertices.push([
      r * Math.cos(t), r * Math.sin(t),
      r * Math.cos(k * t + Math.PI), r * Math.sin(k * t + Math.PI),
    ]);
  }

  const edges: [number, number][] = [];

  // Backbone edges (helical strands)
  for (let i = 0; i < N; i++) {
    edges.push([i, (i + 1) % N]);           // Strand 1
    edges.push([N + i, N + (i + 1) % N]);   // Strand 2
  }

  // Rungs connecting strands every 2nd vertex (base pairs)
  for (let i = 0; i < N; i += 2) {
    edges.push([i, N + i]);
  }

  return { vertices, edges };
}

/** Rectified 5-cell: 10 vertices, 30 edges — midpoints of the pentachoron's edges */
function rectified5cell(): Polytope4D {
  const cell = pentachoron();
  const vertices: Vec4[] = cell.edges.map(([i, j]) => {
    const a = cell.vertices[i], b = cell.vertices[j];
    return [(a[0] + b[0]) / 2, (a[1] + b[1]) / 2, (a[2] + b[2]) / 2, (a[3] + b[3]) / 2] as Vec4;
  });

  const edges: [number, number][] = [];
  for (let i = 0; i < cell.edges.length; i++)
    for (let j = i + 1; j < cell.edges.length; j++) {
      const [a, b] = cell.edges[i], [c, d] = cell.edges[j];
      if (a === c || a === d || b === c || b === d) edges.push([i, j]);
    }
  return { vertices: normalize(vertices), edges };
}

/** (p,q)-duoprism: p*q vertices, 2*p*q edges — Cartesian product of two polygons */
function duoprism(p: number, q: number): Polytope4D {
  const vertices: Vec4[] = [];
  for (let i = 0; i < p; i++)
    for (let j = 0; j < q; j++) {
      const a = (2 * Math.PI * i) / p;
      const b = (2 * Math.PI * j) / q;
      vertices.push([Math.cos(a), Math.sin(a), Math.cos(b), Math.sin(b)]);
    }

  const idx = (i: number, j: number) => i * q + j;
  const edges: [number, number][] = [];
  for (let i = 0; i < p; i++)
    for (let j = 0; j < q; j++) {
      edges.push([idx(i, j), idx((i + 1) % p, j)]);
      edges.push([idx(i, j), idx(i, (j + 1) % q)]);
    }
  return { vertices: normalize(vertices), edges };
}

const generators: Record<Polytope4DType, () => Polytope4D> = {
  pentachoron,
  tesseract,
  hexadecachoron,
  icositetrachoron,
  hexacosichoron,
  doubleHelix4D,
  dualCompound,
  rectified5cell,
  duoprism55: () => duoprism(5, 5),
  duoprism34: () => duoprism(3, 4),
  duoprism64: () => duoprism(6, 4),
  duoprism83: () => duoprism(8, 3),
  duoprism53: () => duoprism(5, 3),
};

const cache = new Map<Polytope4DType, Polytope4D>();

export function getPolytope4D(type: Polytope4DType): Polytope4D {
  let p = cache.get(type);
  if (!p) {
    p = generators[type]();
    cache.set(type, p);
  }
  return p;
}

// ── Stage-split projections (Phase 1 item 13 — Three.js polytope layer) ──────
//
// The Three.js polytope overlay in Phase 3 needs to consume 3D vertices directly
// from the 4D math rather than going through the full canvas2D draw path.
// These two functions split the pipeline so Three.js can take over from stage (b).
//
// Pipeline:
//   (a) project4DTo3D  — 4D → 3D via stereographic projection + SO(4) rotation
//   (b) project3DTo2D  — 3D → 2D via camera (used by canvas2D fallback)
//   (c) draw2D         — existing full-pipeline entry point (unchanged)

/** 4×4 identity rotation matrix (row-major). */
type Mat4x4 = [
  number, number, number, number,
  number, number, number, number,
  number, number, number, number,
  number, number, number, number,
];

function identityMat4(): Mat4x4 {
  return [1,0,0,0, 0,1,0,0, 0,0,1,0, 0,0,0,1];
}

/** Apply a 4×4 matrix to a Vec4 (row-vector × column-major matrix). */
function applyMat4(m: Mat4x4, v: Vec4): Vec4 {
  return [
    m[0]*v[0] + m[4]*v[1] + m[8] *v[2] + m[12]*v[3],
    m[1]*v[0] + m[5]*v[1] + m[9] *v[2] + m[13]*v[3],
    m[2]*v[0] + m[6]*v[1] + m[10]*v[2] + m[14]*v[3],
    m[3]*v[0] + m[7]*v[1] + m[11]*v[2] + m[15]*v[3],
  ];
}

/**
 * Project 4D vertices to 3D via stereographic projection.
 *
 * Each Vec4 `[x,y,z,w]` projects to `[x,y,z] / (1 - w)` (w-axis perspective).
 * A SO(4) rotation matrix `rot4` is applied before projection to animate the
 * 4D rotation that gives polytopes their distinctive spin.
 *
 * Returns a `Float32Array` of interleaved `[x,y,z, x,y,z, …]` triples,
 * one per input vertex — ready for Three.js `BufferAttribute`.
 */
export function project4DTo3D(
  polytope: Polytope4D,
  rot4: Mat4x4 = identityMat4(),
): Float32Array {
  const out = new Float32Array(polytope.vertices.length * 3);
  polytope.vertices.forEach((v, i) => {
    const rv = applyMat4(rot4, v);
    const denom = 1 - rv[3];
    const scale = Math.abs(denom) < 1e-6 ? 1e6 : 1 / denom;
    out[i * 3]     = rv[0] * scale;
    out[i * 3 + 1] = rv[1] * scale;
    out[i * 3 + 2] = rv[2] * scale;
  });
  return out;
}

/**
 * Project 3D vertices (from `project4DTo3D`) to 2D canvas coordinates.
 *
 * Uses a simple perspective camera at `[0, 0, cameraZ]` looking at the origin.
 * Returns a `Float32Array` of interleaved `[x, y, …]` pairs in normalised
 * device coordinates (`-1..1`), one per vertex.
 *
 * The canvas2D `draw2D` function uses the full combined pipeline internally;
 * this split is provided so Phase 3 Three.js code can reuse stage (a) alone.
 */
export function project3DTo2D(
  vertices3d: Float32Array,
  cameraZ = 3.5,
): Float32Array {
  const n = vertices3d.length / 3;
  const out = new Float32Array(n * 2);
  for (let i = 0; i < n; i++) {
    const x = vertices3d[i * 3];
    const y = vertices3d[i * 3 + 1];
    const z = vertices3d[i * 3 + 2];
    const denom = cameraZ - z;
    const scale = Math.abs(denom) < 1e-6 ? 1e6 : cameraZ / denom;
    out[i * 2]     = x * scale;
    out[i * 2 + 1] = y * scale;
  }
  return out;
}