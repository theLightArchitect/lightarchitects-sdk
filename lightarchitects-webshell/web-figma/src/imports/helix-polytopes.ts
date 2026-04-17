/**
 * Helix Polytope Manager — creates and animates 4D polytope meshes
 * embedded in the Hero3D helix scene. Each project gets a polytope
 * placed at its entity strand position (inner ring) or in the dust
 * field (outer ring).
 *
 * Visual differentiation ("planet, not star"):
 *   - AdditiveBlending on edges → glows through bloom
 *   - Opacity breathing (3s sine, staggered) → only luminance-varying elements
 *   - Invisible collider spheres → raycasting targets (enlarged on mobile)
 */
import * as THREE from 'three';
import { getPolytope4D, type Polytope4DType, type Vec4 } from './polytopes4d';
import { getEntityCenter, getSoulCenter, getPrimaryFrame } from './helix-math';
import { PROJECTS, ENTITY_INDEX, type ProjectEntry } from '../data/projects';
import { BLOG_POSTS, type BlogPost } from '../data/blog-posts';

// --- Types ---

interface HelixPolytope {
  project: ProjectEntry;
  edgeLines: THREE.LineSegments;
  vertexPoints: THREE.Points;
  glowOrb: THREE.Points;
  collider: THREE.Mesh;
  edgePositions: Float32Array;
  edgeColors: Float32Array;
  vertPositions: Float32Array;
  data: ReturnType<typeof getPolytope4D>;
  rotated: Vec4[];
  projected: [number, number, number][];
  worldPos: THREE.Vector3;
  currentScale: number;
  baseVertSize: number;
  /** Outer-tier orbit data (null for inner-tier) */
  orbit: { anchor: THREE.Vector3; radius: number; speed: number; phase: number } | null;
  /** Dynamic Y repulsion state (inner-tier only) */
  homeY: number;           // Original Y position (anchor)
  dynamicYOffset: number;  // Current drift from home
  velocityY: number;       // Current drift velocity
  entityIdx: number;       // Entity index for recalculating helix position
  /** Blog post slug — set for blog polytopes, null for project polytopes */
  blogSlug: string | null;
}

// --- Constants ---

const VIEW_DIST = 2.5;
const OVERVIEW_SCALE = 0.08;   // Small markers — expand on focus to reveal wireframe detail
const COLLIDER_RADIUS = 0.25;  // Hit target for interaction
const MOBILE_COLLIDER_MULTIPLIER = 3.0;
const CONVERGE_PARTICLE_COUNT = 60;  // Particles that fly toward the polytope
const CONVERGE_SCATTER_RADIUS = 1.5; // How far particles start from the polytope
const CONVERGE_DURATION = 800;       // ms to fully converge

// --- Manager ---

/** Creates a tight, hot-center glow texture — bright core, steep falloff */
function createHotGlowTexture(): THREE.CanvasTexture {
  const canvas = document.createElement('canvas');
  canvas.width = 64;
  canvas.height = 64;
  const ctx = canvas.getContext('2d');
  if (ctx) {
    const gradient = ctx.createRadialGradient(32, 32, 0, 32, 32, 32);
    gradient.addColorStop(0, 'rgba(255, 255, 255, 1)');
    gradient.addColorStop(0.12, 'rgba(255, 255, 255, 0.92)');
    gradient.addColorStop(0.25, 'rgba(255, 255, 255, 0.55)');
    gradient.addColorStop(0.45, 'rgba(255, 255, 255, 0.15)');
    gradient.addColorStop(0.65, 'rgba(255, 255, 255, 0.03)');
    gradient.addColorStop(1, 'rgba(255, 255, 255, 0)');
    ctx.fillStyle = gradient;
    ctx.fillRect(0, 0, 64, 64);
  }
  return new THREE.CanvasTexture(canvas);
}

export class HelixPolytopeManager {
  private polytopes: HelixPolytope[] = [];
  private group: THREE.Group;
  private outerGroup: THREE.Group;    // Non-rotating group for outer-tier polytopes
  private globalOpacityScale = 1.0;
  private rotationSpeedMultiplier = 1.0;
  private focusedProjectId: string | null = null;
  private hoveredProjectId: string | null = null;
  private focusGlowScale = 1.0;       // 1.0 = normal, ramps to 3.0 when focused
  private isMobile: boolean;

  // Convergence particle system
  private convergePoints: THREE.Points | null = null;
  private convergePositions: Float32Array | null = null;
  private convergeColors: Float32Array | null = null;
  private convergeStartPositions: Float32Array | null = null;
  private convergeTargetPositions: Float32Array | null = null;
  private convergeStartTime = 0;
  private convergeActive = false;
  private convergeProjectId: string | null = null;
  private convergeColor = new THREE.Color();
  private glowTexture: THREE.Texture | null = null;
  private hotGlowTexture: THREE.CanvasTexture;

  constructor(parentGroup: THREE.Group, glowTexture?: THREE.Texture, outerParent?: THREE.Group) {
    this.group = new THREE.Group();
    parentGroup.add(this.group);
    // Outer group lives in a non-rotating parent (syncs Y scroll but not rotation)
    this.outerGroup = new THREE.Group();
    (outerParent ?? parentGroup).add(this.outerGroup);
    this.glowTexture = glowTexture ?? null;
    this.hotGlowTexture = createHotGlowTexture();
    this.isMobile = typeof window !== 'undefined' &&
      window.matchMedia('(pointer: coarse)').matches;
    this.createPolytopes();
    this.createConvergenceSystem();
  }

  /** Returns the outer group so Hero3D can sync its Y position with the helix */
  getOuterGroup(): THREE.Group {
    return this.outerGroup;
  }

  private createPolytopes() {
    const outerProjects = PROJECTS.filter(p => p.tier === 'outer');

    for (const project of PROJECTS) {
      const data = getPolytope4D(project.polytope);

      // Compute world position based on tier.
      // Each inner polytope gets a unique Y offset so they stagger vertically
      // and are distinguishable at all rotation angles.
      const POLYTOPE_BASE_Y = -10.0;
      const POLYTOPE_Y_OFFSETS: Record<string, number> = {
        soul:    0,       // Center anchor
        eva:    -1.5,     // Rail 0: between ayin and seraph
        corso:   1.2,     // Rail 0: below center (nudged up from 0.8)
        quantum: 2.5,     // Rail 0: top (highest, but not off screen)
        seraph:  -3.0,    // Rail 1: bottom (lowest)
        larch:   2.0,     // Rail 1: between quantum and corso (nudged up from 1.6)
        ayin:   -0.8,     // Rail 1: slightly above center
      };
      const yOffset = POLYTOPE_Y_OFFSETS[project.id] ?? 0;
      const POLYTOPE_Y = POLYTOPE_BASE_Y + yOffset;

      let worldPos: THREE.Vector3;
      if (project.id === 'soul') {
        worldPos = getSoulCenter(POLYTOPE_Y);
      } else if (project.id in ENTITY_INDEX) {
        worldPos = getEntityCenter(ENTITY_INDEX[project.id], POLYTOPE_Y).E;
      } else {
        // Outer ring: fixed quadrant anchors with tight independent orbits.
        // Not part of the rotating helix — lives in outerGroup (syncs Y scroll only).
        const idx = outerProjects.indexOf(project);
        // Fixed anchor positions flanking the helix, Y between quantum (-7.5) and soul (-10.0)
        const OUTER_ANCHORS = [
          new THREE.Vector3(-3.2, -8.5, -2.0),   // BEREAN: upper-left, pushed back
          new THREE.Vector3( 3.2, -9.0, -2.5),   // GYM: right, mid-height, pushed back
          new THREE.Vector3(-2.8, -9.5, -1.5),   // LA-SDK: left, lower, pushed back
        ];
        worldPos = OUTER_ANCHORS[idx] ?? new THREE.Vector3(4.0, -9.0, 2.0);
      }

      // Pre-allocate buffers
      const edgePositions = new Float32Array(data.edges.length * 6);
      const edgeColors = new Float32Array(data.edges.length * 6);
      const vertPositions = new Float32Array(data.vertices.length * 3);

      // Edge geometry
      const edgePosAttr = new THREE.BufferAttribute(edgePositions, 3);
      edgePosAttr.setUsage(THREE.DynamicDrawUsage);
      const edgeColAttr = new THREE.BufferAttribute(edgeColors, 3);
      edgeColAttr.setUsage(THREE.DynamicDrawUsage);
      const edgeGeom = new THREE.BufferGeometry();
      edgeGeom.setAttribute('position', edgePosAttr);
      edgeGeom.setAttribute('color', edgeColAttr);

      const edgeMat = new THREE.LineBasicMaterial({
        vertexColors: true,
        transparent: true,
        opacity: 1.0,
        blending: THREE.AdditiveBlending,
        depthWrite: false,
        linewidth: 2,
      });
      const edgeLines = new THREE.LineSegments(edgeGeom, edgeMat);
      edgeLines.position.copy(worldPos);

      // Vertex points (glow dots at each 4D vertex)
      const vertPosAttr = new THREE.BufferAttribute(vertPositions, 3);
      vertPosAttr.setUsage(THREE.DynamicDrawUsage);
      const vertGeom = new THREE.BufferGeometry();
      vertGeom.setAttribute('position', vertPosAttr);
      // Tiny vertex dots — the wireframe edges define the shape, dots just mark vertices
      const vertSize = data.vertices.length > 16 ? 0.02 : data.vertices.length > 10 ? 0.03 : 0.04;
      const vertMat = new THREE.PointsMaterial({
        color: project.color,
        size: vertSize,
        transparent: true,
        opacity: 0.9,
        sizeAttenuation: true,
        blending: THREE.AdditiveBlending,
        depthWrite: false,
      });
      const vertexPoints = new THREE.Points(vertGeom, vertMat);
      vertexPoints.position.copy(worldPos);

      // Glow orb — single centered point with tight hot-center glow
      // Outer-tier projects are smaller and dimmer (distant satellites)
      const isOuter = project.tier === 'outer';
      const glowSize = isOuter ? 0.9 : 1.4;
      const glowOpacity = isOuter ? 0.7 : 1.0;
      const glowGeom = new THREE.BufferGeometry();
      glowGeom.setAttribute('position', new THREE.Float32BufferAttribute([0, 0, 0], 3));
      const glowOrbMat = new THREE.PointsMaterial({
        color: project.color,
        size: glowSize,
        transparent: true,
        opacity: glowOpacity,
        sizeAttenuation: true,
        blending: THREE.AdditiveBlending,
        depthWrite: false,
        depthTest: false,       // Always visible — glows through helix structure
        map: this.hotGlowTexture,
      });
      const glowOrb = new THREE.Points(glowGeom, glowOrbMat);
      glowOrb.position.copy(worldPos);
      glowOrb.userData = { baseOpacity: glowOpacity, baseSize: glowSize };

      // Invisible collider sphere for raycasting
      const colliderGeom = new THREE.SphereGeometry(COLLIDER_RADIUS, 8, 8);
      const colliderMat = new THREE.MeshBasicMaterial({ visible: false });
      const collider = new THREE.Mesh(colliderGeom, colliderMat);
      collider.position.copy(worldPos);
      collider.userData = { projectId: project.id };

      // Mobile touch target: 3× radius (meeting amendment 3)
      if (this.isMobile) {
        collider.scale.setScalar(MOBILE_COLLIDER_MULTIPLIER);
      }

      // Route to correct group: outer polytopes go to non-rotating outerGroup
      const targetGroup = isOuter ? this.outerGroup : this.group;
      targetGroup.add(glowOrb);
      targetGroup.add(edgeLines);
      targetGroup.add(vertexPoints);
      targetGroup.add(collider);

      // Outer polytopes get tight independent orbits around their anchor
      const orbit = isOuter ? {
        anchor: worldPos.clone(),
        radius: 0.12 + Math.random() * 0.08, // 0.12–0.2 unit very tight orbit
        speed: 0.4 + Math.random() * 0.3,    // Slightly different speeds
        phase: Math.random() * Math.PI * 2,   // Random start phase
      } : null;

      // Entity index for dynamic Y repositioning (-1 for soul/outer)
      const entityIdx = (project.id in ENTITY_INDEX) ? ENTITY_INDEX[project.id] : -1;

      this.polytopes.push({
        project,
        edgeLines,
        vertexPoints,
        glowOrb,
        collider,
        edgePositions,
        edgeColors,
        vertPositions,
        data,
        rotated: data.vertices.map(() => [0, 0, 0, 0] as Vec4),
        projected: data.vertices.map(() => [0, 0, 0] as [number, number, number]),
        worldPos,
        currentScale: OVERVIEW_SCALE,
        baseVertSize: vertSize,
        orbit,
        homeY: POLYTOPE_Y,
        dynamicYOffset: 0,
        velocityY: 0,
        entityIdx,
        blogSlug: null,
      });
    }

    // --- Blog polytopes: placed below project polytopes on the helix ---
    this.createBlogPolytopes();
  }

  private createBlogPolytopes() {
    const BLOG_BASE_Y = -20.0;  // Well below project polytopes (-10)
    const BLOG_SPACING = 2.5;   // Vertical spacing between blog posts

    for (let b = 0; b < BLOG_POSTS.length; b++) {
      const post = BLOG_POSTS[b];
      const data = getPolytope4D(post.polytope);
      const blogY = BLOG_BASE_Y - b * BLOG_SPACING;

      // Alternate blog posts between rail 0 and rail 1 for visual variety
      const rail = b % 2;
      const frame = getPrimaryFrame(rail, blogY);
      const worldPos = frame.C.clone();
      // Slight offset along the normal so they sit beside the rail, not on it
      worldPos.addScaledVector(frame.N, 0.4 * (b % 2 === 0 ? 1 : -1));

      // Re-use the same mesh creation pattern as project polytopes
      const edgePositions = new Float32Array(data.edges.length * 6);
      const edgeColors = new Float32Array(data.edges.length * 6);
      const vertPositions = new Float32Array(data.vertices.length * 3);

      const edgePosAttr = new THREE.BufferAttribute(edgePositions, 3);
      edgePosAttr.setUsage(THREE.DynamicDrawUsage);
      const edgeColAttr = new THREE.BufferAttribute(edgeColors, 3);
      edgeColAttr.setUsage(THREE.DynamicDrawUsage);
      const edgeGeom = new THREE.BufferGeometry();
      edgeGeom.setAttribute('position', edgePosAttr);
      edgeGeom.setAttribute('color', edgeColAttr);

      const edgeMat = new THREE.LineBasicMaterial({
        vertexColors: true,
        transparent: true,
        opacity: 1.0,
        blending: THREE.AdditiveBlending,
        depthWrite: false,
        linewidth: 2,
      });
      const edgeLines = new THREE.LineSegments(edgeGeom, edgeMat);
      edgeLines.position.copy(worldPos);

      const vertPosAttr = new THREE.BufferAttribute(vertPositions, 3);
      vertPosAttr.setUsage(THREE.DynamicDrawUsage);
      const vertGeom = new THREE.BufferGeometry();
      vertGeom.setAttribute('position', vertPosAttr);
      const vertSize = data.vertices.length > 16 ? 0.02 : data.vertices.length > 10 ? 0.03 : 0.04;
      const vertMat = new THREE.PointsMaterial({
        color: post.color,
        size: vertSize,
        transparent: true,
        opacity: 0.9,
        sizeAttenuation: true,
        blending: THREE.AdditiveBlending,
        depthWrite: false,
      });
      const vertexPoints = new THREE.Points(vertGeom, vertMat);
      vertexPoints.position.copy(worldPos);

      // Glow orb — slightly smaller than project polytopes
      const glowSize = 1.1;
      const glowOpacity = 0.9;
      const glowGeom = new THREE.BufferGeometry();
      glowGeom.setAttribute('position', new THREE.Float32BufferAttribute([0, 0, 0], 3));
      const glowOrbMat = new THREE.PointsMaterial({
        color: post.color,
        size: glowSize,
        transparent: true,
        opacity: glowOpacity,
        sizeAttenuation: true,
        blending: THREE.AdditiveBlending,
        depthWrite: false,
        depthTest: false,
        map: this.hotGlowTexture,
      });
      const glowOrb = new THREE.Points(glowGeom, glowOrbMat);
      glowOrb.position.copy(worldPos);
      glowOrb.userData = { baseOpacity: glowOpacity, baseSize: glowSize };

      // Collider
      const colliderGeom = new THREE.SphereGeometry(COLLIDER_RADIUS, 8, 8);
      const colliderMat = new THREE.MeshBasicMaterial({ visible: false });
      const collider = new THREE.Mesh(colliderGeom, colliderMat);
      collider.position.copy(worldPos);
      collider.userData = { projectId: `blog:${post.slug}` };

      if (this.isMobile) {
        collider.scale.setScalar(MOBILE_COLLIDER_MULTIPLIER);
      }

      // Blog polytopes rotate with the helix (inner group)
      this.group.add(glowOrb);
      this.group.add(edgeLines);
      this.group.add(vertexPoints);
      this.group.add(collider);

      // Create a minimal ProjectEntry facade so the polytope system works uniformly
      const projectFacade: ProjectEntry = {
        id: `blog:${post.slug}`,
        label: post.tags[0] ?? 'Writing',
        name: post.title,
        tagline: post.excerpt,
        color: post.color,
        polytope: post.polytope,
        polytopeLabel: '',
        vertexCount: data.vertices.length,
        edgeCount: data.edges.length,
        tier: 'inner',
        stats: [post.readTime, post.date, ...post.tags.map(t => `#${t}`)],
      };

      this.polytopes.push({
        project: projectFacade,
        edgeLines,
        vertexPoints,
        glowOrb,
        collider,
        edgePositions,
        edgeColors,
        vertPositions,
        data,
        rotated: data.vertices.map(() => [0, 0, 0, 0] as Vec4),
        projected: data.vertices.map(() => [0, 0, 0] as [number, number, number]),
        worldPos,
        currentScale: OVERVIEW_SCALE,
        baseVertSize: vertSize,
        orbit: null,
        homeY: blogY,
        dynamicYOffset: 0,
        velocityY: 0,
        entityIdx: -1,
        blogSlug: post.slug,
      });
    }
  }

  private createConvergenceSystem() {
    const positions = new Float32Array(CONVERGE_PARTICLE_COUNT * 3);
    const colors = new Float32Array(CONVERGE_PARTICLE_COUNT * 3);

    const geom = new THREE.BufferGeometry();
    const posAttr = new THREE.BufferAttribute(positions, 3);
    posAttr.setUsage(THREE.DynamicDrawUsage);
    geom.setAttribute('position', posAttr);
    const colAttr = new THREE.BufferAttribute(colors, 3);
    colAttr.setUsage(THREE.DynamicDrawUsage);
    geom.setAttribute('color', colAttr);

    const mat = new THREE.PointsMaterial({
      size: 0.06,
      vertexColors: true,
      transparent: true,
      opacity: 0,
      blending: THREE.AdditiveBlending,
      depthWrite: false,
      sizeAttenuation: true,
    });

    this.convergePoints = new THREE.Points(geom, mat);
    this.convergePositions = positions;
    this.convergeColors = colors;
    this.convergeStartPositions = new Float32Array(CONVERGE_PARTICLE_COUNT * 3);
    this.convergeTargetPositions = new Float32Array(CONVERGE_PARTICLE_COUNT * 3);
    this.group.add(this.convergePoints);
  }

  /** Start convergence — particles scatter around polytope then stream toward its vertices */
  startConvergence(projectId: string) {
    const poly = this.getPolytopeByProjectId(projectId);
    if (!poly || !this.convergeStartPositions || !this.convergeTargetPositions || !this.convergePositions) return;

    this.convergeActive = true;
    this.convergeProjectId = projectId;
    this.convergeStartTime = performance.now();
    this.convergeColor.set(poly.project.color);

    // Set particle start positions (scattered) and target positions (polytope vertices)
    for (let i = 0; i < CONVERGE_PARTICLE_COUNT; i++) {
      // Target: cycle through the polytope's projected vertex positions
      const vertIdx = i % poly.data.vertices.length;
      const proj = poly.projected[vertIdx];
      this.convergeTargetPositions[i * 3] = proj[0];
      this.convergeTargetPositions[i * 3 + 1] = proj[1];
      this.convergeTargetPositions[i * 3 + 2] = proj[2];

      // Start: random scatter around the polytope
      const angle = Math.random() * Math.PI * 2;
      const radius = CONVERGE_SCATTER_RADIUS * (0.3 + Math.random() * 0.7);
      const yOff = (Math.random() - 0.5) * CONVERGE_SCATTER_RADIUS;
      this.convergeStartPositions[i * 3] = Math.cos(angle) * radius;
      this.convergeStartPositions[i * 3 + 1] = yOff;
      this.convergeStartPositions[i * 3 + 2] = Math.sin(angle) * radius;

      // Initialize at start position
      this.convergePositions[i * 3] = this.convergeStartPositions[i * 3];
      this.convergePositions[i * 3 + 1] = this.convergeStartPositions[i * 3 + 1];
      this.convergePositions[i * 3 + 2] = this.convergeStartPositions[i * 3 + 2];
    }

    // Position the convergence system at the polytope's world position
    if (this.convergePoints) {
      this.convergePoints.position.copy(poly.worldPos);
      (this.convergePoints.material as THREE.PointsMaterial).opacity = 0.9;
    }
  }

  /** Stop convergence — scatter particles outward and fade */
  stopConvergence() {
    this.convergeActive = false;
    this.convergeProjectId = null;
    if (this.convergePoints) {
      (this.convergePoints.material as THREE.PointsMaterial).opacity = 0;
    }
  }

  /** Screen-space repulsion — slides inner polytopes along helix to avoid visual overlap */
  private applyScreenRepulsion(camera: THREE.Camera) {
    const MIN_SCREEN_DIST = 0.12;   // NDC units — below this, polytopes repel
    const REPULSION_FORCE = 0.008;  // How aggressively they push apart
    const HOME_SPRING = 0.02;       // How strongly they return to home position
    const DAMPING = 0.85;           // Velocity decay per frame
    const MAX_DRIFT = 1.5;          // Maximum Y drift from home

    const tempVec = new THREE.Vector3();
    const innerPolys: HelixPolytope[] = [];

    // Collect inner polytopes and their screen positions
    for (const poly of this.polytopes) {
      if (poly.project.tier === 'outer') continue;
      innerPolys.push(poly);
    }

    // Project to screen space (NDC: -1 to 1)
    const screenPos: { x: number; y: number }[] = [];
    for (const poly of innerPolys) {
      poly.glowOrb.getWorldPosition(tempVec);
      tempVec.project(camera);
      screenPos.push({ x: tempVec.x, y: tempVec.y });
    }

    // Pairwise repulsion (N=7, only 21 checks)
    for (let i = 0; i < innerPolys.length; i++) {
      for (let j = i + 1; j < innerPolys.length; j++) {
        const dx = screenPos[i].x - screenPos[j].x;
        const dy = screenPos[i].y - screenPos[j].y;
        const dist = Math.sqrt(dx * dx + dy * dy);

        if (dist < MIN_SCREEN_DIST && dist > 0.001) {
          const force = (MIN_SCREEN_DIST - dist) * REPULSION_FORCE;
          // Push apart along Y based on which one is higher in home position
          const sign = innerPolys[i].homeY > innerPolys[j].homeY ? 1 : -1;
          innerPolys[i].velocityY += force * sign;
          innerPolys[j].velocityY -= force * sign;
        }
      }
    }

    // Apply spring + damping + clamp
    for (const poly of innerPolys) {
      // Spring toward home
      poly.velocityY -= poly.dynamicYOffset * HOME_SPRING;
      // Damping
      poly.velocityY *= DAMPING;
      // Integrate
      poly.dynamicYOffset += poly.velocityY;
      // Clamp drift
      poly.dynamicYOffset = Math.max(-MAX_DRIFT, Math.min(MAX_DRIFT, poly.dynamicYOffset));
    }
  }

  /** Called every frame from Hero3D's animate loop */
  update(time: number, camera?: THREE.Camera) {
    // --- Screen-space repulsion: slide inner polytopes along helix to avoid overlap ---
    if (camera) {
      this.applyScreenRepulsion(camera);
    }

    for (let p = 0; p < this.polytopes.length; p++) {
      const poly = this.polytopes[p];
      const { data, edgePositions, edgeColors, vertPositions, rotated, projected } = poly;
      const baseColor = new THREE.Color(poly.project.color);
      // Non-focused polytopes shrink toward 0 when something is focused
      const isShrunk = this.focusedProjectId !== null && poly.project.id !== this.focusedProjectId;
      const scale = isShrunk ? poly.currentScale * this.globalOpacityScale : poly.currentScale;

      // Opacity breathing: inner = 3s cycle, outer = 5s (slower, distant star feel)
      const breathePeriod = poly.project.tier === 'outer' ? 5 : 3;
      const breathe = 0.7 + 0.3 * Math.sin(time * (2 * Math.PI / breathePeriod) + p * 0.7);
      const tierScale = poly.project.tier === 'outer' ? 0.6 : 1.0;
      const isFocused = poly.project.id === this.focusedProjectId;
      const isHovered = poly.project.id === this.hoveredProjectId && !isFocused;
      const focusT = isFocused ? (this.focusGlowScale - 1.0) / 0.6 : 0; // 0→1 as glow ramps

      const bvs = poly.baseVertSize;
      const glowMat = poly.glowOrb.material as THREE.PointsMaterial;
      const baseGlowOp = poly.glowOrb.userData.baseOpacity as number;
      const baseGlowSz = poly.glowOrb.userData.baseSize as number;

      // Hover pulse: fast 1s sine, 0→1
      const hoverPulse = isHovered ? 0.5 + 0.5 * Math.sin(time * Math.PI * 2) : 0;

      if (isFocused) {
        // FOCUSED: bright wireframe reveals, glow orb fades to subtle aura
        (poly.edgeLines.material as THREE.LineBasicMaterial).opacity = 1.0;
        (poly.vertexPoints.material as THREE.PointsMaterial).opacity = 0.4 - focusT * 0.2;
        (poly.vertexPoints.material as THREE.PointsMaterial).size = bvs * (1.0 - focusT * 0.5);
        glowMat.opacity = baseGlowOp * (1.0 - focusT * 0.8);
        glowMat.size = baseGlowSz * (1.0 - focusT * 0.5);
      } else if (isHovered) {
        // HOVERED: wireframe peeks through, glow orb pulses larger and brighter
        const hoverEdges = (0.3 + 0.2 * hoverPulse) * tierScale * this.globalOpacityScale;
        (poly.edgeLines.material as THREE.LineBasicMaterial).opacity = hoverEdges;
        (poly.vertexPoints.material as THREE.PointsMaterial).opacity = 0.5 * this.globalOpacityScale;
        (poly.vertexPoints.material as THREE.PointsMaterial).size = bvs * 1.3;
        glowMat.opacity = Math.min(1, (baseGlowOp + 0.15 * hoverPulse) * this.globalOpacityScale);
        glowMat.size = baseGlowSz * (1.15 + 0.15 * hoverPulse);
        // Slightly expand the wireframe scale
        poly.currentScale = OVERVIEW_SCALE * (1.3 + 0.2 * hoverPulse);
      } else {
        // UNFOCUSED: glowing orb of light, wireframe barely visible
        const dimEdges = 0.15 * breathe * tierScale * this.globalOpacityScale;
        (poly.edgeLines.material as THREE.LineBasicMaterial).opacity = dimEdges;
        (poly.vertexPoints.material as THREE.PointsMaterial).opacity =
          0.3 * this.globalOpacityScale;
        (poly.vertexPoints.material as THREE.PointsMaterial).size = bvs;
        glowMat.opacity = (baseGlowOp - 0.05 + 0.1 * breathe) * this.globalOpacityScale;
        glowMat.size = baseGlowSz;
        // Reset scale if was hovered
        if (poly.currentScale !== OVERVIEW_SCALE && !poly.orbit) {
          poly.currentScale = OVERVIEW_SCALE;
        }
      }

      // Outer-tier: update tight orbital position around anchor
      if (poly.orbit) {
        const { anchor, radius, speed, phase } = poly.orbit;
        const ox = anchor.x + radius * Math.cos(time * speed + phase);
        const oy = anchor.y + radius * 0.3 * Math.sin(time * speed * 1.3 + phase);
        const oz = anchor.z + radius * Math.sin(time * speed + phase);
        poly.worldPos.set(ox, oy, oz);
        poly.edgeLines.position.set(ox, oy, oz);
        poly.vertexPoints.position.set(ox, oy, oz);
        poly.glowOrb.position.set(ox, oy, oz);
        poly.collider.position.set(ox, oy, oz);
      }

      // Inner-tier: recompute helix position if dynamicYOffset has changed
      if (!poly.orbit && Math.abs(poly.dynamicYOffset) > 0.001) {
        const newY = poly.homeY + poly.dynamicYOffset;
        let newPos: THREE.Vector3;
        if (poly.project.id === 'soul') {
          newPos = getSoulCenter(newY);
        } else if (poly.entityIdx >= 0) {
          newPos = getEntityCenter(poly.entityIdx, newY).E;
        } else {
          newPos = poly.worldPos; // Fallback — shouldn't happen for inner
        }
        poly.worldPos.copy(newPos);
        poly.edgeLines.position.copy(newPos);
        poly.vertexPoints.position.copy(newPos);
        poly.glowOrb.position.copy(newPos);
        poly.collider.position.copy(newPos);
      }

      // 4D rotation (speed multiplied during fly-in per spec timeline)
      const rotSpeed = this.rotationSpeedMultiplier;
      const c1 = Math.cos(time * 0.35 * rotSpeed);
      const s1 = Math.sin(time * 0.35 * rotSpeed);
      const c2 = Math.cos(time * 0.22 * rotSpeed);
      const s2 = Math.sin(time * 0.22 * rotSpeed);

      for (let i = 0; i < data.vertices.length; i++) {
        const v = data.vertices[i];

        // XW plane rotation
        const x1 = v[0] * c1 - v[3] * s1;
        const w1 = v[0] * s1 + v[3] * c1;
        // YZ plane rotation
        const y1 = v[1] * c2 - v[2] * s2;
        const z1 = v[1] * s2 + v[2] * c2;

        rotated[i][0] = x1;
        rotated[i][1] = y1;
        rotated[i][2] = z1;
        rotated[i][3] = w1;

        // Stereographic projection 4D → 3D
        const dw = VIEW_DIST - w1;
        const projScale = (VIEW_DIST / Math.max(dw, 0.15)) * scale;
        projected[i][0] = x1 * projScale;
        projected[i][1] = y1 * projScale;
        projected[i][2] = z1 * projScale;

        vertPositions[i * 3] = projected[i][0];
        vertPositions[i * 3 + 1] = projected[i][1];
        vertPositions[i * 3 + 2] = projected[i][2];
      }

      // Update edge positions + depth-based vertex colors
      for (let i = 0; i < data.edges.length; i++) {
        const [a, b] = data.edges[i];
        const pa = projected[a], pb = projected[b];

        edgePositions[i * 6] = pa[0];
        edgePositions[i * 6 + 1] = pa[1];
        edgePositions[i * 6 + 2] = pa[2];
        edgePositions[i * 6 + 3] = pb[0];
        edgePositions[i * 6 + 4] = pb[1];
        edgePositions[i * 6 + 5] = pb[2];

        // Depth-based brightness: closer in w → brighter
        const da = 0.7 + 0.3 * Math.max(0, Math.min(1, (rotated[a][3] + 1.2) / 2.4));
        const db = 0.7 + 0.3 * Math.max(0, Math.min(1, (rotated[b][3] + 1.2) / 2.4));
        // Focused: blend color toward white for uniform luminance across all hues
        const whiteMix = isFocused ? focusT * 0.5 : 0; // 0 → 50% white at full focus
        const edgeR = baseColor.r * (1 - whiteMix) + whiteMix;
        const edgeG = baseColor.g * (1 - whiteMix) + whiteMix;
        const edgeB = baseColor.b * (1 - whiteMix) + whiteMix;
        const colorScale = isFocused
          ? 1.2 + focusT * 0.6   // 1.2 → 1.8
          : 0.3 * this.globalOpacityScale;

        edgeColors[i * 6] = edgeR * da * colorScale;
        edgeColors[i * 6 + 1] = edgeG * da * colorScale;
        edgeColors[i * 6 + 2] = edgeB * da * colorScale;
        edgeColors[i * 6 + 3] = edgeR * db * colorScale;
        edgeColors[i * 6 + 4] = edgeG * db * colorScale;
        edgeColors[i * 6 + 5] = edgeB * db * colorScale;
      }

      // Mark buffers for GPU upload
      const edgeGeom = poly.edgeLines.geometry;
      edgeGeom.attributes.position.needsUpdate = true;
      edgeGeom.attributes.color.needsUpdate = true;
      edgeGeom.computeBoundingSphere();

      const vertGeom = poly.vertexPoints.geometry;
      vertGeom.attributes.position.needsUpdate = true;
      vertGeom.computeBoundingSphere();
    }

    // Convergence particle animation
    if (this.convergeActive && this.convergePositions && this.convergeStartPositions && this.convergeTargetPositions && this.convergeColors && this.convergePoints) {
      const elapsed = performance.now() - this.convergeStartTime;
      const t = Math.min(elapsed / CONVERGE_DURATION, 1);
      // Ease-out cubic — fast start, gentle arrival
      const eased = 1 - (1 - t) * (1 - t) * (1 - t);

      // Update target positions from the polytope's current projected vertices (they rotate)
      const poly = this.convergeProjectId ? this.getPolytopeByProjectId(this.convergeProjectId) : null;
      if (poly) {
        for (let i = 0; i < CONVERGE_PARTICLE_COUNT; i++) {
          const vertIdx = i % poly.data.vertices.length;
          const proj = poly.projected[vertIdx];
          this.convergeTargetPositions[i * 3] = proj[0];
          this.convergeTargetPositions[i * 3 + 1] = proj[1];
          this.convergeTargetPositions[i * 3 + 2] = proj[2];
        }
      }

      for (let i = 0; i < CONVERGE_PARTICLE_COUNT; i++) {
        // Stagger each particle slightly — later particles arrive later
        const particleT = Math.max(0, Math.min(1, (eased - (i / CONVERGE_PARTICLE_COUNT) * 0.3) / 0.7));

        this.convergePositions[i * 3] = this.convergeStartPositions[i * 3] + (this.convergeTargetPositions[i * 3] - this.convergeStartPositions[i * 3]) * particleT;
        this.convergePositions[i * 3 + 1] = this.convergeStartPositions[i * 3 + 1] + (this.convergeTargetPositions[i * 3 + 1] - this.convergeStartPositions[i * 3 + 1]) * particleT;
        this.convergePositions[i * 3 + 2] = this.convergeStartPositions[i * 3 + 2] + (this.convergeTargetPositions[i * 3 + 2] - this.convergeStartPositions[i * 3 + 2]) * particleT;

        // Fade in, brighten on arrival
        const brightness = 0.3 + particleT * 0.7;
        this.convergeColors[i * 3] = this.convergeColor.r * brightness;
        this.convergeColors[i * 3 + 1] = this.convergeColor.g * brightness;
        this.convergeColors[i * 3 + 2] = this.convergeColor.b * brightness;
      }

      this.convergePoints.geometry.attributes.position.needsUpdate = true;
      this.convergePoints.geometry.attributes.color.needsUpdate = true;
      this.convergePoints.geometry.computeBoundingSphere();

      // Once fully converged, keep particles on vertices (they follow the 4D rotation)
    }
  }

  // --- Public API ---

  getColliders(): THREE.Mesh[] {
    return this.polytopes.map(p => p.collider);
  }

  getPolytopeByProjectId(id: string): HelixPolytope | undefined {
    return this.polytopes.find(p => p.project.id === id);
  }

  /** Returns the blog slug if this polytope is a blog post, null otherwise */
  getBlogSlugByProjectId(id: string): string | null {
    const poly = this.getPolytopeByProjectId(id);
    return poly?.blogSlug ?? null;
  }

  /** Scale a single polytope (for fly-in/fly-out transitions) */
  setScale(projectId: string, scale: number) {
    const poly = this.getPolytopeByProjectId(projectId);
    if (poly) poly.currentScale = scale;
  }

  /** Set opacity scale for all polytopes (1.0 = normal, 0.1 = faded during FOCUSED) */
  setGlobalOpacityScale(scale: number) {
    this.globalOpacityScale = scale;
  }

  /** Set rotation speed multiplier (2.0 during fly-in, 1.0 normal) */
  setRotationSpeedMultiplier(m: number) {
    this.rotationSpeedMultiplier = m;
  }

  /** Set the focused project + glow intensity (1.0 = normal, up to 3.0 for hero glow) */
  setFocusedGlow(projectId: string | null, scale: number) {
    this.focusedProjectId = projectId;
    this.focusGlowScale = scale;
  }

  /** Set the hovered polytope (null = none hovered) */
  setHoveredId(projectId: string | null) {
    this.hoveredProjectId = projectId;
  }

  /** Set global visibility for all polytopes (0 = hidden, 1 = visible). Used to fade in at scroll position. */
  setGlobalVisibility(v: number) {
    for (const poly of this.polytopes) {
      poly.edgeLines.visible = v > 0.01;
      poly.vertexPoints.visible = v > 0.01;
      poly.collider.visible = v > 0.01;
      if (v > 0.01 && v < 1) {
        (poly.edgeLines.material as THREE.LineBasicMaterial).opacity = v;
        (poly.vertexPoints.material as THREE.PointsMaterial).opacity = v;
      }
    }
  }

  dispose() {
    for (const poly of this.polytopes) {
      poly.edgeLines.geometry.dispose();
      (poly.edgeLines.material as THREE.Material).dispose();
      poly.vertexPoints.geometry.dispose();
      (poly.vertexPoints.material as THREE.Material).dispose();
      poly.glowOrb.geometry.dispose();
      (poly.glowOrb.material as THREE.Material).dispose();
      poly.collider.geometry.dispose();
      (poly.collider.material as THREE.Material).dispose();
    }
    this.group.parent?.remove(this.group);
    this.outerGroup.parent?.remove(this.outerGroup);
  }
}
