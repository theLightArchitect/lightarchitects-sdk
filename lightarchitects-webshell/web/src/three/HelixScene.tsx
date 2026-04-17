/**
 * HelixScene — R3F scene for the growing helix visualisation.
 *
 * Layout:
 *   - Two double-helix spine rails (rail 0 and rail 1), sampled every 0.05 Y units.
 *   - One sphere per SessionStep placed at the step's assigned Y coordinate on
 *     the entity's primary rail.  Colour = actor colour from sceneState.
 *   - Steps fade in with a brief scale animation on first render.
 *   - Camera default: (0, 0, 8), orbitable.
 */
import { useRef, useMemo } from 'react';
import { Canvas, useFrame } from '@react-three/fiber';
import { OrbitControls, Line } from '@react-three/drei';
import * as THREE from 'three';
import { getPrimaryFrame, tMin, tMax } from '../helix-math';
import { useSceneStore } from '../store/sceneState';
import { OrbLayer } from './OrbEntity';

// --- Helix Spine -------------------------------------------------------

/** Samples a rail into an array of Vector3 for <Line>. */
function sampleRail(railIdx: number, samples = 400): THREE.Vector3[] {
  const pts: THREE.Vector3[] = [];
  const step = (tMax - tMin) / samples;
  for (let i = 0; i <= samples; i++) {
    pts.push(getPrimaryFrame(railIdx, tMin + i * step).C);
  }
  return pts;
}

function HelixSpine() {
  // Pre-compute once — the spine doesn't change.
  const rail0 = useMemo(() => sampleRail(0), []);
  const rail1 = useMemo(() => sampleRail(1), []);
  return (
    <>
      <Line points={rail0} color={0x334155} lineWidth={1} />
      <Line points={rail1} color={0x334155} lineWidth={1} />
    </>
  );
}

// --- Step Spheres -------------------------------------------------------

/** A single sphere at the step's helix position. Scales in on mount. */
function StepSphere({ y, color }: { y: number; color: number }) {
  const ref = useRef<THREE.Mesh>(null);

  // Determine position from helix math (rail 0 for all steps in Visual 1).
  const pos = useMemo(() => {
    const { C } = getPrimaryFrame(0, y);
    return C.toArray() as [number, number, number];
  }, [y]);

  // Scale in from 0 → 1 over ~20 frames.
  const scale = useRef(0);
  useFrame(() => {
    if (!ref.current) return;
    scale.current = Math.min(scale.current + 0.05, 1);
    ref.current.scale.setScalar(scale.current);
  });

  return (
    <mesh ref={ref} position={pos} scale={0}>
      <sphereGeometry args={[0.04, 8, 8]} />
      <meshBasicMaterial color={color} />
    </mesh>
  );
}

/** Renders all steps in the Zustand store as spheres on the helix. */
function StepCloud() {
  const steps = useSceneStore((s) => s.steps);
  return (
    <>
      {steps.map((step) => (
        <StepSphere key={step.id} y={step.y} color={step.color} />
      ))}
    </>
  );
}

// --- Status Overlay (DOM, not Three.js) --------------------------------

function StatusBadge() {
  const { connected, reconnecting, attempt } = useSceneStore((s) => s.ayinStatus);
  const stepCount = useSceneStore((s) => s.steps.length);

  let label: string;
  let dotColor: string;
  if (connected) {
    label = `AYIN live · ${stepCount} steps`;
    dotColor = '#22c55e';
  } else if (reconnecting) {
    label = `reconnecting… (${attempt})`;
    dotColor = '#f59e0b';
  } else {
    label = 'AYIN offline';
    dotColor = '#ef4444';
  }

  return (
    <div style={{
      position: 'absolute',
      bottom: 12,
      left: 12,
      display: 'flex',
      alignItems: 'center',
      gap: 6,
      fontSize: 11,
      color: '#94a3b8',
      pointerEvents: 'none',
      fontFamily: 'inherit',
    }}>
      <span style={{ width: 7, height: 7, borderRadius: '50%', background: dotColor, display: 'inline-block' }} />
      {label}
    </div>
  );
}

// --- Root Export --------------------------------------------------------

export function HelixScene() {
  return (
    <div style={{ position: 'relative', width: '100%', height: '100%' }}>
      <Canvas
        camera={{ position: [0, 0, 8], fov: 60 }}
        gl={{ antialias: true, alpha: false }}
        style={{ background: '#0a0a0f' }}
      >
        <ambientLight intensity={0.4} />
        <pointLight position={[3, 5, 3]} intensity={1.2} />
        <HelixSpine />
        <StepCloud />
        <OrbLayer />
        <OrbitControls
          enablePan={false}
          minDistance={3}
          maxDistance={20}
          autoRotate={false}
        />
      </Canvas>
      <StatusBadge />
    </div>
  );
}
