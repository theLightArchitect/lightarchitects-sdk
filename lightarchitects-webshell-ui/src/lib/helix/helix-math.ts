/**
 * Helix position math — extracted from Hero3D.tsx.
 * Pure functions: take numbers, return THREE.Vector3 positions.
 * Shared by Hero3D (rendering) and helix-polytopes (polytope placement).
 */
import * as THREE from 'three';

// --- Constants ---
export const tMin = -35;
export const tMax = 15;

export interface HelixEntity {
  id: string;
  color: number;
  rail: number;
  age: number;
  entries: number;
  strands: number;
}

/** Canonical sibling metadata — colors and rail assignments never change. */
const SIBLING_META: Record<string, { color: number; rail: number }> = {
  eva:     { color: 0xFF1493, rail: 0 },
  corso:   { color: 0x00BFFF, rail: 0 },
  quantum: { color: 0xB44AFF, rail: 0 },
  seraph:  { color: 0xFF0040, rail: 1 },
  larc:    { color: 0xF59E0B, rail: 1 },
  ayin:    { color: 0xFF6D00, rail: 1 },
};

/** Canonical sibling ordering — determines entity index in the helix. */
const SIBLING_ORDER = ['eva', 'corso', 'quantum', 'seraph', 'larc', 'ayin'];

/** SOUL Day 0: 2025-09-30. Used to compute sibling age in days. */
const DAY_0 = new Date('2025-09-30T00:00:00Z').getTime();

/** Derive strand count from entry count — ceil(log2(n+1)) clamped [3, 8]. */
function deriveStrands(entries: number): number {
  return Math.max(3, Math.min(8, Math.ceil(Math.log2(entries + 1))));
}

/**
 * Build HelixEntity[] from SOUL vault health data.
 * Falls back to hardcoded defaults if counts are empty.
 */
export function buildEntities(vaultCounts: Record<string, number> | null): HelixEntity[] {
  const now = Date.now();
  const ageDays = Math.floor((now - DAY_0) / (1000 * 60 * 60 * 24));

  return SIBLING_ORDER.map(id => {
    const meta = SIBLING_META[id];
    const entries = vaultCounts?.[id] ?? DEFAULT_ENTRIES[id] ?? 0;
    return {
      id,
      color: meta.color,
      rail: meta.rail,
      age: ageDays,
      entries,
      strands: deriveStrands(entries),
    };
  });
}

/** Hardcoded fallback counts — used when the webshell backend is unavailable. */
const DEFAULT_ENTRIES: Record<string, number> = {
  eva: 124, corso: 88, quantum: 48, seraph: 16, larc: 57, ayin: 2,
};

/** Default entities — used at import time before vault data arrives. */
export let entities: HelixEntity[] = buildEntities(null);

/** Replace the live entities array. Called by the store when vault data loads. */
export function setEntities(data: HelixEntity[]): void {
  entities = data;
}

export const R_bundle = 1.05;
export const w_twist = 0.76;
export const R_micro = 0.3;
export const w_micro = 1.5;
export const R_nano = 0.08;
export const w_nano = 4.0;
export const R_pico = 0.025;
export const w_pico = 20.0;

// --- Position Functions ---

export function getFade(y: number): number {
  const fadeDist = 4.5;
  if (y > tMax - fadeDist) return Math.max(0, (tMax - y) / fadeDist);
  if (y < tMin + fadeDist) return Math.max(0, (y - tMin) / fadeDist);
  return 1.0;
}

export function getPrimaryFrame(railIdx: number, y: number) {
  const theta = y * w_twist + (railIdx === 0 ? 0 : Math.PI);
  const C = new THREE.Vector3(R_bundle * Math.cos(theta), y, R_bundle * Math.sin(theta));
  const N = new THREE.Vector3(-Math.cos(theta), 0, -Math.sin(theta)).normalize();
  const dx = -R_bundle * w_twist * Math.sin(theta);
  const dz = R_bundle * w_twist * Math.cos(theta);
  const T = new THREE.Vector3(dx, 1, dz).normalize();
  const B = new THREE.Vector3().crossVectors(T, N).normalize();
  return { C, N, B };
}

export function getEntityCenter(entityIdx: number, y: number) {
  const entity = entities[entityIdx];
  const { C, N, B } = getPrimaryFrame(entity.rail, y);
  const phase = (entityIdx % 3) * (Math.PI * 2 / 3);
  const phi = y * w_micro + phase;
  const ageFactor = Math.min(entity.age / 365, 1.0);
  const r_ent = R_micro * (0.7 + 0.3 * ageFactor);
  const E = new THREE.Vector3().copy(C)
    .addScaledVector(N, r_ent * Math.cos(phi))
    .addScaledVector(B, r_ent * Math.sin(phi));
  return { E, N, B };
}

export function getMiniAnchorFrame(entityIdx: number, mIdx: number, y: number) {
  const { E, N, B } = getEntityCenter(entityIdx, y);
  const phase = (mIdx === 0 ? 0 : Math.PI);
  const alpha = y * w_nano + phase;
  const M = new THREE.Vector3().copy(E)
    .addScaledVector(N, R_nano * Math.cos(alpha))
    .addScaledVector(B, R_nano * Math.sin(alpha));
  return { M, N, B };
}

export function getSubStrandPos(
  entityIdx: number, mIdx: number,
  strandIdx: number, strandCount: number, y: number
): THREE.Vector3 {
  const { M, N, B } = getMiniAnchorFrame(entityIdx, mIdx, y);
  const phase = strandIdx * (Math.PI * 2 / strandCount);
  const beta = y * w_pico + phase;
  return new THREE.Vector3().copy(M)
    .addScaledVector(N, R_pico * Math.cos(beta))
    .addScaledVector(B, R_pico * Math.sin(beta));
}

/** SOUL position: midpoint of the two primary rails at Y */
export function getSoulCenter(y: number): THREE.Vector3 {
  const c0 = getPrimaryFrame(0, y).C;
  const c1 = getPrimaryFrame(1, y).C;
  return new THREE.Vector3().addVectors(c0, c1).multiplyScalar(0.5);
}

export function seededRandom(seed: number) {
  let s = seed;
  return () => { s = (s * 16807) % 2147483647; return (s - 1) / 2147483646; };
}
