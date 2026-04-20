<script lang="ts">
  import * as THREE from 'three';
  import { get } from 'svelte/store';
  import { step, authStatus, autoCompleteFromInherited } from '$lib/setup';

  let canvas: HTMLCanvasElement;
  let visible = $state(true);
  let advanced = false;

  /**
   * Auto-skip path: when /api/setup/info reports a canonical credential source
   * (Keychain / file / env), skip Backend → Auth → Model and jump straight
   * to Init. Matches the user thesis: don't re-prompt for creds that the
   * target CLI already has stored.
   *
   * Falls through to the normal Backend step if auto-skip is not eligible
   * or if the /api/setup/save call fails.
   */
  async function tryAutoSkip(): Promise<boolean> {
    const as = get(authStatus);
    const claudeSource = as?.claude?.login_source;
    const codexSource = as?.codex?.login_source;
    if (claudeSource && claudeSource !== 'none') {
      return await autoCompleteFromInherited('lightarchitects', 'anthropic');
    }
    if (codexSource && codexSource !== 'none') {
      return await autoCompleteFromInherited('codex', 'openai');
    }
    return false;
  }

  async function advance() {
    if (advanced) return;
    advanced = true;
    visible = false;
    const skipped = await tryAutoSkip();
    if (skipped) return; // autoCompleteFromInherited already set step to 'init'
    setTimeout(() => step.set('backend'), 600);
  }

  // 600-cell polytope (lifted verbatim from cappy-cortex/cappy-web/src/components/LoadingSplash.svelte)
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
    const clock = new THREE.Clock();
    let animId: number;

    function animate() {
      animId = requestAnimationFrame(animate);
      const t = clock.getElapsedTime();
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

    // Auto-advance after 2.5s
    const timer = setTimeout(advance, 2500);

    return () => {
      clearTimeout(timer);
      cancelAnimationFrame(animId);
      renderer.dispose();
    };
  });
</script>

<!-- Preload fonts -->
<svelte:head>
  <link rel="preconnect" href="https://fonts.googleapis.com" />
  <link rel="preconnect" href="https://fonts.gstatic.com" crossorigin="anonymous" />
  <link href="https://fonts.googleapis.com/css2?family=IBM+Plex+Mono:wght@400;600&family=Raleway:wght@700&display=swap" rel="stylesheet" />
</svelte:head>

<div
  class="splash"
  class:out={!visible}
  onclick={advance}
  role="button"
  tabindex="0"
  onkeydown={(e) => e.key === 'Enter' && advance()}
>
  <canvas bind:this={canvas} class="polytope-canvas"></canvas>

  <div class="hud">
    <div class="title">Light Architects</div>
    <div class="subtitle">Webshell</div>
    <div class="tap-hint">TAP TO CONTINUE</div>
  </div>

  <div class="scanlines"></div>
</div>

<style>
  .splash {
    position: fixed; inset: 0; z-index: 9999;
    background: radial-gradient(ellipse at center, #0a0a20 0%, #03030a 60%, #000000 100%);
    display: flex; align-items: center; justify-content: center;
    cursor: pointer; opacity: 1; transition: opacity 0.6s ease;
  }
  .splash.out { opacity: 0; pointer-events: none; }

  .polytope-canvas { position: absolute; inset: 0; width: 100% !important; height: 100% !important; }

  .hud {
    position: relative; z-index: 2;
    display: flex; flex-direction: column; align-items: center; gap: 6px;
    pointer-events: none;
  }

  .title {
    font-family: 'Raleway', sans-serif; font-size: 52px; font-weight: 700;
    letter-spacing: 0.15em;
    background: linear-gradient(135deg, #ff6600, #ffffff 40%, #00d26a);
    -webkit-background-clip: text; -webkit-text-fill-color: transparent; background-clip: text;
  }

  .subtitle {
    font-family: 'IBM Plex Mono', monospace; font-size: 14px; font-weight: 600;
    letter-spacing: 0.4em; color: rgba(255,255,255,0.5);
    text-transform: uppercase;
  }

  .tap-hint {
    font-family: 'IBM Plex Mono', monospace; font-size: 9px; font-weight: 600;
    letter-spacing: 0.4em; color: rgba(255,140,0,0.55);
    margin-top: 24px;
    animation: tap-pulse 2s ease-in-out infinite;
  }
  @keyframes tap-pulse { 0%,100% { opacity: 0.4; } 50% { opacity: 1; } }

  .scanlines {
    position: absolute; inset: 0; pointer-events: none;
    background: repeating-linear-gradient(0deg, transparent, transparent 2px, rgba(255,140,0,0.03) 2px, rgba(255,140,0,0.03) 3px);
    animation: scanlines-move 8s linear infinite;
  }
  @keyframes scanlines-move { from { background-position: 0 0; } to { background-position: 0 100px; } }
</style>
