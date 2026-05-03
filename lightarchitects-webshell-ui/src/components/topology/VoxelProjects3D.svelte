<script lang="ts">
  import * as THREE from 'three';
  import { projectGroups } from '$lib/stores';
  import type { ProjectGroup, Build } from '$lib/types';

  // ── Public interface ──────────────────────────────────────────────────────

  export interface VoxelHoverData {
    group: ProjectGroup;
    build?: Build;
    screenX: number;
    screenY: number;
  }

  interface Props {
    onVoxelHover?: (data: VoxelHoverData | null) => void;
    onVoxelClick?: (group: ProjectGroup, build: Build) => void;
    onClusterClick?: (group: ProjectGroup) => void;
  }

  let { onVoxelHover, onVoxelClick, onClusterClick }: Props = $props();

  let container: HTMLDivElement | undefined = $state();

  // ── Color palette — one color per cluster slot ────────────────────────────

  const CLUSTER_COLORS = [
    0xFFD700, 0x00BFFF, 0xFF6B9D, 0xB44AFF,
    0xFF6D00, 0x4dffe6, 0x4dff8e, 0xf0c040,
  ];

  function voxelColor(status: string, clusterColor: number): number {
    if (status === 'failed')    return 0xef4444;
    if (status === 'completed') return clusterColor;
    if (status === 'in_progress') return clusterColor;
    return 0x1e293b; // queued/paused — dim
  }

  // ── Three.js scene ────────────────────────────────────────────────────────

  $effect(() => {
    if (!container) return;

    // Reactive dependency — scene rebuilds when project/build data changes
    const groups = $projectGroups;

    const w = container.clientWidth;
    const h = container.clientHeight;
    if (w === 0 || h === 0) return;

    // Scene + fog
    const scene = new THREE.Scene();
    scene.background = new THREE.Color(0x0a0a0f);
    scene.fog = new THREE.FogExp2(0x0a0a0f, 0.018);

    // Camera
    const camera = new THREE.PerspectiveCamera(52, w / h, 0.1, 200);

    // Drag-orbit state (spherical coords relative to scene center)
    let sph = { theta: 0, phi: 0.82, r: 17 };

    function applySph() {
      camera.position.set(
        sph.r * Math.sin(sph.phi) * Math.sin(sph.theta),
        sph.r * Math.cos(sph.phi),
        sph.r * Math.sin(sph.phi) * Math.cos(sph.theta),
      );
      camera.lookAt(0, 0.6, 0);
    }
    applySph();

    // Renderer — clear existing children then append new canvas
    const renderer = new THREE.WebGLRenderer({ antialias: true, powerPreference: 'high-performance' });
    renderer.setPixelRatio(Math.min(window.devicePixelRatio, 1.5));
    renderer.setSize(w, h);
    while (container.firstChild) container.removeChild(container.firstChild);
    container.appendChild(renderer.domElement);

    // Lights
    scene.add(new THREE.AmbientLight(0xffffff, 0.35));
    const dir = new THREE.DirectionalLight(0xffffff, 0.9);
    dir.position.set(6, 12, 8);
    scene.add(dir);

    // Grid floor
    const grid = new THREE.GridHelper(28, 28, 0x1e293b, 0x0f172a);
    grid.position.y = -0.08;
    scene.add(grid);

    // ── Build clusters ──────────────────────────────────────────────────────

    const voxelMap: Array<{ mesh: THREE.Mesh; group: ProjectGroup; build: Build }> = [];
    const platformMap: Array<{ mesh: THREE.Mesh; group: ProjectGroup }> = [];
    const toDispose: Array<THREE.BufferGeometry | THREE.Material> = [];

    const clamped = groups.slice(0, 8);
    const ring = clamped.length <= 1 ? 0 : 4.8;

    clamped.forEach((group, gi) => {
      const angle = (gi / clamped.length) * Math.PI * 2 - Math.PI / 2;
      const cx = ring * Math.cos(angle);
      const cz = ring * Math.sin(angle);
      const col = CLUSTER_COLORS[gi % CLUSTER_COLORS.length];
      const threeCol = new THREE.Color(col);
      const active = group.activePlanCount > 0;

      // Hex platform
      const platGeo = new THREE.CylinderGeometry(1.05, 1.05, 0.1, 6);
      const platMat = new THREE.MeshStandardMaterial({
        color: threeCol,
        emissive: threeCol,
        emissiveIntensity: active ? 0.1 : 0.03,
        metalness: 0.2, roughness: 0.75,
        transparent: true, opacity: 0.85,
      });
      const platform = new THREE.Mesh(platGeo, platMat);
      platform.position.set(cx, 0, cz);
      scene.add(platform);
      platformMap.push({ mesh: platform, group });
      toDispose.push(platGeo, platMat);

      // Platform edge ring
      const ringGeo = new THREE.TorusGeometry(1.05, 0.022, 4, 6);
      const ringMat = new THREE.MeshBasicMaterial({ color: threeCol, transparent: true, opacity: active ? 0.6 : 0.2 });
      const ringMesh = new THREE.Mesh(ringGeo, ringMat);
      ringMesh.rotation.x = Math.PI / 2;
      ringMesh.position.set(cx, 0.06, cz);
      scene.add(ringMesh);
      toDispose.push(ringGeo, ringMat);

      // Stacked voxels
      const builds = group.plans.slice(0, 6);
      builds.forEach((build, bi) => {
        const vc = voxelColor(build.status, col);
        const threeVc = new THREE.Color(vc);
        const eI = build.status === 'in_progress' ? 0.35
                 : build.status === 'failed'       ? 0.45
                 : build.status === 'completed'    ? 0.12
                 : 0.02;

        const row = Math.floor(bi / 3);
        const col3 = bi % 3;
        const px = cx + (col3 - 1) * 0.62;
        const py = 0.28 + row * 0.52;
        const pz = cz;

        const geo = new THREE.BoxGeometry(0.52, 0.44, 0.52);
        const mat = new THREE.MeshStandardMaterial({
          color: threeVc, emissive: threeVc, emissiveIntensity: eI,
          metalness: 0.35, roughness: 0.65,
        });
        const mesh = new THREE.Mesh(geo, mat);
        mesh.position.set(px, py, pz);
        scene.add(mesh);
        voxelMap.push({ mesh, group, build });
        toDispose.push(geo, mat);

        // Edge highlight for running/failed
        if (build.status === 'in_progress' || build.status === 'failed') {
          const edgeGeo = new THREE.EdgesGeometry(geo);
          const edgeMat = new THREE.LineBasicMaterial({
            color: build.status === 'failed' ? 0xef4444 : col,
            transparent: true, opacity: 0.75,
          });
          const edges = new THREE.LineSegments(edgeGeo, edgeMat);
          edges.position.copy(mesh.position);
          scene.add(edges);
          toDispose.push(edgeGeo, edgeMat);
        }
      });
    });

    // ── Interaction ───────────────────────────────────────────────────────

    // Cache mesh arrays once — avoids per-event .map() allocations in hot paths
    const voxelMeshes   = voxelMap.map(v => v.mesh);
    const platformMeshes = platformMap.map(p => p.mesh);

    const ray = new THREE.Raycaster();
    const ptr = new THREE.Vector2();
    let dragging = false;
    let dragMoved = false; // true once the pointer has traveled >3px while dragging
    let lastMx = 0;
    let lastMy = 0;

    function setPointer(clientX: number, clientY: number) {
      const rect = renderer.domElement.getBoundingClientRect();
      ptr.x = ((clientX - rect.left) / rect.width) * 2 - 1;
      ptr.y = -((clientY - rect.top) / rect.height) * 2 + 1;
    }

    function onPointerDown(e: PointerEvent) {
      dragging = true;
      dragMoved = false;
      lastMx = e.clientX;
      lastMy = e.clientY;
      renderer.domElement.setPointerCapture(e.pointerId);
    }

    function onPointerMove(e: PointerEvent) {
      setPointer(e.clientX, e.clientY);
      if (dragging) {
        const dx = e.clientX - lastMx;
        const dy = e.clientY - lastMy;
        if (Math.abs(dx) > 3 || Math.abs(dy) > 3) dragMoved = true;
        sph.theta -= dx * 0.004;
        sph.phi    = Math.max(0.18, Math.min(1.35, sph.phi + dy * 0.004));
        lastMx = e.clientX;
        lastMy = e.clientY;
        applySph();
        return; // skip hover during drag
      }

      ray.setFromCamera(ptr, camera);
      const vHits = ray.intersectObjects(voxelMeshes);
      if (vHits.length) {
        const hit = voxelMap.find(v => v.mesh === vHits[0].object);
        if (hit) { onVoxelHover?.({ group: hit.group, build: hit.build, screenX: e.clientX, screenY: e.clientY }); return; }
      }
      const pHits = ray.intersectObjects(platformMeshes);
      if (pHits.length) {
        const hit = platformMap.find(p => p.mesh === pHits[0].object);
        if (hit) { onVoxelHover?.({ group: hit.group, screenX: e.clientX, screenY: e.clientY }); return; }
      }
      onVoxelHover?.(null);
    }

    function onPointerUp() { dragging = false; }

    function onClick(e: MouseEvent) {
      // dragMoved is cleared here (not in pointerup) so the check fires before
      // the browser-generated click event, which always follows pointerup.
      if (dragMoved) { dragMoved = false; return; }
      setPointer(e.clientX, e.clientY); // ensure ptr is current for this click position
      ray.setFromCamera(ptr, camera);
      const vHits = ray.intersectObjects(voxelMeshes);
      if (vHits.length) {
        const hit = voxelMap.find(v => v.mesh === vHits[0].object);
        if (hit) { onVoxelClick?.(hit.group, hit.build); return; }
      }
      const pHits = ray.intersectObjects(platformMeshes);
      if (pHits.length) {
        const hit = platformMap.find(p => p.mesh === pHits[0].object);
        if (hit) { onClusterClick?.(hit.group); return; }
      }
    }

    function onWheel(e: WheelEvent) {
      if (!e.metaKey && !e.ctrlKey) return; // Wave 1.5 inertia guard
      e.preventDefault();
      sph.r = Math.max(6, Math.min(40, sph.r + e.deltaY * 0.04));
      applySph();
    }

    function onCameraHome() {
      sph = { theta: 0, phi: 0.82, r: 17 };
      applySph();
    }

    renderer.domElement.addEventListener('pointerdown', onPointerDown);
    renderer.domElement.addEventListener('pointermove', onPointerMove);
    renderer.domElement.addEventListener('pointerup', onPointerUp);
    renderer.domElement.addEventListener('click', onClick);
    renderer.domElement.addEventListener('wheel', onWheel, { passive: false });
    window.addEventListener('la:topology-home-camera', onCameraHome);

    // ── Animation loop ────────────────────────────────────────────────────

    let animId = 0;
    let t = 0;
    let lastTimestamp = 0;

    function animate(timestamp: number) {
      animId = requestAnimationFrame(animate);
      const dt = lastTimestamp > 0 ? Math.min((timestamp - lastTimestamp) / 1000, 0.05) : 0.016;
      lastTimestamp = timestamp;
      t += dt;

      for (const { mesh, build } of voxelMap) {
        const mat = mesh.material as THREE.MeshStandardMaterial;
        if (build.status === 'in_progress') {
          const pulse = 0.3 + 0.15 * Math.sin(t * 2.2);
          mat.emissiveIntensity = pulse;
          mesh.scale.setScalar(1 + 0.045 * Math.sin(t * 2.2));
        } else if (build.status === 'failed') {
          mat.emissiveIntensity = 0.35 + 0.25 * Math.abs(Math.sin(t * 3.5));
        }
      }

      renderer.render(scene, camera);
    }
    animate(0);

    // Resize observer
    const resizeObs = new ResizeObserver(() => {
      if (!container) return;
      const nw = container.clientWidth;
      const nh = container.clientHeight;
      if (nw === 0 || nh === 0) return;
      camera.aspect = nw / nh;
      camera.updateProjectionMatrix();
      renderer.setSize(nw, nh);
    });
    resizeObs.observe(container);

    return () => {
      cancelAnimationFrame(animId);
      resizeObs.disconnect();
      renderer.domElement.removeEventListener('pointerdown', onPointerDown);
      renderer.domElement.removeEventListener('pointermove', onPointerMove);
      renderer.domElement.removeEventListener('pointerup', onPointerUp);
      renderer.domElement.removeEventListener('click', onClick);
      renderer.domElement.removeEventListener('wheel', onWheel);
      window.removeEventListener('la:topology-home-camera', onCameraHome);
      scene.traverse(obj => {
        const mesh = obj as THREE.Mesh;
        if (mesh.geometry) mesh.geometry.dispose();
        if (Array.isArray(mesh.material)) mesh.material.forEach(m => m.dispose());
        else if (mesh.material) (mesh.material as THREE.Material).dispose();
      });
      scene.clear();
      toDispose.forEach(obj => obj.dispose());
      renderer.dispose();
    };
  });
</script>

<div
  bind:this={container}
  class="la-topology-container"
  style="width:100%;height:100%;"
  data-testid="voxel-projects-3d"
></div>
