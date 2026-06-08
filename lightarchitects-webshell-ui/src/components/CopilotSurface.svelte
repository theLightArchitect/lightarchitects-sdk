<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { get } from 'svelte/store';
  import * as THREE from 'three';
  import {
    copilotMessages, copilotLoading, siblingHealth, activityFeed,
    clearCopilotHistory,
  } from '$lib/stores';
  import { serverCwd } from '$lib/setup';
  import { handleCopilotEvent, addCopilotMessage, sendCopilotNative } from '$lib/copilot/chat';
  import { SIBLING_COLORS } from '$lib/design-tokens';
  import { renderMarkdown } from '$lib/markdown';
  import { agentDomain } from '$lib/lightspace/vocab';
  import type { SiblingId } from '$lib/types';

  // ── Props ──────────────────────────────────────────────────────────────────
  interface Props {
    onclose?: () => void;
  }
  let { onclose }: Props = $props();

  // ── DOM refs ───────────────────────────────────────────────────────────────
  let topologyEl: HTMLCanvasElement;
  let signalEl: HTMLCanvasElement;
  let streamEl: HTMLDivElement | undefined = $state();
  let inputEl: HTMLTextAreaElement | undefined = $state();

  // ── Local state ────────────────────────────────────────────────────────────
  let inputText = $state('');
  let sending = $state(false);
  let focusedSiblingId: SiblingId | null = $state(null);
  let expandedThinking = $state(new Set<string>());
  let expandedTool = $state(new Set<string>());

  // ── Derived state ──────────────────────────────────────────────────────────
  let msgs = $derived($copilotMessages);
  let health = $derived($siblingHealth);
  let loading = $derived($copilotLoading);

  const SIBLING_LIST = ['soul', 'eva', 'corso', 'quantum', 'seraph', 'ayin', 'laex'] as const;

  const SIBLING_DISPLAY: Record<string, string> = {
    soul: 'SOUL', eva: 'EVA', corso: 'CORSO', quantum: 'QUANTUM',
    seraph: 'SERAPH', ayin: 'AYIN', laex: 'LÆX',
  };
  // Domain names per Lightspace standard Rule 4 — sourced from vocab.ts
  const SIBLING_DOMAINS: Record<string, string> = {
    soul: 'Knowledge', eva: 'DevOps', corso: 'Engineering',
    quantum: 'Research', seraph: 'Security', ayin: 'Observability', laex: 'Standards',
  };

  const LABEL_MODES = ['domain', 'both', 'codename'] as const;
  type LabelMode = typeof LABEL_MODES[number];
  let labelMode = $state<LabelMode>('domain');
  const MODE_LABEL: Record<LabelMode, string> = { domain: 'Domain', both: 'Domain + ID', codename: 'ID only' };
  function cycleMode() {
    const i = LABEL_MODES.indexOf(labelMode);
    labelMode = LABEL_MODES[(i + 1) % LABEL_MODES.length];
  }

  function siblingStatus(id: string): 'online' | 'degraded' | 'offline' | 'unconfigured' {
    return health[id as SiblingId]?.status ?? 'unconfigured';
  }

  // ═══════════════════════════════════════════════════════════════════════════
  // THREE.JS SQUAD TOPOLOGY
  // ═══════════════════════════════════════════════════════════════════════════

  let renderer: THREE.WebGLRenderer | null = null;
  let scene: THREE.Scene | null = null;
  let camera: THREE.PerspectiveCamera | null = null;
  let rafId: number | null = null;
  const clock = new THREE.Clock();

  type SiblingNode = {
    core: THREE.Mesh;
    shell: THREE.LineSegments;
    label: string;
    baseAngle: number;
    orbitRadius: number;
    orbitSpeed: number;
    orbitTilt: number;
    color: THREE.Color;
  };
  let siblingNodes: SiblingNode[] = [];
  let coreMesh: THREE.Mesh | null = null;
  let coreGlow: THREE.Mesh | null = null;

  function initThree() {
    if (!topologyEl) return;
    const W = topologyEl.clientWidth || 400;
    const H = topologyEl.clientHeight || 500;

    renderer = new THREE.WebGLRenderer({ canvas: topologyEl, antialias: true, alpha: true });
    renderer.setSize(W, H, false);
    renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
    renderer.toneMapping = THREE.ACESFilmicToneMapping;
    renderer.toneMappingExposure = 1.3;
    renderer.setClearColor(0x000000, 0);

    scene = new THREE.Scene();
    const sc = scene;
    sc.fog = new THREE.FogExp2(0x020810, 0.055);

    camera = new THREE.PerspectiveCamera(52, W / H, 0.1, 80);
    camera.position.set(0, 2.2, 7.5);
    camera.lookAt(0, 0, 0);

    // Lighting
    sc.add(new THREE.AmbientLight(0x08182a, 3.0));
    const key = new THREE.DirectionalLight(0x3060c0, 2.0);
    key.position.set(4, 8, 4);
    sc.add(key);
    const fill = new THREE.DirectionalLight(0x00d4f5, 0.5);
    fill.position.set(-4, -2, 2);
    sc.add(fill);

    // ── Central core ──
    coreMesh = new THREE.Mesh(
      new THREE.SphereGeometry(0.42, 32, 32),
      new THREE.MeshStandardMaterial({
        color: 0x00d4f5, emissive: 0x00a8c8, emissiveIntensity: 1.0,
        roughness: 0.1, metalness: 0.95,
      })
    );
    sc.add(coreMesh);

    coreGlow = new THREE.Mesh(
      new THREE.SphereGeometry(0.6, 16, 16),
      new THREE.MeshBasicMaterial({ color: 0x00d4f5, transparent: true, opacity: 0.06, side: THREE.BackSide })
    );
    sc.add(coreGlow);

    // Core ring
    const ring = new THREE.Mesh(
      new THREE.TorusGeometry(0.62, 0.012, 8, 80),
      new THREE.MeshStandardMaterial({ color: 0x00d4f5, emissive: 0x00d4f5, emissiveIntensity: 0.6 })
    );
    ring.rotation.x = Math.PI * 0.15;
    sc.add(ring);

    // ── Sibling nodes ──
    SIBLING_LIST.forEach((sid, i) => {
      const hex = SIBLING_COLORS[sid] ?? '#888';
      const color = new THREE.Color(hex);
      const orbitRadius = 2.1 + (i % 2 === 0 ? 0 : 0.28);
      const orbitTilt = ((i % 3) - 1) * 0.55;

      const core = new THREE.Mesh(
        new THREE.IcosahedronGeometry(0.25, 1),
        new THREE.MeshStandardMaterial({
          color, emissive: color,
          emissiveIntensity: siblingStatus(sid) === 'online' ? 0.65 : 0.08,
          roughness: 0.2, metalness: 0.9,
        })
      );

      const edges = new THREE.EdgesGeometry(new THREE.IcosahedronGeometry(0.32, 1));
      const shell = new THREE.LineSegments(
        edges,
        new THREE.LineBasicMaterial({ color, transparent: true, opacity: 0.4 })
      );
      core.add(shell);
      sc.add(core);

      siblingNodes.push({
        core, shell, label: sid,
        baseAngle: (i / SIBLING_LIST.length) * Math.PI * 2,
        orbitRadius, orbitSpeed: 0.09 + i * 0.012,
        orbitTilt, color,
      });
    });

    // Orbit guide ring
    sc.add(Object.assign(new THREE.Mesh(
      new THREE.TorusGeometry(2.25, 0.004, 4, 100),
      new THREE.MeshBasicMaterial({ color: 0x1a3a60, transparent: true, opacity: 0.25 })
    ), { rotation: { x: Math.PI * 0.12, y: 0, z: 0 } }));

    // Starfield
    const starPos: number[] = [];
    for (let s = 0; s < 600; s++) {
      starPos.push((Math.random() - 0.5) * 50, (Math.random() - 0.5) * 50, (Math.random() - 0.5) * 50);
    }
    const starGeo = new THREE.BufferGeometry();
    starGeo.setAttribute('position', new THREE.Float32BufferAttribute(starPos, 3));
    sc.add(new THREE.Points(starGeo, new THREE.PointsMaterial({ color: 0x2040a0, size: 0.025, transparent: true, opacity: 0.5 })));

    animateThree();
  }

  function animateThree() {
    rafId = requestAnimationFrame(animateThree);
    if (!renderer || !scene || !camera) return;
    const t = clock.getElapsedTime();

    // Core pulse
    if (coreMesh && coreGlow) {
      const pulse = 1 + 0.1 * Math.sin(t * (loading ? 5 : 1.8));
      coreMesh.scale.setScalar(pulse);
      coreGlow.scale.setScalar(pulse * 1.15);
      coreMesh.rotation.y = t * 0.3;
      coreMesh.rotation.x = t * 0.15;
      (coreMesh.material as THREE.MeshStandardMaterial).emissiveIntensity =
        loading ? 1.6 + 0.6 * Math.sin(t * 5) : 0.9 + 0.3 * Math.sin(t * 1.8);
    }

    // Sibling orbital animation
    siblingNodes.forEach((node, i) => {
      const angle = node.baseAngle + t * node.orbitSpeed;
      const r = node.orbitRadius;
      const tilt = node.orbitTilt;
      node.core.position.set(
        Math.cos(angle) * r,
        tilt + Math.sin(t * 0.6 + i * 1.1) * 0.18,
        Math.sin(angle) * r,
      );
      node.core.rotation.x = t * 0.35 + i * 0.5;
      node.core.rotation.y = t * 0.55 + i * 0.9;

      const online = siblingStatus(node.label) === 'online';
      const focused = focusedSiblingId === node.label;
      const mat = node.core.material as THREE.MeshStandardMaterial;
      const shellMat = node.shell.material as THREE.LineBasicMaterial;

      if (focused) {
        mat.emissiveIntensity = 1.8;
        shellMat.opacity = 0.95;
        node.core.scale.setScalar(1.25 + 0.1 * Math.sin(t * 4));
      } else if (online) {
        mat.emissiveIntensity = 0.55 + 0.25 * Math.sin(t * 2 + i * 0.9);
        shellMat.opacity = 0.35 + 0.15 * Math.sin(t * 1.4 + i * 0.9);
        node.core.scale.setScalar(1.0);
      } else {
        mat.emissiveIntensity = 0.04 + 0.02 * Math.sin(t * 0.4);
        shellMat.opacity = 0.06;
        node.core.scale.setScalar(1.0);
      }
    });

    // Camera drift
    camera.position.x = Math.sin(t * 0.04) * 0.4;
    camera.position.y = 2.2 + Math.cos(t * 0.06) * 0.25;
    camera.lookAt(0, 0, 0);

    renderer.render(scene, camera);
  }

  function destroyThree() {
    if (rafId) { cancelAnimationFrame(rafId); rafId = null; }
    siblingNodes = [];
    coreMesh = null; coreGlow = null;
    if (renderer) { renderer.dispose(); renderer = null; }
    scene = null; camera = null;
  }

  // ── Raycasting for sibling click ──────────────────────────────────────────
  function onTopologyClick(e: MouseEvent) {
    if (!camera || !topologyEl) return;
    const rect = topologyEl.getBoundingClientRect();
    const ndc = new THREE.Vector2(
      ((e.clientX - rect.left) / rect.width) * 2 - 1,
      -((e.clientY - rect.top) / rect.height) * 2 + 1,
    );
    const rc = new THREE.Raycaster();
    rc.setFromCamera(ndc, camera);
    const meshes = siblingNodes.map(n => n.core);
    const hits = rc.intersectObjects(meshes);
    if (hits.length > 0) {
      const idx = meshes.indexOf(hits[0].object as THREE.Mesh);
      const sid = siblingNodes[idx]?.label as SiblingId;
      focusedSiblingId = focusedSiblingId === sid ? null : sid;
    }
  }

  // ═══════════════════════════════════════════════════════════════════════════
  // SIGNAL CANVAS (p5-style waveform with Canvas2D)
  // ═══════════════════════════════════════════════════════════════════════════

  type Particle = { x: number; y: number; vx: number; vy: number; life: number; color: string; sz: number };
  let particles: Particle[] = [];
  let sigRafId: number | null = null;
  let sigCtx: CanvasRenderingContext2D | null = null;
  let waveT = 0;

  function spawnParticles(count: number, cx?: number) {
    if (!signalEl) return;
    const x0 = cx ?? signalEl.width / 2;
    const y0 = signalEl.height / 2;
    for (let i = 0; i < count; i++) {
      const angle = Math.random() * Math.PI * 2;
      const spd = 1.5 + Math.random() * 4;
      particles.push({
        x: x0 + (Math.random() - 0.5) * 30,
        y: y0 + (Math.random() - 0.5) * 10,
        vx: Math.cos(angle) * spd, vy: Math.sin(angle) * spd - 1.5,
        life: 1.0,
        color: `hsl(${180 + Math.random() * 50}, 100%, ${60 + Math.random() * 25}%)`,
        sz: 1 + Math.random() * 2.5,
      });
    }
  }

  function initSignal() {
    if (!signalEl) return;
    sigCtx = signalEl.getContext('2d');
    animateSignal();
  }

  function animateSignal() {
    sigRafId = requestAnimationFrame(animateSignal);
    if (!sigCtx || !signalEl) return;
    const W = signalEl.width;
    const H = signalEl.height;
    const mid = H / 2;
    const intensity = Math.min(1, inputText.length / 60);
    waveT += 0.035;

    sigCtx.fillStyle = 'rgba(3, 10, 20, 0.45)';
    sigCtx.fillRect(0, 0, W, H);

    // Primary waveform
    sigCtx.beginPath();
    sigCtx.strokeStyle = `rgba(0, 212, 245, ${0.35 + intensity * 0.55})`;
    sigCtx.lineWidth = 1.8;
    for (let x = 0; x <= W; x++) {
      const f = 0.018 + intensity * 0.028;
      const amp = 5 + intensity * 14;
      const y = mid + Math.sin(x * f + waveT) * amp
                    + Math.sin(x * f * 1.9 + waveT * 1.4) * amp * 0.35;
      x === 0 ? sigCtx.moveTo(x, y) : sigCtx.lineTo(x, y);
    }
    sigCtx.stroke();

    // Secondary harmonic
    sigCtx.beginPath();
    sigCtx.strokeStyle = `rgba(240, 192, 64, ${0.12 + intensity * 0.22})`;
    sigCtx.lineWidth = 1;
    for (let x = 0; x <= W; x++) {
      const f = 0.032 + intensity * 0.04;
      const amp = 3 + intensity * 7;
      const y = mid - 14 + Math.sin(x * f + waveT * 1.7) * amp;
      x === 0 ? sigCtx.moveTo(x, y) : sigCtx.lineTo(x, y);
    }
    sigCtx.stroke();

    // Particles
    particles = particles.filter(p => p.life > 0.01);
    for (const p of particles) {
      p.x += p.vx; p.y += p.vy; p.vy += 0.12; p.life -= 0.022;
      const alpha = p.life;
      sigCtx.globalAlpha = Math.max(0, alpha);
      sigCtx.fillStyle = p.color;
      sigCtx.beginPath();
      sigCtx.arc(p.x, p.y, p.sz, 0, Math.PI * 2);
      sigCtx.fill();
    }
    sigCtx.globalAlpha = 1;

    // Idle sparkle dots
    if (intensity === 0 && Math.random() < 0.08) {
      sigCtx.fillStyle = `rgba(0, 212, 245, ${Math.random() * 0.35})`;
      sigCtx.fillRect(Math.random() * W, mid + (Math.random() - 0.5) * 18, 1, 1);
    }
  }

  function destroySignal() {
    if (sigRafId) { cancelAnimationFrame(sigRafId); sigRafId = null; }
    particles = [];
  }

  // ═══════════════════════════════════════════════════════════════════════════
  // MESSAGE HANDLING — delegates to $lib/copilot/chat (shared with Drawer)
  // ═══════════════════════════════════════════════════════════════════════════

  async function sendMessage() {
    const text = inputText.trim();
    if (!text || sending || loading) return;
    inputText = '';
    sending = true;
    spawnParticles(32);
    const cwd = get(serverCwd) ?? '.';
    try {
      // Surface-specific onComplete: particle burst. Voice playback stays in Drawer.
      await sendCopilotNative(text, cwd, () => spawnParticles(24));
    } finally {
      sending = false;
    }
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); sendMessage(); }
    if (e.key === 'Escape') onclose?.();
  }

  // ── Helpers ────────────────────────────────────────────────────────────────
  function siblingColor(sid?: string): string {
    if (!sid) return '#f0c040';
    return SIBLING_COLORS[sid as keyof typeof SIBLING_COLORS] ?? '#f0c040';
  }

  function fmtTime(ts: string): string {
    try { return new Date(ts).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', second: '2-digit', hour12: false }); }
    catch { return ''; }
  }

  function msgClass(content: string): string {
    if (content.startsWith('TOOL:')) return 'tool-start';
    if (content.startsWith('DONE:')) return 'tool-done';
    if (content.startsWith('STATUS:')) return 'status';
    if (content.startsWith('ERR:')) return 'msg-err';
    return '';
  }

  function msgLabel(content: string): string {
    if (content.startsWith('TOOL:')) return content.slice(5).split('\n')[0];
    if (content.startsWith('DONE:')) return content.slice(5).split('\n')[0];
    if (content.startsWith('STATUS:')) return content.slice(7);
    if (content.startsWith('ERR:')) return content.slice(4);
    return '';
  }

  function msgBody(content: string): string {
    if (content.startsWith('TOOL:') || content.startsWith('DONE:')) {
      const lines = content.split('\n');
      return lines.slice(1).join('\n');
    }
    return '';
  }

  function toggleExpand(set: Set<string>, id: string, setter: (s: Set<string>) => void) {
    const next = new Set(set);
    next.has(id) ? next.delete(id) : next.add(id);
    setter(next);
  }

  // ── Resize ─────────────────────────────────────────────────────────────────
  function onResize() {
    if (renderer && camera && topologyEl) {
      const W = topologyEl.clientWidth;
      const H = topologyEl.clientHeight;
      renderer.setSize(W, H, false);
      camera.aspect = W / H;
      camera.updateProjectionMatrix();
    }
    if (signalEl) {
      signalEl.width = signalEl.clientWidth;
      signalEl.height = signalEl.clientHeight;
    }
  }

  // ── Auto-scroll ────────────────────────────────────────────────────────────
  $effect(() => {
    void msgs.length;
    if (streamEl) requestAnimationFrame(() => { if (streamEl) streamEl.scrollTop = streamEl.scrollHeight; });
  });

  onMount(() => {
    if (topologyEl) { topologyEl.width = topologyEl.clientWidth || 400; topologyEl.height = topologyEl.clientHeight || 500; }
    if (signalEl) { signalEl.width = signalEl.clientWidth || 800; signalEl.height = signalEl.clientHeight || 64; }
    initThree();
    initSignal();
    window.addEventListener('resize', onResize);
    setTimeout(() => inputEl?.focus(), 80);
  });

  onDestroy(() => {
    destroyThree();
    destroySignal();
    window.removeEventListener('resize', onResize);
  });
</script>

<svelte:head>
  <link rel="preconnect" href="https://fonts.googleapis.com">
  <link href="https://fonts.googleapis.com/css2?family=Aldrich&display=swap" rel="stylesheet">
</svelte:head>

<!-- ═══════════════════════════════════════════════════════════════════════════
     SURFACE OVERLAY
     ═══════════════════════════════════════════════════════════════════════════ -->
<div class="surface" role="dialog" aria-modal="true" aria-label="Copilot Surface">

  <!-- ── Header bar ───────────────────────────────────────────────────────── -->
  <header class="surface-header">
    <div class="header-left">
      <span class="logo-mark">⬡</span>
      <span class="logo-text">SIGNAL SUBSTRATE</span>
      <span class="header-sep">│</span>
      <span class="header-sub">LA COPILOT</span>
      {#if loading}
        <span class="pulse-dot" aria-hidden="true"></span>
        <span class="header-status">PROCESSING</span>
      {:else}
        <span class="header-status dim">READY</span>
      {/if}
    </div>
    <div class="header-right">
      {#if focusedSiblingId}
        <span class="focused-chip" style="--scolor: {siblingColor(focusedSiblingId)}">
          ⬡ {focusedSiblingId.toUpperCase()}
          <button class="chip-clear" onclick={() => focusedSiblingId = null} aria-label="clear focus">×</button>
        </span>
      {/if}
      <button class="icon-btn" onclick={() => { clearCopilotHistory(); }} title="Clear history">⌂</button>
      <button class="icon-btn" onclick={onclose} title="Close (Esc)">✕</button>
    </div>
  </header>

  <!-- ── Main body ────────────────────────────────────────────────────────── -->
  <div class="surface-body">

    <!-- LEFT: Three.js topology -->
    <aside class="topology-panel">
      <canvas
        bind:this={topologyEl}
        class="topology-canvas"
        onclick={onTopologyClick}
        aria-label="Squad topology — click a sibling to focus"
      ></canvas>

      <!-- Sibling status legend -->
      <div class="sibling-legend">
        {#each SIBLING_LIST as sid}
          {@const status = siblingStatus(sid)}
          {@const focused = focusedSiblingId === sid}
          <button
            class="sibling-chip"
            class:focused
            class:online={status === 'online'}
            class:offline={status === 'offline' || status === 'unconfigured'}
            class:chip-both={labelMode === 'both'}
            style="--sc: {siblingColor(sid)}"
            onclick={() => focusedSiblingId = focused ? null : sid as SiblingId}
            title="{SIBLING_DISPLAY[sid]} · {SIBLING_DOMAINS[sid]} · {status}"
          >
            <span class="sc-dot"></span>
            <span class="sc-label">
              {#if labelMode === 'both'}
                <span class="sc-name">{SIBLING_DISPLAY[sid]}</span>
                <span class="sc-domain">{SIBLING_DOMAINS[sid]}</span>
              {:else if labelMode === 'codename'}
                {SIBLING_DISPLAY[sid]}
              {:else}
                {SIBLING_DOMAINS[sid]}
              {/if}
            </span>
            <span class="sc-status">{status === 'online' ? '●' : '○'}</span>
          </button>
        {/each}
        <button
          class="sibling-chip mode-toggle"
          onclick={cycleMode}
          title="Switch label mode — current: {MODE_LABEL[labelMode]}"
          aria-label="Toggle label mode"
        >⇄ {MODE_LABEL[labelMode]}</button>
      </div>
    </aside>

    <!-- RIGHT: Conversation stream -->
    <section class="stream-panel">
      <div class="stream-scroll" bind:this={streamEl}>
        {#if msgs.length === 0}
          <div class="empty-state">
            <div class="empty-glyph">⬡</div>
            <p class="empty-title">Signal substrate ready</p>
            <p class="empty-sub">Transmit a message to the squad</p>
          </div>
        {:else}
          {#each msgs as msg (msg.id)}
            {#if msg.role === 'user'}
              <!-- USER TRANSMISSION -->
              <div class="msg-row user-row">
                <div class="msg-meta user-meta">
                  <span class="msg-label-you">YOU</span>
                  <span class="msg-time">{fmtTime(msg.timestamp)}</span>
                </div>
                <div class="msg-tile user-tile">
                  <p class="msg-text">{msg.content}</p>
                </div>
              </div>

            {:else if msg.role === 'assistant'}
              <!-- AGENT RESPONSE -->
              <div class="msg-row agent-row">
                <div class="msg-meta agent-meta">
                  <span class="sibling-badge" style="--sc: {siblingColor(msg.sibling)}">
                    {agentDomain(msg.sibling ?? 'system', labelMode === 'codename')}
                  </span>
                  <span class="msg-time">{fmtTime(msg.timestamp)}</span>
                </div>
                <div class="msg-tile agent-tile" style="--sc: {siblingColor(msg.sibling)}">
                  <div class="msg-md">
                    {@html renderMarkdown(msg.content)}
                  </div>
                </div>
              </div>

            {:else if msg.kind === 'thinking'}
              <!-- THINKING BLOCK -->
              <div class="msg-row system-row">
                <button
                  class="thinking-header"
                  onclick={() => toggleExpand(expandedThinking, msg.id, v => expandedThinking = v)}
                  aria-expanded={expandedThinking.has(msg.id)}
                >
                  <span class="thinking-icon">◈</span>
                  <span class="thinking-label">REASONING</span>
                  <span class="thinking-chevron">{expandedThinking.has(msg.id) ? '▲' : '▼'}</span>
                </button>
                {#if expandedThinking.has(msg.id)}
                  <pre class="thinking-body">{msg.content}</pre>
                {/if}
              </div>

            {:else if msg.role === 'system' && msgClass(msg.content) === 'tool-start'}
              <!-- TOOL INVOCATION -->
              {@const label = msgLabel(msg.content)}
              {@const body = msgBody(msg.content)}
              <div class="msg-row tool-row">
                <button
                  class="tool-header"
                  onclick={() => toggleExpand(expandedTool, msg.id, v => expandedTool = v)}
                  aria-expanded={expandedTool.has(msg.id)}
                >
                  <span class="tool-icon">⬢</span>
                  <span class="tool-name">{label}</span>
                  <span class="tool-tag">INVOKE</span>
                  <span class="tool-chevron">{expandedTool.has(msg.id) ? '▲' : '▼'}</span>
                </button>
                {#if expandedTool.has(msg.id) && body}
                  <pre class="tool-body">{body}</pre>
                {/if}
              </div>

            {:else if msg.role === 'system' && msgClass(msg.content) === 'tool-done'}
              <!-- TOOL RESULT -->
              {@const label = msgLabel(msg.content)}
              {@const body = msgBody(msg.content)}
              {@const ok = !label.includes('err')}
              <div class="msg-row tool-done-row" class:ok class:fail={!ok}>
                <div class="tool-done-header">
                  <span class="tool-done-icon">{ok ? '◈' : '✗'}</span>
                  <span class="tool-done-label">{label}</span>
                </div>
                {#if body}
                  <pre class="tool-done-body">{body}</pre>
                {/if}
              </div>

            {:else if msg.role === 'system' && msgClass(msg.content) === 'msg-err'}
              <!-- ERROR -->
              <div class="msg-row err-row">
                <span class="err-icon">✗</span>
                <span class="err-text">{msgLabel(msg.content)}</span>
              </div>

            {:else if msg.role === 'system'}
              <!-- STATUS UPDATE -->
              <div class="msg-row status-row">
                <span class="status-text">{msgLabel(msg.content) || msg.content}</span>
                <span class="msg-time">{fmtTime(msg.timestamp)}</span>
              </div>
            {/if}
          {/each}

          {#if loading}
            <div class="msg-row agent-row">
              <div class="msg-tile agent-tile loading-tile">
                <div class="loading-bars">
                  <span></span><span></span><span></span>
                </div>
              </div>
            </div>
          {/if}
        {/if}
      </div>
    </section>
  </div>

  <!-- ── Input bar ─────────────────────────────────────────────────────────── -->
  <footer class="surface-footer">
    <canvas bind:this={signalEl} class="signal-canvas" aria-hidden="true"></canvas>

    <div class="input-row">
      <div class="input-wrap">
        <textarea
          bind:this={inputEl}
          bind:value={inputText}
          class="signal-input"
          placeholder="Transmit to the squad…"
          rows="1"
          onkeydown={onKeydown}
          disabled={sending || loading}
          aria-label="Copilot input"
        ></textarea>
        <div class="input-hints">
          <span class="hint-key">⏎ Send</span>
          <span class="hint-sep">·</span>
          <span class="hint-key">⇧⏎ Newline</span>
          <span class="hint-sep">·</span>
          <span class="hint-key">ESC Close</span>
          {#if inputText.length > 0}
            <span class="char-count">{inputText.length}</span>
          {/if}
        </div>
      </div>
      <button
        class="send-btn"
        class:active={inputText.trim().length > 0 && !loading}
        onclick={sendMessage}
        disabled={!inputText.trim() || loading || sending}
        aria-label="Send message"
      >
        {#if loading || sending}
          <span class="send-spin">◌</span>
        {:else}
          <span class="send-icon">⟶</span>
        {/if}
      </button>
    </div>
  </footer>
</div>

<style>
  @import url('https://fonts.googleapis.com/css2?family=Aldrich&display=swap');

  /* ── CSS Variables ────────────────────────────────────────────────────────── */
  :root {
    --cs-bg: #02080f;
    --cs-surface: #070e1c;
    --cs-surface-2: #0a1628;
    --cs-border: rgba(0, 160, 220, 0.12);
    --cs-border-strong: rgba(0, 212, 245, 0.25);
    --cs-signal: #00d4f5;
    --cs-signal-dim: rgba(0, 212, 245, 0.35);
    --cs-gold: #f0c040;
    --cs-gold-dim: rgba(240, 192, 64, 0.3);
    --cs-muted: #3a5070;
    --cs-text: #c8d8e8;
    --cs-text-dim: #4a6080;
    --cs-err: #ff4455;
    --cs-ok: #00e080;
    --cs-tool: #1a4a2a;
    --cs-tool-border: #00a050;
    --cs-font-display: 'Aldrich', monospace;
    --cs-font-mono: 'JetBrains Mono Variable', 'JetBrains Mono', monospace;
  }

  /* ── Surface shell ────────────────────────────────────────────────────────── */
  .surface {
    position: fixed;
    inset: 0;
    z-index: 55;
    display: flex;
    flex-direction: column;
    background: var(--cs-bg);
    font-family: var(--cs-font-mono);
    color: var(--cs-text);
    overflow: hidden;
  }

  /* grain texture overlay */
  .surface::before {
    content: '';
    position: absolute;
    inset: 0;
    background-image: url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='200' height='200'%3E%3Cfilter id='n'%3E%3CfeTurbulence type='fractalNoise' baseFrequency='0.9' numOctaves='4' stitchTiles='stitch'/%3E%3C/filter%3E%3Crect width='200' height='200' filter='url(%23n)' opacity='0.025'/%3E%3C/svg%3E");
    pointer-events: none;
    z-index: 0;
  }

  .surface > * { position: relative; z-index: 1; }

  /* ── Header ───────────────────────────────────────────────────────────────── */
  .surface-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    height: 44px;
    padding: 0 16px;
    background: var(--cs-surface);
    border-bottom: 1px solid var(--cs-border);
    flex-shrink: 0;
  }

  .header-left, .header-right {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .logo-mark {
    color: var(--cs-signal);
    font-size: 16px;
    filter: drop-shadow(0 0 6px var(--cs-signal));
  }

  .logo-text {
    font-family: var(--cs-font-display);
    font-size: 13px;
    letter-spacing: 0.2em;
    color: var(--cs-signal);
    text-shadow: 0 0 12px rgba(0, 212, 245, 0.5);
  }

  .header-sep { color: var(--cs-border-strong); font-size: 14px; }

  .header-sub {
    font-family: var(--cs-font-display);
    font-size: 11px;
    letter-spacing: 0.15em;
    color: var(--cs-gold);
    opacity: 0.8;
  }

  .header-status {
    font-family: var(--cs-font-display);
    font-size: 10px;
    letter-spacing: 0.15em;
    color: var(--cs-signal);
  }

  .header-status.dim { color: var(--cs-muted); }

  .pulse-dot {
    width: 6px; height: 6px;
    border-radius: 50%;
    background: var(--cs-signal);
    box-shadow: 0 0 8px var(--cs-signal);
    animation: dot-pulse 0.8s ease-in-out infinite alternate;
  }

  @keyframes dot-pulse {
    from { opacity: 0.4; transform: scale(0.8); }
    to   { opacity: 1.0; transform: scale(1.1); }
  }

  .focused-chip {
    display: flex;
    align-items: center;
    gap: 5px;
    padding: 2px 8px 2px 6px;
    border-radius: 3px;
    background: rgba(0,0,0,0.4);
    border: 1px solid var(--scolor);
    font-size: 10px;
    color: var(--scolor);
    font-family: var(--cs-font-display);
    letter-spacing: 0.1em;
    filter: drop-shadow(0 0 4px var(--scolor));
  }

  .chip-clear {
    background: none; border: none;
    color: inherit; cursor: pointer;
    font-size: 12px; padding: 0; line-height: 1;
    opacity: 0.6;
    transition: opacity 0.15s;
  }
  .chip-clear:hover { opacity: 1; }

  .icon-btn {
    background: none; border: none;
    color: var(--cs-muted); cursor: pointer;
    font-size: 13px; padding: 4px 6px;
    border-radius: 3px;
    transition: color 0.15s, background 0.15s;
    font-family: var(--cs-font-mono);
  }
  .icon-btn:hover { color: var(--cs-signal); background: rgba(0,212,245,0.06); }

  /* ── Body ─────────────────────────────────────────────────────────────────── */
  .surface-body {
    flex: 1;
    display: flex;
    min-height: 0;
    overflow: hidden;
  }

  /* ── Topology panel ───────────────────────────────────────────────────────── */
  .topology-panel {
    width: 36%;
    min-width: 260px;
    max-width: 420px;
    display: flex;
    flex-direction: column;
    border-right: 1px solid var(--cs-border);
    background: rgba(0, 8, 20, 0.6);
    overflow: hidden;
  }

  .topology-canvas {
    flex: 1;
    width: 100%;
    display: block;
    cursor: crosshair;
  }

  .sibling-legend {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    padding: 10px 12px;
    border-top: 1px solid var(--cs-border);
    background: rgba(0, 4, 12, 0.8);
  }

  .sibling-chip {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 3px 8px;
    border-radius: 2px;
    background: rgba(0,0,0,0.4);
    border: 1px solid rgba(255,255,255,0.05);
    cursor: pointer;
    font-family: var(--cs-font-display);
    font-size: 9px;
    letter-spacing: 0.12em;
    color: var(--cs-muted);
    transition: all 0.2s;
  }

  .sibling-chip.online { color: var(--sc); border-color: rgba(var(--sc), 0.3); }
  .sibling-chip.focused {
    color: var(--sc);
    border-color: var(--sc);
    background: rgba(0,0,0,0.6);
    box-shadow: 0 0 8px rgba(0,0,0,0.5), inset 0 0 8px rgba(0,0,0,0.3), 0 0 0 1px var(--sc);
    filter: drop-shadow(0 0 4px var(--sc));
  }
  .sibling-chip:hover:not(.focused) { border-color: rgba(255,255,255,0.12); color: var(--cs-text); }

  .sc-dot {
    width: 5px; height: 5px;
    border-radius: 50%;
    background: var(--sc);
    box-shadow: 0 0 4px var(--sc);
    flex-shrink: 0;
  }
  .sibling-chip.offline .sc-dot { background: var(--cs-muted); box-shadow: none; }

  .sc-label {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 1px;
    line-height: 1;
  }
  .chip-both { align-items: flex-start; padding-top: 4px; padding-bottom: 4px; }
  .sc-name { font-size: 9px; letter-spacing: 0.12em; }
  .sc-domain { font-size: 7px; letter-spacing: 0.06em; opacity: 0.5; font-family: var(--cs-font-mono); }
  .chip-both .sc-dot { margin-top: 3px; }
  .chip-both .sc-status { margin-top: 1px; }
  .sc-status { font-size: 8px; opacity: 0.5; }
  .mode-toggle {
    margin-left: auto;
    border-style: dashed;
    opacity: 0.55;
    letter-spacing: 0.08em;
    font-size: 8px;
  }
  .mode-toggle:hover { opacity: 1; color: var(--cs-gold); border-color: var(--cs-gold); }

  /* ── Stream panel ─────────────────────────────────────────────────────────── */
  .stream-panel {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-width: 0;
    overflow: hidden;
  }

  .stream-scroll {
    flex: 1;
    overflow-y: auto;
    padding: 16px 20px;
    display: flex;
    flex-direction: column;
    gap: 10px;
    scrollbar-width: thin;
    scrollbar-color: var(--cs-border-strong) transparent;
  }

  .stream-scroll::-webkit-scrollbar { width: 4px; }
  .stream-scroll::-webkit-scrollbar-track { background: transparent; }
  .stream-scroll::-webkit-scrollbar-thumb { background: var(--cs-border-strong); border-radius: 2px; }

  /* ── Empty state ────────────────────────────────────────────────────────── */
  .empty-state {
    flex: 1; display: flex; flex-direction: column;
    align-items: center; justify-content: center;
    padding: 60px 20px; gap: 8px;
  }

  .empty-glyph {
    font-size: 48px;
    color: var(--cs-signal-dim);
    filter: drop-shadow(0 0 16px var(--cs-signal-dim));
    animation: glyph-float 3s ease-in-out infinite;
  }

  @keyframes glyph-float {
    0%, 100% { transform: translateY(0); }
    50% { transform: translateY(-8px); }
  }

  .empty-title {
    font-family: var(--cs-font-display);
    font-size: 13px;
    letter-spacing: 0.15em;
    color: var(--cs-signal);
    opacity: 0.6;
    margin: 0;
  }

  .empty-sub {
    font-size: 11px;
    color: var(--cs-text-dim);
    margin: 0;
    font-family: var(--cs-font-display);
    letter-spacing: 0.1em;
  }

  /* ── Message rows ─────────────────────────────────────────────────────────── */
  .msg-row { display: flex; flex-direction: column; gap: 4px; }

  .msg-meta {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .user-meta { justify-content: flex-end; }
  .agent-meta { justify-content: flex-start; }

  .msg-label-you {
    font-family: var(--cs-font-display);
    font-size: 9px;
    letter-spacing: 0.15em;
    color: var(--cs-signal);
    opacity: 0.7;
  }

  .sibling-badge {
    font-family: var(--cs-font-display);
    font-size: 9px;
    letter-spacing: 0.12em;
    color: var(--sc);
    text-shadow: 0 0 6px var(--sc);
  }

  .msg-time {
    font-size: 9px;
    color: var(--cs-text-dim);
    font-family: var(--cs-font-display);
  }

  .msg-tile {
    max-width: 82%;
    padding: 10px 14px;
    border-radius: 3px;
    font-size: 13px;
    line-height: 1.65;
    word-break: break-word;
  }

  /* User tile — right-aligned, sharp cyan border */
  .user-row { align-items: flex-end; }

  .user-tile {
    background: rgba(0, 212, 245, 0.06);
    border: 1px solid rgba(0, 212, 245, 0.18);
    border-left: 3px solid var(--cs-signal);
    color: var(--cs-text);
  }

  .msg-text { margin: 0; white-space: pre-wrap; }

  /* Agent tile — left-aligned, sibling color left bar */
  .agent-row { align-items: flex-start; }

  .agent-tile {
    background: rgba(10, 20, 40, 0.7);
    border: 1px solid rgba(255,255,255,0.05);
    border-left: 3px solid var(--sc, var(--cs-gold));
    color: var(--cs-text);
  }

  /* Markdown content */
  .msg-md :global(p) { margin: 0 0 8px; }
  .msg-md :global(p:last-child) { margin-bottom: 0; }
  .msg-md :global(code) {
    font-family: var(--cs-font-mono);
    font-size: 11px;
    background: rgba(0,0,0,0.4);
    border: 1px solid var(--cs-border);
    padding: 1px 4px;
    border-radius: 2px;
    color: var(--cs-signal);
  }
  .msg-md :global(pre) {
    background: rgba(0,0,0,0.5);
    border: 1px solid var(--cs-border);
    padding: 10px 12px;
    border-radius: 3px;
    overflow-x: auto;
    font-size: 11px;
    margin: 8px 0;
  }
  .msg-md :global(pre code) { background: none; border: none; padding: 0; }
  .msg-md :global(h1), .msg-md :global(h2), .msg-md :global(h3) {
    font-family: var(--cs-font-display);
    letter-spacing: 0.08em;
    color: var(--cs-signal);
    margin: 12px 0 4px;
  }
  .msg-md :global(ul), .msg-md :global(ol) { margin: 6px 0; padding-left: 18px; }
  .msg-md :global(li) { margin: 3px 0; }
  .msg-md :global(a) { color: var(--cs-signal); text-decoration: underline; }

  /* Loading tile */
  .loading-tile {
    padding: 12px 14px;
    border: 1px solid rgba(240,192,64,0.15);
    border-left-color: var(--cs-gold);
  }

  .loading-bars {
    display: flex;
    gap: 5px;
    align-items: center;
    height: 18px;
  }

  .loading-bars span {
    width: 3px; height: 12px;
    background: var(--cs-gold);
    border-radius: 2px;
    animation: bar-pulse 1s ease-in-out infinite;
  }
  .loading-bars span:nth-child(2) { animation-delay: 0.15s; }
  .loading-bars span:nth-child(3) { animation-delay: 0.3s; }

  @keyframes bar-pulse {
    0%, 100% { transform: scaleY(0.4); opacity: 0.4; }
    50% { transform: scaleY(1.0); opacity: 1.0; }
  }

  /* ── Thinking block ───────────────────────────────────────────────────────── */
  .system-row { align-items: flex-start; }

  .thinking-header {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 5px 10px;
    background: rgba(0,0,0,0.3);
    border: 1px solid rgba(100,80,200,0.2);
    border-radius: 3px;
    cursor: pointer;
    font-family: var(--cs-font-display);
    font-size: 10px;
    letter-spacing: 0.1em;
    color: rgba(140,120,255,0.7);
    transition: all 0.15s;
    width: 100%;
    text-align: left;
  }
  .thinking-header:hover { border-color: rgba(140,120,255,0.4); color: rgba(160,140,255,0.9); }

  .thinking-icon { font-size: 11px; }
  .thinking-label { flex: 1; }
  .thinking-chevron { font-size: 9px; opacity: 0.6; }

  .thinking-body {
    padding: 8px 12px;
    margin: 0;
    font-size: 11px;
    line-height: 1.6;
    color: rgba(140,120,255,0.6);
    font-style: italic;
    background: rgba(20,10,40,0.5);
    border: 1px solid rgba(100,80,200,0.15);
    border-top: none;
    border-radius: 0 0 3px 3px;
    white-space: pre-wrap;
    word-break: break-word;
  }

  /* ── Tool blocks ──────────────────────────────────────────────────────────── */
  .tool-row { align-items: flex-start; }

  .tool-header {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 5px 10px;
    background: rgba(0,20,10,0.5);
    border: 1px solid rgba(0,120,60,0.3);
    border-radius: 3px;
    cursor: pointer;
    width: 100%;
    text-align: left;
    font-family: var(--cs-font-mono);
    font-size: 11px;
    color: #00c870;
    transition: all 0.15s;
  }
  .tool-header:hover { border-color: rgba(0,200,100,0.5); }

  .tool-icon { font-size: 11px; opacity: 0.8; }
  .tool-name { flex: 1; }
  .tool-tag {
    font-family: var(--cs-font-display);
    font-size: 9px;
    letter-spacing: 0.12em;
    color: rgba(0,200,100,0.5);
    padding: 1px 5px;
    border: 1px solid rgba(0,200,100,0.2);
    border-radius: 2px;
  }
  .tool-chevron { font-size: 9px; opacity: 0.5; }

  .tool-body {
    margin: 0;
    padding: 8px 12px;
    font-size: 10px;
    line-height: 1.6;
    color: #00a050;
    background: rgba(0,15,8,0.7);
    border: 1px solid rgba(0,120,60,0.2);
    border-top: none;
    border-radius: 0 0 3px 3px;
    white-space: pre-wrap;
    word-break: break-word;
    overflow-x: auto;
  }

  /* ── Tool done ─────────────────────────────────────────────────────────── */
  .tool-done-row {
    padding: 4px 10px;
    border-radius: 3px;
    border: 1px solid rgba(0,200,100,0.15);
    background: rgba(0,10,5,0.4);
  }
  .tool-done-row.fail { border-color: rgba(255,60,60,0.15); background: rgba(20,0,0,0.4); }

  .tool-done-header {
    display: flex;
    align-items: center;
    gap: 6px;
    font-family: var(--cs-font-mono);
    font-size: 10px;
    color: var(--cs-ok);
  }
  .tool-done-row.fail .tool-done-header { color: var(--cs-err); }

  .tool-done-body {
    margin: 4px 0 0;
    font-size: 10px;
    color: rgba(0,180,80,0.7);
    white-space: pre-wrap;
    word-break: break-word;
    overflow-x: auto;
  }

  /* ── Error / status ───────────────────────────────────────────────────────── */
  .err-row {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 4px 10px;
    background: rgba(30,0,0,0.4);
    border: 1px solid rgba(255,60,60,0.2);
    border-radius: 3px;
  }
  .err-icon { color: var(--cs-err); font-size: 12px; }
  .err-text { font-size: 11px; color: rgba(255,100,100,0.8); }

  .status-row {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 2px 4px;
  }
  .status-text {
    font-family: var(--cs-font-display);
    font-size: 10px;
    letter-spacing: 0.08em;
    color: var(--cs-text-dim);
  }

  /* ── Footer / input bar ───────────────────────────────────────────────────── */
  .surface-footer {
    flex-shrink: 0;
    border-top: 1px solid var(--cs-border);
    background: var(--cs-surface);
    display: flex;
    flex-direction: column;
  }

  .signal-canvas {
    width: 100%;
    height: 52px;
    display: block;
    background: var(--cs-bg);
    border-bottom: 1px solid var(--cs-border);
  }

  .input-row {
    display: flex;
    align-items: flex-end;
    gap: 10px;
    padding: 10px 16px 12px;
  }

  .input-wrap {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .signal-input {
    width: 100%;
    background: rgba(0,8,20,0.8);
    border: 1px solid var(--cs-border);
    border-radius: 3px;
    color: var(--cs-text);
    font-family: var(--cs-font-mono);
    font-size: 13px;
    line-height: 1.6;
    padding: 8px 12px;
    resize: none;
    outline: none;
    transition: border-color 0.2s;
    min-height: 40px;
    max-height: 160px;
    overflow-y: auto;
    box-sizing: border-box;
  }

  .signal-input:focus {
    border-color: var(--cs-signal-dim);
    box-shadow: 0 0 0 1px rgba(0,212,245,0.08), 0 0 16px rgba(0,212,245,0.04);
  }

  .signal-input::placeholder { color: var(--cs-text-dim); }
  .signal-input:disabled { opacity: 0.5; cursor: not-allowed; }

  .input-hints {
    display: flex;
    align-items: center;
    gap: 6px;
    font-family: var(--cs-font-display);
    font-size: 9px;
    letter-spacing: 0.08em;
    color: var(--cs-text-dim);
    padding: 0 2px;
  }

  .hint-key { color: rgba(0,212,245,0.35); }
  .hint-sep { opacity: 0.3; }
  .char-count { margin-left: auto; color: var(--cs-signal); opacity: 0.6; }

  .send-btn {
    width: 44px; height: 44px;
    border-radius: 3px;
    background: rgba(0,8,20,0.9);
    border: 1px solid var(--cs-border);
    color: var(--cs-muted);
    font-size: 18px;
    cursor: pointer;
    transition: all 0.2s;
    display: flex; align-items: center; justify-content: center;
    flex-shrink: 0;
  }

  .send-btn.active {
    border-color: var(--cs-signal);
    color: var(--cs-signal);
    box-shadow: 0 0 12px rgba(0,212,245,0.2);
  }
  .send-btn.active:hover {
    background: rgba(0,212,245,0.1);
    box-shadow: 0 0 20px rgba(0,212,245,0.3);
  }
  .send-btn:disabled:not(.active) { cursor: not-allowed; opacity: 0.4; }

  .send-icon { font-family: var(--cs-font-display); }
  .send-spin {
    animation: spin-slow 1.2s linear infinite;
    display: inline-block;
  }

  @keyframes spin-slow {
    from { transform: rotate(0deg); }
    to { transform: rotate(360deg); }
  }
</style>
