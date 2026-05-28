<script lang="ts">
  import * as THREE from 'three';
  import { EffectComposer } from 'three/examples/jsm/postprocessing/EffectComposer.js';
  import { RenderPass } from 'three/examples/jsm/postprocessing/RenderPass.js';
  import { UnrealBloomPass } from 'three/examples/jsm/postprocessing/UnrealBloomPass.js';
  import { OutputPass } from 'three/examples/jsm/postprocessing/OutputPass.js';
  import { gitforestTree, gitforestPulses } from '$lib/stores';
  import { get } from 'svelte/store';
  import type { GitForestTopology, BranchNode } from '$lib/gitforest';
  import { countActiveWorktrees, computeFadeLevel, polytopeClusterFor } from '$lib/gitforest';
  import { PulseLayer } from '$lib/pulseLayer';
  import { navigate } from '$lib/routes';
  import BranchTooltip from '$lib/../components/topology/BranchTooltip.svelte';
  import { fetchGitLog, commitTypeColor, localBranchName } from '$lib/gitlog';
  import type { GitLogData, GitCommit, GitBranch } from '$lib/gitlog';
  import { FOREST_REPO_NAMES } from '$lib/github';
  import { getToken } from '$lib/auth';

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

  // ─── Local filesystem paths for each FOREST_REPO_NAMES entry ────────────
  // Used by /api/git/log to fetch real commit history.
  // If a path fails to resolve, the component falls back to seed data.
  const REPO_LOCAL_PATHS: Record<string, string> = {
    'lightarchitects-sdk': `${window.location.origin.includes('localhost') ? '' : ''}/Users/kft/Projects/lightarchitects-sdk`,
    'SOUL-DEV':            '/Users/kft/Projects/SOUL/SOUL-DEV',
    'CORSO-DEV':           '/Users/kft/Projects/CORSO/MCP/CORSO-DEV',
  };

  // ─── Git log data per repo (fetched from /api/git/log) ────────────────────
  let gitLogs = $state<Map<string, GitLogData>>(new Map());

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

  // ── Phase 6: BranchTooltip hover state ───────────────────────────────────
  let tooltipNodeId = $state<string | null>(null);
  let tooltipAnchor = $state<DOMRect | null>(null);
  let hoverTimer: ReturnType<typeof setTimeout> | null = null;

  function showTooltip(nodeId: string, btn: HTMLButtonElement) {
    if (hoverTimer) { clearTimeout(hoverTimer); hoverTimer = null; }
    hoverTimer = setTimeout(() => {
      tooltipNodeId = nodeId;
      tooltipAnchor = btn.getBoundingClientRect();
    }, 150);
  }

  function hideTooltip() {
    if (hoverTimer) { clearTimeout(hoverTimer); hoverTimer = null; }
    tooltipNodeId = null;
    tooltipAnchor = null;
  }

  function handleHitboxClick(nodeId: string) {
    hideTooltip();
    navigate('/builds', {});
  }

  function handleContextMenu(e: MouseEvent, nodeId: string, btn: HTMLButtonElement) {
    e.preventDefault();
    tooltipNodeId = nodeId;
    tooltipAnchor = btn.getBoundingClientRect();
  }
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

  // ─── Fetch real git log data for each repo ────────────────────────────────
  $effect(() => {
    const token = getToken() ?? '';
    const names = Object.keys(REPO_LOCAL_PATHS) as (keyof typeof REPO_LOCAL_PATHS)[];
    for (const name of names) {
      const cwd = REPO_LOCAL_PATHS[name] ?? '';
      if (!cwd) continue;
      fetchGitLog(cwd, 40, token || undefined)
        .then(data => {
          gitLogs = new Map(gitLogs).set(name, data);
        })
        .catch(() => { /* fall back to seed data silently */ });
    }
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
    // Transparent background — canvas2d git graph renders below this layer.
    // scene.background intentionally omitted (alpha: true on renderer).
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

    const renderer = new THREE.WebGLRenderer({ antialias: true, alpha: true, powerPreference: 'high-performance' });
    renderer.setPixelRatio(Math.min(window.devicePixelRatio, 1.5));
    renderer.setSize(w, h);
    renderer.setClearColor(0x000000, 0);  // fully transparent clear
    renderer.toneMapping = THREE.ReinhardToneMapping;
    renderer.toneMappingExposure = 0.9;  // slightly dimmed — background atmosphere role
    while (container.firstChild) container.removeChild(container.firstChild);
    container.appendChild(renderer.domElement);

    // Holographic bloom pipeline (§19)
    const composer = new EffectComposer(renderer);
    composer.addPass(new RenderPass(scene, camera));
    const bloomPass = new UnrealBloomPass(new THREE.Vector2(w, h), 0.40, 0.30, 0.18);
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

    let animId: number;

    function hexColor(n: number): string {
      return `#${n.toString(16).padStart(6, '0')}`;
    }

    /** Hash a string to one of 7 branch-lane accent colors. */
    function hashColor(str: string): number {
      let h = 0;
      for (let i = 0; i < str.length; i++) { h = ((h << 5) - h) + str.charCodeAt(i); h |= 0; }
      const palette = [0x38bdf8, 0x4dff8e, 0xa78bfa, 0xf97316, 0x4dffe6, 0xff7eb6, 0xf5d440];
      return palette[Math.abs(h) % palette.length] ?? 0x94a3b8;
    }

    /** Convert hex int to CSS color string with optional alpha hex suffix. */
    function hc(n: number, a = ''): string { return hexColor(n) + a; }

    // Hover state for commit tooltip
    let hoveredRepo: string | null = null;
    let hoveredCommitIdx = -1;

    // ─── Draw commit tooltip overlay ────────────────────────────────────────
    function drawCommitTooltip(c: GitCommit, ax: number, ay: number, px: number, pw: number) {
      const W = 190; const H = 62;
      let tx = ax - W / 2;
      tx = Math.max(px + 4, Math.min(px + pw - W - 4, tx));
      const ty = ay < canvas.height * 0.5 ? ay + 14 : ay - H - 10;
      ctx.fillStyle = 'rgba(2,4,12,0.97)';
      ctx.strokeStyle = '#1e3a5f';
      ctx.lineWidth = 1;
      // @ts-expect-error — roundRect is modern Canvas 2D API, present in Chrome 99+
      if (ctx.roundRect) { ctx.beginPath(); (ctx as CanvasRenderingContext2D & { roundRect: (x:number,y:number,w:number,h:number,r:number)=>void }).roundRect(tx, ty, W, H, 3); ctx.fill(); ctx.stroke(); }
      else { ctx.fillRect(tx, ty, W, H); ctx.strokeRect(tx, ty, W, H); }
      ctx.font = `700 8.5px 'JetBrains Mono Variable', monospace`;
      ctx.fillStyle = hc(commitTypeColor(c.message));
      ctx.fillText(c.shortSha, tx + 8, ty + 14);
      ctx.font = `8.5px 'JetBrains Mono Variable', monospace`;
      ctx.fillStyle = '#e2e8f0';
      ctx.fillText(c.message.slice(0, 28) + (c.message.length > 28 ? '…' : ''), tx + 8, ty + 27);
      ctx.font = `7.5px 'JetBrains Mono Variable', monospace`;
      ctx.fillStyle = '#64748b';
      ctx.fillText(c.author, tx + 8, ty + 40);
      ctx.fillText(c.timestamp.slice(0, 16), tx + 8, ty + 52);
    }

    // ─── Draw a repo panel using real git log data ───────────────────────────
    function drawGitGraph(
      data: GitLogData, repo: Repo, ri: number,
      panelX: number, panelW: number,
      graphY0: number, graphH: number,
      t: number,
    ) {
      const [tcN] = repoTrunkColor(ri);
      const tc = hc(tcN);

      // Lane assignment: main=0, feat/→1,2,3 upward, fix/→-1,-2 downward
      const laneMap = new Map<string, number>();
      const allLocal = data.branches
        .map(b => localBranchName(b.name))
        .filter(n => !n.startsWith('remotes/') && n.length > 0);
      const sorted = [
        ...allLocal.filter(n => n === 'main' || n === 'master'),
        ...allLocal.filter(n => n.startsWith('feat/')),
        ...allLocal.filter(n => !['main','master'].includes(n) && !n.startsWith('feat/') && !n.startsWith('fix/')),
        ...allLocal.filter(n => n.startsWith('fix/')),
      ];
      let laneUp = 0; let laneDn = -1;
      sorted.forEach(n => {
        if (n === 'main' || n === 'master') laneMap.set(n, 0);
        else if (n.startsWith('fix/')) laneMap.set(n, laneDn--);
        else laneMap.set(n, ++laneUp);
      });
      const maxUp = laneUp; const maxDn = Math.abs(laneDn) - 1;
      const totalLanes = maxUp + maxDn + 1;
      const laneSpacing = graphH / Math.max(totalLanes + 1, 4);
      function laneY(lane: number): number {
        return graphY0 + (maxUp - lane + 1) * laneSpacing;
      }

      // Commit area geometry
      const RAIL_W = 68;
      const cax = panelX + RAIL_W;
      const caw = panelW - RAIL_W - 6;
      const commits = data.commits;
      const N = Math.min(commits.length, 22);
      const sp = N > 1 ? caw / (N - 1) : caw;

      const shaIdx = new Map<string, number>(commits.map((c, i) => [c.sha, i]));
      const shortIdx = new Map<string, number>(commits.map((c, i) => [c.shortSha, i]));

      function getCLane(c: GitCommit): number {
        for (const ref of c.refs) {
          const stripped = ref.replace(/^HEAD -> /, '').replace(/^(origin|upstream)\//, '').trim();
          const l = laneMap.get(stripped); if (l !== undefined) return l;
        }
        const pSha = c.parentShas[0] ?? '';
        const pIdx = shaIdx.get(pSha) ?? shortIdx.get(pSha) ?? -1;
        return pIdx >= 0 ? getCLane(commits[pIdx]!) : 0;
      }
      function cx2(idx: number): number {
        return N <= 1 ? cax + caw / 2 : cax + (N - 1 - idx) * sp;
      }

      // Draw dashed lane lines + labels
      const drawnLanes = new Set<number>();
      sorted.slice(0, 8).forEach(name => {
        const lane = laneMap.get(name) ?? 0;
        if (drawnLanes.has(lane)) return; drawnLanes.add(lane);
        const ly = laneY(lane);
        const lcolor = lane === 0 ? tc : hc(hashColor(name));
        ctx.strokeStyle = lcolor + '35';
        ctx.lineWidth = 0.8;
        ctx.setLineDash([3, 5]);
        ctx.beginPath(); ctx.moveTo(cax, ly); ctx.lineTo(cax + caw, ly); ctx.stroke();
        ctx.setLineDash([]);

        const brLabel = name.replace(/^(feat|fix|chore|refactor)\//, '').slice(0, 12);
        ctx.font = `8px 'JetBrains Mono Variable', monospace`;
        ctx.fillStyle = lcolor + 'bb';
        ctx.textAlign = 'right';
        ctx.fillText(brLabel, cax - 5, ly + 3);
        ctx.textAlign = 'left';

        const isCurrent = data.branches.some(b => b.isCurrent && localBranchName(b.name) === name);
        if (isCurrent) {
          ctx.font = `700 7px 'JetBrains Mono Variable', monospace`;
          ctx.fillStyle = lcolor;
          ctx.fillText('◈', panelX + 2, ly + 3);
        }
      });

      // Draw edges
      for (let i = 0; i < N - 1; i++) {
        const c = commits[i]!; const nc = commits[i + 1]!;
        const x1 = cx2(i); const y1 = laneY(getCLane(c));
        const x2 = cx2(i + 1); const y2 = laneY(getCLane(nc));
        const lane = getCLane(c);
        const edgeColor = lane === 0 ? tc : hc(hashColor(sorted.find(n => (laneMap.get(n) ?? -99) === lane) ?? ''));
        ctx.strokeStyle = edgeColor + '60'; ctx.lineWidth = 1.2;
        ctx.beginPath(); ctx.moveTo(x2, y2);
        if (Math.abs(y1 - y2) < 2) ctx.lineTo(x1, y1);
        else ctx.bezierCurveTo(x2 + sp * 0.45, y2, x1 - sp * 0.45, y1, x1, y1);
        ctx.stroke();
        // Merge arcs
        if (c.parentShas.length > 1) {
          c.parentShas.slice(1).forEach(ps => {
            const pi = shaIdx.get(ps) ?? shortIdx.get(ps) ?? -1;
            if (pi < 0 || pi >= N) return;
            const mx = cx2(pi); const my = laneY(getCLane(commits[pi]!));
            ctx.strokeStyle = '#a78bfa55'; ctx.lineWidth = 0.8;
            ctx.beginPath(); ctx.moveTo(x1, y1);
            ctx.quadraticCurveTo((x1 + mx) / 2, y1, mx, my); ctx.stroke();
          });
        }
      }

      // Draw commit nodes
      for (let i = 0; i < N; i++) {
        const c = commits[i]!;
        const x = cx2(i); const lane = getCLane(c); const y = laneY(lane);
        const isMerge = c.parentShas.length > 1;
        const dotCol = commitTypeColor(c.message);
        const isHovered = hoveredRepo === repo.name && hoveredCommitIdx === i;
        const pulse = 0.7 + 0.3 * Math.sin(t * 1.6 + i * 0.8);

        if (isHovered) {
          ctx.beginPath(); ctx.arc(x, y, 9, 0, Math.PI * 2);
          ctx.strokeStyle = hc(dotCol, 'aa'); ctx.lineWidth = 1.5; ctx.stroke();
        }
        if (isMerge) {
          ctx.beginPath(); ctx.arc(x, y, 6, 0, Math.PI * 2);
          ctx.strokeStyle = '#a78bfa55'; ctx.lineWidth = 0.8; ctx.stroke();
        }
        ctx.beginPath(); ctx.arc(x, y, isMerge ? 4.5 : 3.5, 0, Math.PI * 2);
        ctx.fillStyle = hc(dotCol); ctx.globalAlpha = lane === 0 ? 0.95 : 0.70;
        ctx.fill(); ctx.globalAlpha = 1;

        // SHA below node
        ctx.font = `7.5px 'JetBrains Mono Variable', monospace`;
        ctx.fillStyle = '#334155'; ctx.textAlign = 'center';
        ctx.fillText(c.shortSha, x, y + 13); ctx.textAlign = 'left';

        // Message above for recent main-lane commits
        if (lane === 0 && i < 6) {
          const m = c.message.slice(0, 12) + (c.message.length > 12 ? '…' : '');
          ctx.font = `7px 'JetBrains Mono Variable', monospace`;
          ctx.fillStyle = hc(dotCol) + (isHovered ? 'ee' : '80');
          ctx.textAlign = 'center'; ctx.fillText(m, x, y - 7); ctx.textAlign = 'left';
        }

        // Tooltip
        if (isHovered) drawCommitTooltip(c, x, y, panelX, panelW);

        // A11y hitbox
        hitboxPending.push({ id: `${repo.name}/${c.shortSha}`, label: `${c.shortSha}: ${c.message}`, gateState: 'clean', x, y, cw: canvas.width, ch: canvas.height });
      }
    }

    // ─── Fallback graph when git data not yet available ──────────────────────
    function drawSeedGraph(repo: Repo, ri: number, panelX: number, panelW: number, graphY0: number, graphH: number, t: number) {
      const [tcN] = repoTrunkColor(ri);
      const tc = hc(tcN);
      const cax = panelX + 10;
      const centerY = graphY0 + graphH * 0.5;
      // Minimal "loading" indicator
      ctx.font = `8px 'JetBrains Mono Variable', monospace`;
      ctx.fillStyle = tc + '55';
      ctx.textAlign = 'left';
      const dots = '.'.repeat(1 + Math.floor(t) % 3);
      ctx.fillText(`loading git data${dots}`, cax, centerY);
      ctx.textAlign = 'left';
      // Pulse line
      ctx.strokeStyle = tc + '22'; ctx.lineWidth = 1;
      ctx.beginPath(); ctx.moveTo(cax, centerY + 10); ctx.lineTo(panelX + panelW - 6, centerY + 10); ctx.stroke();
      // Seed branches (placeholder dots)
      repo.branches.slice(0, 3).forEach((b, bi) => {
        const bColor = hc(GATE_COLORS[b.gateState]);
        const bx = cax + (panelW - 80) * (0.3 + bi * 0.3);
        const by = centerY + (bi - 1) * 16;
        ctx.beginPath(); ctx.arc(bx, by, 3, 0, Math.PI * 2);
        ctx.fillStyle = bColor + '66'; ctx.fill();
        ctx.font = `7px 'JetBrains Mono Variable', monospace`;
        ctx.fillStyle = bColor + '66'; ctx.textAlign = 'center';
        ctx.fillText(b.name.slice(0, 10), bx, by + 10); ctx.textAlign = 'left';
      });
    }

    // ─── Main draw loop ──────────────────────────────────────────────────────
    const HEADER_H = 38;
    const PAD_X = 8;

    function draw() {
      animId = requestAnimationFrame(draw);
      const cw = canvas.width;
      const ch = canvas.height;
      const t = performance.now() * 0.001;

      // Semi-transparent fill so Three.js polytopes show through subtly
      ctx.clearRect(0, 0, cw, ch);
      ctx.fillStyle = 'rgba(2,4,8,0.93)';
      ctx.fillRect(0, 0, cw, ch);

      // Blueprint grid
      const gs = Math.max(cw, ch) / 22;
      ctx.lineWidth = 0.4;
      for (let x = 0; x < cw; x += gs) {
        ctx.strokeStyle = (Math.round(x / gs) % 4 === 0) ? 'rgba(26,110,192,0.16)' : 'rgba(26,110,192,0.05)';
        ctx.beginPath(); ctx.moveTo(x, 0); ctx.lineTo(x, ch); ctx.stroke();
      }
      for (let y = 0; y < ch; y += gs) {
        ctx.strokeStyle = (Math.round(y / gs) % 4 === 0) ? 'rgba(26,110,192,0.16)' : 'rgba(26,110,192,0.05)';
        ctx.beginPath(); ctx.moveTo(0, y); ctx.lineTo(cw, y); ctx.stroke();
      }

      const n = repos.length;
      const panelW = (cw - PAD_X * 2 - (n - 1)) / n;
      l1ColW = panelW;
      l1PadX = PAD_X;

      // Hero mode: full-panel single repo
      if (selectedRepoIdx !== null && selectedRepoIdx < repos.length) {
        const repo = repos[selectedRepoIdx]!;
        const ri = selectedRepoIdx;
        const data = gitLogs.get(repo.name);
        drawPanelHeader(repo, ri, PAD_X, cw - PAD_X * 2, ch, true);
        if (data && data.commits.length > 0) {
          drawGitGraph(data, repo, ri, PAD_X, cw - PAD_X * 2, HEADER_H + 6, ch - HEADER_H - 6, t);
        } else {
          drawSeedGraph(repo, ri, PAD_X, cw - PAD_X * 2, HEADER_H + 6, ch - HEADER_H - 6, t);
        }
        // Back button
        ctx.font = '700 9px monospace';
        ctx.fillStyle = '#475569';
        ctx.fillText('← ALL', PAD_X + 4, ch - 8);
        hitboxPending = [];
        hitboxFrame = (hitboxFrame + 1) % 15;
        if (hitboxFrame === 0) hitboxes = hitboxPending;
        return;
      }

      repos.forEach((repo, ri) => {
        const panelX = PAD_X + ri * (panelW + 1);
        const data = gitLogs.get(repo.name);
        drawPanelHeader(repo, ri, panelX, panelW, ch, false);
        if (data && data.commits.length > 0) {
          drawGitGraph(data, repo, ri, panelX, panelW, HEADER_H + 4, ch - HEADER_H - 8, t);
        } else {
          drawSeedGraph(repo, ri, panelX, panelW, HEADER_H + 4, ch - HEADER_H - 8, t);
        }
        // Panel divider
        if (ri < repos.length - 1) {
          ctx.strokeStyle = 'rgba(30,58,95,0.45)';
          ctx.lineWidth = 0.5;
          ctx.beginPath();
          ctx.moveTo(panelX + panelW + 0.5, HEADER_H);
          ctx.lineTo(panelX + panelW + 0.5, ch - 4);
          ctx.stroke();
        }
      });

      hitboxFrame = (hitboxFrame + 1) % 15;
      if (hitboxFrame === 0) hitboxes = hitboxPending;
      hitboxPending = [];
    }

    // ─── Panel header: repo name + stats ────────────────────────────────────
    function drawPanelHeader(repo: Repo, ri: number, panelX: number, panelW: number, ch: number, hero: boolean) {
      const [tcN, wcN] = repoTrunkColor(ri);
      const tc = hc(tcN); const wc = hc(wcN);
      const data = gitLogs.get(repo.name);
      const commitCount = data ? data.commits.length : repo.commitCount;
      const branchCount = data
        ? new Set(data.branches.map(b => localBranchName(b.name)).filter(n => n && !n.includes('HEAD'))).size
        : repo.branches.length;
      const activeBranch = data?.branches.find(b => b.isCurrent)?.name ?? 'main';

      // Subtle header background
      ctx.fillStyle = 'rgba(2,4,12,0.6)';
      ctx.fillRect(panelX, 0, panelW, HEADER_H - 2);

      // Accent bar at top
      const grad = ctx.createLinearGradient(panelX, 0, panelX + panelW, 0);
      grad.addColorStop(0, tc + '00'); grad.addColorStop(0.3, tc + 'cc'); grad.addColorStop(1, wc + '44');
      ctx.fillStyle = grad;
      ctx.fillRect(panelX, 0, panelW, 2);

      const fontSize = hero ? 12 : Math.max(9, Math.min(11, Math.floor(panelW * 0.065)));
      ctx.font = `700 ${fontSize}px 'JetBrains Mono Variable', monospace`;
      ctx.fillStyle = tc;
      ctx.textAlign = 'left';
      const nameLabel = hero ? repo.name : repo.name.slice(0, Math.floor(panelW / (fontSize * 0.62)));
      ctx.fillText(nameLabel, panelX + 5, 15);

      ctx.font = `${Math.max(7, fontSize - 2)}px 'JetBrains Mono Variable', monospace`;
      ctx.fillStyle = '#475569';
      ctx.fillText(`${commitCount}+ c · ${branchCount} br · ${activeBranch.slice(0, 12)}`, panelX + 5, 28);

      // Header bottom rule
      ctx.strokeStyle = tc + '18';
      ctx.lineWidth = 0.5;
      ctx.beginPath(); ctx.moveTo(panelX, HEADER_H); ctx.lineTo(panelX + panelW, HEADER_H); ctx.stroke();
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
  <!-- Phase 6: hover → BranchTooltip; click → navigate('/builds'); contextmenu → pin tooltip -->
  {#each hitboxes as hb (hb.id)}
    <button
      class="forest-hitbox"
      style:left="{(hb.x / hb.cw) * 100}%"
      style:top="{(hb.y / hb.ch) * 100}%"
      aria-label="{hb.label} — {hb.gateState.replace('_', ' ')}"
      aria-describedby={tooltipNodeId === hb.id ? `branch-tooltip-${hb.id}` : undefined}
      onmouseenter={(e) => showTooltip(hb.id, e.currentTarget as HTMLButtonElement)}
      onmouseleave={hideTooltip}
      onfocusin={(e) => showTooltip(hb.id, e.currentTarget as HTMLButtonElement)}
      onfocusout={hideTooltip}
      onclick={() => handleHitboxClick(hb.id)}
      oncontextmenu={(e) => handleContextMenu(e, hb.id, e.currentTarget as HTMLButtonElement)}
    ></button>
  {/each}

  <!-- Phase 6: BranchTooltip — rendered when a hitbox is hovered/focused -->
  {#if tooltipNodeId && tooltipAnchor}
    <BranchTooltip
      nodeId={tooltipNodeId}
      anchor={tooltipAnchor}
      onclose={hideTooltip}
    />
  {/if}

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
    z-index: 2;
    will-change: transform;
  }
  .forest-three {
    position: absolute;
    inset: 0;
    z-index: 1;
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
