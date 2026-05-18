<script lang="ts">
  import * as THREE from 'three';
  import { step, setupComplete } from '$lib/setup';
  import { api } from '$lib/api';
  import PreflightPanel from '../../components/PreflightPanel.svelte';
  import type { PreflightReport } from '$lib/types';

  const subsystems = [
    { polytope: 'Pentachoron', sibling: 'QUANTUM' },
    { polytope: 'Tesseract', sibling: 'AYIN' },
    { polytope: 'Hexadecachoron', sibling: 'CORSO' },
    { polytope: 'Icositetrachoron', sibling: 'LÆX' },
    { polytope: 'Rectified 5-Cell', sibling: 'EVA' },
    { polytope: 'Duoprism', sibling: 'SERAPH' },
  ];

  let activeIdx = $state(0);
  let doneIdxs = $state<number[]>([]);

  // Preflight gate — fetch before starting the subsystem tick animation.
  let preflightReport = $state<PreflightReport | null>(null);
  // Degraded: show panel but allow operator to continue; Blocked: panel shown, tick paused.
  let preflightLoading = $state(false);
  let preflightDismissed = $state(false);

  async function fetchPreflight() {
    preflightLoading = true;
    try {
      preflightReport = await api.fetchPreflight();
    } catch {
      // Network error — treat as unknown; don't block startup.
      preflightReport = null;
    }
    preflightLoading = false;
  }

  async function handleRefresh() {
    preflightLoading = true;
    try {
      preflightReport = await api.refreshPreflight();
    } catch {
      preflightLoading = false;
    }
    preflightLoading = false;
  }

  $effect(() => {
    void fetchPreflight();
  });

  // Tick animation only starts once preflight is resolved AND not Blocked
  // (or the operator has dismissed the panel after a Degraded result).
  let canTick = $derived(
    preflightReport !== null &&
    (preflightReport.overall !== 'Blocked' || preflightDismissed)
  );

  $effect(() => {
    if (!canTick) return;
    let idx = 0;
    const tick = () => {
      if (idx >= subsystems.length) {
        setupComplete.set(true);
        step.set('done');
        return;
      }
      activeIdx = idx;
      setTimeout(() => {
        doneIdxs = [...doneIdxs, idx];
        idx++;
        setTimeout(tick, 80);
      }, 300);
    };
    setTimeout(tick, 200);
  });

  // ── 600-cell polytope background (same as SplashStep) ──────────────────────
  let canvas: HTMLCanvasElement;

  function gen600Cell() {
    const phi = (1 + Math.sqrt(5)) / 2;
    const verts: number[][] = [];
    for (const s of [-1, 1]) {
      verts.push([s,0,0,0]); verts.push([0,s,0,0]);
      verts.push([0,0,s,0]); verts.push([0,0,0,s]);
    }
    for (const a of [-1,1]) for (const b of [-1,1]) for (const c of [-1,1]) for (const d of [-1,1])
      verts.push([a/2, b/2, c/2, d/2]);
    const evenPerms4 = [
      [0,1,2,3],[0,2,3,1],[0,3,1,2],[1,0,3,2],[1,2,0,3],[1,3,2,0],
      [2,0,1,3],[2,1,3,0],[2,3,0,1],[3,0,2,1],[3,1,0,2],[3,2,1,0],
    ];
    const base = [0, 1/(2*phi), 0.5, phi/2];
    for (const perm of evenPerms4) {
      const p = perm.map(i => base[i]);
      for (const s1 of [-1,1]) for (const s2 of [-1,1]) for (const s3 of [-1,1]) {
        let si = 0;
        verts.push(p.map(x => x === 0 ? 0 : x * [s1,s2,s3][si++]));
      }
    }
    let minDist2 = Infinity;
    const n = verts.length;
    for (let i = 0; i < n; i++) for (let j = i+1; j < n; j++) {
      let d2 = 0; for (let k = 0; k < 4; k++) d2 += (verts[i][k]-verts[j][k])**2;
      if (d2 < minDist2 - 1e-6) minDist2 = d2;
    }
    const edges: [number,number][] = [];
    const tol = minDist2 * 1.02;
    for (let i = 0; i < n; i++) for (let j = i+1; j < n; j++) {
      let d2 = 0; for (let k = 0; k < 4; k++) d2 += (verts[i][k]-verts[j][k])**2;
      if (d2 <= tol) edges.push([i, j]);
    }
    return { verts, edges };
  }

  function rotZW(v: number[], a: number) { return [v[0], v[1], v[2]*Math.cos(a)-v[3]*Math.sin(a), v[2]*Math.sin(a)+v[3]*Math.cos(a)]; }
  function rotXW(v: number[], a: number) { return [v[0]*Math.cos(a)-v[3]*Math.sin(a), v[1], v[2], v[0]*Math.sin(a)+v[3]*Math.cos(a)]; }
  function rotYW(v: number[], a: number) { return [v[0], v[1]*Math.cos(a)-v[3]*Math.sin(a), v[2], v[1]*Math.sin(a)+v[3]*Math.cos(a)]; }
  function proj4(v: number[]): THREE.Vector3 {
    const d = 2.0, w = Math.max(d - v[3], 0.35), s = 2.4 / w;
    return new THREE.Vector3(v[0]*s, v[1]*s, v[2]*s);
  }

  const { verts: baseVerts, edges } = gen600Cell();

  $effect(() => {
    if (!canvas) return;
    const w = window.innerWidth, h = window.innerHeight;
    const scene = new THREE.Scene();
    const camera = new THREE.PerspectiveCamera(55, w/h, 0.1, 100);
    camera.position.set(0, 0, 4);
    camera.lookAt(0, -0.5, 0);
    const renderer = new THREE.WebGLRenderer({ canvas, alpha: true, antialias: true });
    renderer.setSize(w, h);
    renderer.setPixelRatio(Math.min(devicePixelRatio, 2));
    renderer.setClearColor(0x000000, 0);

    const positions = new Float32Array(edges.length * 6);
    const geo = new THREE.BufferGeometry();
    geo.setAttribute('position', new THREE.BufferAttribute(positions, 3));
    const mat = new THREE.LineBasicMaterial({ color: 0xff6600, transparent: true, opacity: 0.75, blending: THREE.AdditiveBlending, depthWrite: false });
    const lineSegs = new THREE.LineSegments(geo, mat);
    scene.add(lineSegs);

    const posGlow = new Float32Array(edges.length * 6);
    const geoGlow = new THREE.BufferGeometry();
    geoGlow.setAttribute('position', new THREE.BufferAttribute(posGlow, 3));
    const matGlow = new THREE.LineBasicMaterial({ color: 0x00d26a, transparent: true, opacity: 0.18, blending: THREE.AdditiveBlending, depthWrite: false });
    const lineSegsGlow = new THREE.LineSegments(geoGlow, matGlow);
    lineSegsGlow.scale.setScalar(1.05);
    scene.add(lineSegsGlow);

    const nVerts = baseVerts.length;
    const nodeGeo = new THREE.BufferGeometry();
    const nodePos = new Float32Array(nVerts * 3);
    nodeGeo.setAttribute('position', new THREE.BufferAttribute(nodePos, 3));
    scene.add(new THREE.Points(nodeGeo, new THREE.PointsMaterial({ color: 0xffcc44, size: 0.045, transparent: true, opacity: 0.9, blending: THREE.AdditiveBlending, depthWrite: false })));

    const projected3D: THREE.Vector3[] = Array.from({ length: nVerts }, () => new THREE.Vector3());
    const timer = new THREE.Timer();
    let animId: number;

    function animate() {
      animId = requestAnimationFrame(animate);
      timer.update();
      const t = timer.getElapsed();
      for (let i = 0; i < nVerts; i++) {
        let v = rotZW(baseVerts[i], t * 0.42);
        v = rotXW(v, t * 0.25); v = rotYW(v, t * 0.12);
        projected3D[i].copy(proj4(v));
      }
      const posArr = lineSegs.geometry.attributes.position.array as Float32Array;
      let idx = 0;
      for (const [i, j] of edges) {
        posArr[idx++]=projected3D[i].x; posArr[idx++]=projected3D[i].y; posArr[idx++]=projected3D[i].z;
        posArr[idx++]=projected3D[j].x; posArr[idx++]=projected3D[j].y; posArr[idx++]=projected3D[j].z;
      }
      lineSegs.geometry.attributes.position.needsUpdate = true;
      posGlow.set(posArr);
      lineSegsGlow.geometry.attributes.position.needsUpdate = true;
      for (let i = 0; i < nVerts; i++) { nodePos[i*3]=projected3D[i].x; nodePos[i*3+1]=projected3D[i].y; nodePos[i*3+2]=projected3D[i].z; }
      nodeGeo.attributes.position.needsUpdate = true;
      mat.opacity = 0.60 + Math.sin(t * 1.4) * 0.22;
      matGlow.opacity = 0.12 + Math.sin(t * 1.4 + Math.PI) * 0.08;
      renderer.render(scene, camera);
    }
    animate();

    return () => {
      cancelAnimationFrame(animId);
      renderer.dispose();
    };
  });
</script>

<div class="step">
  <canvas bind:this={canvas} class="polytope-canvas"></canvas>
  <div class="scanlines"></div>

  <div class="content">
    <h2 class="title">Initialising Subsystems</h2>

    {#if preflightReport !== null && preflightReport.overall !== 'Ready'}
      <div class="preflight-wrapper">
        <PreflightPanel
          report={preflightReport}
          loading={preflightLoading}
          onRefresh={handleRefresh}
        />
        {#if preflightReport.overall === 'Degraded' && !preflightDismissed}
          <button class="continue-btn" onclick={() => preflightDismissed = true}>
            Continue anyway
          </button>
        {/if}
      </div>
    {:else if preflightLoading && preflightReport === null}
      <p class="preflight-checking">Checking infrastructure…</p>
    {/if}

    <div class="checklist">
      {#each subsystems as s, i}
        <div class="row" class:active={activeIdx === i && !doneIdxs.includes(i)} class:done={doneIdxs.includes(i)}>
          <div class="indicator">
            {#if doneIdxs.includes(i)}
              <span class="checkmark">✓</span>
            {:else if activeIdx === i}
              <span class="spinner">◌</span>
            {:else}
              <span class="pending">○</span>
            {/if}
          </div>
          <div class="row-info">
            <span class="polytope-name">{s.polytope}</span>
            <span class="sibling-name">/ {s.sibling}</span>
          </div>
        </div>
      {/each}
    </div>
  </div>
</div>

<style>
  .step {
    position: fixed; inset: 0;
    background: radial-gradient(ellipse at center, #0a0a20 0%, #03030a 60%, #000000 100%);
    display: flex; align-items: center; justify-content: center;
  }

  .polytope-canvas {
    position: absolute; inset: 0;
    width: 100% !important; height: 100% !important;
    pointer-events: none;
  }

  .scanlines {
    position: absolute; inset: 0; pointer-events: none;
    background: repeating-linear-gradient(0deg, transparent, transparent 2px, rgba(255,140,0,0.03) 2px, rgba(255,140,0,0.03) 3px);
    animation: scanlines-move 8s linear infinite;
  }
  @keyframes scanlines-move { from { background-position: 0 0; } to { background-position: 0 100px; } }

  .content {
    position: relative; z-index: 2;
    display: flex; flex-direction: column; align-items: center; gap: 2rem;
    padding: 2rem;
  }

  .preflight-wrapper { width:min(480px,90vw); display:flex; flex-direction:column; gap:0.75rem; }
  .preflight-checking { font-family:'IBM Plex Mono',monospace; font-size:0.75rem; color:#475569; }
  .continue-btn { align-self:flex-end; padding:0.35rem 0.85rem; border-radius:4px; border:1px solid #334155; background:transparent; color:#94a3b8; font-family:'IBM Plex Mono',monospace; font-size:0.7rem; cursor:pointer; transition:border-color 0.15s,color 0.15s; }
  .continue-btn:hover { border-color:#ff6600; color:#ff6600; }

  .title { font-family:'Raleway',sans-serif; font-size:1.75rem; font-weight:700; color:#e2e8f0; margin:0; letter-spacing:0.05em; }

  .checklist { display:flex; flex-direction:column; gap:0.6rem; width:320px; }
  .row { display:flex; align-items:center; gap:0.75rem; padding:0.5rem 0.75rem; border-radius:6px; transition:background 0.2s; }
  .row.active { background:rgba(255,102,0,0.08); }
  .row.done { opacity:0.6; }

  .indicator { width:1.2rem; text-align:center; font-size:1rem; }
  .checkmark { color:#00d26a; }
  .spinner { color:#ff6600; animation:spin 0.8s linear infinite; display:inline-block; }
  .pending { color:#334155; }
  @keyframes spin { from { transform:rotate(0deg); } to { transform:rotate(360deg); } }

  .row-info { display:flex; gap:0.5rem; align-items:baseline; }
  .polytope-name { font-family:'IBM Plex Mono',monospace; font-size:0.8rem; color:#94a3b8; }
  .sibling-name { font-family:'IBM Plex Mono',monospace; font-size:0.7rem; color:#475569; }
</style>
