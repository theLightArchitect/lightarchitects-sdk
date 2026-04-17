/**
 * OrbEntity — renders all active retrieval orbs from the Zustand orbQueue.
 *
 * Visual behaviour (faithful to actual SOUL helix retrieval):
 *   1. Orb spawns at the issuing sibling's last step position on the helix.
 *   2. Orb travels to each hit step in order (nearest-first), dwelling briefly
 *      at each one and emitting a glow ring that lingers after it departs.
 *   3. Orb returns to origin and fades out.
 *
 * All position math delegates to orbPositionFromPath() — pure, no state.
 * All ring pulse math delegates to waypointPulseIntensity() — pure, no state.
 * Per-frame Three.js mutations are done via refs, not React state.
 */
import { useRef, useMemo } from 'react';
import { useFrame } from '@react-three/fiber';
import * as THREE from 'three';
import { getPrimaryFrame } from '../helix-math';
import { useSceneStore } from '../store/sceneState';
import { useShallow } from 'zustand/react/shallow';
import {
  orbPositionFromPath,
  waypointPulseIntensity,
} from './orbAnimator';
import type { OrbWaypoint } from './orbAnimator';

// --- Shared geometry (allocated once, reused across all orb instances) ---

const ORB_GEO = new THREE.SphereGeometry(0.07, 12, 12);
const RING_GEO = new THREE.RingGeometry(0.09, 0.14, 24);

// --- Orb sphere ---------------------------------------------------------

function OrbSphere({ orbId }: { orbId: string }) {
  const ref = useRef<THREE.Mesh>(null);
  const mat = useRef(
    new THREE.MeshBasicMaterial({ color: 0x00f5ff, transparent: true, opacity: 1 }),
  );

  useFrame(() => {
    if (!ref.current) return;
    const orb = useSceneStore.getState().orbQueue.find((o) => o.id === orbId);
    if (!orb) { ref.current.visible = false; return; }

    const pos = orbPositionFromPath(orb.elapsed, orb.waypoints);
    ref.current.position.copy(pos);
    ref.current.visible = true;

    // Fade out toward the end of the return journey.
    const remaining = orb.totalDuration - orb.elapsed;
    mat.current.opacity = remaining < 0.3 ? remaining / 0.3 : 1.0;
  });

  return <mesh ref={ref} geometry={ORB_GEO} material={mat.current} visible={false} />;
}

// --- Hit glow ring ------------------------------------------------------

/**
 * A flat ring placed at a hit step's helix position.
 * Scales and fades with waypointPulseIntensity().
 * Faces the camera automatically via lookAt in useFrame.
 */
function HitRing({ orbId, waypoint }: { orbId: string; waypoint: OrbWaypoint }) {
  const ref = useRef<THREE.Mesh>(null);
  const mat = useRef(
    new THREE.MeshBasicMaterial({
      color: 0x00f5ff,
      transparent: true,
      opacity: 0,
      side: THREE.DoubleSide,
    }),
  );

  // Pre-compute the world position of this hit step on the helix rail.
  const worldPos = useMemo(
    () => getPrimaryFrame(0, waypoint.y).C.clone(),
    [waypoint.y],
  );

  useFrame(({ camera }) => {
    if (!ref.current) return;
    const orb = useSceneStore.getState().orbQueue.find((o) => o.id === orbId);
    if (!orb) { ref.current.visible = false; return; }

    const intensity = waypointPulseIntensity(orb.elapsed, waypoint);
    if (intensity <= 0) {
      ref.current.visible = false;
      return;
    }

    ref.current.visible = true;
    ref.current.position.copy(worldPos);
    // Bill-board: always face the camera so the ring is always visible.
    ref.current.lookAt(camera.position);

    const scale = 1.0 + intensity * 2.5;
    ref.current.scale.setScalar(scale);
    mat.current.opacity = intensity * 0.8;
  });

  return (
    <mesh
      ref={ref}
      geometry={RING_GEO}
      material={mat.current}
      visible={false}
    />
  );
}

// --- Orb group ----------------------------------------------------------

/** Renders one orb + all its hit rings. */
function OrbGroup({ orbId }: { orbId: string }) {
  // Read the waypoints once at mount — they don't change after spawn.
  // useShallow prevents infinite re-renders from the .find()?.waypoints ?? []
  // pattern (Zustand v5 uses Object.is; new array references trigger re-renders).
  const waypoints = useSceneStore(
    useShallow((s) => s.orbQueue.find((o) => o.id === orbId)?.waypoints ?? []),
  );
  const hitWaypoints = waypoints.filter((wp) => wp.isPulse);

  return (
    <group>
      <OrbSphere orbId={orbId} />
      {hitWaypoints.map((wp, i) => (
        <HitRing key={`${orbId}-ring-${i}`} orbId={orbId} waypoint={wp} />
      ))}
    </group>
  );
}

// --- Root layer ---------------------------------------------------------

/** Ticks orb elapsed time and renders all active orbs + their hit rings. */
export function OrbLayer() {
  // useShallow prevents infinite re-renders from .map() creating new array
  // references on every selector call (Zustand v5 Object.is comparison).
  const orbIds = useSceneStore(useShallow((s) => s.orbQueue.map((o) => o.id)));
  const tickOrbs = useSceneStore((s) => s.tickOrbs);

  // Advance elapsed time for all orbs every frame.
  useFrame((_state, delta) => {
    tickOrbs(delta);
  });

  return (
    <>
      {orbIds.map((id) => (
        <OrbGroup key={id} orbId={id} />
      ))}
    </>
  );
}
