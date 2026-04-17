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

export const entities: HelixEntity[] = [
  // Rail 0: EVA, CORSO (polytope), QUANTUM (strand only)
  { id: "eva",     color: 0xFF1493, rail: 0, age: 171, entries: 124, strands: 6 },
  { id: "corso",   color: 0x00BFFF, rail: 0, age: 43,  entries: 88,  strands: 6 },
  { id: "quantum", color: 0xB44AFF, rail: 0, age: 172, entries: 48,  strands: 5 },
  // Rail 1: SERAPH (polytope), L-ARC (polytope), AYIN (polytope)
  { id: "seraph",  color: 0xFF0040, rail: 1, age: 22,  entries: 16,  strands: 4 },
  { id: "larc",    color: 0xF59E0B, rail: 1, age: 381, entries: 57,  strands: 7 },
  { id: "ayin",    color: 0xFF6D00, rail: 1, age: 5,   entries: 2,   strands: 7 },
];

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
