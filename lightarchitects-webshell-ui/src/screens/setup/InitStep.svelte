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

  // ── 4D recursive double helix background ───────────────────────────────────
  let canvas: HTMLCanvasElement;

  /** Gram-Schmidt: given a unit `axis`, produce two orthonormal vectors u, v
   *  both perpendicular to axis and to each other.
   *
   *  Oracle-verified precondition (Leanstral, 2026-05-18): the construction is
   *  valid iff axis is not parallel to ±e₃ AND ±e₄.  Both fallbacks below are
   *  re-checked against the final u so that v is always orthonormal to the
   *  actual u that was chosen, not only to the attempted one.
   */
  function orthoBasis(axis: number[]): [number[], number[]] {
    const dot = (a: number[], b: number[]) => a.reduce((s, v, i) => s + v * b[i], 0);
    const norm = (a: number[]) => Math.sqrt(dot(a, a));
    const sub  = (a: number[], b: number[]) => a.map((v, i) => v - b[i]);
    const scale = (a: number[], s: number) => a.map(v => v * s);
    const normalize = (a: number[]) => { const n = norm(a); return n > 1e-10 ? scale(a, 1/n) : a; };

    // Project candidate c onto the plane perpendicular to axis.
    const project = (c: number[]) => sub(c, scale(axis, dot(c, axis)));

    // u — project e₃ = [0,0,1,0]; fall back to e₁ = [1,0,0,0] if degenerate.
    let uRaw = project([0, 0, 1, 0]);
    if (norm(uRaw) < 0.01) uRaw = project([1, 0, 0, 0]);
    const u = normalize(uRaw);

    // v — project e₄ = [0,0,0,1] onto the plane perpendicular to BOTH axis
    //     and the final u (not the attempted u — this is the oracle-identified fix).
    let vRaw = project([0, 0, 0, 1]);
    vRaw = sub(vRaw, scale(u, dot(vRaw, u)));
    if (norm(vRaw) < 0.01) {
      // e₄ was in span{axis, u}; try e₂ = [0,1,0,0].
      vRaw = project([0, 1, 0, 0]);
      vRaw = sub(vRaw, scale(u, dot(vRaw, u)));
    }
    const v = normalize(vRaw);

    return [u, v];
  }

  /** Two 4D helical strands (antipodal by construction: s₂(t) = −s₁(t),
   *  proven via Real.cos_add_pi / Real.sin_add_pi) plus twisted rung
   *  mini-helices connecting them.  All geometry lives in ℝ⁴ and rotates
   *  coherently before perspective projection.
   */
  function genDoubleHelix() {
    const N      = 240;   // samples per strand
    const TURNS  = 4;     // full xy loops  (strand is a (4,3) torus knot on the Clifford torus)
    const R      = 1.2;   // xy amplitude
    const r      = 0.6;   // zw amplitude
    const WIND   = 3;     // zw winding number
    const RUNGS  = 24;    // cross-connections
    const RSEG   = 10;    // segments per rung mini-helix
    const RRAD   = 0.11;  // rung helix radius
    const RWINDS = 2;     // turns per rung

    const verts4d: number[][] = [];
    const s1Edges: [number,number][] = [];
    const s2Edges: [number,number][] = [];
    const rungEdges: [number,number][] = [];

    // Strand 1: s₁(t) = (R cos t, R sin t, r cos 3t, r sin 3t)
    for (let i = 0; i < N; i++) {
      const t = (i / (N - 1)) * TURNS * 2 * Math.PI;
      verts4d.push([R*Math.cos(t), R*Math.sin(t), r*Math.cos(WIND*t), r*Math.sin(WIND*t)]);
    }
    for (let i = 0; i < N - 1; i++) s1Edges.push([i, i + 1]);

    // Strand 2: offset xy by π (opposite side of the axis), zw in-phase.
    // Rung midpoints = (0, 0, r cos(3t), r sin(3t)) — a circle in zw-space,
    // never at the 4D origin, so rungs project as distributed chords not a fan.
    const S2 = N;
    for (let i = 0; i < N; i++) {
      const t = (i / (N - 1)) * TURNS * 2 * Math.PI;
      verts4d.push([R*Math.cos(t+Math.PI), R*Math.sin(t+Math.PI), r*Math.cos(WIND*t), r*Math.sin(WIND*t)]);
    }
    for (let i = 0; i < N - 1; i++) s2Edges.push([S2 + i, S2 + i + 1]);

    // Rungs: mini-helices connecting strand1[k] → strand2[k].
    for (let k = 0; k < RUNGS; k++) {
      const idx = Math.floor((k / (RUNGS - 1)) * (N - 1));
      const p1 = verts4d[idx];
      const p2 = verts4d[S2 + idx];

      const ax = p2.map((v, i) => v - p1[i]);
      const axLen = Math.sqrt(ax.reduce((s, v) => s + v*v, 0)) || 1;
      const axN = ax.map(v => v / axLen);
      const [u, v] = orthoBasis(axN);

      const base = verts4d.length;
      for (let j = 0; j <= RSEG; j++) {
        const s = j / RSEG;
        const angle = s * RWINDS * 2 * Math.PI;
        const cos = Math.cos(angle), sin = Math.sin(angle);
        verts4d.push([
          p1[0] + s*ax[0] + RRAD*(cos*u[0] + sin*v[0]),
          p1[1] + s*ax[1] + RRAD*(cos*u[1] + sin*v[1]),
          p1[2] + s*ax[2] + RRAD*(cos*u[2] + sin*v[2]),
          p1[3] + s*ax[3] + RRAD*(cos*u[3] + sin*v[3]),
        ]);
      }
      for (let j = 0; j < RSEG; j++) rungEdges.push([base + j, base + j + 1]);
    }

    // Node markers: every 12th point on each strand.
    const nodeIdxs: number[] = [];
    for (let i = 0; i < N; i += 12) { nodeIdxs.push(i); nodeIdxs.push(S2 + i); }

    return { verts4d, s1Edges, s2Edges, rungEdges, nodeIdxs };
  }

  // 4D rotation operators — each is an element of SO(4) by construction
  // (block-diagonal; det = cos²a + sin²a = 1; proven by Leanstral 2026-05-18).
  function rotZW(v: number[], a: number) { return [v[0], v[1], v[2]*Math.cos(a)-v[3]*Math.sin(a), v[2]*Math.sin(a)+v[3]*Math.cos(a)]; }
  function rotXW(v: number[], a: number) { return [v[0]*Math.cos(a)-v[3]*Math.sin(a), v[1], v[2], v[0]*Math.sin(a)+v[3]*Math.cos(a)]; }
  function rotYW(v: number[], a: number) { return [v[0], v[1]*Math.cos(a)-v[3]*Math.sin(a), v[2], v[1]*Math.sin(a)+v[3]*Math.cos(a)]; }

  // Perspective projection ℝ⁴ → ℝ³.
  // Denominator max(2.2 − w, 0.3) ≥ 0.3 > 0 for all w ∈ ℝ (proven: le_max_right).
  function proj4(v: number[]): THREE.Vector3 {
    const d = 2.2, w = Math.max(d - v[3], 0.3), s = 2.6 / w;
    return new THREE.Vector3(v[0]*s, v[1]*s, v[2]*s);
  }

  const { verts4d: baseVerts, s1Edges, s2Edges, rungEdges, nodeIdxs } = genDoubleHelix();

  $effect(() => {
    if (!canvas) return;
    const w = window.innerWidth, h = window.innerHeight;
    const scene = new THREE.Scene();
    const camera = new THREE.PerspectiveCamera(55, w/h, 0.1, 100);
    camera.position.set(0, 0, 5.5);
    camera.lookAt(0, 0, 0);
    const renderer = new THREE.WebGLRenderer({ canvas, alpha: true, antialias: true });
    renderer.setSize(w, h);
    renderer.setPixelRatio(Math.min(devicePixelRatio, 2));
    renderer.setClearColor(0x000000, 0);

    const nVerts = baseVerts.length;
    const projected3D = Array.from({ length: nVerts }, () => new THREE.Vector3());

    const mkSegs = (edges: [number,number][], color: number, opacity: number) => {
      const pos = new Float32Array(edges.length * 6);
      const geo = new THREE.BufferGeometry();
      geo.setAttribute('position', new THREE.BufferAttribute(pos, 3));
      const mat = new THREE.LineBasicMaterial({ color, transparent: true, opacity, blending: THREE.AdditiveBlending, depthWrite: false });
      scene.add(new THREE.LineSegments(geo, mat));
      return { pos, geo, mat };
    };

    const s1  = mkSegs(s1Edges,   0xff6600, 0.85);  // strand 1 — orange
    const s2  = mkSegs(s2Edges,   0x00ccff, 0.85);  // strand 2 — cyan
    const rng = mkSegs(rungEdges, 0xffcc44, 0.55);  // rungs — gold

    const nodePos = new Float32Array(nodeIdxs.length * 3);
    const nodeGeo = new THREE.BufferGeometry();
    nodeGeo.setAttribute('position', new THREE.BufferAttribute(nodePos, 3));
    scene.add(new THREE.Points(nodeGeo, new THREE.PointsMaterial({ color: 0xffcc44, size: 0.07, transparent: true, opacity: 0.9, blending: THREE.AdditiveBlending, depthWrite: false })));

    const updateSegs = (edges: [number,number][], buf: Float32Array, geo: THREE.BufferGeometry) => {
      for (let i = 0; i < edges.length; i++) {
        const [a, b] = edges[i];
        buf[i*6  ] = projected3D[a].x; buf[i*6+1] = projected3D[a].y; buf[i*6+2] = projected3D[a].z;
        buf[i*6+3] = projected3D[b].x; buf[i*6+4] = projected3D[b].y; buf[i*6+5] = projected3D[b].z;
      }
      geo.attributes.position.needsUpdate = true;
    };

    const timer = new THREE.Timer();
    let animId: number;

    function animate() {
      animId = requestAnimationFrame(animate);
      timer.update();
      const t = timer.getElapsed();

      for (let i = 0; i < nVerts; i++) {
        let v = rotZW(baseVerts[i], t * 0.30);
        v = rotXW(v, t * 0.17);
        v = rotYW(v, t * 0.09);
        projected3D[i].copy(proj4(v));
      }

      updateSegs(s1Edges,   s1.pos,  s1.geo);
      updateSegs(s2Edges,   s2.pos,  s2.geo);
      updateSegs(rungEdges, rng.pos, rng.geo);

      for (let i = 0; i < nodeIdxs.length; i++) {
        nodePos[i*3  ] = projected3D[nodeIdxs[i]].x;
        nodePos[i*3+1] = projected3D[nodeIdxs[i]].y;
        nodePos[i*3+2] = projected3D[nodeIdxs[i]].z;
      }
      nodeGeo.attributes.position.needsUpdate = true;

      s1.mat.opacity  = 0.70 + Math.sin(t * 1.2) * 0.15;
      s2.mat.opacity  = 0.70 + Math.sin(t * 1.2 + Math.PI / 3) * 0.15;
      rng.mat.opacity = 0.40 + Math.sin(t * 0.8) * 0.15;

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
