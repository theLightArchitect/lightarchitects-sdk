<script lang="ts">
  import * as THREE from 'three';
  import { EffectComposer } from 'three/examples/jsm/postprocessing/EffectComposer.js';
  import { RenderPass } from 'three/examples/jsm/postprocessing/RenderPass.js';
  import { UnrealBloomPass } from 'three/examples/jsm/postprocessing/UnrealBloomPass.js';
  import {
    tMin, tMax,
    getFade, getPrimaryFrame, getEntityCenter, getMiniAnchorFrame,
    getSubStrandPos, seededRandom, buildEntities, setEntities,
  } from '$lib/helix/helix-math';
  import type { HelixEntity } from '$lib/helix/helix-math';
  import { HelixPolytopeManager } from '$lib/helix/helix-polytopes';
  import { HelixInteraction } from '$lib/helix/helix-interaction';
  import { helixEntries, promotionFeed, waves, activityFeed, copilotLoading, vaultCounts, activeHelixNode, buildFocusActive } from '$lib/stores';
  import type { SoulPromotionPayload } from '$lib/types';
  import { api } from '$lib/api';

  // Phase 12 — queue of soul_promotion events awaiting a THREE.Line spawn
  // in the render loop. Module-scoped so the promotion $effect can enqueue
  // without holding a reference to the scene (which lives in the render
  // effect's closure).
  const pendingLineageSpawns: SoulPromotionPayload[] = [];

  // Phase 12ext — queue of static :LINKS_TO edges (fetched once on mount)
  // awaiting a persistent THREE.Line spawn in the render loop. Drained on
  // the first animate() tick after the fetch resolves.
  type StaticEdgeSpec = {
    source: string;
    target: string;
    sourceSibling: string;
    targetSibling: string;
  };
  const pendingStaticEdges: StaticEdgeSpec[] = [];

  let container: HTMLDivElement;

  // --- Reactive entities from SOUL vault ---
  // When vault health data arrives, rebuild the entities array and bump
  // the generation counter to trigger a full scene rebuild.
  let helixGeneration = $state(0);
  let entities: HelixEntity[] = buildEntities(null);
  $effect(() => {
    const counts = $vaultCounts;
    const newEntities = buildEntities(counts);
    setEntities(newEntities);
    entities = newEntities;
    helixGeneration += 1;
  });

  // Phase 20 — pulse intensity for helix rotation. Spiked on every new
  // activity event (not just start-of-turn), decays exponentially each frame.
  // Tracks feed length so each new event re-spikes the pulse — matching the
  // lÆx0-cli SiblingWave.spike() pattern.
  let helixPulse = 0;
  let lastActivityLen = 0;
  $effect(() => {
    const len = $activityFeed.length;
    if (len > lastActivityLen) {
      helixPulse = 1.0;
      lastActivityLen = len;
    }
  });

  // Phase 9.7 — live helix_entry → orb-spawn counter. `orbCount` is derived
  // from the store length so reactivity flows straight from SSE to DOM.
  // Phase 10.9 adds strand fusion via the existing `waves` store (fed by
  // SSE `strand_activation` events).
  let orbCount = $derived($helixEntries.length);
  let pulseKey = $state(0);
  let lastSeenEntries = 0;
  $effect(() => {
    const len = $helixEntries.length;
    if (len > lastSeenEntries) {
      pulseKey += 1;
      lastSeenEntries = len;
      if (typeof window !== 'undefined') {
        (window as unknown as { __helixOrbCount: number }).__helixOrbCount = len;
        window.dispatchEvent(new CustomEvent('helix-orb-spawned', {
          detail: { count: len, entry: $helixEntries[0] },
        }));
      }
    }
  });

  // Phase 10.8 — promotion lineage edges. Each soul_promotion SSE event
  // pushes an entry onto a rolling list; the overlay renders them as pulses
  // with a 3s lifetime. This is the MVP — full 3D THREE.Line geometry with
  // UnrealBloom is Phase 11.
  type LineagePulse = { id: string; sibling: string; path: string; bornAt: number };
  let lineage = $state<LineagePulse[]>([]);
  let lastPromotionLen = 0;
  const LINEAGE_TTL_MS = 3_000;
  $effect(() => {
    const feed = $promotionFeed;
    if (feed.length > lastPromotionLen) {
      const now = performance.now();
      const freshPromos = feed.slice(0, feed.length - lastPromotionLen);
      const fresh = freshPromos.map((p, i) => ({
        id: `${p.memo_id}-${now}-${i}`,
        sibling: p.sibling,
        path: p.path,
        bornAt: now,
      }));
      lineage = [...fresh, ...lineage].slice(0, 12);
      // Phase 12 — also queue each promotion for a THREE.Line spawn in the
      // render loop. The animate() closure drains this queue every frame.
      pendingLineageSpawns.push(...freshPromos);
      lastPromotionLen = feed.length;
      if (typeof window !== 'undefined') {
        (window as unknown as { __helixLineageCount?: number }).__helixLineageCount = feed.length;
        window.dispatchEvent(new CustomEvent('helix-lineage-spawned', {
          detail: { count: feed.length, promotion: feed[0] },
        }));
      }
    }
  });

  // Phase 12ext — fetch :LINKS_TO edges once on mount. Bounded to 500 to
  // keep Three.js draw-calls modest on first paint; real deployments can
  // raise via the `limit` query param. Silently tolerates errors — if the
  // Neo4j tier isn't attached the endpoint returns an empty list and no
  // static edges render (dynamic promotion lines still work).
  $effect(() => {
    let cancelled = false;
    api.getSoulEdges(500)
      .then(res => {
        if (cancelled) return;
        for (const e of res.edges) {
          pendingStaticEdges.push({
            source: e.source,
            target: e.target,
            sourceSibling: e.source_sibling,
            targetSibling: e.target_sibling,
          });
        }
        if (typeof window !== 'undefined') {
          (window as unknown as { __helix3DStaticEdgeTotal?: number }).__helix3DStaticEdgeTotal =
            res.total;
        }
      })
      .catch(() => {
        // Endpoint may be absent in older server builds — silent ignore.
      });
    return () => {
      cancelled = true;
    };
  });

  // Garbage-collect expired lineage pulses every 500ms.
  $effect(() => {
    const id = setInterval(() => {
      const now = performance.now();
      const kept = lineage.filter(p => now - p.bornAt < LINEAGE_TTL_MS);
      if (kept.length !== lineage.length) lineage = kept;
    }, 500);
    return () => clearInterval(id);
  });

  // Phase 10.9 — strand fusion: derive per-sibling amplitude from the
  // `waves` store (spikeSibling fed by strand_activation SSE). Exposed as
  // `window.__helixStrandWaves` for deterministic E2E assertion.
  $effect(() => {
    const w = $waves;
    if (typeof window !== 'undefined') {
      const amplitudes: Record<string, number> = {};
      for (const [sid, wave] of Object.entries(w)) {
        const samples = wave?.samples ?? [];
        amplitudes[sid] = samples.length > 0
          ? Math.max(...samples.slice(-20).map(Math.abs))
          : 0;
      }
      (window as unknown as { __helixStrandWaves: Record<string, number> }).__helixStrandWaves = amplitudes;
    }
  });

  function createGlowTexture(): THREE.CanvasTexture {
    const canvas = document.createElement('canvas');
    canvas.width = 64;
    canvas.height = 64;
    const ctx = canvas.getContext('2d');
    if (ctx) {
      const gradient = ctx.createRadialGradient(32, 32, 0, 32, 32, 32);
      gradient.addColorStop(0, 'rgba(255, 255, 255, 1)');
      gradient.addColorStop(0.2, 'rgba(255, 255, 255, 0.8)');
      gradient.addColorStop(0.5, 'rgba(255, 255, 255, 0.2)');
      gradient.addColorStop(1, 'rgba(255, 255, 255, 0)');
      ctx.fillStyle = gradient;
      ctx.fillRect(0, 0, 64, 64);
    }
    return new THREE.CanvasTexture(canvas);
  }

  $effect(() => {
    if (!container) return;
    // Read helixGeneration to establish dependency — when vault data
    // arrives, this entire effect re-runs and rebuilds the scene with
    // real entity counts from the SOUL vault.
    void helixGeneration;

    const width = container.clientWidth;
    const height = container.clientHeight;

    const scene = new THREE.Scene();
    scene.fog = new THREE.FogExp2(0x000000, 0.06);

    const glowTexture = createGlowTexture();

    const camera = new THREE.PerspectiveCamera(60, width / height, 0.1, 200);
    camera.position.set(0, 0, 5.5);
    camera.lookAt(0, 0, 0);

    const renderer = new THREE.WebGLRenderer({ alpha: true, antialias: true, powerPreference: 'high-performance' });
    renderer.setPixelRatio(Math.min(window.devicePixelRatio, 1.5));
    renderer.setSize(width, height);
    while (container.firstChild) container.removeChild(container.firstChild);
    container.appendChild(renderer.domElement);

    const renderScene = new RenderPass(scene, camera);
    const composer = new EffectComposer(renderer);
    composer.addPass(renderScene);
    const bloomPass = new UnrealBloomPass(new THREE.Vector2(width, height), 1.0, 0.6, 0.25);
    bloomPass.threshold = 0.25;
    bloomPass.strength = 1.1;
    bloomPass.radius = 0.6;
    composer.addPass(bloomPass);

    // --- Atmospheric dust ---
    const dustGroup = new THREE.Group();
    scene.add(dustGroup);

    const palette = [
      new THREE.Color(0xFF1493),
      new THREE.Color(0x00BFFF),
      new THREE.Color(0xB44AFF),
      new THREE.Color(0xFFD700),
      new THREE.Color(0xFF6D00),
      new THREE.Color(0xffffff),
    ];

    const fineDustCount = 600;
    const fineDustGeom = new THREE.BufferGeometry();
    const fineDustPos = new Float32Array(fineDustCount * 3);
    const fineDustCol = new Float32Array(fineDustCount * 3);
    for (let i = 0; i < fineDustCount; i++) {
      fineDustPos[i*3] = (Math.random() - 0.5) * 15;
      fineDustPos[i*3+1] = (Math.random() - 0.5) * 15;
      fineDustPos[i*3+2] = (Math.random() - 0.5) * 15;
      const col = palette[Math.floor(Math.random() * palette.length)];
      fineDustCol[i*3] = col.r; fineDustCol[i*3+1] = col.g; fineDustCol[i*3+2] = col.b;
    }
    fineDustGeom.setAttribute('position', new THREE.BufferAttribute(fineDustPos, 3));
    fineDustGeom.setAttribute('color', new THREE.BufferAttribute(fineDustCol, 3));
    const fineDustMat = new THREE.PointsMaterial({
      size: 0.05, transparent: true, opacity: 0.25, vertexColors: true,
      blending: THREE.AdditiveBlending, depthWrite: false, map: glowTexture,
    });
    dustGroup.add(new THREE.Points(fineDustGeom, fineDustMat));

    const bokehCount = 30;
    const bokehGeom = new THREE.BufferGeometry();
    const bokehPos = new Float32Array(bokehCount * 3);
    const bokehCol = new Float32Array(bokehCount * 3);
    for (let i = 0; i < bokehCount; i++) {
      bokehPos[i*3] = (Math.random() - 0.5) * 20;
      bokehPos[i*3+1] = (Math.random() - 0.5) * 20;
      bokehPos[i*3+2] = (Math.random() - 0.5) * 8 - 4;
      const col = palette[Math.floor(Math.random() * 3)];
      bokehCol[i*3] = col.r; bokehCol[i*3+1] = col.g; bokehCol[i*3+2] = col.b;
    }
    bokehGeom.setAttribute('position', new THREE.BufferAttribute(bokehPos, 3));
    bokehGeom.setAttribute('color', new THREE.BufferAttribute(bokehCol, 3));
    const bokehMat = new THREE.PointsMaterial({
      size: 0.12, transparent: true, opacity: 0.05, vertexColors: true,
      blending: THREE.AdditiveBlending, depthWrite: false, map: glowTexture,
    });
    const bokehSystem = new THREE.Points(bokehGeom, bokehMat);
    dustGroup.add(bokehSystem);

    scene.add(new THREE.AmbientLight(0xffffff, 0.15));

    const group = new THREE.Group();
    scene.add(group);
    const outerPolytopeGroup = new THREE.Group();
    scene.add(outerPolytopeGroup);

    const polytopeManager = new HelixPolytopeManager(group, glowTexture, outerPolytopeGroup);

    const makePoints = (posArray: number[], colArray: number[], size: number, opacity: number) => {
      const geom = new THREE.BufferGeometry();
      geom.setAttribute('position', new THREE.Float32BufferAttribute(posArray, 3));
      geom.setAttribute('color', new THREE.Float32BufferAttribute(colArray, 3));
      return new THREE.Points(geom, new THREE.PointsMaterial({
        size, sizeAttenuation: true, vertexColors: true, transparent: true, opacity,
        blending: THREE.AdditiveBlending, depthWrite: false, map: glowTexture,
      }));
    };

    const makeLines = (posArray: number[], colArray: number[], opacity: number, linewidth = 1) => {
      const geom = new THREE.BufferGeometry();
      geom.setAttribute('position', new THREE.Float32BufferAttribute(posArray, 3));
      geom.setAttribute('color', new THREE.Float32BufferAttribute(colArray, 3));
      return new THREE.LineSegments(geom, new THREE.LineBasicMaterial({
        vertexColors: true, transparent: true, opacity,
        blending: THREE.AdditiveBlending, depthWrite: false, linewidth,
      }));
    };

    const entityColors = entities.map(e => new THREE.Color(e.color));

    // 1. PRIMARY RAILS
    const numSegments = 800;
    const pRailPos: number[] = [], pRailCol: number[] = [];
    for (let i = 0; i < numSegments; i++) {
      const y1 = tMin + (tMax - tMin) * (i / numSegments);
      const y2 = tMin + (tMax - tMin) * ((i + 1) / numSegments);
      const p0_1 = getPrimaryFrame(0, y1).C, p0_2 = getPrimaryFrame(0, y2).C;
      const p1_1 = getPrimaryFrame(1, y1).C, p1_2 = getPrimaryFrame(1, y2).C;
      const c1 = 0.08 * getFade(y1), c2 = 0.08 * getFade(y2);
      pRailPos.push(p0_1.x, p0_1.y, p0_1.z, p0_2.x, p0_2.y, p0_2.z);
      pRailCol.push(c1, c1, c1, c2, c2, c2);
      pRailPos.push(p1_1.x, p1_1.y, p1_1.z, p1_2.x, p1_2.y, p1_2.z);
      pRailCol.push(c1, c1, c1, c2, c2, c2);
    }
    group.add(makeLines(pRailPos, pRailCol, 0.15, 1));

    // 2. MICRO STRANDS
    const domPos: number[] = [], domCol: number[] = [];
    const mRailPos: number[] = [], mRailCol: number[] = [];
    const microSegments = 3500;
    for (let i = 0; i < microSegments; i++) {
      const t1 = tMin + (tMax - tMin) * (i / microSegments);
      const t2 = tMin + (tMax - tMin) * ((i + 1) / microSegments);
      for (let s = 0; s < 6; s++) {
        const entity = entities[s];
        const ageFactor = Math.min(entity.age / 365, 1.0);
        const hsl = { h: 0, s: 0, l: 0 };
        entityColors[s].getHSL(hsl);
        const col = new THREE.Color().setHSL(hsl.h, Math.min(hsl.s * 1.2, 1.0), Math.min(hsl.l * 1.5, 0.7));
        const breathe1 = Math.sin(t1 * 0.5 + s * 1.7) * 0.1 + 0.9;
        const breathe2 = Math.sin(t2 * 0.5 + s * 1.7) * 0.1 + 0.9;
        const baseOpacity = 0.8 + 0.2 * ageFactor;
        const op1 = baseOpacity * breathe1 * getFade(t1);
        const op2 = baseOpacity * breathe2 * getFade(t2);
        const numM0 = Math.ceil(entity.strands / 2), numM1 = Math.floor(entity.strands / 2);
        for (let d = 0; d < numM0; d++) {
          const d1 = getSubStrandPos(s, 0, d, numM0, t1), d2 = getSubStrandPos(s, 0, d, numM0, t2);
          domPos.push(d1.x, d1.y, d1.z, d2.x, d2.y, d2.z);
          domCol.push(col.r*op1, col.g*op1, col.b*op1, col.r*op2, col.g*op2, col.b*op2);
        }
        for (let d = 0; d < numM1; d++) {
          const d1 = getSubStrandPos(s, 1, d, numM1, t1), d2 = getSubStrandPos(s, 1, d, numM1, t2);
          domPos.push(d1.x, d1.y, d1.z, d2.x, d2.y, d2.z);
          domCol.push(col.r*op1, col.g*op1, col.b*op1, col.r*op2, col.g*op2, col.b*op2);
        }
        const mOp = 0.2 + 0.5 * ageFactor;
        const m1a = getMiniAnchorFrame(s, 0, t1).M, m1b = getMiniAnchorFrame(s, 0, t2).M;
        const m2a = getMiniAnchorFrame(s, 1, t1).M, m2b = getMiniAnchorFrame(s, 1, t2).M;
        const mop1 = 0.35 * mOp * getFade(t1), mop2 = 0.35 * mOp * getFade(t2);
        mRailPos.push(m1a.x, m1a.y, m1a.z, m1b.x, m1b.y, m1b.z);
        mRailCol.push(mop1, mop1, mop1, mop2, mop2, mop2);
        mRailPos.push(m2a.x, m2a.y, m2a.z, m2b.x, m2b.y, m2b.z);
        mRailCol.push(mop1, mop1, mop1, mop2, mop2, mop2);
      }
    }
    group.add(makeLines(domPos, domCol, 1.0, 1.5));
    group.add(makeLines(mRailPos, mRailCol, 1.0, 1.0));

    // 3. ENTITY NODES
    const mNodesPos: number[] = [], mNodesCol: number[] = [];
    const mNodesBrightPos: number[] = [], mNodesBrightCol: number[] = [];
    const mNodesHaloPos: number[] = [], mNodesHaloCol: number[] = [];
    for (let s = 0; s < 6; s++) {
      const entity = entities[s];
      const ageFactor = Math.min(entity.age / 365, 1.0);
      const hsl = { h: 0, s: 0, l: 0 };
      entityColors[s].getHSL(hsl);
      const col = new THREE.Color().setHSL(hsl.h, hsl.s * (0.4 + 0.6 * ageFactor), hsl.l);
      if (entity.id === 'seraph') {
        const E_pos = getEntityCenter(s, 0).E;
        mNodesPos.push(E_pos.x, E_pos.y, E_pos.z);
        mNodesCol.push(col.r * 1.5 * getFade(0), col.g * 1.5 * getFade(0), col.b * 1.5 * getFade(0));
      } else {
        const rnd = seededRandom(42 + s);
        const baseSpacing = (tMax - tMin) / Math.max(entity.entries, 1);
        let currY = tMin;
        while (currY <= tMax) {
          const E_pos = getEntityCenter(s, currY).E;
          const fade = getFade(currY);
          const significance = rnd() * 10;
          if (significance >= 7.0) {
            mNodesBrightPos.push(E_pos.x, E_pos.y, E_pos.z);
            mNodesBrightCol.push(col.r * 2.0 * fade, col.g * 2.0 * fade, col.b * 2.0 * fade);
            mNodesHaloPos.push(E_pos.x, E_pos.y, E_pos.z);
            mNodesHaloCol.push(col.r * 0.5 * fade, col.g * 0.5 * fade, col.b * 0.5 * fade);
          } else {
            mNodesPos.push(E_pos.x, E_pos.y, E_pos.z);
            const scaleFactor = 0.5 + (significance / 10.0);
            mNodesCol.push(col.r * 1.5 * scaleFactor * fade, col.g * 1.5 * scaleFactor * fade, col.b * 1.5 * scaleFactor * fade);
          }
          currY += baseSpacing * (0.7 + 0.6 * rnd());
        }
      }
    }
    group.add(makePoints(mNodesPos, mNodesCol, 0.06, 0.9));
    group.add(makePoints(mNodesBrightPos, mNodesBrightCol, 0.12, 1.0));
    group.add(makePoints(mNodesHaloPos, mNodesHaloCol, 0.25, 0.4));

    // 4. STEPS & RUNGS
    const pStepPos: number[] = [], pStepCol: number[] = [];
    const connPos: number[] = [], connCol: number[] = [];
    const pNodesPos: number[] = [], pNodesCol: number[] = [];
    const stepHaloPos: number[] = [], stepHaloCol: number[] = [];
    const mStepPos: number[] = [], mStepCol: number[] = [];
    const crossConnPos: number[] = [], crossConnCol: number[] = [];
    const convergenceNodes: Array<{ particles: THREE.Points; pAngles: Float32Array; pRadii: Float32Array; center: THREE.Vector3 }> = [];
    const communityColors = [
      new THREE.Color(0xffffff), new THREE.Color(0x3B82F6),
      new THREE.Color(0x10B981), new THREE.Color(0x8B5CF6),
    ];

    const numTicks = Math.round((tMax - tMin) / 0.05);
    for (let tick = 0; tick <= numTicks; tick++) {
      const y = tMin + tick * 0.05;
      const isPrimary = tick % 20 === 0, isSub = !isPrimary && tick % 5 === 0;
      const isSubSub = !isPrimary && !isSub;
      const fadeY = getFade(y);

      if (tick % 4 === 0) {
        for (let s = 0; s < 6; s++) {
          const ageFactor = Math.min(entities[s].age / 365, 1.0);
          const mOp = (0.2 + 0.4 * ageFactor) * fadeY;
          const m1 = getMiniAnchorFrame(s, 0, y).M, m2 = getMiniAnchorFrame(s, 1, y).M;
          mStepPos.push(m1.x, m1.y, m1.z, m2.x, m2.y, m2.z);
          mStepCol.push(0.35*mOp, 0.35*mOp, 0.35*mOp, 0.35*mOp, 0.35*mOp, 0.35*mOp);
        }
      }

      if (tick % 7 === 0) {
        for (let s = 0; s < 6; s++) {
          if (Math.random() < 0.3) {
            const eCenter = getEntityCenter(s, y).E;
            const pFrame = getPrimaryFrame(entities[s].rail, y).C;
            const c = entityColors[s];
            crossConnPos.push(eCenter.x, eCenter.y, eCenter.z, pFrame.x, pFrame.y, pFrame.z);
            crossConnCol.push(c.r*0.3*fadeY, c.g*0.3*fadeY, c.b*0.3*fadeY, 0.15*fadeY, 0.15*fadeY, 0.15*fadeY);
          }
        }
      }

      if (tick % 5 === 0) {
        for (let s1 = 0; s1 < 6; s1++) {
          for (let s2 = s1 + 1; s2 < 6; s2++) {
            if (Math.random() < 0.12) {
              const e1 = getEntityCenter(s1, y).E, e2 = getEntityCenter(s2, y).E;
              const c1 = entityColors[s1], c2 = entityColors[s2];
              const linkType = Math.random();
              if (linkType < 0.4) {
                crossConnPos.push(e1.x, e1.y, e1.z, e2.x, e2.y, e2.z);
                crossConnCol.push(c1.r*0.4*fadeY, c1.g*0.4*fadeY, c1.b*0.4*fadeY, c2.r*0.4*fadeY, c2.g*0.4*fadeY, c2.b*0.4*fadeY);
              } else if (linkType < 0.6) {
                crossConnPos.push(e1.x, e1.y, e1.z, e2.x, e2.y, e2.z);
                crossConnCol.push(0.8*fadeY, 0.1*fadeY, 0.1*fadeY, 0.8*fadeY, 0.1*fadeY, 0.1*fadeY);
              } else {
                const mid = new THREE.Vector3().addVectors(e1, e2).multiplyScalar(0.5);
                mid.x += (Math.random() - 0.5) * 0.8; mid.z += (Math.random() - 0.5) * 0.8;
                crossConnPos.push(e1.x, e1.y, e1.z, mid.x, mid.y, mid.z, mid.x, mid.y, mid.z, e2.x, e2.y, e2.z);
                crossConnCol.push(c1.r*0.3*fadeY, c1.g*0.3*fadeY, c1.b*0.3*fadeY, c2.r*0.3*fadeY, c2.g*0.3*fadeY, c2.b*0.3*fadeY);
                crossConnCol.push(c2.r*0.3*fadeY, c2.g*0.3*fadeY, c2.b*0.3*fadeY, c2.r*0.3*fadeY, c2.g*0.3*fadeY, c2.b*0.3*fadeY);
              }
            }
          }
        }
      }

      const c1 = getPrimaryFrame(0, y).C, c2 = getPrimaryFrame(1, y).C;
      const center = new THREE.Vector3().addVectors(c1, c2).multiplyScalar(0.5);
      let isShared = isPrimary ? Math.random() < 0.85 : isSub ? Math.random() < 0.35 : Math.random() < 0.04;

      if (isShared) {
        const allActive: number[] = [];
        for (let s = 0; s < 6; s++) {
          const entity = entities[s];
          if (entity.id === 'seraph') { if (isPrimary && Math.abs(y) < 0.05) allActive.push(s); }
          else if (entity.id === 'quantum') { if (isPrimary && Math.random() > 0.6) allActive.push(s); }
          else { if (Math.random() > 0.4) allActive.push(s); }
        }
        if (allActive.length === 0) { allActive.push(Math.floor(Math.random() * 3), Math.floor(Math.random() * 3) + 3); }
        else if (allActive.length === 1 && allActive[0] !== 5) {
          const side = allActive[0] < 3 ? 1 : 0;
          allActive.push(Math.floor(Math.random() * 3) + side * 3);
        }
        let r = 0, g = 0, b = 0;
        allActive.forEach(s => { r += entityColors[s].r; g += entityColors[s].g; b += entityColors[s].b; });
        r /= allActive.length; g /= allActive.length; b /= allActive.length;
        const isGlowingWhite = Math.random() < 0.15;
        const stepR = (isGlowingWhite ? 1.0 : r) * fadeY;
        const stepG = (isGlowingWhite ? 1.0 : g) * fadeY;
        const stepB = (isGlowingWhite ? 1.0 : b) * fadeY;

        if (isPrimary) {
          pStepPos.push(c1.x, c1.y, c1.z, c2.x, c2.y, c2.z, c1.x, c1.y+0.025, c1.z, c2.x, c2.y+0.025, c2.z, c1.x, c1.y-0.025, c1.z, c2.x, c2.y-0.025, c2.z);
          for (let k = 0; k < 6; k++) pStepCol.push(stepR, stepG, stepB);
          stepHaloPos.push(center.x, center.y, center.z);
          stepHaloCol.push(stepR*0.5*fadeY, stepG*0.5*fadeY, stepB*0.5*fadeY);
        } else if (isSub) {
          pStepPos.push(c1.x, c1.y, c1.z, c2.x, c2.y, c2.z, c1.x, c1.y+0.01, c1.z, c2.x, c2.y+0.01, c2.z);
          for (let k = 0; k < 4; k++) pStepCol.push(stepR, stepG, stepB);
        } else {
          pStepPos.push(c1.x, c1.y, c1.z, c2.x, c2.y, c2.z);
          pStepCol.push(stepR, stepG, stepB, stepR, stepG, stepB);
        }

        allActive.forEach(s => {
          const E_pos = getEntityCenter(s, y).E;
          const c = entityColors[s];
          connPos.push(E_pos.x, E_pos.y, E_pos.z, center.x, center.y, center.z);
          let op = 0.5 * fadeY;
          if (entities[s].id === 'corso') op = 0.35 * fadeY;
          else if (entities[s].id === 'quantum') op = 0.25 * fadeY;
          else if (entities[s].id === 'seraph') op = 0.8 * fadeY;
          const cw = isGlowingWhite ? 0.8 * fadeY : 0;
          connCol.push(c.r*op+cw, c.g*op+cw, c.b*op+cw, 1.0*fadeY, 1.0*fadeY, 1.0*fadeY);
        });
        pNodesPos.push(center.x, center.y, center.z);
        pNodesCol.push(stepR * 1.2 * fadeY, stepG * 1.2 * fadeY, stepB * 1.2 * fadeY);

        if (isPrimary && isGlowingWhite && fadeY > 0.1) {
          const commColor = communityColors[Math.floor(Math.random() * communityColors.length)];
          const pGeo = new THREE.BufferGeometry();
          const pPos = new Float32Array(8 * 3);
          const pAngles = new Float32Array(8), pRadii = new Float32Array(8);
          for (let k = 0; k < 8; k++) {
            pAngles[k] = Math.random() * Math.PI * 2;
            pRadii[k] = Math.random() * 0.5;
            pPos[k*3] = center.x; pPos[k*3+1] = center.y; pPos[k*3+2] = center.z;
          }
          pGeo.setAttribute('position', new THREE.BufferAttribute(pPos, 3));
          const pMat = new THREE.PointsMaterial({ color: commColor, size: 0.03, transparent: true, opacity: 0.7 * fadeY, blending: THREE.AdditiveBlending, depthWrite: false, map: glowTexture });
          const pPoints = new THREE.Points(pGeo, pMat);
          group.add(pPoints);
          convergenceNodes.push({ particles: pPoints, pAngles, pRadii, center: center.clone() });
        }
      } else {
        if (isPrimary) {
          pStepPos.push(c1.x, c1.y, c1.z, c2.x, c2.y, c2.z, c1.x, c1.y+0.025, c1.z, c2.x, c2.y+0.025, c2.z, c1.x, c1.y-0.025, c1.z, c2.x, c2.y-0.025, c2.z);
          for (let k = 0; k < 6; k++) pStepCol.push(0.08*fadeY, 0.08*fadeY, 0.08*fadeY);
          stepHaloPos.push(center.x, center.y, center.z);
          stepHaloCol.push(0.05*fadeY, 0.05*fadeY, 0.05*fadeY);
        } else if (isSub) {
          pStepPos.push(c1.x, c1.y, c1.z, c2.x, c2.y, c2.z, c1.x, c1.y+0.01, c1.z, c2.x, c2.y+0.01, c2.z);
          for (let k = 0; k < 4; k++) pStepCol.push(0.05*fadeY, 0.05*fadeY, 0.05*fadeY);
        } else if (isSubSub) {
          pStepPos.push(c1.x, c1.y, c1.z, c2.x, c2.y, c2.z);
          pStepCol.push(0.02*fadeY, 0.02*fadeY, 0.02*fadeY, 0.02*fadeY, 0.02*fadeY, 0.02*fadeY);
        }
      }
    }
    group.add(makeLines(pStepPos, pStepCol, 0.15, 1));
    group.add(makeLines(mStepPos, mStepCol, 1.0, 1.0));
    group.add(makeLines(connPos, connCol, 1.0, 1.5));
    group.add(makeLines(crossConnPos, crossConnCol, 0.8, 0.7));
    group.add(makePoints(pNodesPos, pNodesCol, 0.12, 0.8));
    group.add(makePoints(stepHaloPos, stepHaloCol, 0.08, 0.4));

    // 5. RAG AGENT ORBS
    const agentCount = 60;
    const agentGeo = new THREE.BufferGeometry();
    const agentPos = new Float32Array(agentCount * 3);
    const agentColor = new Float32Array(agentCount * 3);
    type AgentState = 'TRAVERSE' | 'HIT' | 'JUMP' | 'SPAWN';
    interface AgentData {
      entityIdx: number; mIdx: number; y: number; speed: number;
      retrievalMode: string; state: AgentState; stateTimer: number;
      targetNode: { center: THREE.Vector3 } | null;
      color: THREE.Color; oldColor: THREE.Color;
      jumpStart: THREE.Vector3; jumpEnd: THREE.Vector3; jumpControl: THREE.Vector3;
    }
    const agentData: AgentData[] = [];
    for (let i = 0; i < agentCount; i++) {
      const entityIdx = Math.floor(Math.random() * 6);
      const entity = entities[entityIdx];
      const mIdx = Math.floor(Math.random() * entity.strands);
      const y = tMin + Math.random() * (tMax - tMin);
      const retrievalMode = entity.entries < 20 ? 'KeywordDominated' : entity.entries > 150 ? 'GraphWeighted' : 'Balanced';
      const baseSpeed = (1.5 + Math.random() * 2.5) * (Math.random() > 0.5 ? 1 : -1);
      const speed = retrievalMode === 'KeywordDominated' ? baseSpeed * 0.3 : retrievalMode === 'GraphWeighted' ? baseSpeed * 1.8 : baseSpeed;
      const startNode = convergenceNodes.length > 0 ? convergenceNodes[Math.floor(Math.random() * convergenceNodes.length)] : { center: new THREE.Vector3() };
      agentData.push({ entityIdx, mIdx, y, speed, retrievalMode, state: 'TRAVERSE', stateTimer: 0, targetNode: null, color: entityColors[entityIdx].clone(), oldColor: new THREE.Color(), jumpStart: new THREE.Vector3(), jumpEnd: new THREE.Vector3(), jumpControl: new THREE.Vector3() });
      agentPos[i*3] = startNode.center.x; agentPos[i*3+1] = startNode.center.y; agentPos[i*3+2] = startNode.center.z;
      agentColor[i*3] = 1; agentColor[i*3+1] = 1; agentColor[i*3+2] = 1;
    }
    agentGeo.setAttribute('position', new THREE.BufferAttribute(agentPos, 3));
    agentGeo.setAttribute('color', new THREE.BufferAttribute(agentColor, 3));
    const agentMat = new THREE.PointsMaterial({ size: 0.18, sizeAttenuation: true, map: glowTexture, transparent: true, opacity: 1.0, vertexColors: true, blending: THREE.AdditiveBlending, depthWrite: false, depthTest: false });
    group.add(new THREE.Points(agentGeo, agentMat));

    // 6. ACTIVE SESSION NODES — Layer 1 overlay
    // Collider spheres spawned for each helix_entry SSE event. These are the
    // only interactive nodes in the helix — dormant vault entries are too
    // small and numerous to raycast against.
    const activeNodeGroup = new THREE.Group();
    group.add(activeNodeGroup);
    const activeNodeRaycaster = new THREE.Raycaster();
    const activeNodeMouse = new THREE.Vector2();
    interface ActiveNode {
      mesh: THREE.Mesh;
      glow: THREE.Points;
      sibling: string;
      path: string;
      significance: number;
      excerpt: string;
      bornAt: number;
    }
    const activeNodes: ActiveNode[] = [];
    const ACTIVE_NODE_TTL_MS = 30_000; // 30s visibility

    /** Pending helix_entry events to spawn as active nodes. */
    const pendingActiveNodes: import('$lib/types').HelixEntrySsePayload[] = [];

    // Watch for new helix entries — queue them for spawn in the render loop.
    let lastHelixLen = 0;

    function spawnActiveNode(entry: import('$lib/types').HelixEntrySsePayload) {
      const sibIdx = entities.findIndex(e => e.id === entry.sibling);
      if (sibIdx < 0) return;

      // Place at a deterministic Y position from the path hash
      let h = 0x811c9dc5 >>> 0;
      for (let i = 0; i < entry.path.length; i++) {
        h ^= entry.path.charCodeAt(i);
        h = Math.imul(h, 0x01000193) >>> 0;
      }
      const y = 2.3 + (-4.6) * (h / 0xffffffff);
      const pos = getEntityCenter(sibIdx, y).E;

      const color = new THREE.Color(entities[sibIdx].color);
      const sig = entry.significance ?? 5.0;
      const radius = 0.04 + (sig / 10) * 0.06; // 0.04–0.10

      // Invisible collider sphere for raycasting
      const geo = new THREE.SphereGeometry(radius * 3, 8, 8);
      const mat = new THREE.MeshBasicMaterial({ transparent: true, opacity: 0, depthWrite: false });
      const mesh = new THREE.Mesh(geo, mat);
      mesh.position.copy(pos);
      mesh.userData = { activeNode: true, index: activeNodes.length };
      activeNodeGroup.add(mesh);

      // Visible glow point
      const glowGeo = new THREE.BufferGeometry();
      glowGeo.setAttribute('position', new THREE.Float32BufferAttribute([pos.x, pos.y, pos.z], 3));
      glowGeo.setAttribute('color', new THREE.Float32BufferAttribute([color.r * 2, color.g * 2, color.b * 2], 3));
      const glowMat = new THREE.PointsMaterial({
        size: radius * 4,
        sizeAttenuation: true,
        vertexColors: true,
        transparent: true,
        opacity: 1.0,
        blending: THREE.AdditiveBlending,
        depthWrite: false,
        map: glowTexture,
      });
      const glow = new THREE.Points(glowGeo, glowMat);
      group.add(glow);

      activeNodes.push({
        mesh, glow,
        sibling: entry.sibling ?? 'unknown',
        path: entry.path,
        significance: sig,
        excerpt: entry.content_excerpt ?? '',
        bornAt: performance.now(),
      });
    }

    // --- Camera control ---
    let pointerX = 0, pointerY = 0;
    const cameraControl = { targetX: 0, targetY: 0, targetZ: 5.5, targetRotX: 0, targetRotZ: 0, lookAt: new THREE.Vector3(0, 0, 0), lerpSpeed: 0.03, scrollLocked: false };
    const handleMouseMove = (event: MouseEvent) => {
      pointerX = event.clientX / window.innerWidth;
      pointerY = event.clientY / window.innerHeight;
      cameraControl.targetX = (pointerX - 0.5) * 2.0;
      cameraControl.targetY = -(pointerY - 0.5) * 2.0;
      cameraControl.targetRotX = (pointerY - 0.5) * 0.15;
      cameraControl.targetRotZ = (pointerX - 0.5) * 0.15;

      // Active node raycasting — only check colliders in activeNodeGroup
      if (activeNodes.length > 0) {
        const rect = renderer.domElement.getBoundingClientRect();
        activeNodeMouse.x = ((event.clientX - rect.left) / rect.width) * 2 - 1;
        activeNodeMouse.y = -((event.clientY - rect.top) / rect.height) * 2 + 1;
        activeNodeRaycaster.setFromCamera(activeNodeMouse, camera);
        const colliders = activeNodes.map(n => n.mesh);
        const hits = activeNodeRaycaster.intersectObjects(colliders);
        if (hits.length > 0) {
          const idx = hits[0].object.userData.index as number;
          const node = activeNodes[idx];
          if (node) {
            renderer.domElement.style.cursor = 'pointer';
            activeHelixNode.set({
              sibling: node.sibling,
              path: node.path,
              significance: node.significance,
              excerpt: node.excerpt,
              screenX: event.clientX,
              screenY: event.clientY,
            });
          }
        } else {
          renderer.domElement.style.cursor = 'default';
          activeHelixNode.set(null);
        }
      }
    };
    window.addEventListener('mousemove', handleMouseMove);

    // Interaction layer
    renderer.domElement.style.pointerEvents = 'auto';
    const interaction = new HelixInteraction(camera, renderer.domElement, cameraControl, polytopeManager, () => group.position.y);

    const clock = new THREE.Clock();
    let animationFrameId: number;
    const currentGroupY = { value: -2.0 };

    // Phase 12 — THREE.Line promotion-lineage state.
    // Each entry holds the line mesh + its material handle (for opacity tween)
    // + spawn timestamp. Lines live inside `group` so they rotate with the
    // rest of the helix scene. 3s TTL matches the DOM overlay timing.
    type LineageLine = {
      line: THREE.Line;
      material: THREE.LineBasicMaterial;
      bornAt: number;
    };
    const activeLineageLines: LineageLine[] = [];
    const LINEAGE_TTL_MS = 3000;
    const LINEAGE_SEGMENTS = 32;

    function spawn3DLineageLine(promo: SoulPromotionPayload) {
      const idx = entities.findIndex(e => e.id === promo.sibling);
      if (idx < 0) return; // unknown sibling — skip visual
      // Sample points along the sibling's entity curve from top to bottom of
      // the visible helix range. Each point is a Vector3 in helix-local
      // coords; adding to `group` inherits the helix rotation.
      const points: THREE.Vector3[] = [];
      const yStart = 2.5;
      const yEnd = -2.5;
      for (let i = 0; i <= LINEAGE_SEGMENTS; i++) {
        const y = yStart + (yEnd - yStart) * (i / LINEAGE_SEGMENTS);
        points.push(getEntityCenter(idx, y).E);
      }
      const geo = new THREE.BufferGeometry().setFromPoints(points);

      // Phase 12.2 — per-vertex color gradient. Bright at top (hot origin),
      // dimmer at bottom (promoted/cold destination). Uses the sibling's
      // canonical color as the base; brightness tweens along the path so the
      // eye reads direction of flow without an arrow head.
      const baseColor = new THREE.Color(entities[idx].color);
      const colorAttr = new Float32Array((LINEAGE_SEGMENTS + 1) * 3);
      for (let i = 0; i <= LINEAGE_SEGMENTS; i++) {
        const t = i / LINEAGE_SEGMENTS;
        const brightness = 1.0 - t * 0.7; // 1.0 → 0.3
        colorAttr[i * 3] = baseColor.r * brightness;
        colorAttr[i * 3 + 1] = baseColor.g * brightness;
        colorAttr[i * 3 + 2] = baseColor.b * brightness;
      }
      geo.setAttribute('color', new THREE.BufferAttribute(colorAttr, 3));

      const mat = new THREE.LineBasicMaterial({
        vertexColors: true,
        transparent: true,
        opacity: 1.0,
        linewidth: 2,
      });
      const line = new THREE.Line(geo, mat);
      group.add(line);
      activeLineageLines.push({ line, material: mat, bornAt: performance.now() });

      // Dev hook for E2E — exposes the count of active 3D lineage lines.
      if (typeof window !== 'undefined') {
        (window as unknown as { __helix3DLineageActive: number }).__helix3DLineageActive =
          activeLineageLines.length;
      }
    }

    // Phase 12ext — persistent static-edge state. Unlike promotion lineage,
    // these never expire; they encode the structural :LINKS_TO graph.
    const activeStaticEdges: THREE.Line[] = [];

    // Deterministic vault-path → Y coord hash so two runs place the same
    // Step at the same visual altitude. Uses FNV-1a over UTF-16 code units
    // then maps to [yStart, yEnd].
    function vaultPathToY(path: string): number {
      let h = 0x811c9dc5 >>> 0;
      for (let i = 0; i < path.length; i++) {
        h ^= path.charCodeAt(i);
        h = Math.imul(h, 0x01000193) >>> 0;
      }
      const frac = (h / 0xffffffff); // [0, 1]
      const yStart = 2.3;
      const yEnd = -2.3;
      return yStart + (yEnd - yStart) * frac;
    }

    function spawn3DStaticEdge(spec: StaticEdgeSpec) {
      const srcIdx = entities.findIndex(e => e.id === spec.sourceSibling);
      const tgtIdx = entities.findIndex(e => e.id === spec.targetSibling);
      if (srcIdx < 0 || tgtIdx < 0) return; // non-canonical sibling → skip.
      const srcY = vaultPathToY(spec.source);
      const tgtY = vaultPathToY(spec.target);
      const srcPos = getEntityCenter(srcIdx, srcY).E.clone();
      const tgtPos = getEntityCenter(tgtIdx, tgtY).E.clone();

      // 12 intermediate samples for a subtle bend through the helix interior
      // instead of a chord across empty space. Linear interp is fine — it's
      // the vertex-color gradient + bloom that give the impression of flow.
      const SEGMENTS = 12;
      const points: THREE.Vector3[] = [];
      for (let i = 0; i <= SEGMENTS; i++) {
        const t = i / SEGMENTS;
        points.push(new THREE.Vector3().lerpVectors(srcPos, tgtPos, t));
      }
      const geo = new THREE.BufferGeometry().setFromPoints(points);

      // Per-vertex color gradient: source sibling color → target sibling
      // color. Same direction-without-arrowhead idiom as the dynamic
      // promotion lines, so the two visual classes feel like one family.
      const srcColor = new THREE.Color(entities[srcIdx].color);
      const tgtColor = new THREE.Color(entities[tgtIdx].color);
      const colorAttr = new Float32Array((SEGMENTS + 1) * 3);
      for (let i = 0; i <= SEGMENTS; i++) {
        const t = i / SEGMENTS;
        const dim = 0.55; // persistent lines stay dim enough not to dominate.
        colorAttr[i * 3] = (srcColor.r * (1 - t) + tgtColor.r * t) * dim;
        colorAttr[i * 3 + 1] = (srcColor.g * (1 - t) + tgtColor.g * t) * dim;
        colorAttr[i * 3 + 2] = (srcColor.b * (1 - t) + tgtColor.b * t) * dim;
      }
      geo.setAttribute('color', new THREE.BufferAttribute(colorAttr, 3));

      const mat = new THREE.LineBasicMaterial({
        vertexColors: true,
        transparent: true,
        opacity: 0.65,
      });
      const line = new THREE.Line(geo, mat);
      group.add(line);
      activeStaticEdges.push(line);

      if (typeof window !== 'undefined') {
        (window as unknown as { __helix3DStaticEdgeCount: number })
          .__helix3DStaticEdgeCount = activeStaticEdges.length;
      }
    }

    const animate = () => {
      animationFrameId = requestAnimationFrame(animate);
      const time = clock.getElapsedTime();

      // Phase 12 — drain pending promotion-lineage spawns + tween opacity
      // on live ones. Done first so a new line renders at full opacity on
      // the same frame it's spawned.
      while (pendingLineageSpawns.length > 0) {
        const promo = pendingLineageSpawns.shift();
        if (promo) spawn3DLineageLine(promo);
      }

      // Phase 12ext — drain static :LINKS_TO edge queue, but only N per
      // frame so the initial fetch of 500 edges spreads over ~8 frames at
      // 60fps instead of hitching a single frame. No TTL — these persist.
      const STATIC_EDGE_BUDGET_PER_FRAME = 64;
      for (let i = 0; i < STATIC_EDGE_BUDGET_PER_FRAME && pendingStaticEdges.length > 0; i++) {
        const spec = pendingStaticEdges.shift();
        if (spec) spawn3DStaticEdge(spec);
      }
      const now = performance.now();
      let anyRemoved = false;
      for (let i = activeLineageLines.length - 1; i >= 0; i--) {
        const { line, material, bornAt } = activeLineageLines[i];
        const age = now - bornAt;
        if (age >= LINEAGE_TTL_MS) {
          group.remove(line);
          line.geometry.dispose();
          material.dispose();
          activeLineageLines.splice(i, 1);
          anyRemoved = true;
        } else {
          // Linear fade; quadratic felt too aggressive in informal testing.
          material.opacity = 1.0 - age / LINEAGE_TTL_MS;
        }
      }
      if (anyRemoved && typeof window !== 'undefined') {
        (window as unknown as { __helix3DLineageActive: number }).__helix3DLineageActive =
          activeLineageLines.length;
      }

      const focusIntensity = interaction.getFocusIntensity();

      // Phase 2b — bloom coupling: helix "breathes harder" while copilot thinks.
      // Lerp bloom strength toward target at ~3% per frame (smooth 0.5s ramp).
      const bloomTarget = $copilotLoading ? 1.6 : 1.0;
      bloomPass.strength += (bloomTarget - bloomPass.strength) * 0.03;

      // Layer 1 — drain pending active session nodes
      const entries = $helixEntries;
      if (entries.length > lastHelixLen) {
        const fresh = entries.slice(0, entries.length - lastHelixLen);
        for (const e of fresh) spawnActiveNode(e);
        lastHelixLen = entries.length;
      }

      // Layer 1 — GC expired active nodes (fade out over last 3s of TTL)
      for (let i = activeNodes.length - 1; i >= 0; i--) {
        const age = now - activeNodes[i].bornAt;
        if (age >= ACTIVE_NODE_TTL_MS) {
          activeNodeGroup.remove(activeNodes[i].mesh);
          activeNodes[i].mesh.geometry.dispose();
          (activeNodes[i].mesh.material as THREE.Material).dispose();
          group.remove(activeNodes[i].glow);
          activeNodes[i].glow.geometry.dispose();
          (activeNodes[i].glow.material as THREE.Material).dispose();
          activeNodes.splice(i, 1);
          // Re-index userData for raycasting
          for (let j = 0; j < activeNodes.length; j++) {
            activeNodes[j].mesh.userData.index = j;
          }
        } else if (age > ACTIVE_NODE_TTL_MS - 3000) {
          // Fade out in the last 3s
          const fadeT = (ACTIVE_NODE_TTL_MS - age) / 3000;
          (activeNodes[i].glow.material as THREE.PointsMaterial).opacity = fadeT;
        }
      }

      // Layer 2 — dim baseline when build is actively thinking
      const dimTarget = $buildFocusActive ? 0.3 : 1.0;
      fineDustMat.opacity = 0.25 * dimTarget;

      // Phase 20: pulse decays toward baseline; every activity event re-spikes.
      // Decay 0.96 at 60fps ≈ 1.0s half-life. Multiplier 0.025 gives 8× speed
      // boost at peak — clearly visible rotation acceleration.
      helixPulse *= 0.96;
      const rotationSpeed = 0.003 + helixPulse * 0.025;
      group.rotation.y += rotationSpeed * (1.0 - focusIntensity * 0.95);

      fineDustMat.opacity = 0.25 * (1.0 - focusIntensity * 0.7);
      bokehMat.opacity = 0.05 * (1.0 - focusIntensity * 0.8);
      agentMat.opacity = 0.95 * (1.0 - focusIntensity * 0.8);

      dustGroup.rotation.y -= 0.0002 * (1.0 - focusIntensity * 0.9);
      dustGroup.rotation.x += 0.0001 * (1.0 - focusIntensity * 0.9);

      const bPos = bokehSystem.geometry.attributes.position.array as Float32Array;
      for (let i = 0; i < bokehCount; i++) { bPos[i*3+1] += 0.001; if (bPos[i*3+1] > 10) bPos[i*3+1] = -10; }
      bokehSystem.geometry.attributes.position.needsUpdate = true;

      // Agent animation
      const aPos = agentGeo.attributes.position.array as Float32Array;
      const aCol = agentGeo.attributes.color.array as Float32Array;
      const dt = 0.016;
      for (let i = 0; i < agentCount; i++) {
        const ad = agentData[i];
        if (ad.state === 'TRAVERSE') {
          const currentSpeed = ad.speed * (0.5 + Math.sin(time * 5 + i) * 0.5);
          ad.y += currentSpeed * 0.01;
          if (ad.y > tMax) ad.y = tMin;
          if (ad.y < tMin) ad.y = tMax;
          const fadeY = getFade(ad.y);
          const pos = getMiniAnchorFrame(ad.entityIdx, ad.mIdx, ad.y).M;
          aPos[i*3] = pos.x; aPos[i*3+1] = pos.y; aPos[i*3+2] = pos.z;
          aCol[i*3] = Math.min(ad.color.r * 1.3, 1.0) * fadeY;
          aCol[i*3+1] = Math.min(ad.color.g * 1.3, 1.0) * fadeY;
          aCol[i*3+2] = Math.min(ad.color.b * 1.3, 1.0) * fadeY;
          const hitChance = ad.retrievalMode === 'KeywordDominated' ? 0.002 : ad.retrievalMode === 'GraphWeighted' ? 0.015 : 0.005;
          if (Math.random() < hitChance) { ad.state = 'HIT'; ad.stateTimer = 0; }
        } else if (ad.state === 'HIT') {
          ad.stateTimer += dt;
          const fadeY = getFade(ad.y);
          aCol[i*3] = Math.min(ad.color.r * 1.5, 1.0) * fadeY;
          aCol[i*3+1] = Math.min(ad.color.g * 1.5, 1.0) * fadeY;
          aCol[i*3+2] = Math.min(ad.color.b * 1.5, 1.0) * fadeY;
          if (ad.stateTimer > 0.3) {
            const jumpChance = ad.retrievalMode === 'KeywordDominated' ? 0.0 : ad.retrievalMode === 'GraphWeighted' ? 0.8 : 0.3;
            if (Math.random() < jumpChance) {
              ad.state = 'JUMP'; ad.stateTimer = 0;
              ad.oldColor.copy(ad.color);
              ad.jumpStart.set(aPos[i*3], aPos[i*3+1], aPos[i*3+2]);
              ad.entityIdx = Math.floor(Math.random() * 6);
              ad.color = entityColors[ad.entityIdx].clone();
              ad.mIdx = Math.floor(Math.random() * entities[ad.entityIdx].strands);
              ad.jumpEnd.copy(getMiniAnchorFrame(ad.entityIdx, ad.mIdx, ad.y).M);
              ad.jumpControl.set(0, ad.y, 0);
            } else { ad.state = 'TRAVERSE'; }
          }
        } else if (ad.state === 'JUMP') {
          ad.stateTimer += dt;
          const progress = Math.min(ad.stateTimer / 0.4, 1.0), inv = 1 - progress;
          const curY = inv*inv*ad.jumpStart.y + 2*inv*progress*ad.jumpControl.y + progress*progress*ad.jumpEnd.y;
          const fadeY = getFade(curY);
          aPos[i*3] = inv*inv*ad.jumpStart.x + 2*inv*progress*ad.jumpControl.x + progress*progress*ad.jumpEnd.x;
          aPos[i*3+1] = curY;
          aPos[i*3+2] = inv*inv*ad.jumpStart.z + 2*inv*progress*ad.jumpControl.z + progress*progress*ad.jumpEnd.z;
          aCol[i*3] = (ad.oldColor.r*inv + ad.color.r*progress) * fadeY;
          aCol[i*3+1] = (ad.oldColor.g*inv + ad.color.g*progress) * fadeY;
          aCol[i*3+2] = (ad.oldColor.b*inv + ad.color.b*progress) * fadeY;
          if (progress >= 1.0) ad.state = 'TRAVERSE';
        }
      }
      agentGeo.attributes.position.needsUpdate = true;
      agentGeo.attributes.color.needsUpdate = true;

      polytopeManager.setGlobalVisibility(1.0);
      polytopeManager.update(time, camera);
      interaction.update();

      const bob = Math.sin(time * (Math.PI * 2 / 10)) * 0.1;
      group.position.y = currentGroupY.value + bob;
      outerPolytopeGroup.position.y = currentGroupY.value + bob;

      convergenceNodes.forEach(node => {
        const positions = node.particles.geometry.attributes.position.array as Float32Array;
        for (let i = 0; i < 8; i++) {
          node.pRadii[i] += 0.002;
          if (node.pRadii[i] > 0.5) { node.pRadii[i] = 0; node.pAngles[i] = Math.random() * Math.PI * 2; }
          const rr = node.pRadii[i], a = node.pAngles[i];
          positions[i*3] = node.center.x + rr * Math.cos(a);
          positions[i*3+1] = node.center.y + rr * Math.sin(time + a) * 0.5;
          positions[i*3+2] = node.center.z + rr * Math.sin(a);
        }
        node.particles.geometry.attributes.position.needsUpdate = true;
      });

      group.rotation.x += (cameraControl.targetRotX - group.rotation.x) * 0.03;
      group.rotation.z += (cameraControl.targetRotZ - group.rotation.z) * 0.03;
      camera.position.x += (cameraControl.targetX - camera.position.x) * cameraControl.lerpSpeed;
      camera.position.y += (cameraControl.targetY - camera.position.y) * cameraControl.lerpSpeed;
      camera.position.z += (cameraControl.targetZ - camera.position.z) * cameraControl.lerpSpeed;
      camera.lookAt(cameraControl.lookAt);

      composer.render();
    };
    animate();

    const obs = new ResizeObserver(() => {
      const w = container.clientWidth, h = container.clientHeight;
      if (w <= 0 || h <= 0) return;
      camera.aspect = w / h;
      camera.updateProjectionMatrix();
      renderer.setSize(w, h);
      composer.setSize(w, h);
    });
    obs.observe(container);

    return () => {
      obs.disconnect();
      window.removeEventListener('mousemove', handleMouseMove);
      cancelAnimationFrame(animationFrameId);
      interaction.dispose();
      polytopeManager.dispose();
      // Dispose active session nodes
      for (const n of activeNodes) {
        n.mesh.geometry.dispose();
        (n.mesh.material as THREE.Material).dispose();
        n.glow.geometry.dispose();
        (n.glow.material as THREE.Material).dispose();
      }
      activeNodes.length = 0;
      activeHelixNode.set(null);
      renderer.dispose();
      glowTexture.dispose();
      fineDustGeom.dispose();
      fineDustMat.dispose();
      bokehGeom.dispose();
      bokehMat.dispose();
      agentGeo.dispose();
      agentMat.dispose();
    };
  });
</script>

<div style="width:100%;height:100%;background:#000;position:relative;box-shadow: -30px 0 60px rgba(255,215,0,0.05), -15px 0 30px rgba(255,20,147,0.03);">
  <!-- Three.js renderer mounts its <canvas> into this bound div. Keep it
       sibling to the overlays rather than a parent so Svelte's {#if} block
       DOM reconciliation doesn't get confused by Three.js DOM mutations. -->
  <div bind:this={container} style="position:absolute;inset:0;"></div>

  {#if $helixEntries.length > 0}
    <div
      class="absolute top-2 right-2 flex items-center gap-1.5 px-2 py-1 rounded-full
             bg-[#FFD700]/20 border border-[#FFD700] pointer-events-none
             animate-pulse z-10"
      data-testid="helix-orb-pulse"
      data-orb-count={$helixEntries.length}
      data-pulse-key={pulseKey}
    >
      <span class="w-1.5 h-1.5 rounded-full bg-[#FFD700]"></span>
      <span class="text-[9px] font-mono text-[#FFD700]">+{$helixEntries.length}</span>
    </div>
  {/if}

  <!-- Phase 10.8 — promotion lineage pulses (last 12, 3s TTL each) -->
  {#if lineage.length > 0}
    <div
      class="absolute top-10 right-2 flex flex-col gap-1 pointer-events-none max-w-[220px] z-10"
      data-testid="helix-lineage"
      data-lineage-count={lineage.length}
    >
      {#each lineage as pulse (pulse.id)}
        <div
          class="flex items-center gap-1.5 px-1.5 py-0.5 rounded border border-[#FFD700]/40
                 bg-[#FFD700]/10 text-[9px] font-mono text-[#FFD700]/80"
        >
          <span class="w-1 h-1 rounded-full bg-[#FFD700] animate-ping"></span>
          <span>hot→cold</span>
          <span class="text-[#FFD700]/70">{pulse.sibling}</span>
        </div>
      {/each}
    </div>
  {/if}
</div>
