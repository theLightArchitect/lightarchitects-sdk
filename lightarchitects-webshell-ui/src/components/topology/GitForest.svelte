<script lang="ts">
  import * as THREE from 'three';
  import { EffectComposer } from 'three/examples/jsm/postprocessing/EffectComposer.js';
  import { RenderPass } from 'three/examples/jsm/postprocessing/RenderPass.js';
  import { UnrealBloomPass } from 'three/examples/jsm/postprocessing/UnrealBloomPass.js';
  import { OutputPass } from 'three/examples/jsm/postprocessing/OutputPass.js';
  import { LineMaterial } from 'three/examples/jsm/lines/LineMaterial.js';
  import { LineSegments2 } from 'three/examples/jsm/lines/LineSegments2.js';
  import { LineGeometry } from 'three/examples/jsm/lines/LineGeometry.js';
  import { gitforestTree, gitforestPulses } from '$lib/stores';
  import { get } from 'svelte/store';
  import type { GitForestTopology, BranchNode } from '$lib/gitforest';
  import { countActiveWorktrees, computeFadeLevel, polytopeClusterFor } from '$lib/gitforest';
  import { PulseLayer } from '$lib/pulseLayer';

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

  // ─── Layout positions (assigned by index) ────────────────────────────────

  const REPO_POSITIONS: THREE.Vector3[] = [
    new THREE.Vector3(-6,  0,  2),   // sdk   — left, slightly back
    new THREE.Vector3( 0,  0, -1),   // soul  — centre, slightly forward
    new THREE.Vector3( 6,  0,  1),   // corso — right
  ];

  function repoDataToRepo(d: { id: string; name: string; commitCount: number; fileCount: number; branches: Branch[] }, idx: number): Repo {
    return {
      id: d.id,
      name: d.name,
      commitCount: d.commitCount,
      fileCount: d.fileCount,
      pos: REPO_POSITIONS[idx] ?? new THREE.Vector3(idx * 5.5, 0, 0),
      branches: d.branches,
    };
  }

  /** Map a `GitForestTopology` (BranchNode tree) into the internal `Repo[]`
   *  rendering representation.  Depth-0 nodes become repo roots; depth-2
   *  (build) nodes become branches; depth-3 (wave_cluster) worktrees map
   *  to the Branch.worktrees array. */
  function topologyToRepos(topology: GitForestTopology): Repo[] {
    const repos: Repo[] = [];
    let idx = 0;
    for (const node of Object.values(topology.nodes)) {
      if (node.depth !== 1) continue;  // program-level = one repo trunk per program
      const branches: Branch[] = node.children
        .map(cid => topology.nodes[cid])
        .filter(Boolean)
        .filter(c => c.depth === 2)
        .map(build => ({
          name: build.name,
          divergeCommit: 0,
          commitCount: build.build_progress?.waves_done ?? 0,
          filesModified: 0,
          gateState: (build.overlay.ci_status === 'success' ? 'clean'
            : build.overlay.ci_status === 'failure' ? 'failed'
            : build.overlay.hitl_state === 'pending' ? 'hitl_pending'
            : 'writing') as GateState,
          isGhost: build.overlay.lifecycle === 'abandoned',
          worktrees: build.worktrees.map(w => ({
            domain: w.domain as AgentDomain,
            commitCount: w.commits,
          })),
        }));
      repos.push({
        id: node.id,
        name: node.name,
        commitCount: countActiveWorktrees(node.id, topology.nodes),
        fileCount: 0,
        pos: REPO_POSITIONS[idx] ?? new THREE.Vector3(idx * 5.5, 0, 0),
        branches,
      });
      idx++;
    }
    return repos.length > 0 ? repos : [];
  }

  // ─── Static seed — real repo data (used until GitHub fetch resolves) ─────
  // lightarchitects-sdk: 333 commits / 1726 files
  // SOUL-DEV: 64 commits / 291 files
  // CORSO-DEV: 85 commits / 589 files

  const SEED_REPOS: Repo[] = [
    {
      id: 'sdk',
      name: 'lightarchitects-sdk',
      commitCount: 333,
      fileCount: 1726,
      pos: REPO_POSITIONS[0],
      branches: [
        {
          name: 'feat/acp-paused',
          divergeCommit: 245,
          commitCount: 24,
          filesModified: 142,
          gateState: 'ghost',
          isGhost: true,
          worktrees: [],
        },
        {
          name: 'feat/webshell-sprint3',
          divergeCommit: 290,
          commitCount: 18,
          filesModified: 87,
          gateState: 'hitl_pending',
          isGhost: false,
          worktrees: [{ domain: 'engineer', commitCount: 3 }],
        },
        {
          name: 'fix/security-88-92',
          divergeCommit: 315,
          commitCount: 8,
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
      pos: REPO_POSITIONS[1],
      branches: [
        {
          name: 'feat/helix-of-helices-spec',
          divergeCommit: 42,
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
      pos: REPO_POSITIONS[2],
      branches: [
        {
          name: 'feat/trinity-v8',
          divergeCommit: 60,
          commitCount: 7,
          filesModified: 33,
          gateState: 'writing',
          isGhost: false,
          worktrees: [{ domain: 'quality', commitCount: 2 }],
        },
      ],
    },
  ];

  // ─── Live repos state (seed until gitforestTree store populates) ─────────
  // Phase 5 wires the SSE stream → gitforestTree store → repos reactive update.
  // GitHub PAT is server-side only (github_token_store.rs, Phase 4) — no
  // client-side token reads.

  let repos = $state<Repo[]>(SEED_REPOS);
  // prefers-reduced-motion: initial value + live MediaQueryList listener (Phase 3.5)
  const motionMq = window.matchMedia('(prefers-reduced-motion: reduce)');
  let motionEnabled = $state(!motionMq.matches);
  $effect(() => {
    const handler = (e: MediaQueryListEvent) => { motionEnabled = !e.matches; };
    motionMq.addEventListener('change', handler);
    return () => motionMq.removeEventListener('change', handler);
  });
  // Memoised active-worktree counts per nodeId; invalidated on gitforestTree update
  let activeCountCache = $state(new Map<string, number>());

  // ── A11y substrate (Phase 3.5) ────────────────────────────────────────────
  // Shadow hitbox positions — updated at 4Hz from draw() via hitboxPending
  interface HitboxEntry { id: string; label: string; gateState: GateState; x: number; y: number; cw: number; ch: number; }
  let hitboxes = $state<HitboxEntry[]>([]);
  let hitboxPending: HitboxEntry[] = [];
  let hitboxFrame = 0;
  // Live region announcement (rate-limited: one update per 2s)
  let liveAnnouncement = $state('');
  let liveAnnouncedAt = 0;

  $effect(() => {
    const unsubscribe = gitforestTree.subscribe(topology => {
      if (!topology) return;
      const live = topologyToRepos(topology);
      if (live.length > 0) repos = live;
      // Rebuild active-count cache on tree update (O(nodes) not O(frames))
      const cache = new Map<string, number>();
      for (const id of Object.keys(topology.nodes)) {
        cache.set(id, countActiveWorktrees(id, topology.nodes));
      }
      activeCountCache = cache;
    });
    return unsubscribe;
  });

  // ─── Scale constants ──────────────────────────────────────────────────────
  //
  // Previous values produced invisible trunks:
  //   COMMIT_H=0.028 → SOUL trunk only 1.8 units at r=26 camera
  //   TRUNK_GIRTH_K=0.007 → trunk radii 0.12–0.29 units (sub-pixel at distance)
  //   emissiveIntensity=0.22 on trunk → no bloom, invisible against dark bg

  const COMMIT_H       = 0.065;  // branch/worktree Y units per commit
  const TRUNK_GIRTH_K  = 0.014;  // trunk radiusBot = sqrt(fileCount) * K
  const BRANCH_GIRTH_K = 0.005;  // branch radius = sqrt(filesModified) * K
  const WKTREE_R       = 0.075;  // worktree sub-branch radius
  const NODE_R         = 0.13;   // commit sphere radius

  // ─── Repo identity colors (trunk/halo) — each repo gets a distinct hue ──
  // Index matches REPO_POSITIONS / FOREST_REPO_NAMES order.
  // sdk=cyan, soul=gold, corso=violet

  const REPO_TRUNK_COLORS: [trunk: number, wire: number][] = [
    [0x0ea5e9, 0x38bdf8],  // sdk  — sky blue
    [0xf5d440, 0xfde68a],  // soul — gold
    [0x8b5cf6, 0xa78bfa],  // corso — violet
  ];

  function repoTrunkColor(idx: number): [number, number] {
    return REPO_TRUNK_COLORS[idx] ?? REPO_TRUNK_COLORS[0];
  }

  // ─── Gate → color (GIT-2) ────────────────────────────────────────────────
  // Deuteranopia + protanopia safe: blue-orange-teal axis (no red/green confusion).
  // Validated: clean=teal, failed=orange, hitl=amber-gold, writing=sky-blue, ghost=slate.
  const GATE_COLORS: Record<GateState, number> = {
    clean:        0x17c3b2,  // teal — distinguishable from all other states
    hitl_pending: 0xfbbf24,  // amber-gold — warm, not red
    merge_ready:  0xe2f542,  // yellow-lime — high-luminance, safe
    failed:       0xf97316,  // orange — safe for deuteranopia/protanopia
    writing:      0x38bdf8,  // sky-blue — distinct from teal
    ghost:        0x334155,  // slate — neutral; reduced opacity on merged branches
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

  let selectedRepoIdx = $state<number | null>(null);

  // Cubic Bezier scalar interpolation (used by 2D renderer)
  function bezierPoint(p0: number, p1: number, p2: number, p3: number, t: number): number {
    const u = 1 - t;
    return u*u*u*p0 + 3*u*u*t*p1 + 3*u*t*t*p2 + t*t*t*p3;
  }

  // ─── Component bindings ───────────────────────────────────────────────────

  let container: HTMLDivElement | undefined = $state();
  let canvas2d: HTMLCanvasElement | undefined = $state();
  let overlayCanvas: HTMLCanvasElement | undefined = $state();

  // ─── Three.js forest scene (GIT-1) ────────────────────────────────────────

  $effect(() => {
    if (!container) return;

    const w = container.clientWidth || 400;
    const h = container.clientHeight || 300;

    const scene = new THREE.Scene();
    scene.background = new THREE.Color(0x020408);
    scene.fog = new THREE.FogExp2(0x020408, 0.007);  // reduced — was 0.012 (ate trunks)

    // Pre-compute max trunk height so lookAt can be set to keep all trunks in frame.
    // Uses the same formula as buildTrunk: n^0.50 * 0.50
    const trunkH = (n: number) => Math.sqrt(n) * 0.50;
    const maxTrunkH = Math.max(...repos.map(r => trunkH(r.commitCount)));
    const lookAtY   = maxTrunkH * 0.55;  // aim at ~55% of tallest tree — vertically centers grove

    const camera = new THREE.PerspectiveCamera(58, w / h, 0.1, 200);
    // theta=0.0 positions camera dead-on: repos at x=-6,0,+6 appear symmetric left/centre/right
    // phi=0.80 is more side-on (less overhead) so trunks read as vertical, not diagonal
    let sph = { theta: 0.0, phi: 0.80, r: 30 };

    function applySph() {
      camera.position.set(
        sph.r * Math.sin(sph.phi) * Math.sin(sph.theta),
        sph.r * Math.cos(sph.phi),
        sph.r * Math.sin(sph.phi) * Math.cos(sph.theta),
      );
      camera.lookAt(0, lookAtY, 0);
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
    const bloomPass = new UnrealBloomPass(new THREE.Vector2(w, h), 0.65, 0.38, 0.12);
    composer.addPass(bloomPass);
    composer.addPass(new OutputPass());

    scene.add(new THREE.AmbientLight(0x003355, 0.45));

    const toDispose: Array<THREE.BufferGeometry | THREE.Material> = [];

    // ── Phase 3: PulseLayer, frustum, polytope InstancedMesh pool ────────
    const pulseLayer = new PulseLayer();

    // Frustum updated each animate() tick from camera matrices.
    // Used for per-Mesh culling in renderNode (not per-Group — THREE.Group has no geometry)
    const frustum = new THREE.Frustum();
    const projScreenMatrix = new THREE.Matrix4();

    // InstancedMesh pool: one per polytope kind, shared across all build nodes.
    // 4 draw calls total at 50 visible clusters × 4 kinds (H2 design choice DC-9).
    const MAX_CLUSTERS = 100;
    const polyGeos: Record<string, THREE.BufferGeometry> = {
      pentachoron:      new THREE.TetrahedronGeometry(0.35, 0),
      tesseract:        new THREE.BoxGeometry(0.5, 0.5, 0.5),
      hexadecachoron:   new THREE.OctahedronGeometry(0.38, 0),
      icositetrachoron: new THREE.DodecahedronGeometry(0.32, 0),
    };
    const polyMats: Record<string, THREE.MeshStandardMaterial> = {};
    const polyMeshes: Record<string, THREE.InstancedMesh> = {};
    const POLY_COLORS: Record<string, number> = {
      pentachoron:      0x4dffe6,
      tesseract:        0x4d8eff,
      hexadecachoron:   0xa874ff,
      icositetrachoron: 0xff7eb6,
    };
    for (const kind of Object.keys(polyGeos)) {
      toDispose.push(polyGeos[kind]!);
      const mat = new THREE.MeshStandardMaterial({
        color: POLY_COLORS[kind],
        emissive: new THREE.Color(POLY_COLORS[kind]),
        emissiveIntensity: 0.6,
        transparent: true,
        opacity: 0.55,
        blending: THREE.AdditiveBlending,
        depthWrite: false,
        wireframe: false,
      });
      toDispose.push(mat);
      polyMats[kind] = mat;
      const im = new THREE.InstancedMesh(polyGeos[kind]!, mat, MAX_CLUSTERS);
      im.count = 0;
      im.frustumCulled = true;  // engine uses instancedMesh bounding sphere for cull
      scene.add(im);
      polyMeshes[kind] = im;
    }
    // Per-frame: positions of active clusters keyed by kind
    const clusterPositions: Record<string, THREE.Vector3[]> = {
      pentachoron: [], tesseract: [], hexadecachoron: [], icositetrachoron: [],
    };

    // ── Pulse overlay — Phase 2 (haloMeshes driven by PulseLayer in Phase 3) ─
    const haloMeshes = new Map<string, THREE.Mesh>();
    // pulseIntensities retained as passthrough from PulseLayer.opacities (scaled ×3.0)
    const pulseIntensities = new Map<string, number>();

    // webglcontextlost fallback (iter-9 B2 fold — Safari / Intel HD mitigation)
    renderer.domElement.addEventListener('webglcontextlost', (e: Event) => {
      e.preventDefault();
      cancelAnimationFrame(animId);
    }, { once: true });

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

    function buildTrunk(
      repo: Repo,
      repoIdx: number,
      group: THREE.Group,
    ): { height: number; radiusBot: number } {
      // sqrt(n) * 0.50 → SDK=9.1u  SOUL=4.0u  CORSO=4.6u  ratio 2.3:1
      // Camera lookAt set to maxTrunkH * 0.48 so all trunks stay in frustum
      const height    = trunkH(repo.commitCount);
      const radiusBot = Math.sqrt(repo.fileCount) * TRUNK_GIRTH_K;
      const radiusTop = radiusBot * 0.42;
      const [tc, wc]  = repoTrunkColor(repoIdx);

      // Core cylinder — lower emissive so it doesn't blow out; wire edges carry the form
      const geo = new THREE.CylinderGeometry(radiusTop, radiusBot, height, 8, 1);
      geo.translate(0, height / 2, 0);
      toDispose.push(geo);
      group.add(new THREE.Mesh(geo, holoMat(tc, 0.42, 0.62)));

      const edges = new THREE.EdgesGeometry(geo);
      toDispose.push(edges);
      group.add(new THREE.LineSegments(edges, wireMat(wc, 0.82)));

      // Milestone rings — use warm white for contrast against trunk hue
      const ringCount = Math.min(Math.floor(repo.commitCount / 25), 12);
      for (let ri = 1; ri <= ringCount; ri++) {
        const t       = ri / (ringCount + 1);
        const ringY   = t * height;
        const frac    = ringY / height;
        const ringRad = radiusBot + (radiusTop - radiusBot) * frac;
        const rGeo    = new THREE.RingGeometry(ringRad * 0.88, ringRad * 1.38, 20);
        rGeo.rotateX(-Math.PI / 2);
        rGeo.translate(0, ringY, 0);
        toDispose.push(rGeo);
        // Alternate: even rings use trunk hue, odd rings use white accent
        const ringColor = ri % 2 === 0 ? tc : 0xe2e8f0;
        group.add(new THREE.Mesh(rGeo, holoMat(ringColor, 0.62, 0.5)));
      }

      // Wide ground halo — anchors tree to floor; pulse overlay boosts emissiveIntensity
      const halo = new THREE.RingGeometry(radiusBot * 0.9, radiusBot * 3.2, 32);
      halo.rotateX(-Math.PI / 2);
      halo.translate(0, 0.01, 0);
      toDispose.push(halo);
      const haloMesh = new THREE.Mesh(halo, holoMat(tc, 0.48, 0.48));
      haloMeshes.set(repo.id, haloMesh);
      group.add(haloMesh);

      // Base disc
      const disc = new THREE.CylinderGeometry(radiusBot * 1.1, radiusBot * 1.1, 0.018, 16);
      toDispose.push(disc);
      group.add(new THREE.Mesh(disc, holoMat(tc, 0.92, 0.88)));

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

    // ── renderNode: recursive BranchNode renderer (Phase 3 — live data path) ─
    // Replaces flat repos.forEach when gitforestTree has live topology.
    // Bounded recursion invariant: depth ≤ 3 (main=0, program=1, build=2, wave_cluster=3).
    // Frustum culling: per-Mesh before scene.add (not per-Group — Group has no geometry).
    // LOD: >200 visible nodes caps at simplified geometry via mesh.userData.lod flag.

    let renderedNodeCount = 0;
    const HARD_NODE_CAP = 200;

    function addWithCull(mesh: THREE.Mesh): void {
      mesh.updateMatrixWorld();
      if (!mesh.geometry.boundingSphere) mesh.geometry.computeBoundingSphere();
      if (renderedNodeCount < HARD_NODE_CAP && frustum.intersectsObject(mesh)) {
        scene.add(mesh);
        renderedNodeCount++;
      }
    }

    function renderNode(
      nodeId: string,
      nodes: Record<string, BranchNode>,
      worldPos: THREE.Vector3,
      parentHeight: number,
      depth: number,
    ): void {
      if (depth > 3) throw new Error('GitForest depth > 3 invariant violated');
      const node = nodes[nodeId];
      if (!node) return;

      const fade = node.overlay.lifecycle === 'merged' || node.overlay.lifecycle === 'abandoned'
        ? computeFadeLevel(node.overlay.merged_at ?? null)
        : 1.0;

      // Camera distance for LOD: simplified geometry beyond 40 units
      const distToCamera = worldPos.distanceTo(camera.position);
      const useLOD = distToCamera > 40;

      if (depth === 0) {
        // main trunk — anchor sphere
        const geo = useLOD
          ? new THREE.SphereGeometry(0.3, 4, 3)
          : new THREE.SphereGeometry(0.5, 8, 6);
        toDispose.push(geo);
        const mesh = new THREE.Mesh(geo, holoMat(0x22c55e, 0.7 * fade, 0.8 * fade));
        mesh.position.copy(worldPos);
        addWithCull(mesh);
      } else if (depth === 1) {
        // program-level: horizontal disc
        const geo = useLOD
          ? new THREE.CylinderGeometry(0.25, 0.25, 0.06, 6, 1)
          : new THREE.CylinderGeometry(0.35, 0.35, 0.06, 12, 1);
        toDispose.push(geo);
        const color = REPO_TRUNK_COLORS[(renderedNodeCount % 3)] ?? REPO_TRUNK_COLORS[0];
        const mesh = new THREE.Mesh(geo, holoMat(color[0], 0.55 * fade, 0.7 * fade));
        mesh.position.copy(worldPos);
        addWithCull(mesh);
      } else if (depth === 2) {
        // build-level: cylinder trunk
        const height = (node.build_progress?.waves_done ?? 1) * 0.8;
        const geo = useLOD
          ? new THREE.CylinderGeometry(0.1, 0.18, height, 5, 1)
          : new THREE.CylinderGeometry(0.12, 0.22, height, 8, 1);
        geo.translate(0, height / 2, 0);
        toDispose.push(geo);
        const ciColor = node.overlay.ci_status === 'success' ? GATE_COLORS.clean
          : node.overlay.ci_status === 'failure' ? GATE_COLORS.failed
          : GATE_COLORS.writing;
        const mesh = new THREE.Mesh(geo, holoMat(ciColor, 0.6 * fade, 0.75 * fade));
        mesh.position.copy(worldPos);
        addWithCull(mesh);

        // Register polytope cluster if active worktrees present (memoised count)
        const activeCount = activeCountCache.get(nodeId) ?? 0;
        if (activeCount > 0 && !useLOD) {
          const kinds = polytopeClusterFor(activeCount);
          for (const kind of kinds) {
            if (kind in clusterPositions) {
              clusterPositions[kind as keyof typeof clusterPositions]?.push(
                worldPos.clone().add(new THREE.Vector3(0, height + 0.5, 0)),
              );
            }
          }
        }
      } else if (depth === 3) {
        // wave-cluster leaf: small sphere
        const geo = useLOD
          ? new THREE.SphereGeometry(0.08, 3, 2)
          : new THREE.SphereGeometry(0.12, 5, 4);
        toDispose.push(geo);
        const mesh = new THREE.Mesh(
          geo,
          holoMat(node.overlay.hitl_state === 'pending' ? 0xf59e0b : 0x4d8eff, 0.7 * fade, 0.8 * fade),
        );
        mesh.position.copy(worldPos);
        addWithCull(mesh);
      }

      // Recurse into children — spread them around the parent position
      const children = node.children.map(cid => nodes[cid]).filter(Boolean) as BranchNode[];
      children.forEach((child, ci) => {
        const angle = ci * 2.399963;  // golden angle spacing
        const lateral = 2.5 + ci * 0.6;
        const childPos = worldPos.clone().add(new THREE.Vector3(
          Math.cos(angle) * lateral,
          depth === 0 ? 0.5 : 1.2,
          Math.sin(angle) * lateral,
        ));
        renderNode(child.id, nodes, childPos, parentHeight, depth + 1);
      });
    }

    // ── Update polytope InstancedMesh matrices for current cluster positions ─
    function flushPolytopeInstances(): void {
      const tempMatrix = new THREE.Matrix4();
      const tempQuat   = new THREE.Quaternion();
      const tempScale  = new THREE.Vector3(1, 1, 1);
      for (const kind of Object.keys(polyMeshes)) {
        const positions = clusterPositions[kind as keyof typeof clusterPositions] ?? [];
        const im = polyMeshes[kind]!;
        const count = Math.min(positions.length, MAX_CLUSTERS);
        im.count = count;
        for (let i = 0; i < count; i++) {
          tempMatrix.compose(positions[i]!, tempQuat, tempScale);
          im.setMatrixAt(i, tempMatrix);
        }
        im.instanceMatrix.needsUpdate = true;
        im.computeBoundingSphere();
        // Reset for next frame
        positions.length = 0;
      }
    }

    // ── Build grove (seed-data legacy path + live-data renderNode path) ──

    const currentTopology = get(gitforestTree);
    if (currentTopology) {
      // Live path: recursive BranchNode renderer with frustum culling
      renderedNodeCount = 0;
      renderNode(currentTopology.root_id, currentTopology.nodes, new THREE.Vector3(0, 0, 0), 0, 0);
      flushPolytopeInstances();
    } else {
      // Seed path: existing Repo[] renderer (flat iteration, no frustum cull)
      repos.forEach((repo, repoIdx) => {
        const group = new THREE.Group();
        group.position.copy(repo.pos);
        const { height, radiusBot } = buildTrunk(repo, repoIdx, group);
        repo.branches.forEach((b, bi) =>
          buildBranch(b, bi, repo, height, radiusBot, group),
        );
        scene.add(group);
      });
    }

    // Blueprint grid floor — AdditiveBlending required: dark colors add nothing to near-black bg
    // Center lines: 0x1a6ec0 (R26,G110,B192) → adds (16,66,115) on top of black = visible mid-blue
    // Division lines: 0x0c3a70 (R12,G58,B112) → adds (7,35,67) = subtle dark-blue lattice
    const grid = new THREE.GridHelper(44, 44, 0x1a6ec0, 0x0c3a70);
    grid.position.y = -0.05;
    const gridMats = Array.isArray(grid.material) ? grid.material : [grid.material];
    gridMats.forEach(m => {
      const lm = m as THREE.LineBasicMaterial;
      lm.transparent = true;
      lm.opacity = 0.62;
      lm.blending = THREE.AdditiveBlending;
      lm.depthWrite = false;
    });
    scene.add(grid);

    // Wire pulse store → PulseLayer.enqueue (Phase 3 replaces direct intensity set)
    const unsubPulses = gitforestPulses.subscribe(nodeIds => {
      for (const id of nodeIds) {
        pulseLayer.enqueue(id);
      }
      // Live region announcement — rate-limited to one per 2s (Phase 3.5)
      if (nodeIds.length > 0) {
        const now = performance.now();
        if (now - liveAnnouncedAt > 2000) {
          liveAnnouncement = nodeIds.length === 1
            ? `Branch activity: ${nodeIds[0]}`
            : `${nodeIds.length} branches active`;
          liveAnnouncedAt = now;
        }
      }
    });

    // PulseLayer tick at 4Hz (setInterval 250ms — deliberate: JS single-threaded,
    // no race with rAF; tick mutates ring buffer between frames safely)
    const pulseTickId = setInterval(() => {
      if (!motionEnabled) return;  // prefers-reduced-motion: skip tick entirely
      pulseLayer.tick();
      // Sync PulseLayer opacities → pulseIntensities (scaled ×3.0 to match Phase 2 peak)
      for (const [id, opacity] of pulseLayer.opacities) {
        pulseIntensities.set(id, opacity * 3.0);
      }
      // Evict entries no longer in PulseLayer
      for (const id of pulseIntensities.keys()) {
        if (!pulseLayer.opacities.has(id)) pulseIntensities.delete(id);
      }
    }, 250);

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
      // prefers-reduced-motion guard (Phase 3.5 wires the listener; guard is here now)
      if (!motionEnabled) {
        composer.render();
        return;
      }
      if (!dragging) {
        sph.theta += 0.00025;
        applySph();
      }
      // Update frustum from camera (used by addWithCull inside next renderNode rebuild)
      camera.updateMatrixWorld();
      projScreenMatrix.multiplyMatrices(camera.projectionMatrix, camera.matrixWorldInverse);
      frustum.setFromProjectionMatrix(projScreenMatrix);

      // Apply PulseLayer opacities to halo meshes (PulseLayer.tick runs at 4Hz in setInterval)
      for (const [id, intensity] of pulseIntensities) {
        const mesh = haloMeshes.get(id);
        if (mesh) {
          (mesh.material as THREE.MeshStandardMaterial).emissiveIntensity = 0.48 + intensity * 2.0;
        }
      }
      // Restore baseline for halos no longer pulsing
      for (const [id, mesh] of haloMeshes) {
        if (!pulseIntensities.has(id)) {
          (mesh.material as THREE.MeshStandardMaterial).emissiveIntensity = 0.48;
        }
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
      clearInterval(pulseTickId);
      pulseLayer.destroy();
      unsubPulses();
      resizeObs.disconnect();
      el.removeEventListener('pointerdown', onPDown);
      el.removeEventListener('pointermove', onPMove);
      window.removeEventListener('pointerup', onPUp);
      el.removeEventListener('wheel', onWheel);
      toDispose.forEach(x => x.dispose());
      for (const im of Object.values(polyMeshes)) im.dispose();
      renderer.dispose();
      composer.dispose();
    };
  });

  // ─── 2D flat git-graph renderer ──────────────────────────────────────────

  $effect(() => {
    if (!canvas2d) return;

    const canvas = canvas2d;
    const ctxRaw = canvas.getContext('2d');
    if (!ctxRaw) return;
    const ctx: CanvasRenderingContext2D = ctxRaw;

    function resize() {
      canvas.width  = canvas.offsetWidth  || 400;
      canvas.height = canvas.offsetHeight || 300;
    }
    resize();

    const maxCommits = Math.max(...repos.map(r => r.commitCount));
    let animId: number;

    function hexColor(n: number): string {
      return `#${n.toString(16).padStart(6, '0')}`;
    }

    function draw() {
      animId = requestAnimationFrame(draw);
      const cw = canvas.width;
      const ch = canvas.height;
      const t = performance.now() * 0.001;

      ctx.clearRect(0, 0, cw, ch);
      ctx.fillStyle = '#020408';
      ctx.fillRect(0, 0, cw, ch);

      // Blueprint grid — cell size relative to canvas so it adapts to any viewport
      const gs = Math.max(cw, ch) / 20;
      ctx.lineWidth = 0.5;
      for (let x = 0; x < cw; x += gs) {
        ctx.strokeStyle = (Math.round(x / gs) % 4 === 0) ? 'rgba(26,110,192,0.2)' : 'rgba(26,110,192,0.07)';
        ctx.beginPath(); ctx.moveTo(x, 0); ctx.lineTo(x, ch); ctx.stroke();
      }
      for (let y = 0; y < ch; y += gs) {
        ctx.strokeStyle = (Math.round(y / gs) % 4 === 0) ? 'rgba(26,110,192,0.2)' : 'rgba(26,110,192,0.07)';
        ctx.beginPath(); ctx.moveTo(0, y); ctx.lineTo(cw, y); ctx.stroke();
      }

      // ── Layout constants that scale with repo count ───────────────────────
      // Two-pass sizing: compute font sizes first (needed for padX), then
      // derive padX from the longest label so no name ever clips at the edge.
      const n = repos.length;

      // Preliminary colW estimate (padX unknown yet — use 5% placeholder)
      const colWEst     = (cw * 0.90) / n;
      const nameFontSz  = Math.max(7, Math.min(11, Math.floor(colWEst * 0.075)));
      const statsFontSz = Math.max(6, Math.min(9,  Math.floor(colWEst * 0.058)));
      const labelFontSz = Math.max(6, Math.min(8,  Math.floor(colWEst * 0.052)));

      // Half-width of the longest repo label in px (monospace ≈ 0.62× font size)
      const maxNameLen  = Math.max(...repos.map(r => r.name.length));
      const labelHalfW  = (maxNameLen * nameFontSz * 0.62) / 2;
      // padX must also accommodate the outermost branch overhang (colWEst * 0.46)
      const branchOverhang = colWEst * 0.46;
      const padX   = Math.max(labelHalfW + 6, branchOverhang + 4, cw * 0.03);
      const drawW  = cw - 2 * padX;
      const colW   = drawW / n;
      const labelH = Math.max(28, ch * 0.09);
      const baseY  = ch - labelH;
      const availH = baseY - 12;

      // Update layout refs for click/hover handlers
      l1ColW = colW;
      l1PadX = padX;

      // ── L2: hero mode for a selected repo ──────────────────────────────
      if (selectedRepoIdx !== null && selectedRepoIdx < repos.length) {
        drawHero(repos[selectedRepoIdx], selectedRepoIdx);
        return;
      }

      repos.forEach((repo, ri) => {
        const cx       = padX + colW * (ri + 0.5);
        // Trunk height fills most of availH, scaled by sqrt(commitCount)/sqrt(max)
        const trunkPx  = (Math.sqrt(repo.commitCount) / Math.sqrt(maxCommits)) * availH * 0.88;
        const topY     = baseY - trunkPx;
        const [tcN, wcN] = repoTrunkColor(ri);
        const tc       = hexColor(tcN);
        const wc       = hexColor(wcN);

        // Trunk gradient
        const grad = ctx.createLinearGradient(cx, baseY, cx, topY);
        grad.addColorStop(0, tc + '55');
        grad.addColorStop(1, wc + 'dd');
        ctx.strokeStyle = grad;
        ctx.lineWidth = Math.max(1.5, colW * 0.016);
        ctx.setLineDash([]);
        ctx.beginPath();
        ctx.moveTo(cx, baseY);
        ctx.lineTo(cx, topY);
        ctx.stroke();

        // Milestone tick marks
        const ringCount = Math.min(Math.floor(repo.commitCount / 25), 12);
        for (let mi = 1; mi <= ringCount; mi++) {
          const frac  = mi / (ringCount + 1);
          const ringY = baseY - frac * trunkPx;
          const len   = mi % 4 === 0 ? Math.max(6, colW * 0.06) : Math.max(3, colW * 0.03);
          ctx.strokeStyle = mi % 2 === 0 ? tc + 'aa' : '#e2e8f0' + '55';
          ctx.lineWidth = 0.8;
          ctx.beginPath();
          ctx.moveTo(cx - len, ringY);
          ctx.lineTo(cx + len, ringY);
          ctx.stroke();
        }

        // Trunk tip glow pulse
        const tipPulse = 0.65 + 0.35 * Math.sin(t * 1.4 + ri * 1.2);
        ctx.fillStyle = tc;
        ctx.globalAlpha = tipPulse;
        ctx.beginPath();
        ctx.arc(cx, topY, Math.max(2.5, colW * 0.025), 0, Math.PI * 2);
        ctx.fill();
        ctx.globalAlpha = 1;

        // Branches — length capped at colW * 0.46 so they stay within their column
        repo.branches.forEach((branch, bi) => {
          const forkFrac = branch.divergeCommit / repo.commitCount;
          const forkY    = baseY - forkFrac * trunkPx;
          const branchPx = Math.min(branch.commitCount * (colW * 0.055), colW * 0.46);
          const dir      = bi % 2 === 0 ? 1 : -1;
          const bColor   = hexColor(GATE_COLORS[branch.gateState]);

          const cp1X = cx + dir * branchPx * 0.25;
          const cp1Y = forkY - branchPx * 0.08;
          const cp2X = cx + dir * branchPx * 0.65;
          const cp2Y = forkY - branchPx * 0.38;
          const tipX = cx + dir * branchPx;
          const tipY = forkY - branchPx * 0.52;

          if (branch.isGhost) {
            ctx.setLineDash([4, 3]);
            ctx.strokeStyle = bColor;
            ctx.globalAlpha = 0.22;
          } else {
            ctx.setLineDash([]);
            ctx.strokeStyle = bColor;
            ctx.globalAlpha = 0.88;
          }
          ctx.lineWidth = 1.5;
          ctx.beginPath();
          ctx.moveTo(cx, forkY);
          ctx.bezierCurveTo(cp1X, cp1Y, cp2X, cp2Y, tipX, tipY);
          ctx.stroke();
          ctx.setLineDash([]);
          ctx.globalAlpha = 1;

          // Collect hitbox for a11y overlay (Phase 3.5 — flushed to $state at 4Hz)
          hitboxPending.push({ id: `${repo.name}/${branch.name}`, label: branch.name, gateState: branch.gateState, x: tipX, y: tipY, cw: canvas.width, ch: canvas.height });

          if (!branch.isGhost) {
            // Commit dots with staggered pulse
            const nodeCap = Math.min(branch.commitCount, 8);
            for (let ni = 0; ni < nodeCap; ni++) {
              const nt   = (ni + 1) / (nodeCap + 1);
              const dotX = bezierPoint(cx, cp1X, cp2X, tipX, nt);
              const dotY = bezierPoint(forkY, cp1Y, cp2Y, tipY, nt);
              const pulse = 0.5 + 0.5 * Math.sin(t * 2.2 + ni * 0.9 + bi * 1.5 + ri * 2.3);
              ctx.fillStyle = bColor;
              ctx.globalAlpha = pulse;
              ctx.beginPath();
              ctx.arc(dotX, dotY, Math.max(1.5, colW * 0.013), 0, Math.PI * 2);
              ctx.fill();
              ctx.globalAlpha = 1;
            }

            // Branch label near tip — char limit scales with column width
            const maxChars = Math.max(8, Math.floor(colW / (labelFontSz * 0.62)));
            ctx.font = `${labelFontSz}px monospace`;
            ctx.fillStyle = bColor;
            ctx.globalAlpha = 0.65;
            ctx.textAlign = dir > 0 ? 'left' : 'right';
            const shortName = branch.name.replace(/^(feat|fix|chore)\//, '').slice(0, maxChars);
            ctx.fillText(shortName, tipX + dir * 4, tipY - 2);
            ctx.globalAlpha = 1;
            ctx.textAlign = 'left';

            // Worktree sub-branches
            branch.worktrees.forEach((wt, wi) => {
              const wtT   = Math.min(0.42 + wi * 0.22, 0.88);
              const wtX   = bezierPoint(cx, cp1X, cp2X, tipX, wtT);
              const wtY2  = bezierPoint(forkY, cp1Y, cp2Y, tipY, wtT);
              const wtLen = wt.commitCount * Math.max(6, colW * 0.06);
              const wtCol = hexColor(AGENT_COLORS[wt.domain]);
              const offX  = dir * 0.4 + (wi % 2 === 0 ? 0.9 : -0.9);
              const offY  = -1;
              const mag   = Math.sqrt(offX*offX + offY*offY);
              ctx.strokeStyle = wtCol;
              ctx.lineWidth = 1;
              ctx.globalAlpha = 0.72;
              ctx.beginPath();
              ctx.moveTo(wtX, wtY2);
              ctx.lineTo(wtX + (offX/mag)*wtLen, wtY2 + (offY/mag)*wtLen);
              ctx.stroke();
              ctx.globalAlpha = 1;
            });
          }
        });

        // Repo name + stats label
        ctx.textAlign = 'center';
        ctx.font = `bold ${nameFontSz}px monospace`;
        ctx.fillStyle = tc;
        ctx.fillText(repo.name, cx, baseY + labelH * 0.42);
        ctx.font = `${statsFontSz}px monospace`;
        ctx.fillStyle = '#475569';
        ctx.fillText(`${repo.commitCount}c · ${repo.fileCount}f`, cx, baseY + labelH * 0.80);
        ctx.textAlign = 'left';
      });

      // Flush hitbox positions to $state every ~15 frames (≈4Hz at 60fps)
      hitboxFrame = (hitboxFrame + 1) % 15;
      if (hitboxFrame === 0) {
        hitboxes = hitboxPending;
      }
      hitboxPending = [];
    }

    // ── L2 hero renderer ─────────────────────────────────────────────────
    // Draws a single repo in full-canvas detail mode.

    function drawHero(repo: Repo, riOrig: number) {
      const cw   = canvas.width;
      const ch   = canvas.height;
      const t    = performance.now() * 0.001;
      const cx   = cw / 2;

      const labelH = Math.max(28, ch * 0.09);
      const baseY  = ch - labelH;
      const availH = baseY - 48;   // reserve 48px at top for title

      const [tcN, wcN] = repoTrunkColor(riOrig);
      const tc = hexColor(tcN);
      const wc = hexColor(wcN);

      // Title
      ctx.textAlign = 'center';
      ctx.font = 'bold 13px monospace';
      ctx.fillStyle = tc;
      ctx.fillText(repo.name, cx, 22);
      ctx.font = '9px monospace';
      ctx.fillStyle = '#475569';
      ctx.fillText(`${repo.commitCount} commits  ·  ${repo.fileCount} files`, cx, 36);
      ctx.textAlign = 'left';

      const trunkPx = availH * 0.92;
      const topY    = baseY - trunkPx;

      // Trunk
      const grad = ctx.createLinearGradient(cx, baseY, cx, topY);
      grad.addColorStop(0, tc + '55');
      grad.addColorStop(1, wc + 'dd');
      ctx.strokeStyle = grad;
      ctx.lineWidth = 3;
      ctx.setLineDash([]);
      ctx.beginPath();
      ctx.moveTo(cx, baseY);
      ctx.lineTo(cx, topY);
      ctx.stroke();

      // Milestone rings + commit count labels
      const ringCount = Math.min(Math.floor(repo.commitCount / 25), 12);
      for (let mi = 1; mi <= ringCount; mi++) {
        const frac   = mi / (ringCount + 1);
        const ringY  = baseY - frac * trunkPx;
        const tickW  = mi % 4 === 0 ? 18 : 9;
        ctx.strokeStyle = mi % 2 === 0 ? tc + 'aa' : '#e2e8f0' + '55';
        ctx.lineWidth = 0.8;
        ctx.beginPath();
        ctx.moveTo(cx - tickW, ringY);
        ctx.lineTo(cx + tickW, ringY);
        ctx.stroke();
        if (mi % 4 === 0) {
          ctx.font = '7px monospace';
          ctx.fillStyle = '#334155';
          ctx.textAlign = 'right';
          ctx.fillText(`${Math.floor(frac * repo.commitCount)}c`, cx - tickW - 3, ringY + 3);
          ctx.textAlign = 'left';
        }
      }

      // Trunk tip pulse
      const tipPulse = 0.65 + 0.35 * Math.sin(t * 1.4 + riOrig * 1.2);
      ctx.fillStyle = tc;
      ctx.globalAlpha = tipPulse;
      ctx.beginPath();
      ctx.arc(cx, topY, 4, 0, Math.PI * 2);
      ctx.fill();
      ctx.globalAlpha = 1;

      // Branches — full horizontal spread
      const maxHalf = cw * 0.42;
      repo.branches.forEach((branch, bi) => {
        const forkFrac = branch.divergeCommit / repo.commitCount;
        const forkY    = baseY - forkFrac * trunkPx;
        const branchPx = Math.min(branch.commitCount * 18, maxHalf);
        const dir      = bi % 2 === 0 ? 1 : -1;
        const bColor   = hexColor(GATE_COLORS[branch.gateState]);

        const cp1X = cx + dir * branchPx * 0.25;
        const cp1Y = forkY - branchPx * 0.08;
        const cp2X = cx + dir * branchPx * 0.65;
        const cp2Y = forkY - branchPx * 0.38;
        const tipX = cx + dir * branchPx;
        const tipY = forkY - branchPx * 0.52;

        if (branch.isGhost) {
          ctx.setLineDash([6, 4]); ctx.globalAlpha = 0.25;
        } else {
          ctx.setLineDash([]);     ctx.globalAlpha = 0.9;
        }
        ctx.strokeStyle = bColor;
        ctx.lineWidth = 2;
        ctx.beginPath();
        ctx.moveTo(cx, forkY);
        ctx.bezierCurveTo(cp1X, cp1Y, cp2X, cp2Y, tipX, tipY);
        ctx.stroke();
        ctx.setLineDash([]); ctx.globalAlpha = 1;

        if (!branch.isGhost) {
          // Commit nodes with index labels
          const nodeCap = Math.min(branch.commitCount, 10);
          for (let ni = 0; ni < nodeCap; ni++) {
            const nt   = (ni + 1) / (nodeCap + 1);
            const dotX = bezierPoint(cx, cp1X, cp2X, tipX, nt);
            const dotY = bezierPoint(forkY, cp1Y, cp2Y, tipY, nt);
            const pulse = 0.6 + 0.4 * Math.sin(t * 2.2 + ni * 0.9 + bi * 1.5);
            ctx.fillStyle = bColor;
            ctx.globalAlpha = pulse;
            ctx.beginPath();
            ctx.arc(dotX, dotY, 3.5, 0, Math.PI * 2);
            ctx.fill();
            ctx.globalAlpha = 0.5;
            ctx.font = '7px monospace';
            ctx.textAlign = 'center';
            ctx.fillText(`C${nodeCap - ni}`, dotX, dotY - 6);
            ctx.globalAlpha = 1;
            ctx.textAlign = 'left';
          }

          // Branch name + metadata at tip
          ctx.textAlign = dir > 0 ? 'left' : 'right';
          const lx = tipX + dir * 8;
          ctx.font = 'bold 9px monospace';
          ctx.fillStyle = bColor;
          ctx.fillText(branch.name, lx, tipY - 4);
          ctx.font = '7px monospace';
          ctx.fillStyle = '#475569';
          ctx.fillText(`${branch.commitCount}c · ${branch.filesModified}f`, lx, tipY + 7);
          ctx.textAlign = 'left';

          // Worktrees — labeled
          branch.worktrees.forEach((wt, wi) => {
            const wtT   = Math.min(0.42 + wi * 0.22, 0.88);
            const wtX   = bezierPoint(cx, cp1X, cp2X, tipX, wtT);
            const wtY2  = bezierPoint(forkY, cp1Y, cp2Y, tipY, wtT);
            const wtLen = wt.commitCount * 22;
            const wtCol = hexColor(AGENT_COLORS[wt.domain]);
            const offX  = dir * 0.4 + (wi % 2 === 0 ? 0.9 : -0.9);
            const offY  = -1;
            const mag   = Math.sqrt(offX * offX + offY * offY);
            const wtTX  = wtX + (offX / mag) * wtLen;
            const wtTY  = wtY2 + (offY / mag) * wtLen;
            ctx.strokeStyle = wtCol;
            ctx.lineWidth = 1.5;
            ctx.globalAlpha = 0.82;
            ctx.beginPath();
            ctx.moveTo(wtX, wtY2);
            ctx.lineTo(wtTX, wtTY);
            ctx.stroke();
            ctx.globalAlpha = 0.7;
            ctx.font = '7px monospace';
            ctx.fillStyle = wtCol;
            ctx.textAlign = 'center';
            ctx.fillText(wt.domain, wtTX, wtTY - 5);
            ctx.globalAlpha = 1;
            ctx.textAlign = 'left';
          });
        }
      });

      // Back button — fixed top-left
      const bx = 8, by = 8, bw = 72, bh = 22;
      ctx.fillStyle = 'rgba(2,4,8,0.8)';
      ctx.fillRect(bx, by, bw, bh);
      ctx.strokeStyle = 'rgba(58,63,71,0.8)';
      ctx.lineWidth = 1;
      ctx.strokeRect(bx, by, bw, bh);
      ctx.fillStyle = '#94a3b8';
      ctx.font = '700 9px monospace';
      ctx.fillText('< FOREST', bx + 9, by + 14);
    }

    // ── Layout refs shared with click/hover handlers ───────────────────
    let l1ColW = 0;
    let l1PadX = 0;

    function handleClick(e: MouseEvent) {
      const rect = canvas.getBoundingClientRect();
      const mx   = (e.clientX - rect.left) * (canvas.width  / rect.width);
      const my   = (e.clientY - rect.top)  * (canvas.height / rect.height);

      if (selectedRepoIdx !== null) {
        if (mx < 80 && my < 30) selectedRepoIdx = null;
        return;
      }
      if (l1ColW <= 0) return;
      const ri = Math.floor((mx - l1PadX) / l1ColW);
      if (ri >= 0 && ri < repos.length) selectedRepoIdx = ri;
    }

    function handleMouseMove(e: MouseEvent) {
      const rect = canvas.getBoundingClientRect();
      const mx   = (e.clientX - rect.left) * (canvas.width  / rect.width);
      const my   = (e.clientY - rect.top)  * (canvas.height / rect.height);

      if (selectedRepoIdx !== null) {
        canvas.style.cursor = (mx < 80 && my < 30) ? 'pointer' : 'default';
      } else if (l1ColW > 0) {
        const ri = Math.floor((mx - l1PadX) / l1ColW);
        canvas.style.cursor = (ri >= 0 && ri < repos.length) ? 'pointer' : 'default';
      }
    }

    canvas.addEventListener('click', handleClick);
    canvas.addEventListener('mousemove', handleMouseMove);

    draw();

    const ro = new ResizeObserver(resize);
    ro.observe(canvas);

    return () => {
      cancelAnimationFrame(animId);
      ro.disconnect();
      canvas.removeEventListener('click', handleClick);
      canvas.removeEventListener('mousemove', handleMouseMove);
      canvas.style.cursor = '';
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
  <!-- 2D canvas: primary tree rendering -->
  <canvas bind:this={canvas2d} class="forest-canvas"></canvas>
  <!-- Three.js container: selective agent-presence polytope overlay (always mounted,
       rendered only when BranchNode worktrees are present — no operator mode toggle) -->
  <div bind:this={container} class="forest-three" aria-hidden="true"></div>
  <!-- Hologram scan-line overlay -->
  <canvas bind:this={overlayCanvas} class="forest-overlay" aria-hidden="true"></canvas>

  <!-- A11y shadow hitboxes (Phase 3.5): invisible buttons over branch tips for keyboard/SR users -->
  {#each hitboxes as hb (hb.id)}
    <button
      class="forest-hitbox"
      style:left="{(hb.x / hb.cw) * 100}%"
      style:top="{(hb.y / hb.ch) * 100}%"
      aria-label="{hb.label} — {hb.gateState.replace('_', ' ')}"
      onclick={() => { /* Phase 6 BranchTooltip click handler wired here */ }}
    ></button>
  {/each}

  <!-- Reduced-motion static badge (shown when prefers-reduced-motion: reduce) -->
  {#if !motionEnabled}
    <div class="forest-reduced-badge" role="status" aria-label="Animations paused — reduced motion">
      ◈ static
    </div>
  {/if}

  <!-- Live region: announces pulse events to screen readers (rate-limited 1/2s) -->
  <div role="status" aria-live="polite" aria-atomic="true" class="forest-live-region">
    {liveAnnouncement}
  </div>
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
    width: 100%;
    height: 100%;
    will-change: transform;
  }
  .forest-three {
    position: absolute;
    inset: 0;
    pointer-events: none;
    will-change: transform;
  }
  .forest-overlay {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    pointer-events: none;
    mix-blend-mode: screen;
    will-change: transform;
  }

  /* Shadow hitboxes: visible only to keyboard/SR users via focus ring */
  .forest-hitbox {
    position: absolute;
    transform: translate(-50%, -50%);
    width: 28px;
    height: 28px;
    background: transparent;
    border: none;
    cursor: pointer;
    padding: 0;
    /* Clip visually, show on focus for keyboard users */
    clip-path: inset(50%);
    border-radius: 50%;
  }
  .forest-hitbox:focus-visible {
    clip-path: none;
    outline: 2px solid #38bdf8;
    outline-offset: 2px;
    background: rgba(56, 189, 248, 0.15);
  }

  /* Reduced-motion badge */
  .forest-reduced-badge {
    position: absolute;
    top: 8px;
    right: 8px;
    font-size: 10px;
    font-family: monospace;
    color: #64748b;
    background: rgba(2, 4, 8, 0.75);
    padding: 3px 7px;
    border-radius: 4px;
    border: 1px solid #1e293b;
    pointer-events: none;
  }

  /* Live region: visually hidden, announced to screen readers */
  .forest-live-region {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip: rect(0 0 0 0);
    white-space: nowrap;
    pointer-events: none;
  }
</style>
