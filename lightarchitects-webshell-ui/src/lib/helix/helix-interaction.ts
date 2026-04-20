/**
 * Helix Interaction — raycaster, state machine, camera fly-in/fly-out.
 *
 * States: OVERVIEW → HOVER → FOCUSED
 * Camera: commandeers cameraControl targets, never sets camera.position directly.
 * Fly-out: blends targets over 200ms to prevent one-frame snap (meeting amendment 1).
 */
import * as THREE from 'three';
import { PROJECTS, type ProjectEntry } from './projects';
import { BLOG_POSTS, type BlogPost } from './blog-posts';
import type { HelixPolytopeManager } from './helix-polytopes';

type State = 'OVERVIEW' | 'HOVER' | 'FOCUSED';

export interface CameraControl {
  targetX: number;
  targetY: number;
  targetZ: number;
  targetRotX: number;
  targetRotZ: number;
  lookAt: THREE.Vector3;
  lerpSpeed: number;
  scrollLocked: boolean;
}

const DEFAULT_CAMERA_Z = 5.5;
const MICROSCOPE_ZOOM_Z = 2.0;   // Intimate — polytope fills ~48% of viewport
const MACRO_ZOOM_Z = 8.0;        // Zoomed out to see full helix (SOUL)
const GROUP_OFFSET_X = 0;        // Hero3D group.position.x — centered

type FocusCallback = (project: ProjectEntry | null) => void;

const FLY_IN_DURATION = 800;
const FLY_OUT_DURATION = 600;
const BLEND_BACK_DURATION = 200;

export class HelixInteraction {
  private state: State = 'OVERVIEW';
  private camera: THREE.PerspectiveCamera;
  private canvas: HTMLCanvasElement;
  private cameraControl: CameraControl;
  private polytopeManager: HelixPolytopeManager;
  private raycaster = new THREE.Raycaster();
  private mouse = new THREE.Vector2();
  private colliders: THREE.Mesh[];

  private hoveredId: string | null = null;
  private focusedId: string | null = null;
  private convergedProjectId: string | null = null;
  private onFocus: FocusCallback = () => {};
  private onBlogFocus: (post: BlogPost | null) => void = () => {};
  private onHover: (project: ProjectEntry | null) => void = () => {};

  // Transition state
  private transitionStart = 0;
  private isTransitioning = false;
  private transitionDirection: 'in' | 'out' = 'in';

  // Fly-out blend state (meeting amendment 1)
  private blendBackStart = 0;
  private blendFromX = 0;
  private blendFromY = 0;
  private isBlendingBack = false;
  private preFocusTargetX = 0;
  private preFocusTargetY = 0;

  private getGroupY: () => number;

  constructor(
    camera: THREE.PerspectiveCamera,
    canvas: HTMLCanvasElement,
    cameraControl: CameraControl,
    polytopeManager: HelixPolytopeManager,
    getGroupY: () => number = () => 0,
  ) {
    this.camera = camera;
    this.canvas = canvas;
    this.cameraControl = cameraControl;
    this.polytopeManager = polytopeManager;
    this.getGroupY = getGroupY;
    this.colliders = polytopeManager.getColliders();

    canvas.addEventListener('mousemove', this.handleMouseMove);
    canvas.addEventListener('click', this.handleClick);
    window.addEventListener('keydown', this.handleKeyDown);
  }

  setOnFocus(cb: FocusCallback) {
    this.onFocus = cb;
  }

  setOnBlogFocus(cb: (post: BlogPost | null) => void) {
    this.onBlogFocus = cb;
  }

  setOnHover(cb: (project: ProjectEntry | null) => void) {
    this.onHover = cb;
  }

  private handleMouseMove = (e: MouseEvent) => {
    const rect = this.canvas.getBoundingClientRect();
    this.mouse.x = ((e.clientX - rect.left) / rect.width) * 2 - 1;
    this.mouse.y = -((e.clientY - rect.top) / rect.height) * 2 + 1;

    if (this.state === 'FOCUSED') return;

    this.raycaster.setFromCamera(this.mouse, this.camera);
    const hits = this.raycaster.intersectObjects(this.colliders);

    if (hits.length > 0) {
      const id = hits[0].object.userData.projectId as string;
      if (this.hoveredId !== id) {
        this.hoveredId = id;
        this.state = 'HOVER';
        this.canvas.style.cursor = 'pointer';
        this.polytopeManager.setHoveredId(id);
        const project = PROJECTS.find(p => p.id === id);
        if (project) this.onHover(project);
      }
    } else if (this.state === 'HOVER') {
      this.hoveredId = null;
      this.state = 'OVERVIEW';
      this.canvas.style.cursor = 'default';
      this.polytopeManager.setHoveredId(null);
      this.onHover(null);
    }
  };

  private handleClick = () => {
    if (this.isTransitioning) return;

    if (this.state === 'HOVER' && this.hoveredId) {
      this.focusOn(this.hoveredId);
    } else if (this.state === 'FOCUSED') {
      this.unfocus();
    }
  };

  private handleKeyDown = (e: KeyboardEvent) => {
    if (e.key === 'Escape' && this.state === 'FOCUSED') {
      this.unfocus();
    }
  };

  private focusOn(projectId: string) {
    const poly = this.polytopeManager.getPolytopeByProjectId(projectId);
    if (!poly) return;

    this.focusedId = projectId;
    this.state = 'FOCUSED';

    // Save pre-focus mouse targets for blend-back
    this.preFocusTargetX = this.cameraControl.targetX;
    this.preFocusTargetY = this.cameraControl.targetY;

    // Lock scroll + override camera targets
    this.cameraControl.scrollLocked = true;
    this.cameraControl.lerpSpeed = 0.06;

    const worldPos = poly.worldPos;

    // All world positions are in group-local space. Camera is in world space.
    // The group sits at x=GROUP_OFFSET_X and bobs on Y via scroll + sine.
    // Polytopes also have z-depth on the helix spiral (front/back).
    const worldX = worldPos.x + GROUP_OFFSET_X;
    const worldY = worldPos.y + this.getGroupY();
    const worldZ = worldPos.z;  // Helix spiral z-depth

    // Camera positions in front of the polytope, looking at it.
    // Z target = polytope z + zoom distance (camera is always in front).
    const cameraZ = worldZ + MICROSCOPE_ZOOM_Z;

    // Slight left offset so focused polytope sits right-of-center, leaving room for bottom-left UI
    const FOCUS_CAMERA_LEFT_OFFSET = -1.0;

    // Uniform camera behavior for all polytopes
    this.cameraControl.targetX = worldX + FOCUS_CAMERA_LEFT_OFFSET;
    this.cameraControl.targetY = worldY;
    this.cameraControl.targetZ = cameraZ;
    this.cameraControl.lookAt.set(worldX, worldY, worldZ);

    this.transitionStart = performance.now();
    this.transitionDirection = 'in';
    this.isTransitioning = true;

    // Determine if this is a blog polytope or a project polytope
    const blogSlug = this.polytopeManager.getBlogSlugByProjectId(projectId);
    if (blogSlug) {
      const blogPost = BLOG_POSTS.find(p => p.slug === blogSlug);
      if (blogPost) this.onBlogFocus(blogPost);
    } else {
      const project = PROJECTS.find(p => p.id === projectId);
      if (project) this.onFocus(project);
    }
  }

  unfocus() {
    if (!this.focusedId) return;

    this.transitionStart = performance.now();
    this.transitionDirection = 'out';
    this.isTransitioning = true;

    this.onFocus(null);
    this.onBlogFocus(null);
  }

  /** Called every frame from Hero3D animate loop */
  update() {
    if (!this.isTransitioning && !this.isBlendingBack) return;

    const now = performance.now();

    if (this.isTransitioning) {
      const duration = this.transitionDirection === 'in' ? FLY_IN_DURATION : FLY_OUT_DURATION;
      const elapsed = now - this.transitionStart;
      // Quartic ease — fast start, gentle arrival
      const t = Math.min(elapsed / duration, 1);
      const eased = t < 0.5
        ? 8 * t * t * t * t
        : 1 - 8 * (1 - t) * (1 - t) * (1 - t) * (1 - t);

      if (this.transitionDirection === 'in') {
        // Scale polytope up — at microscope zoom it should fill the view as a strand building block
        const targetScale = 0.55;  // Intimate scale — fills ~48% of viewport at Z=2.0
        this.polytopeManager.setScale(this.focusedId!, 0.08 + (targetScale - 0.08) * eased);
        // Rotation speed 2× during fly-in, settle on arrival
        this.polytopeManager.setRotationSpeedMultiplier(t < 0.75 ? 2.0 : 1.0 + (1.0 - t) * 4.0);
        // Fade non-focused polytopes
        this.polytopeManager.setGlobalOpacityScale(1.0 - eased * 0.85);
        // Ramp focus glow: 1.0 → 1.6 (brighter edges + slightly bigger vertex dots)
        this.polytopeManager.setFocusedGlow(this.focusedId!, 1.0 + eased * 0.6);
      } else {
        // Scale polytope back to overview
        this.polytopeManager.setScale(this.focusedId!, 0.55 - (0.55 - 0.08) * eased);
        this.polytopeManager.setRotationSpeedMultiplier(1.0);
        this.polytopeManager.setGlobalOpacityScale(0.1 + eased * 0.9);
        // Ramp focus glow back down: 1.6 → 1.0
        this.polytopeManager.setFocusedGlow(this.focusedId!, 1.6 - eased * 0.6);
      }

      if (t >= 1) {
        this.isTransitioning = false;

        if (this.transitionDirection === 'out') {
          // Fly-out complete — start blend-back (meeting amendment 1)
          this.cameraControl.scrollLocked = false;
          this.cameraControl.lerpSpeed = 0.03;
          this.cameraControl.targetZ = DEFAULT_CAMERA_Z;
          this.cameraControl.lookAt.set(GROUP_OFFSET_X, 0, 0);

          // Blend targets from fly-out position toward mouse targets over 200ms
          this.blendFromX = this.cameraControl.targetX;
          this.blendFromY = this.cameraControl.targetY;
          this.blendBackStart = now;
          this.isBlendingBack = true;

          this.polytopeManager.setFocusedGlow(null, 1.0);
          this.focusedId = null;
          this.state = 'OVERVIEW';
        }
      }
    }

    if (this.isBlendingBack) {
      const elapsed = now - this.blendBackStart;
      const t = Math.min(elapsed / BLEND_BACK_DURATION, 1);

      // Blend targets smoothly — prevents the one-frame snap
      this.cameraControl.targetX = this.blendFromX + (this.preFocusTargetX - this.blendFromX) * t;
      this.cameraControl.targetY = this.blendFromY + (this.preFocusTargetY - this.blendFromY) * t;

      if (t >= 1) {
        this.isBlendingBack = false;
      }
    }
  }

  /** Highlight a project's polytope — glow + slight scale, NO camera zoom.
   *  Called when carousel arrows change the active card. */
  highlightProject(projectId: string) {
    // Reset previous highlight
    if (this.convergedProjectId && this.convergedProjectId !== projectId) {
      this.polytopeManager.setScale(this.convergedProjectId, 0.08);
    }
    this.convergedProjectId = projectId;
    this.polytopeManager.stopConvergence();
    this.polytopeManager.startConvergence(projectId);

    // Slight scale bump to make the highlighted polytope visible — no camera move
    this.polytopeManager.setScale(projectId, 0.15);
  }

  /** Navigate to a project — full camera zoom, polytope becomes hero.
   *  Called when user clicks "Explore" on a card. */
  navigateToProject(projectId: string) {
    // If already focused on something else, reset it
    if (this.focusedId && this.focusedId !== projectId) {
      this.polytopeManager.setScale(this.focusedId, 0.08);
      this.polytopeManager.setGlobalOpacityScale(1.0);
    }

    // Reset previous convergence
    if (this.convergedProjectId) {
      this.polytopeManager.setScale(this.convergedProjectId, 0.08);
    }
    this.convergedProjectId = projectId;
    this.polytopeManager.stopConvergence();
    this.polytopeManager.startConvergence(projectId);

    // Full focus — camera flies to polytope, it becomes the hero
    this.focusOn(projectId);
  }

  getHoveredId(): string | null {
    return this.hoveredId;
  }

  getState(): State {
    return this.state;
  }

  /** Returns a 0→1 value representing how "focused" the scene is.
   *  0 = OVERVIEW (full spin), 1 = fully FOCUSED (helix paused). */
  getFocusIntensity(): number {
    if (this.state === 'FOCUSED' && !this.isTransitioning) return 1.0;
    if (!this.isTransitioning) return 0.0;
    const now = performance.now();
    const duration = this.transitionDirection === 'in' ? FLY_IN_DURATION : FLY_OUT_DURATION;
    const t = Math.min((now - this.transitionStart) / duration, 1);
    return this.transitionDirection === 'in' ? t : 1.0 - t;
  }

  dispose() {
    this.canvas.removeEventListener('mousemove', this.handleMouseMove);
    this.canvas.removeEventListener('click', this.handleClick);
    window.removeEventListener('keydown', this.handleKeyDown);
  }
}
