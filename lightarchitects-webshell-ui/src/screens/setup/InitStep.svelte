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

  // ── 4D polytope background — morphs through each subsystem's polytope ────────
  let canvas: HTMLCanvasElement;

  type Poly = { verts: number[][], edges: [number,number][] };

  // SO(4) rotation operators (det=1, oracle-verified Leanstral 2026-05-18).
  function rotZW(v: number[], a: number) { return [v[0], v[1], v[2]*Math.cos(a)-v[3]*Math.sin(a), v[2]*Math.sin(a)+v[3]*Math.cos(a)]; }
  function rotXW(v: number[], a: number) { return [v[0]*Math.cos(a)-v[3]*Math.sin(a), v[1], v[2], v[0]*Math.sin(a)+v[3]*Math.cos(a)]; }
  function rotYW(v: number[], a: number) { return [v[0], v[1]*Math.cos(a)-v[3]*Math.sin(a), v[2], v[1]*Math.sin(a)+v[3]*Math.cos(a)]; }
  function proj4(v: number[]): THREE.Vector3 {
    const d = 2.2, w = Math.max(d - v[3], 0.3), s = 2.6 / w;
    return new THREE.Vector3(v[0]*s, v[1]*s, v[2]*s);
  }

  function simplex5cell(): Poly {
    // Project 5 standard-basis vectors of ℝ⁵ (minus centroid) to ℝ⁴ via Gram-Schmidt.
    const raw = Array.from({length:5}, (_,k) => Array.from({length:5}, (_,j) => (j===k?1:0)-0.2));
    const basis: number[][] = [];
    for (const v of raw) {
      if (basis.length === 4) break;
      let u = [...v];
      for (const b of basis) { const d = u.reduce((s,x,i)=>s+x*b[i],0); for (let i=0;i<5;i++) u[i]-=d*b[i]; }
      const len = Math.sqrt(u.reduce((s,x)=>s+x*x,0));
      if (len > 1e-10) basis.push(u.map(x=>x/len));
    }
    const verts = raw.map(v5 => {
      const v4 = basis.map(b => v5.reduce((s,x,i)=>s+x*b[i],0));
      const n = Math.sqrt(v4.reduce((s,x)=>s+x*x,0));
      return v4.map(x=>x/n);
    });
    const edges: [number,number][] = [];
    for (let i=0;i<5;i++) for (let j=i+1;j<5;j++) edges.push([i,j]);
    return { verts, edges };
  }

  function tesseract(): Poly {
    const verts: number[][] = [];
    for (const a of [-1,1]) for (const b of [-1,1]) for (const c of [-1,1]) for (const d of [-1,1])
      verts.push([a/2,b/2,c/2,d/2]);
    const edges: [number,number][] = [];
    for (let i=0;i<16;i++) for (let j=i+1;j<16;j++) {
      let diff=0; for (let k=0;k<4;k++) if (verts[i][k]!==verts[j][k]) diff++;
      if (diff===1) edges.push([i,j]);
    }
    return { verts, edges };
  }

  function hexadecachoron(): Poly {
    const verts: number[][] = [[1,0,0,0],[-1,0,0,0],[0,1,0,0],[0,-1,0,0],[0,0,1,0],[0,0,-1,0],[0,0,0,1],[0,0,0,-1]];
    const edges: [number,number][] = [];
    for (let i=0;i<8;i++) for (let j=i+1;j<8;j++) {
      let d2=0; for (let k=0;k<4;k++) d2+=(verts[i][k]-verts[j][k])**2;
      if (Math.abs(d2-2)<0.01) edges.push([i,j]);
    }
    return { verts, edges };
  }

  function icositetrachoron(): Poly {
    const verts: number[][] = [];
    for (let i=0;i<4;i++) for (let j=i+1;j<4;j++) for (const si of [-1,1]) for (const sj of [-1,1]) {
      const v=[0,0,0,0]; v[i]=si/Math.SQRT2; v[j]=sj/Math.SQRT2; verts.push(v);
    }
    const edges: [number,number][] = [];
    for (let i=0;i<verts.length;i++) for (let j=i+1;j<verts.length;j++) {
      let d2=0; for (let k=0;k<4;k++) d2+=(verts[i][k]-verts[j][k])**2;
      if (Math.abs(d2-1)<0.01) edges.push([i,j]);
    }
    return { verts, edges };
  }

  function rectified5cell(): Poly {
    const { verts: v5 } = simplex5cell();
    const verts: number[][] = [];
    const pairs: [number,number][] = [];
    for (let i=0;i<5;i++) for (let j=i+1;j<5;j++) {
      const mid = v5[i].map((x,k)=>(x+v5[j][k])/2);
      const n = Math.sqrt(mid.reduce((s,x)=>s+x*x,0));
      verts.push(mid.map(x=>x/n));
      pairs.push([i,j]);
    }
    const edges: [number,number][] = [];
    for (let i=0;i<10;i++) for (let j=i+1;j<10;j++) {
      const [a,b]=pairs[i],[c,d]=pairs[j];
      if (a===c||a===d||b===c||b===d) edges.push([i,j]);
    }
    return { verts, edges };
  }

  function duoprism55(): Poly {
    const N=5, verts: number[][] = [], edges: [number,number][] = [];
    for (let i=0;i<N;i++) for (let j=0;j<N;j++) {
      const ai=2*Math.PI*i/N, aj=2*Math.PI*j/N;
      verts.push([Math.cos(ai)/Math.SQRT2, Math.sin(ai)/Math.SQRT2, Math.cos(aj)/Math.SQRT2, Math.sin(aj)/Math.SQRT2]);
    }
    for (let i=0;i<N;i++) for (let j=0;j<N;j++) {
      edges.push([i*N+j, ((i+1)%N)*N+j]);
      edges.push([i*N+j, i*N+((j+1)%N)]);
    }
    return { verts, edges };
  }

  // One polytope per subsystem — named to match the checklist.
  const polytopes: Poly[] = [
    simplex5cell(),      // Pentachoron  / QUANTUM
    tesseract(),         // Tesseract    / AYIN
    hexadecachoron(),    // Hexadecachoron / CORSO
    icositetrachoron(),  // Icositetrachoron / LÆX
    rectified5cell(),    // Rectified 5-Cell / EVA
    duoprism55(),        // Duoprism     / SERAPH
  ];

  // Plain object shared between the reactive watcher and the animation loop.
  // Not $state — the animation loop reads it every frame without creating a dependency.
  const morphState = { target: 0, alpha: 1.0 };

  $effect(() => {
    // Advance polytope when a subsystem completes.
    const next = Math.min(doneIdxs.length, polytopes.length - 1);
    if (next !== morphState.target) { morphState.target = next; morphState.alpha = 0.0; }
  });

  $effect(() => {
    if (!canvas) return;
    const w = window.innerWidth, h = window.innerHeight;
    const scene = new THREE.Scene();
    const camera = new THREE.PerspectiveCamera(55, w/h, 0.1, 100);
    camera.position.set(0, 0, 4.5);
    const renderer = new THREE.WebGLRenderer({ canvas, alpha: true, antialias: true });
    renderer.setSize(w, h);
    renderer.setPixelRatio(Math.min(devicePixelRatio, 2));
    renderer.setClearColor(0x000000, 0);

    const maxE = Math.max(...polytopes.map(p=>p.edges.length));
    const maxV = Math.max(...polytopes.map(p=>p.verts.length));

    // Two draw sets for crossfade (A = fading out, B = fading in).
    const mkSet = (opacity: number) => {
      const pos = new Float32Array(maxE * 6);
      const geo = new THREE.BufferGeometry();
      geo.setAttribute('position', new THREE.BufferAttribute(pos, 3));
      const mat = new THREE.LineBasicMaterial({ color: 0xff6600, transparent: true, opacity, blending: THREE.AdditiveBlending, depthWrite: false });
      const matG = new THREE.LineBasicMaterial({ color: 0x00d26a, transparent: true, opacity: opacity*0.22, blending: THREE.AdditiveBlending, depthWrite: false });
      const geoG = new THREE.BufferGeometry();
      geoG.setAttribute('position', new THREE.BufferAttribute(new Float32Array(maxE*6), 3));
      const segs = new THREE.LineSegments(geo, mat); segs.scale.setScalar(1); scene.add(segs);
      const glowSegs = new THREE.LineSegments(geoG, matG); glowSegs.scale.setScalar(1.04); scene.add(glowSegs);
      return { pos, geo, mat, matG, geoG };
    };
    const setA = mkSet(0.75);
    const setB = mkSet(0.0);

    const nodePos = new Float32Array(maxV * 3);
    const nodeGeo = new THREE.BufferGeometry();
    nodeGeo.setAttribute('position', new THREE.BufferAttribute(nodePos, 3));
    const nodeMat = new THREE.PointsMaterial({ color: 0xffcc44, size: 0.055, transparent: true, opacity: 0.9, blending: THREE.AdditiveBlending, depthWrite: false });
    scene.add(new THREE.Points(nodeGeo, nodeMat));

    // Per-polytope projected vertex cache.
    const projCache = polytopes.map(p => p.verts.map(() => new THREE.Vector3()));

    const writeSet = (set: typeof setA, polyIdx: number) => {
      const { edges } = polytopes[polyIdx];
      const proj = projCache[polyIdx];
      for (let i=0;i<edges.length;i++) {
        const [a,b]=edges[i];
        set.pos[i*6  ]=proj[a].x; set.pos[i*6+1]=proj[a].y; set.pos[i*6+2]=proj[a].z;
        set.pos[i*6+3]=proj[b].x; set.pos[i*6+4]=proj[b].y; set.pos[i*6+5]=proj[b].z;
      }
      set.geo.setDrawRange(0, edges.length*2);
      set.geo.attributes.position.needsUpdate = true;
      (set.geoG.attributes.position.array as Float32Array).set(set.pos);
      set.geoG.setDrawRange(0, edges.length*2);
      set.geoG.attributes.position.needsUpdate = true;
    };

    let currentIdx = 0;
    let targetIdx  = 0;
    const timer = new THREE.Timer();
    let prevT = 0;
    let animId: number;

    function animate() {
      animId = requestAnimationFrame(animate);
      timer.update();
      const t = timer.getElapsed();
      const dt = t - prevT; prevT = t;

      // Advance morph.
      if (morphState.target !== targetIdx) { targetIdx = morphState.target; morphState.alpha = 0.0; }
      if (morphState.alpha < 1.0) {
        morphState.alpha = Math.min(1.0, morphState.alpha + dt / 1.4);
        if (morphState.alpha >= 1.0) { currentIdx = targetIdx; morphState.alpha = 1.0; }
      }
      const ease = morphState.alpha < 1.0
        ? morphState.alpha * morphState.alpha * (3 - 2 * morphState.alpha)  // smoothstep
        : 1.0;

      // Project all needed polytopes under the current 4D rotation.
      const needed = new Set([currentIdx, targetIdx]);
      for (const idx of needed) {
        const { verts } = polytopes[idx];
        const proj = projCache[idx];
        for (let i=0;i<verts.length;i++) {
          let v = rotZW(verts[i], t*0.42);
          v = rotXW(v, t*0.25); v = rotYW(v, t*0.12);
          proj[i].copy(proj4(v));
        }
      }

      if (ease < 1.0) {
        writeSet(setA, currentIdx);
        writeSet(setB, targetIdx);
        const pulse = 0.60 + Math.sin(t*1.4)*0.22;
        setA.mat.opacity = pulse * (1 - ease);
        setA.matG.opacity = pulse * (1 - ease) * 0.22;
        setB.mat.opacity = pulse * ease;
        setB.matG.opacity = pulse * ease * 0.22;
      } else {
        writeSet(setA, currentIdx);
        const pulse = 0.60 + Math.sin(t*1.4)*0.22;
        setA.mat.opacity = pulse;
        setA.matG.opacity = pulse * 0.22;
        setB.mat.opacity = 0; setB.matG.opacity = 0;
      }

      // Nodes from current polytope.
      const nv = polytopes[currentIdx].verts.length;
      const proj = projCache[currentIdx];
      for (let i=0;i<nv;i++) { nodePos[i*3]=proj[i].x; nodePos[i*3+1]=proj[i].y; nodePos[i*3+2]=proj[i].z; }
      nodeGeo.setDrawRange(0, nv);
      nodeGeo.attributes.position.needsUpdate = true;

      renderer.render(scene, camera);
    }
    animate();

    return () => { cancelAnimationFrame(animId); renderer.dispose(); };
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
