<script lang="ts">
  import * as THREE from 'three';
  import { EffectComposer } from 'three/examples/jsm/postprocessing/EffectComposer.js';
  import { RenderPass } from 'three/examples/jsm/postprocessing/RenderPass.js';
  import { UnrealBloomPass } from 'three/examples/jsm/postprocessing/UnrealBloomPass.js';
  import { OutputPass } from 'three/examples/jsm/postprocessing/OutputPass.js';

  // ─── Types ────────────────────────────────────────────────────────────────

  type GateState = 'clean' | 'hitl_pending' | 'merge_ready' | 'failed' | 'writing' | 'ghost';
  type AgentDomain = 'engineer' | 'quality' | 'security' | 'ops' | 'researcher' | 'knowledge' | 'testing' | 'squad';

  interface Worktree {
    domain: AgentDomain;
    commitCount: number;
  }

  interface Branch {
    name: string;
    divergeCommit: number;
    commitCount: number;
    filesModified: number;
    gateState: GateState;
    isGhost: boolean;
    worktrees: Worktree[];
  }

  interface Repo {
    id: string;
    name: string;
    commitCount: number;
    fileCount: number;
    pos: THREE.Vector3;
    branches: Branch[];
  }

  // ─── Static seed — real repo data ────────────────────────────────────────
  // lightarchitects-sdk: 333 commits / 1726 files (git log --oneline | wc -l)
  // SOUL-DEV: 64 commits / 291 files
  // CORSO-DEV: 85 commits / 589 files

  const SEED_REPOS: Repo[] = [
    {
      id: 'sdk',
      name: 'lightarchitects-sdk',
      commitCount: 333,
      fileCount: 1726,
      pos: new THREE.Vector3(-7, 0, 0),
      branches: [
        {
          name: 'feat/acp-paused',
          divergeCommit: 295,
          commitCount: 24,
          filesModified: 142,
          gateState: 'ghost',
          isGhost: true,
          worktrees: [],
        },
        {
          name: 'feat/steady-wishing-sparkle',
          divergeCommit: 325,
          commitCount: 5,
          filesModified: 87,
          gateState: 'hitl_pending',
          isGhost: false,
          worktrees: [{ domain: 'engineer', commitCount: 3 }],
        },
        {
          name: 'fix/security-88-92',
          divergeCommit: 330,
          commitCount: 2,
          filesModified: 12,
          gateState: 'merge_ready',
          isGhost: false,
          worktrees: [],
        },
      ],
    },
    {
      id: 'soul',
      name: 'SOUL-DEV',
      commitCount: 64,
      fileCount: 291,
      pos: new THREE.Vector3(0, 0, 3),
      branches: [
        {
          name: 'feat/helix-of-helices-spec',
          divergeCommit: 52,
          commitCount: 12,
          filesModified: 45,
          gateState: 'clean',
          isGhost: false,
          worktrees: [
            { domain: 'researcher', commitCount: 2 },
            { domain: 'knowledge', commitCount: 1 },
          ],
        },
      ],
    },
    {
      id: 'corso',
      name: 'CORSO-DEV',
      commitCount: 85,
      fileCount: 589,
      pos: new THREE.Vector3(7, 0, 1),
      branches: [],
    },
  ];

  // ─── Scale constants ──────────────────────────────────────────────────────

  const COMMIT_H       = 0.028;  // scene Y units per commit
  const TRUNK_GIRTH_K  = 0.007;  // trunk radiusBot = sqrt(fileCount) * K
  const BRANCH_GIRTH_K = 0.0028; // branch radius = sqrt(filesModified) * K
  const WKTREE_R       = 0.042;  // worktree sub-branch radius
  const NODE_R         = 0.088;  // commit sphere radius

  // ─── Gate → color (GIT-2) ────────────────────────────────────────────────

  const GATE_COLORS: Record<GateState, number> = {
    clean:        0x22c55e,
    hitl_pending: 0xf59e0b,
    merge_ready:  0xffd700,
    failed:       0xef4444,
    writing:      0x00c8ff,
    ghost:        0x334155,
  };

  // ─── Agent domain → color (GIT-3) ────────────────────────────────────────

  const AGENT_COLORS: Record<AgentDomain, number> = {
    engineer:   0x4d8eff,
    quality:    0xa874ff,
    security:   0xff4d4d,
    ops:        0xff8e3c,
    researcher: 0x4dffe6,
    knowledge:  0xf5d440,
    testing:    0x4dff8e,
    squad:      0xff7eb6,
  };

  // ─── Component bindings ───────────────────────────────────────────────────

  let container: HTMLDivElement | undefined = $state();
  let overlayCanvas: HTMLCanvasElement | undefined = $state();

  // ─── Three.js forest scene (GIT-1) ────────────────────────────────────────

  $effect(() => {
    if (!container) return;

    const w = container.clientWidth || 400;
    const h = container.clientHeight || 300;

    const scene = new THREE.Scene();
    scene.background = new THREE.Color(0x020408);
    scene.fog = new THREE.FogExp2(0x020408, 0.012);

    // Elevated orbit camera
    const camera = new THREE.PerspectiveCamera(45, w / h, 0.1, 200);
    let sph = { theta: -0.4, phi: 0.70, r: 26 };

    function applySph() {
      camera.position.set(
        sph.r * Math.sin(sph.phi) * Math.sin(sph.theta),
        sph.r * Math.cos(sph.phi),
        sph.r * Math.sin(sph.phi) * Math.cos(sph.theta),
      );
      camera.lookAt(0, 4, 0);
    }
    applySph();

    const renderer = new THREE.WebGLRenderer({ antialias: true, powerPreference: 'high-performance' });
    renderer.setPixelRatio(Math.min(window.devicePixelRatio, 1.5));
    renderer.setSize(w, h);
    renderer.toneMapping = THREE.ReinhardToneMapping;
    renderer.toneMappingExposure = 1.4;
    while (container.firstChild) container.removeChild(container.firstChild);
    container.appendChild(renderer.domElement);

    // Holographic bloom pipeline (§19)
    const composer = new EffectComposer(renderer);
    composer.addPass(new RenderPass(scene, camera));
    const bloomPass = new UnrealBloomPass(new THREE.Vector2(w, h), 0.85, 0.35, 0.08);
    composer.addPass(bloomPass);
    composer.addPass(new OutputPass());

    scene.add(new THREE.AmbientLight(0x003355, 0.45));

    const toDispose: Array<THREE.BufferGeometry | THREE.Material> = [];

    // ── Material helpers ──────────────────────────────────────────────────

    function holoMat(color: number, emissiveInt = 0.45, opacity = 0.82): THREE.MeshStandardMaterial {
      const m = new THREE.MeshStandardMaterial({
        color,
        emissive: new THREE.Color(color),
        emissiveIntensity: emissiveInt,
        transparent: true,
        opacity,
        blending: THREE.AdditiveBlending,
        depthWrite: false,
        roughness: 0.4,
        metalness: 0.6,
      });
      toDispose.push(m);
      return m;
    }

    function wireMat(color: number, opacity = 0.28): THREE.LineBasicMaterial {
      const m = new THREE.LineBasicMaterial({
        color,
        transparent: true,
        opacity,
        blending: THREE.AdditiveBlending,
        depthWrite: false,
      });
      toDispose.push(m);
      return m;
    }

    function ghostMat(color: number): THREE.LineDashedMaterial {
      const m = new THREE.LineDashedMaterial({
        color,
        transparent: true,
        opacity: 0.2,
        dashSize: 0.14,
        gapSize: 0.09,
        blending: THREE.AdditiveBlending,
        depthWrite: false,
      });
      toDispose.push(m);
      return m;
    }

    // ── Trunk ─────────────────────────────────────────────────────────────

    function buildTrunk(repo: Repo, group: THREE.Group): { height: number; radiusBot: number } {
      const height    = repo.commitCount * COMMIT_H;
      const radiusBot = Math.sqrt(repo.fileCount) * TRUNK_GIRTH_K;
      const radiusTop = radiusBot * 0.38;

      const geo = new THREE.CylinderGeometry(radiusTop, radiusBot, height, 8, 1);
      geo.translate(0, height / 2, 0);
      toDispose.push(geo);
      group.add(new THREE.Mesh(geo, holoMat(0x0ea5e9, 0.22, 0.65)));

      const edges = new THREE.EdgesGeometry(geo);
      toDispose.push(edges);
      group.add(new THREE.LineSegments(edges, wireMat(0x38bdf8, 0.35)));

      // Ground ring at tree base
      const ring = new THREE.RingGeometry(radiusBot * 0.95, radiusBot * 1.4, 32);
      ring.rotateX(-Math.PI / 2);
      ring.translate(0, 0.005, 0);
      toDispose.push(ring);
      group.add(new THREE.Mesh(ring, holoMat(0x0ea5e9, 0.65, 0.7)));

      return { height, radiusBot };
    }

    // ── Branch (feat/<codename> or fix/*) ─────────────────────────────────

    function buildBranch(
      branch: Branch,
      branchIdx: number,
      repo: Repo,
      trunkH: number,
      trunkRad: number,
      group: THREE.Group,
    ) {
      const forkY     = (branch.divergeCommit / repo.commitCount) * trunkH;
      const branchLen = branch.commitCount * COMMIT_H;
      // Golden angle spacing keeps branches from overlapping
      const azimuth   = branchIdx * 2.399963;
      const lateralR  = Math.max(trunkRad * 4.5, 0.9) + branchIdx * 0.35;

      const dx = Math.cos(azimuth) * lateralR;
      const dz = Math.sin(azimuth) * lateralR;

      // Catmull-Rom control points: fork point → arc out → tip
      const p0 = new THREE.Vector3(0,           forkY,                0);
      const p1 = new THREE.Vector3(dx * 0.22,   forkY + branchLen * 0.28,  dz * 0.22);
      const p2 = new THREE.Vector3(dx * 0.68,   forkY + branchLen * 0.68,  dz * 0.68);
      const p3 = new THREE.Vector3(dx,           forkY + branchLen,         dz);

      const curve  = new THREE.CatmullRomCurve3([p0, p1, p2, p3]);
      const bColor = GATE_COLORS[branch.gateState];

      if (branch.isGhost) {
        // Merged ghost — dashed line only (§ "light not matter", post-merge persistence)
        const pts    = curve.getPoints(24);
        const lineGeo = new THREE.BufferGeometry().setFromPoints(pts);
        toDispose.push(lineGeo);
        const line = new THREE.Line(lineGeo, ghostMat(bColor));
        line.computeLineDistances();
        group.add(line);
        return;
      }

      const bRadius = Math.max(0.045, Math.min(
        Math.sqrt(branch.filesModified) * BRANCH_GIRTH_K,
        trunkRad * 0.78,
      ));
      const tubeGeo = new THREE.TubeGeometry(curve, 16, bRadius, 5, false);
      toDispose.push(tubeGeo);
      group.add(new THREE.Mesh(tubeGeo, holoMat(bColor, 0.55, 0.78)));

      const edgesGeo = new THREE.EdgesGeometry(tubeGeo, 28);
      toDispose.push(edgesGeo);
      group.add(new THREE.LineSegments(edgesGeo, wireMat(bColor, 0.22)));

      // Commit node spheres at uniform spacing
      const nodeCap = Math.min(branch.commitCount, 10);
      for (let i = 0; i < nodeCap; i++) {
        const t   = (i + 1) / (nodeCap + 1);
        const pos = curve.getPoint(t);
        const nGeo = new THREE.SphereGeometry(NODE_R, 5, 4);
        toDispose.push(nGeo);
        const nm = new THREE.Mesh(nGeo, holoMat(bColor, 0.88, 0.92));
        nm.position.copy(pos);
        group.add(nm);
      }

      // Worktree sub-branches (GIT-3)
      branch.worktrees.forEach((wt, wi) => buildWorktree(wt, wi, curve, group));
    }

    // ── Agent worktree sub-branch (GIT-3) ────────────────────────────────

    function buildWorktree(
      wt: Worktree,
      idx: number,
      parentCurve: THREE.CatmullRomCurve3,
      group: THREE.Group,
    ) {
      const t0     = Math.min(0.45 + idx * 0.18, 0.9);
      const origin = parentCurve.getPoint(t0);
      const tang   = parentCurve.getTangent(t0).normalize();

      const perp   = new THREE.Vector3(-tang.z, 0, tang.x).normalize();
      const wtLen  = wt.commitCount * COMMIT_H * 0.9;
      const tip    = origin.clone()
        .add(perp.clone().multiplyScalar(wtLen * 0.65))
        .add(new THREE.Vector3(0, wtLen * 0.45, 0));

      const mid      = origin.clone().lerp(tip, 0.5);
      const wtCurve  = new THREE.CatmullRomCurve3([origin, mid, tip]);
      const wtColor  = AGENT_COLORS[wt.domain];
      const wtGeo    = new THREE.TubeGeometry(wtCurve, 8, WKTREE_R, 4, false);
      toDispose.push(wtGeo);
      group.add(new THREE.Mesh(wtGeo, holoMat(wtColor, 0.78, 0.85)));
    }

    // ── Build grove ───────────────────────────────────────────────────────

    SEED_REPOS.forEach(repo => {
      const group = new THREE.Group();
      group.position.copy(repo.pos);
      const { height, radiusBot } = buildTrunk(repo, group);
      repo.branches.forEach((b, bi) =>
        buildBranch(b, bi, repo, height, radiusBot, group),
      );
      scene.add(group);
    });

    // Blueprint grid floor
    const grid = new THREE.GridHelper(44, 44, 0x0d2544, 0x071a33);
    grid.position.y = -0.05;
    const gridMats = Array.isArray(grid.material) ? grid.material : [grid.material];
    gridMats.forEach(m => {
      (m as THREE.LineBasicMaterial).transparent = true;
      (m as THREE.LineBasicMaterial).opacity = 0.35;
    });
    scene.add(grid);

    // ── Orbit interaction ─────────────────────────────────────────────────

    let dragging = false;
    let lastPt = { x: 0, y: 0 };
    const el = renderer.domElement;

    function onPDown(e: PointerEvent) { dragging = true; lastPt = { x: e.clientX, y: e.clientY }; }
    function onPMove(e: PointerEvent) {
      if (!dragging) return;
      sph.theta -= (e.clientX - lastPt.x) * 0.005;
      sph.phi    = Math.max(0.15, Math.min(1.45, sph.phi + (e.clientY - lastPt.y) * 0.005));
      lastPt     = { x: e.clientX, y: e.clientY };
      applySph();
    }
    function onPUp() { dragging = false; }
    function onWheel(e: WheelEvent) {
      sph.r = Math.max(6, Math.min(65, sph.r + e.deltaY * 0.012));
      applySph();
    }

    el.addEventListener('pointerdown', onPDown);
    el.addEventListener('pointermove', onPMove);
    window.addEventListener('pointerup', onPUp);
    el.addEventListener('wheel', onWheel, { passive: true });

    // ── Animation loop ────────────────────────────────────────────────────

    let animId = 0;
    function animate() {
      animId = requestAnimationFrame(animate);
      if (!dragging) {
        sph.theta += 0.0007;
        applySph();
      }
      composer.render();
    }
    animate();

    // ── Resize ────────────────────────────────────────────────────────────

    const resizeObs = new ResizeObserver(() => {
      const nw = container!.clientWidth;
      const nh = container!.clientHeight;
      if (nw > 0 && nh > 0) {
        camera.aspect = nw / nh;
        camera.updateProjectionMatrix();
        renderer.setSize(nw, nh);
        composer.setSize(nw, nh);
        bloomPass.setSize(nw, nh);
      }
    });
    resizeObs.observe(container!);

    return () => {
      cancelAnimationFrame(animId);
      resizeObs.disconnect();
      el.removeEventListener('pointerdown', onPDown);
      el.removeEventListener('pointermove', onPMove);
      window.removeEventListener('pointerup', onPUp);
      el.removeEventListener('wheel', onWheel);
      toDispose.forEach(x => x.dispose());
      renderer.dispose();
      composer.dispose();
    };
  });

  // ─── Scan line / grain overlay (§19 atmospheric layer) ───────────────────

  $effect(() => {
    if (!overlayCanvas) return;

    const canvas = overlayCanvas;

    function resize() {
      canvas.width  = canvas.offsetWidth  || 1;
      canvas.height = canvas.offsetHeight || 1;
    }
    resize();

    const ctx = canvas.getContext('2d');
    if (!ctx) return;

    let animId = 0;

    function drawOverlay() {
      animId = requestAnimationFrame(drawOverlay);
      const cw = canvas.width;
      const ch = canvas.height;
      ctx!.clearRect(0, 0, cw, ch);

      // Moving horizontal scan line (Stark hologram §19)
      const scanY = (Date.now() * 0.038) % ch;
      for (let y = 0; y < ch; y += 3) {
        const dist = Math.abs(y - scanY);
        if (dist > 9) continue;
        const alpha = (1 - dist / 9) * 0.055;
        ctx!.fillStyle = `rgba(0,200,255,${alpha})`;
        ctx!.fillRect(0, y, cw, 1);
      }

      // Subtle static grain every ~30 frames
      if (Math.random() < 0.03) {
        const imgData = ctx!.createImageData(cw, ch);
        const d = imgData.data;
        for (let i = 0; i < d.length; i += 4) {
          const v = Math.random() < 0.004 ? Math.floor(Math.random() * 40) : 0;
          d[i] = 0; d[i+1] = Math.floor(v * 0.78); d[i+2] = v; d[i+3] = v;
        }
        ctx!.putImageData(imgData, 0, 0);
      }
    }

    drawOverlay();

    const ro = new ResizeObserver(resize);
    ro.observe(canvas);

    return () => {
      cancelAnimationFrame(animId);
      ro.disconnect();
    };
  });
</script>

<div class="forest-wrap">
  <div bind:this={container} class="forest-canvas"></div>
  <canvas bind:this={overlayCanvas} class="forest-overlay" aria-hidden="true"></canvas>
</div>

<style>
  .forest-wrap {
    position: relative;
    width: 100%;
    height: 100%;
    background: #020408;
    overflow: hidden;
  }
  .forest-canvas {
    position: absolute;
    inset: 0;
  }
  .forest-overlay {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    pointer-events: none;
    mix-blend-mode: screen;
  }
</style>
