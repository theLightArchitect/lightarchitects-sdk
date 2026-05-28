<script lang="ts">
  import { get } from 'svelte/store';
  import {
    copilotMessages, copilotLoading, currentBuildId, activeBuild,
    findings, selectedPillar, focusedSibling, spikeSibling,
    buildBuildContext, authProfile, ollamaConfig, terminalConnected,
    builds, siblingHealth, alertStats, drawerHeightPx, waves,
    clearCopilotHistory, isNativeAgent, voiceEnabled, activityFeed,
    snapshotContextForCopilot, copilotContextStatus, recentEventBuffer, copilotGrounding,
    ayinStatus,
  } from '$lib/stores';
  import { navigate } from '$lib/routes';
  import { SIBLING_COLORS } from '$lib/design-tokens';
  import { api } from '$lib/api';
  import { authHeaders } from '$lib/auth';
  import { parseCommand, SLASH_COMMANDS } from '$lib/commands';
  import { connectSSE, disconnectSSE, reconnectSSE, sseConnected } from '$lib/sse';
  import { TerminalWS, AgentWS } from '$lib/ws';
  import SiblingDispatch from './SiblingDispatch.svelte';
  import SiblingBadge from './SiblingBadge.svelte';
  import TurnLineageStrip from './TurnLineageStrip.svelte';
  import StrategyPhaseRibbon from './StrategyPhaseRibbon.svelte';
  import AyinTracesPanel from './panels/AyinTracesPanel.svelte';
  import CopilotContextTray from './CopilotContextTray.svelte';
  import ContextBar from './ContextBar.svelte';
  import OllamaConfigModal from './OllamaConfigModal.svelte';
  import SettingsOverlay from './SettingsOverlay.svelte';
  import PolytopeIcon from './PolytopeIcon.svelte';
  import { settingsOpen, pendingResumeSessionId, serverCwd, persistedConfig, selectedModel } from '$lib/setup';
  import { strategyHitl, copilotDrawerOpen } from '$lib/stores';
  import { drawerWidthPx } from '$lib/stores';
  import { selectedPreset, selectedTarget, PRESET_DISPLAY, quickPickOpen } from '$lib/cockpit/stores';
  import { parseChips } from '$lib/cockpit/copilotChips';
  import { saveSettingsDebounced } from '$lib/settings-persistence';
  import { renderMarkdown } from '$lib/markdown';
  import type { CopilotMessage, SiblingId, AgentEvent, CopilotContextSnapshot } from '$lib/types';
  import { Terminal } from '@xterm/xterm';
  import { FitAddon } from '@xterm/addon-fit';

  // --- Drawer state ---
  let open = $state(false);
  // Sync open → shared store so PolytopeButton can reflect active state
  $effect(() => { copilotDrawerOpen.set(open); });
  let heightPx = $state(350);   // kept for compatibility; sidebar uses widthPx
  let widthPx = $state(320);

  // P1-4: latest loop_count from the activity feed (CopilotActivityEvent).
  let latestLoopCount = $derived(
    (() => {
      for (let i = $activityFeed.length - 1; i >= 0; i--) {
        const e = $activityFeed[i];
        if (e.source === 'copilot' && e.event.loop_count != null) {
          return e.event.loop_count;
        }
      }
      return null;
    })()
  );
  const MIN_HEIGHT = 180;
  const MAX_HEIGHT_RATIO = 0.85;
  const MIN_WIDTH = 260;
  const MAX_WIDTH_RATIO = 0.5;

  // Sidebar is a left panel — height is full viewport; publish width to layout.
  $effect(() => {
    drawerHeightPx.set(0);
  });
  $effect(() => {
    const next = open ? widthPx : 0;
    if (get(drawerWidthPx) !== next) drawerWidthPx.set(next);
  });
  // Cleanup @-file autocomplete timer on drawer close/unmount.
  $effect(() => {
    if (!open && atFetchTimer !== null) {
      clearTimeout(atFetchTimer);
      atFetchTimer = null;
    }
    return () => {
      if (atFetchTimer !== null) clearTimeout(atFetchTimer);
    };
  });

  function onWindowResize() {
    const maxH = Math.floor(window.innerHeight * MAX_HEIGHT_RATIO);
    if (heightPx > maxH) heightPx = maxH;
    const maxW = Math.floor(window.innerWidth * MAX_WIDTH_RATIO);
    if (widthPx > maxW) widthPx = maxW;
  }

  // --- Session state ---
  let mode = $state<'chat' | 'terminal'>('chat');
  /** Position mode — 'drawer' renders pinned to bottom; 'overlay' floats freely. */
  let positionMode = $state<'drawer' | 'overlay'>('drawer');
  let terminalEl: HTMLDivElement | undefined = $state();
  let sharedBuildId = $state<string | null>(null);
  let cwd = $derived($serverCwd);
  let showOllamaModal = $state(false);
  let input = $state('');
  let showSuggestions = $state(false);
  let slashSuggestionIndex = $state(0);
  let tesseractOpen = $state(false);
  let tracesOpen = $state(false);
  let searchQuery = $state('');
  let showSearch = $state(false);
  let messagesEl: HTMLDivElement | undefined = $state();
  let oscillatorEl: HTMLCanvasElement | undefined = $state();
  let audioEl: HTMLAudioElement | undefined = $state();
  // Re-derives whenever the event buffer changes so the tray stays fresh.
  let contextSnapshot = $derived.by<CopilotContextSnapshot | null>(() => {
    const _buf = $recentEventBuffer; // track store for reactivity
    return _buf.length > 0 ? snapshotContextForCopilot() : null;
  });
  let voicePlaying = $state(false);

  // --- Native agent bridge state ---
  //
  // Phase-10 (Phase 4): native agents (`lightarchitects_native`) route through
  // the HTTP SSE path `POST /api/builds/:id/copilot` directly — no WebSocket
  // bridge. The bridge spawned a sub-process that immediately exited, leaving
  // the drawer stuck in a "Thinking…" state with no provider response. Other
  // agent kinds keep their WS bridge until they too move to SSE.
  let agentWs: AgentWS | null = $state(null);
  let buildAgentKind = $state<string | undefined>(undefined);

  // Wire AgentWS only for non-native agent kinds that still use the WS bridge.
  // For `lightarchitects_native`, sendMessage() uses api.copilotChatNative
  // directly (HTTP SSE).
  $effect(() => {
    const buildId = sharedBuildId;
    const isLaNative = $isNativeAgent || buildAgentKind === 'lightarchitects_native';
    if (!buildId || isLaNative) {
      agentWs?.disconnect();
      agentWs = null;
      return;
    }

    const ws = new AgentWS(
      buildId,
      (ev: AgentEvent) => handleAgentEvent(ev),
      () => { /* connected — no-op; non-native bridge has no queue today */ },
      () => {
        // disconnected — if we were mid-turn, surface it so the UI doesn't hang
        if (get(copilotLoading)) {
          copilotLoading.set(false);
          addMessage('system', 'Agent connection lost. Reconnecting…');
        }
      },
    );
    ws.connect();
    agentWs = ws;

    return () => {
      ws.disconnect();
      agentWs = null;
    };
  });

  // History search — filter by case-insensitive substring match on content
  const filteredMessages = $derived(
    searchQuery.trim()
      ? $copilotMessages.filter(m => m.content.toLowerCase().includes(searchQuery.toLowerCase()))
      : $copilotMessages
  );

  // --- @-file autocomplete ---
  let atSuggestions = $state<string[]>([]);
  let atQuery = $state('');
  let atSuggestionIndex = $state(0);
  let atFetchTimer: ReturnType<typeof setTimeout> | null = null;

  function extractAtQuery(val: string): string | null {
    const m = val.match(/@([\w./\-]*)$/);
    return m ? m[1] : null;
  }

  function handleInputExtended() {
    showSuggestions = input.startsWith('/');
    const q = extractAtQuery(input);
    if (q !== null) {
      atQuery = q;
      if (atFetchTimer !== null) clearTimeout(atFetchTimer);
      atFetchTimer = setTimeout(async () => {
        try {
          const results = await api.listFiles(q);
          atSuggestions = results;
          atSuggestionIndex = 0;
        } catch { atSuggestions = []; }
      }, 200);
    } else {
      atSuggestions = [];
      atQuery = '';
    }
  }

  function acceptAtSuggestion(path: string) {
    input = input.replace(/@[\w./\-]*$/, `${path} `);
    atSuggestions = [];
    atQuery = '';
  }

  function handleInputKeydownExtended(e: KeyboardEvent) {
    if (atSuggestions.length > 0) {
      if (e.key === 'ArrowDown') { e.preventDefault(); atSuggestionIndex = (atSuggestionIndex + 1) % atSuggestions.length; return; }
      if (e.key === 'ArrowUp') { e.preventDefault(); atSuggestionIndex = (atSuggestionIndex - 1 + atSuggestions.length) % atSuggestions.length; return; }
      if (e.key === 'Tab' || e.key === 'Enter') {
        if (atSuggestions[atSuggestionIndex]) { e.preventDefault(); acceptAtSuggestion(atSuggestions[atSuggestionIndex]); return; }
      }
      if (e.key === 'Escape') { atSuggestions = []; return; }
    }
    if (showSuggestions && matchingCommands.length > 0) {
      if (e.key === 'ArrowDown') { e.preventDefault(); slashSuggestionIndex = (slashSuggestionIndex + 1) % matchingCommands.length; return; }
      if (e.key === 'ArrowUp') { e.preventDefault(); slashSuggestionIndex = (slashSuggestionIndex - 1 + matchingCommands.length) % matchingCommands.length; return; }
      if (e.key === 'Enter' && matchingCommands[slashSuggestionIndex]) { e.preventDefault(); selectCommand(matchingCommands[slashSuggestionIndex].name); return; }
      if (e.key === 'Escape') { showSuggestions = false; return; }
    }
    handleKeydown(e);
  }

  // --- Paste image ---
  function handlePaste(e: ClipboardEvent) {
    const items = e.clipboardData?.items;
    if (!items) return;
    for (const item of items) {
      if (item.type.startsWith('image/')) {
        e.preventDefault();
        const file = item.getAsFile();
        if (!file) continue;
        const reader = new FileReader();
        reader.onload = () => {
          const tag = `[image: ${file.name || 'clipboard'}]`;
          input = input ? `${input} ${tag}` : tag;
        };
        reader.readAsDataURL(file);
        return;
      }
    }
  }

  // --- Drag-drop file ---
  let dragOver = $state(false);

  function handleDragOver(e: DragEvent) { e.preventDefault(); dragOver = true; }
  function handleDragLeave() { dragOver = false; }

  function handleDrop(e: DragEvent) {
    e.preventDefault();
    dragOver = false;
    const files = e.dataTransfer?.files;
    if (!files?.length) return;
    for (const file of files) {
      if (file.type.startsWith('image/')) {
        const tag = `[image: ${file.name}]`;
        input = input ? `${input} ${tag}` : tag;
      } else if (file.size < 64 * 1024) {
        const reader = new FileReader();
        reader.onload = () => {
          const text = reader.result as string;
          const snippet = text.slice(0, 2000);
          const tag = `\`\`\`\n// ${file.name}\n${snippet}${text.length > 2000 ? '\n…(truncated)' : ''}\n\`\`\``;
          input = input ? `${input}\n${tag}` : tag;
        };
        reader.readAsText(file);
      }
    }
  }

  // --- Copy-code-block Svelte action ---
  // Attaches a "Copy" button to every <pre><code> block in the node's subtree.
  // Re-runs on DOM mutations so dynamically rendered markdown is covered.
  function codeBlockCopy(node: HTMLElement) {
    function attach() {
      node.querySelectorAll('pre').forEach(pre => {
        if (pre.querySelector('.la-copy-btn')) return; // already attached
        const btn = document.createElement('button');
        btn.className = 'la-copy-btn';
        btn.textContent = 'Copy';
        btn.style.cssText = [
          'position:absolute', 'top:4px', 'right:4px',
          'font-size:9px', 'padding:1px 6px',
          'background:rgba(255,215,0,0.08)', 'color:#FFD700',
          'border:1px solid rgba(255,215,0,0.2)', 'border-radius:3px',
          'cursor:pointer', 'transition:background 0.15s',
        ].join(';');
        btn.addEventListener('mouseenter', () => { btn.style.background = 'rgba(255,215,0,0.18)'; });
        btn.addEventListener('mouseleave', () => { btn.style.background = 'rgba(255,215,0,0.08)'; });
        btn.addEventListener('click', () => {
          const code = pre.querySelector('code')?.textContent ?? pre.textContent ?? '';
          navigator.clipboard.writeText(code).then(() => {
            btn.textContent = 'Copied!';
            setTimeout(() => { btn.textContent = 'Copy'; }, 1500);
          }).catch(() => {});
        });
        pre.style.position = 'relative';
        pre.appendChild(btn);
      });
    }

    attach();
    const observer = new MutationObserver(attach);
    observer.observe(node, { childList: true, subtree: true });
    return { destroy() { observer.disconnect(); } };
  }

  // Render composite oscilloscope from all sibling waves
  $effect(() => {
    const canvas = oscillatorEl;
    if (!canvas) return;
    const waveData = $waves;
    const dpr = window.devicePixelRatio || 1;
    const w = canvas.width;
    const h = canvas.height;
    const ctx = canvas.getContext('2d');
    if (!ctx) return;
    ctx.clearRect(0, 0, w, h);
    // Dim background
    ctx.fillStyle = 'rgba(0,0,0,0.3)';
    ctx.fillRect(0, 0, w, h);
    // Grid
    ctx.strokeStyle = 'rgba(255,255,255,0.03)';
    ctx.lineWidth = 0.5;
    for (let y = 0; y < h; y += 8 * dpr) {
      ctx.beginPath(); ctx.moveTo(0, y); ctx.lineTo(w, y); ctx.stroke();
    }
    // Blend each sibling wave
    const siblings = Object.keys(waveData) as string[];
    for (const sid of siblings) {
      const wave = waveData[sid];
      const samples = wave?.samples ?? [];
      if (samples.length === 0) continue;
      const color = (SIBLING_COLORS as Record<string, string>)[sid] ?? '#FFD700';
      const step = w / samples.length;
      ctx.beginPath();
      ctx.strokeStyle = color;
      ctx.lineWidth = 1;
      ctx.globalAlpha = 0.55;
      ctx.shadowColor = color;
      ctx.shadowBlur = 3;
      for (let i = 0; i < samples.length; i++) {
        const x = i * step;
        const y = h / 2 - samples[i] * (h * 0.38);
        if (i === 0) ctx.moveTo(x, y); else ctx.lineTo(x, y);
      }
      ctx.stroke();
    }
    ctx.globalAlpha = 1;
    ctx.shadowBlur = 0;
  });

  // Sync sharedBuildId from global store on mount
  $effect(() => {
    const id = $currentBuildId;
    if (id && !sharedBuildId) sharedBuildId = id;
  });

  // --- EVA voice playback ---
  async function playVoice(text: string): Promise<void> {
    const buildId = sharedBuildId;
    if (!buildId || !text.trim()) return;
    try {
      const resp = await fetch(`/api/builds/${buildId}/copilot/voice`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json', ...authHeaders() },
        body: JSON.stringify({ text: text.slice(0, 500) }),
      });
      if (!resp.ok) return;
      const blob = await resp.blob();
      const url = URL.createObjectURL(blob);
      if (audioEl) {
        audioEl.src = url;
        voicePlaying = true;
        audioEl.onended = () => { voicePlaying = false; URL.revokeObjectURL(url); };
        audioEl.onerror = () => { voicePlaying = false; URL.revokeObjectURL(url); };
        audioEl.play().catch(() => { voicePlaying = false; URL.revokeObjectURL(url); });
      } else {
        URL.revokeObjectURL(url);
      }
    } catch { voicePlaying = false; }
  }

  // --- Agent event handler — converts NDJSON bridge events into chat messages ---
  function handleAgentEvent(ev: AgentEvent): void {
    switch (ev.type) {
      case 'text':
        // If a text chunk arrives without loading=true (e.g. server-initiated
        // or E2E injection), set it so the UI shows the spinner and subsequent
        // chunks append rather than spawning duplicate bubbles.
        if (!get(copilotLoading)) copilotLoading.set(true);
        // Append to the current assistant message if one is in flight,
        // otherwise start a new one.
        copilotMessages.update((msgs) => {
          const updated = [...msgs];
          const last = updated[updated.length - 1];
          if (last && last.role === 'assistant' && get(copilotLoading)) {
            updated[updated.length - 1] = { ...last, content: last.content + ev.chunk };
          } else {
            updated.push({ id: crypto.randomUUID(), role: 'assistant', content: ev.chunk, timestamp: new Date().toISOString() });
          }
          return updated;
        });
        break;
      case 'thinking':
        addMessage('system', ev.content, undefined, 'thinking');
        break;
      case 'tool_start':
        addMessage('system', `[${ev.name}] ${JSON.stringify(ev.input)}`);
        break;
      case 'tool_complete':
        addMessage('system', `${ev.success ? '✅' : '❌'} [${ev.id}] ${ev.duration_ms}ms${ev.result ? '\n' + ev.result : ''}`);
        break;
      case 'status_update':
        addMessage('system', ev.text);
        break;
      case 'error':
        addMessage('system', `Error: ${ev.message}`);
        copilotLoading.set(false);
        break;
      case 'complete':
        copilotLoading.set(false);
        if (get(voiceEnabled)) {
          const msgs = get(copilotMessages);
          const last = msgs.findLast(m => m.role === 'assistant');
          if (last?.content) playVoice(last.content);
        }
        break;
      case 'token_usage':
        // Silently ignore — AgentConsole shows token stats if user wants detail
        break;
      case 'heartbeat':
        break;
      default:
        break;
    }
  }

  async function ensureBuild(): Promise<string> {
    if (sharedBuildId) return sharedBuildId;
    const existing = $currentBuildId;
    if (existing) { sharedBuildId = existing; return existing; }
    const profile = $authProfile;
    const body: Record<string, unknown> = { cwd };
    if (profile === 'ollama' && $ollamaConfig) {
      body.ollama_base_url = $ollamaConfig.baseUrl;
      body.ollama_model = $ollamaConfig.model;
      body.ollama_auth_token = $ollamaConfig.apiKey;
    }
    // If the webshell was launched with --resume-session (typically via
    // the /webshell plugin slash command from a running Claude Code or
    // Codex session), forward the UUID on this first build so the next
    // copilot turn invokes `claude --resume <id>` and continues the
    // terminal session's conversation. Consume-then-clear so a manual
    // second build doesn't accidentally re-resume the same thread.
    const resumeId = $pendingResumeSessionId;
    if (resumeId) {
      body.resume_session_id = resumeId;
      pendingResumeSessionId.set(null);
    }
    const resp = await api.createBuild(body) as {
      build_id: string;
      agent?: { kind: string; backend?: string };
    };
    sharedBuildId = resp.build_id;
    currentBuildId.set(resp.build_id);
    if (resp.agent?.kind) {
      buildAgentKind = resp.agent.kind;
    }
    return resp.build_id;
  }

  // ── Native interrupt / clear controls ─────────────────────────────────────
  /** True while an interrupt POST is in-flight (shows "Interrupting…" pill). */
  let interrupting = $state(false);

  /** Auto-clear interrupting flag once the turn finishes. */
  $effect(() => { if (!$copilotLoading) interrupting = false; });

  /**
   * Signal the server to stop the current native turn.
   * Idempotent — safe to call when nothing is running.
   */
  async function sendInterrupt() {
    const buildId = sharedBuildId;
    if (!buildId) return;
    interrupting = true;
    try {
      await fetch(`/api/builds/${buildId}/copilot/interrupt`, {
        method: 'POST',
        headers: authHeaders(),
      });
    } catch { /* best-effort; turn will time out naturally */ }
  }

  /**
   * Clear the conversation: wipe local history and delete the server-side
   * helix session file so the next turn starts from a blank context.
   */
  async function sendClear() {
    clearCopilotHistory();
    searchQuery = '';
    showSearch = false;
    const buildId = sharedBuildId;
    if (!buildId) return;
    try {
      await fetch(`/api/builds/${buildId}/copilot/clear`, {
        method: 'POST',
        headers: authHeaders(),
      });
    } catch { /* best-effort */ }
  }

  // Fork-to-terminal state — enabled once at least one chat turn has
  // landed (server-side session_id is populated after turn 1). The button
  // sits in the chat-mode header; clicking POSTs /api/session/fork which
  // (on macOS) spawns a Terminal.app window running `claude --resume <id>`
  // or `codex exec resume <id>`. On other platforms the response carries
  // the command string for manual copy-paste.
  let forking = $state(false);
  let forkResult = $state<{ launched: boolean; command: string; platform: string } | null>(null);
  let forkError = $state<string | null>(null);
  let canFork = $derived(
    Boolean(sharedBuildId) &&
    $copilotMessages.some(m => m.role === 'assistant' || m.role === 'user'),
  );

  async function forkToTerminal() {
    if (!sharedBuildId || forking) return;
    forking = true;
    forkError = null;
    forkResult = null;
    try {
      const resp = await api.forkSession(sharedBuildId);
      forkResult = { launched: resp.launched, command: resp.command, platform: resp.platform };
      // Auto-dismiss the banner after 8s on successful launch.
      if (resp.launched) {
        setTimeout(() => { forkResult = null; }, 8000);
      }
    } catch (err) {
      forkError = err instanceof Error ? err.message : 'Failed to fork session';
    } finally {
      forking = false;
    }
  }

  function dismissForkResult() { forkResult = null; forkError = null; }

  async function copyForkCommand() {
    if (!forkResult) return;
    try { await navigator.clipboard.writeText(forkResult.command); } catch { /* ignore */ }
  }

  // Shared SSE subscription tied to session
  $effect(() => {
    if (!sharedBuildId) return;
    connectSSE(sharedBuildId);
    return () => disconnectSSE();
  });

  // xterm.js lifecycle — connects immediately when terminal mode opens.
  // Uses build-bound PTY if a build exists, otherwise standalone PTY
  // (inherits server CWD from the parent coding session).
  $effect(() => {
    if (mode !== 'terminal' || !terminalEl || !open) return;

    const term = new Terminal({
      cursorBlink: true,
      fontSize: 13,
      fontFamily: 'monospace',
      theme: { background: '#0a0a0a', foreground: '#e2e8f0', cursor: '#FFD700', selectionBackground: '#FFD70044' },
    });
    const fitAddon = new FitAddon();
    term.loadAddon(fitAddon);
    term.open(terminalEl);
    fitAddon.fit();

    const ws = new TerminalWS(
      sharedBuildId,
      (data) => term.write(data),
      () => { terminalConnected.set(true); ws.sendResize(term.cols, term.rows); },
      () => { terminalConnected.set(false); },
    );
    ws.connect();
    term.onData(data => ws.sendText(data));

    const ro = new ResizeObserver(() => { fitAddon.fit(); ws.sendResize(term.cols, term.rows); });
    ro.observe(terminalEl);

    return () => { ws.disconnect(); term.dispose(); ro.disconnect(); terminalConnected.set(false); };
  });

  // --- Context ---
  let contextString = $derived(
    buildBuildContext(
      $activeBuild, $selectedPillar,
      $activeBuild ? $findings.filter(f => f.buildId === $activeBuild!.id) : [],
    )
  );

  let matchingCommands = $derived(
    input.startsWith('/')
      ? SLASH_COMMANDS.filter(c =>
          c.name.startsWith(input.slice(1).toLowerCase()) ||
          c.alias?.some(a => a.startsWith(input.slice(1).toLowerCase()))
        ).slice(0, 8)
      : []
  );

  // Reset index when the match list changes (typing narrows/widens candidates)
  $effect(() => {
    matchingCommands; // track
    slashSuggestionIndex = 0;
  });

  // --- Rich context string shown in drawer header ---
  let contextBadge = $derived(() => {
    const b = $activeBuild;
    if (b) return `${b.name.slice(0, 24)} · ${b.metaSkill}`;
    const bCount = $builds.length;
    return bCount > 0 ? `${bCount} build${bCount > 1 ? 's' : ''}` : 'no active build';
  });

  // --- Messages ---
  let thinkingOpen = $state<Record<string, boolean>>({});

  function addMessage(role: CopilotMessage['role'], content: string, sibling?: SiblingId, kind?: CopilotMessage['kind']) {
    const msg: CopilotMessage = { id: crypto.randomUUID(), role, content, sibling, timestamp: new Date().toISOString(), kind };
    copilotMessages.update(m => [...m, msg]);
    return msg;
  }

  function mockStream(content: string, sibling?: SiblingId) {
    const msg: CopilotMessage = { id: crypto.randomUUID(), role: 'assistant', content: '', sibling, timestamp: new Date().toISOString() };
    copilotMessages.update(m => [...m, msg]);
    copilotLoading.set(true);
    let i = 0;
    const iv = setInterval(() => {
      if (i < content.length) {
        const chunk = content.slice(0, ++i);
        copilotMessages.update(msgs => {
          const updated = [...msgs];
          const last = updated[updated.length - 1];
          if (last?.role === 'assistant') updated[updated.length - 1] = { ...last, content: chunk };
          return updated;
        });
      } else { clearInterval(iv); copilotLoading.set(false); }
    }, 15);
  }

  async function sendMessage() {
    const text = input.trim();
    if (!text || get(copilotLoading)) return;
    input = '';
    showSuggestions = false;
    const { command, args } = parseCommand(text);

    // Control commands (clear, focus, navigate, etc.) execute locally — no copilot turn.
    if (command && ['clear', 'focus', 'navigate', 'notify', 'terminal', 'settings', 'theme', 'panel'].includes(command.name)) {
      addMessage('system', `/${command.name} ${args}`.trim());
      if (command.name === 'clear') { void sendClear(); return; }
      try { await command.execute(args); }
      catch (err) { addMessage('system', `Error: ${err instanceof Error ? err.message : 'Unknown error'}`); }
      return;
    }

    // Everything else (including meta-skill QUICK actions like /build, /secure, /research)
    // routes through the copilot subprocess for real agent execution.
    addMessage('user', text);
    copilotLoading.set(true);
    let buildId: string | null = null;
    try { buildId = await ensureBuild(); }
    catch { mockStream('Could not create build session. Is the webshell running?'); copilotLoading.set(false); return; }

    // Native agent path (Phase-10 Phase 4): streaming SSE via HTTP POST.
    // Replaces the WebSocket bridge (which spawned a sub-process that exited
    // immediately, leaving the drawer with no response).  Each SSE frame is
    // routed through handleAgentEvent — the same handler the WS path used —
    // so chat UI logic stays unchanged.
    const isNative = buildAgentKind === 'lightarchitects_native' || get(isNativeAgent);
    if (isNative) {
      try {
        const ctx = snapshotContextForCopilot();
        const { grounding } = await api.copilotChatNative(
          buildId!,
          text,
          (ev) => handleAgentEvent(ev as AgentEvent),
          { recentEvents: ctx.recentEvents, uiContext: ctx.uiContext },
        );
        if (grounding !== null) copilotGrounding.set(grounding);
      } catch (err) {
        const detail = err instanceof Error ? err.message : 'Unknown error';
        handleAgentEvent({ type: 'error', message: `Native SSE failed: ${detail}` } as AgentEvent);
      }
      return;
    }

    // Non-native WS bridge path (Claude CLI / Codex CLI agent kinds today).
    if (agentWs) {
      if (agentWs.connected) {
        agentWs.sendMessage(text);
      } else {
        addMessage('system', 'Agent bridge connecting — please resend shortly.');
      }
      return;
    }

    // Fallback: legacy HTTP POST (non-native builds, Ollama, Anthropic CLI modes)
    try {
      const ctx = snapshotContextForCopilot();
      const { response: result, grounding } = await api.copilotChat(
        buildId!,
        `[Context]\n${contextString}\n\n[User]\n${text}`,
        { recentEvents: ctx.recentEvents, uiContext: ctx.uiContext },
      );
      if (grounding !== null) copilotGrounding.set(grounding);
      const response = typeof result === 'object' && result !== null && 'response' in result
        ? String((result as Record<string, unknown>).response)
        : 'No response from provider.';
      // SSE delivers copilot_response chunks via the broadcast channel.
      // The HTTP response text is intentionally discarded — the frontend
      // receives real streaming output via WebEvent::CopilotResponse SSE events.
      void response;
    } catch { mockStream('Could not reach AI provider. Check webshell logs.'); }
  }

  function selectCommand(name: string) { input = `/${name} `; showSuggestions = false; }
  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); sendMessage(); }
    if (e.key === 'Escape') {
      // Priority: interrupt running turn > close suggestions > clear input > close drawer
      if ($copilotLoading) { void sendInterrupt(); return; }
      showSuggestions = false;
      if (!input) open = false;
    }
  }
  function handleInput() {
    const isSlash = input.startsWith('/');
    showSuggestions = isSlash;
    if (isSlash && input.length === 1) {
      import('$lib/tutorial').then(m => m.runTutorial('t4'));
    }
  }

  function handleDispatch(sibling: SiblingId, prompt?: string) {
    focusedSibling.set(sibling);
    spikeSibling(sibling);
    addMessage('system', prompt ? `Dispatching ${sibling.toUpperCase()} with: "${prompt}"` : `Dispatching ${sibling.toUpperCase()}`);
    const buildId = $currentBuildId;
    if (!buildId) {
      addMessage('system', `No active build — cannot dispatch to ${sibling.toUpperCase()}.`);
      return;
    }
    api.dispatchSibling(buildId, sibling, sibling, prompt ?? '')
      .then((result) => {
        if (result.response) {
          addMessage('assistant', result.response, sibling);
        } else {
          addMessage('system', `${sibling.toUpperCase()} returned no output.`);
        }
      })
      .catch((err) => {
        addMessage('system', `${sibling.toUpperCase()} dispatch failed: ${err?.message ?? 'unknown error'}`);
      });
  }

  // Auto-scroll
  $effect(() => { $copilotMessages; if (messagesEl) requestAnimationFrame(() => { if (messagesEl) messagesEl.scrollTop = messagesEl.scrollHeight; }); });

  // --- Drag to resize (right edge — horizontal) ---
  let dragging = false;
  let dragStartX = 0;
  let dragStartW = 0;

  function onDragStart(e: MouseEvent) {
    if (!open) return;
    dragging = true;
    dragStartX = e.clientX;
    dragStartW = widthPx;
    e.preventDefault();
  }

  function onDragMove(e: MouseEvent) {
    if (!dragging || !open) return;
    const delta = e.clientX - dragStartX;
    const maxW = Math.floor(window.innerWidth * MAX_WIDTH_RATIO);
    widthPx = Math.min(maxW, Math.max(MIN_WIDTH, dragStartW + delta));
  }

  function onDragEnd() { dragging = false; saveSettingsDebounced(); }

  function onSeparatorKeydown(e: KeyboardEvent) {
    const step = 20;
    const maxW = Math.floor(window.innerWidth * MAX_WIDTH_RATIO);
    if (e.key === 'ArrowRight') { e.preventDefault(); widthPx = Math.min(maxW, widthPx + step); }
    else if (e.key === 'ArrowLeft') { e.preventDefault(); widthPx = Math.max(MIN_WIDTH, widthPx - step); }
  }

  // ── Model picker (⌘⇧M) ─────────────────────────────────────────────────────
  let showModelPicker = $state(false);
  let modelPickerModels = $state<Array<{ id: string; label: string; tier: string }>>([]);
  let modelPickerLoading = $state(false);

  async function openModelPicker() {
    if (modelPickerModels.length === 0) {
      modelPickerLoading = true;
      try {
        const backend = get(persistedConfig)?.backend ?? 'anthropic';
        const url = new URL('/api/setup/models', window.location.origin);
        url.searchParams.set('backend', backend);
        const resp = await fetch(url.toString());
        if (resp.ok) {
          const data = await resp.json();
          modelPickerModels = data.models ?? [];
        }
      } catch {
        // silently ignore — picker stays empty
      } finally {
        modelPickerLoading = false;
      }
    }
    showModelPicker = true;
  }

  function selectPickerModel(id: string) {
    selectedModel.set(id);
    showModelPicker = false;
  }

  // Global keyboard shortcuts
  function onGlobalKeydown(e: KeyboardEvent) {
    if (e.key === '`' && (e.ctrlKey || e.metaKey)) {
      e.preventDefault();
      open = !open;
      if (!open) showSuggestions = false;
    }
    // ⌘⇧M — model picker
    if (e.key === 'M' && e.shiftKey && (e.ctrlKey || e.metaKey)) {
      e.preventDefault();
      if (showModelPicker) { showModelPicker = false; } else { openModelPicker(); }
    }
    // Ctrl+F inside the open drawer toggles history search
    if (e.key === 'f' && (e.ctrlKey || e.metaKey) && open && mode === 'chat') {
      e.preventDefault();
      showSearch = !showSearch;
      if (!showSearch) searchQuery = '';
    }
  }

  // Custom event bridge — other screens dispatch `la:open-copilot` (open/toggle)
  // or `la:toggle-copilot` (Ctrl+` hotkey in app.svelte) to drive this drawer
  // without importing its local state.
  $effect(() => {
    const handler = () => { open = !open; if (!open) showSuggestions = false; };
    window.addEventListener('la:open-copilot', handler);
    window.addEventListener('la:toggle-copilot', handler);
    return () => {
      window.removeEventListener('la:open-copilot', handler);
      window.removeEventListener('la:toggle-copilot', handler);
    };
  });

  // Polytope radial menu bridge — PolytopeButton fan actions route through
  // window events so the button doesn't need to import this drawer's internals.
  $effect(() => {
    const forkHandler  = () => { if (!forking) void forkToTerminal(); };
    const searchHandler = () => { showSearch = !showSearch; if (!showSearch) searchQuery = ''; };
    const clearHandler  = () => { void sendClear(); };
    const posHandler    = () => { positionMode = positionMode === 'drawer' ? 'overlay' : 'drawer'; };

    window.addEventListener('la:copilot-fork',     forkHandler);
    window.addEventListener('la:copilot-search',   searchHandler);
    window.addEventListener('la:copilot-clear',    clearHandler);
    window.addEventListener('la:copilot-position', posHandler);
    return () => {
      window.removeEventListener('la:copilot-fork',     forkHandler);
      window.removeEventListener('la:copilot-search',   searchHandler);
      window.removeEventListener('la:copilot-clear',    clearHandler);
      window.removeEventListener('la:copilot-position', posHandler);
    };
  });

  // ── E2E injection bridge ────────────────────────────────────────────────────
  // Only active in dev/test. Allows Playwright tests to inject synthetic
  // AgentEvents without a real WebSocket connection.
  $effect(() => {
    if (import.meta.env.PROD) return;

    const injectHandler = (e: Event) => {
      const detail = (e as CustomEvent).detail as { events?: import('$lib/types').AgentEvent[] } | undefined;
      if (!detail?.events) return;
      for (const ev of detail.events) {
        handleAgentEvent(ev);
      }
    };
    window.addEventListener('la:e2e-inject-agent-events', injectHandler);

    const rawHandler = (e: Event) => {
      const detail = (e as CustomEvent).detail as { raw?: string } | undefined;
      if (!detail?.raw) return;
      // Simulate AgentWS.onmessage parsing the raw string
      try {
        const parsed = JSON.parse(detail.raw) as Record<string, unknown>;
        if (parsed.type === 'text' && typeof parsed.chunk === 'string') {
          handleAgentEvent(parsed as import('$lib/types').AgentEvent);
        } else {
          handleAgentEvent({ type: 'error', message: `Malformed event: ${String(parsed.type)}` });
        }
      } catch {
        handleAgentEvent({ type: 'text', chunk: detail.raw });
      }
    };
    window.addEventListener('la:e2e-inject-raw-ws', rawHandler);

    const disconnectHandler = () => {
      // Simulate the onClose callback path
      if (get(copilotLoading)) {
        copilotLoading.set(false);
        addMessage('system', 'Agent connection lost. Reconnecting…');
      }
    };
    window.addEventListener('la:e2e-simulate-ws-disconnect', disconnectHandler);

    return () => {
      window.removeEventListener('la:e2e-inject-agent-events', injectHandler);
      window.removeEventListener('la:e2e-inject-raw-ws', rawHandler);
      window.removeEventListener('la:e2e-simulate-ws-disconnect', disconnectHandler);
    };
  });
</script>

<svelte:window
  onmousemove={onDragMove}
  onmouseup={onDragEnd}
  onkeydown={onGlobalKeydown}
  onresize={onWindowResize}
/>

<!-- Sidebar container — left panel, full height below header -->
<div
  data-testid="copilot-drawer"
  data-card-role="copilot-drawer"
  class="fixed top-[56px] left-0 bottom-0 z-30 flex flex-row"
  style="width: {open ? widthPx + 'px' : '0px'}; overflow: hidden; transition: width 0.18s ease; border-right: 1px solid var(--la-drawer-border);"
>
  <!-- Sidebar body (flex-col, full width) -->
  <div class="flex flex-col flex-1 min-w-0 overflow-hidden">

  <!-- Top gradient stripe -->
  <div class="h-px shrink-0 w-full" style="background: linear-gradient(90deg, transparent, rgba(255,215,0,0.3) 40%, rgba(255,20,147,0.15) 80%, transparent);"></div>

  <!-- ── Header — 3-zone geometric strip ──────────────────────────────────
       Left:   mode tabs (CHAT / TERM) — underline-active, no border wrapper
       Center: identity + preset chip + micro-stats
       Right:  icon-only action column cells
       ──────────────────────────────────────────────────────────────────── -->
  <div class="copilot-header" aria-label="Copilot controls">

    <!-- ZONE A — Mode tabs (left column cells) -->
    <button
      onclick={() => { mode = 'chat'; if (!open) open = true; }}
      class="hdr-tab {mode === 'chat' && open ? 'hdr-tab--on' : ''}"
      title="Chat mode"
      aria-label="Chat mode"
    >
      <span class="hdr-tab-icon" aria-hidden="true">⌨</span>
      <span>CHAT</span>
    </button>
    <button
      onclick={() => { mode = 'terminal'; if (!open) open = true; }}
      class="hdr-tab {mode === 'terminal' && open ? 'hdr-tab--on' : ''}"
      title="Terminal (PTY)"
      aria-label="Terminal mode"
    >
      <span class="hdr-tab-icon" aria-hidden="true">&gt;_</span>
      <span>TERM</span>
    </button>

    <!-- Spacer — pushes ghost actions and collapse to the right -->
    <div class="hdr-spacer"></div>

    <!-- Ghost actions — invisible at rest, reveal on header hover (CSS only, no JS) -->
    {#if open}
      <div class="hdr-actions" role="toolbar" aria-label="Copilot actions">

        <!-- Preset chip — leftmost in ghost group -->
        <button
          onclick={() => quickPickOpen.set(true)}
          class="hdr-preset"
          title="Active preset — click to change"
        >{PRESET_DISPLAY[$selectedPreset]}</button>

        <!-- SSE warn (persistent when disconnected) -->
        {#if !$sseConnected && $currentBuildId}
          <button
            onclick={() => reconnectSSE($currentBuildId!)}
            class="hdr-action hdr-action--warn"
            title="SSE disconnected — reconnect"
            aria-label="Reconnect SSE"
          >⟳</button>
        {/if}

        {#if mode === 'chat'}
          {#if $copilotLoading}
            <button
              onclick={() => void sendInterrupt()}
              class="hdr-action hdr-action--warn"
              title="Stop generation (Esc)"
              aria-label="Stop generation"
            >■</button>
          {/if}
          <button
            onclick={forkToTerminal}
            disabled={!canFork || forking}
            class="hdr-action {canFork && !forking ? 'hdr-action--gold' : 'hdr-action--disabled'}"
            title="{canFork ? 'Fork to terminal (claude --resume)' : 'Send a message first'}"
            aria-label="Fork to terminal"
          >⎋</button>
          <button
            onclick={() => { showSearch = !showSearch; if (!showSearch) searchQuery = ''; }}
            class="hdr-action {showSearch ? 'hdr-action--on' : ''}"
            title="Search history (Ctrl+F)"
            aria-label="Search history"
          >⌕</button>
          <button
            onclick={() => void sendClear()}
            class="hdr-action"
            title="Clear history + server memory"
            aria-label="Clear chat history"
          >✕</button>
        {/if}

        {#if $ayinStatus === 'connected'}
          <button
            onclick={() => navigate('/observability')}
            class="hdr-action hdr-action--ayin"
            title="View session spans in AYIN Lineage Circuit"
            aria-label="View in AYIN"
          >AYIN →</button>
        {/if}

        <button
          onclick={() => { positionMode = positionMode === 'drawer' ? 'overlay' : 'drawer'; }}
          class="hdr-action {positionMode === 'overlay' ? 'hdr-action--on' : ''}"
          title="Toggle overlay mode"
          aria-label="Toggle position mode"
        >{positionMode === 'overlay' ? '⊠' : '⊡'}</button>

        <div class="hdr-action-wrap" style="position: relative;">
          <button
            onclick={() => settingsOpen.update(v => !v)}
            class="hdr-action {$settingsOpen ? 'hdr-action--on' : ''}"
            title="Settings"
            aria-label="Settings"
          >⚙</button>
          {#if $settingsOpen}
            <div class="absolute bottom-full right-0 mb-1">
              <SettingsOverlay />
            </div>
          {/if}
        </div>

      </div>
    {/if}

    <!-- Collapse / expand — always visible, never ghosted -->
    <button
      onclick={() => { open = !open; }}
      class="hdr-action hdr-collapse"
      title="{open ? 'Collapse (Ctrl+`)' : 'Open (Ctrl+`)'}"
      aria-label="{open ? 'Collapse' : 'Expand'} copilot"
    >{open ? '◂' : '▸'}</button>
  </div>

  <!-- ── BODY (only when open) ───────────────────────────────── -->
  {#if open}
    <div class="flex-1 flex overflow-hidden bg-[var(--la-bg-frame)] min-h-0">

      <!-- ── TERMINAL MODE — pure PTY, opens immediately in inherited CWD ── -->
      {#if mode === 'terminal'}
        <div class="flex-1 flex flex-col overflow-hidden min-h-0">
          <div class="flex items-center gap-2 px-4 py-1.5 border-b border-[var(--la-drawer-border)] bg-[var(--la-bg-frame)] shrink-0">
            <div class="w-1.5 h-1.5 rounded-full {$terminalConnected ? 'bg-green-500' : 'bg-[var(--la-text-dim)]'}" style="box-shadow: 0 0 4px {$terminalConnected ? '#22c55e' : 'transparent'}"></div>
            <span class="text-[9px] text-[var(--la-text-dim)] font-mono">{$terminalConnected ? 'PTY live' : 'connecting…'} · {cwd}</span>
            {#if sharedBuildId}
              <span class="text-[9px] text-[var(--la-text-dim)] font-mono">· build {sharedBuildId.slice(0, 8)}…</span>
            {/if}
          </div>
          {#if !$terminalConnected}
            <div class="flex-1 flex items-center justify-center bg-[var(--la-bg-void)]">
              <div class="flex items-center gap-3">
                <div class="w-3 h-3 border-2 border-[var(--la-focus-ring)] border-t-transparent rounded-full animate-spin"></div>
                <span class="text-[11px] text-[var(--la-text-dim)] font-mono">Connecting to PTY at {cwd}…</span>
              </div>
            </div>
          {/if}
          <div bind:this={terminalEl} class="overflow-hidden bg-[var(--la-bg-void)] min-h-0 {$terminalConnected ? 'flex-1' : 'h-0'}" style="font-family: monospace; contain: strict;"></div>
        </div>

      <!-- ── CHAT MODE ── -->
      {:else}
        <div class="flex-1 flex overflow-hidden">
          <!-- Messages + input -->
          <div class="flex-1 flex flex-col overflow-hidden">
            <ContextBar />
            {#if forkError}
              <div class="px-3 py-1.5 border-b border-red-500/40 bg-red-500/10 flex items-center gap-2 shrink-0">
                <span class="text-[10px] text-red-300">Fork failed: {forkError}</span>
                <div class="flex-1"></div>
                <button onclick={dismissForkResult} class="text-[10px] text-red-300/70 hover:text-red-200">✕</button>
              </div>
            {:else if forkResult}
              {#if forkResult.launched}
                <div class="px-3 py-1.5 border-b border-[var(--la-focus-ring)]/30 bg-[var(--la-focus-ring)]/5 flex items-center gap-2 shrink-0">
                  <span class="text-[10px] text-[var(--la-agent-testing)]">Opened in Terminal ({forkResult.platform}). Conversation continues in both places — same session.</span>
                  <div class="flex-1"></div>
                  <button onclick={dismissForkResult} class="text-[10px] text-[var(--la-agent-testing)]/70 hover:text-[var(--la-agent-testing)]">✕</button>
                </div>
              {:else}
                <div class="px-3 py-1.5 border-b border-[var(--la-agent-performance)]/40 bg-[var(--la-agent-performance)]/10 flex items-start gap-2 shrink-0">
                  <div class="flex-1">
                    <div class="text-[10px] text-[var(--la-agent-performance)] mb-1">
                      No native terminal launcher on <span class="font-mono">{forkResult.platform}</span> yet — run this in your terminal:
                    </div>
                    <code class="text-[10px] text-[var(--la-text-bright)] bg-[var(--la-bg-void)] px-2 py-0.5 rounded border border-[var(--la-drawer-border)] font-mono select-all">{forkResult.command}</code>
                  </div>
                  <button onclick={copyForkCommand} class="text-[10px] text-[var(--la-agent-performance)] hover:text-[var(--la-agent-quality)] px-1.5 py-0.5 rounded border border-[var(--la-agent-performance)]/40">Copy</button>
                  <button onclick={dismissForkResult} class="text-[10px] text-[var(--la-agent-performance)]/70 hover:text-[var(--la-agent-performance)]">✕</button>
                </div>
              {/if}
            {/if}
            {#if showSearch}
              <div class="flex items-center gap-2 px-3 py-1.5 border-b border-[var(--la-drawer-border)] bg-[var(--la-bg-frame)] shrink-0">
                <span class="text-[10px] text-[var(--la-text-dim)]">⌕</span>
                <!-- svelte-ignore a11y_autofocus -->
                <input
                  type="text"
                  bind:value={searchQuery}
                  autofocus
                  placeholder="Search history…"
                  class="flex-1 bg-transparent text-xs text-[var(--la-text-bright)] placeholder:text-[var(--la-hair-strong)] outline-none font-mono"
                  onkeydown={(e) => { if (e.key === 'Escape') { showSearch = false; searchQuery = ''; } }}
                />
                {#if searchQuery}
                  <span class="text-[9px] text-[var(--la-text-dim)]">
                    {filteredMessages.length}/{$copilotMessages.length}
                  </span>
                {/if}
                <button
                  onclick={() => { showSearch = false; searchQuery = ''; }}
                  class="text-[9px] text-[var(--la-text-dim)] hover:text-[var(--la-text-bright)]"
                  aria-label="Close search"
                >✕</button>
              </div>
            {/if}
            {#if $strategyHitl}
              <div class="px-3 pt-2 shrink-0">
                <StrategyPhaseRibbon
                  requestId={$strategyHitl.requestId}
                  question={$strategyHitl.question}
                  header={$strategyHitl.header}
                  options={$strategyHitl.options}
                  buildId={$strategyHitl.buildId || sharedBuildId || ''}
                  sessionId={$strategyHitl.sessionId}
                  onResolved={() => strategyHitl.set(null)}
                />
              </div>
            {/if}
            <div
              bind:this={messagesEl}
              class="flex-1 overflow-y-auto p-3 space-y-2 transition-colors {dragOver ? 'bg-[var(--la-focus-ring)]/5 ring-1 ring-inset ring-[var(--la-focus-ring)]/20' : ''}"
              role="log"
              aria-label="Chat messages"
              aria-live="polite"
              ondragover={handleDragOver}
              ondragleave={handleDragLeave}
              ondrop={handleDrop}
              use:codeBlockCopy
            >
              {#if $copilotMessages.length === 0}
                <div class="flex flex-col items-center justify-center h-full text-[var(--la-text-dim)] gap-2">
                  <p class="text-xs">Start a conversation · Use <kbd class="bg-[var(--la-drawer-border)] px-1 rounded">/</kbd> for slash commands</p>
                  <div class="flex flex-wrap gap-1.5 justify-center">
                    {#each ['/build', '/secure', '/research', '/deploy', '/quality', '/clear'] as cmd}
                      <button onclick={() => { input = cmd + ' '; }}
                        class="text-[10px] px-2 py-1 rounded bg-[var(--la-bg-elev-1)] border border-[var(--la-drawer-border)] hover:border-[var(--la-hair-strong)] transition-colors">{cmd}</button>
                    {/each}
                  </div>
                </div>
              {:else if filteredMessages.length === 0}
                <div class="flex flex-col items-center justify-center h-full text-[var(--la-text-dim)] gap-1">
                  <p class="text-xs">No messages match "<span class="text-[var(--la-text-dim)] font-mono">{searchQuery}</span>"</p>
                </div>
              {:else}
                {#each filteredMessages as msg (msg.id)}
                  {#if msg.kind === 'thinking'}
                    <!-- Collapsible thinking block — full-width, no bubble chrome -->
                    <div class="thinking-wrap">
                      <button
                        class="thinking-toggle"
                        onclick={() => { thinkingOpen[msg.id] = !thinkingOpen[msg.id]; }}
                        aria-expanded={thinkingOpen[msg.id] ?? false}
                      >
                        <span class="thinking-chevron">{thinkingOpen[msg.id] ? '▾' : '▸'}</span>
                        <span class="thinking-label">Thinking</span>
                        <span class="thinking-chars">{msg.content.length.toLocaleString()} chars</span>
                      </button>
                      {#if thinkingOpen[msg.id]}
                        <div class="thinking-body">
                          <span class="chat-md-content">{@html renderMarkdown(msg.content)}</span>
                        </div>
                      {/if}
                    </div>
                  {:else}
                    <div class="flex {msg.role === 'user' ? 'justify-end' : msg.role === 'system' ? 'justify-center' : 'justify-start'}">
                      <div class="max-w-[80%] px-3 py-1.5 rounded-lg text-xs chat-bubble
                        {msg.role === 'user' ? 'bg-[var(--la-focus-ring)]/90 text-[var(--la-bg-frame)]' :
                         msg.role === 'system' ? 'bg-[var(--la-drawer-border)]/50 text-[var(--la-text-dim)] border border-[var(--la-drawer-border)]' :
                         'bg-[var(--la-bg-elev-1)] border border-[var(--la-drawer-border)] text-[var(--la-text-bright)]'}">
                        {#if msg.sibling}
                          <SiblingBadge sibling={msg.sibling} size="sm" />
                          <span class="text-[var(--la-text-dim)] mx-1">·</span>
                        {/if}
                        {#if msg.role === 'user'}
                          <span class="chat-user-content">{msg.content}</span>
                        {:else}
                          <span class="chat-md-content">{@html renderMarkdown(msg.content)}</span>
                          {#if msg.role === 'assistant'}
                            {@const chips = parseChips(msg.content)}
                            {#if chips.length > 0}
                              <div class="flex flex-wrap gap-1 mt-1.5">
                                {#each chips as chip (chip.id)}
                                  <button
                                    onclick={chip.action}
                                    class="text-[8px] px-1.5 py-0.5 border border-[var(--la-struct-primary)]/40 text-[var(--la-struct-primary)] hover:bg-[var(--la-struct-primary)]/10 transition-colors font-mono"
                                  >{chip.label}</button>
                                {/each}
                              </div>
                            {/if}
                            {#if msg.turn_span_id}
                              <TurnLineageStrip turnSpanId={msg.turn_span_id} />
                            {/if}
                          {/if}
                        {/if}
                      </div>
                    </div>
                  {/if}
                {/each}
                {#if $copilotLoading}
                  <div class="flex justify-start items-center gap-2">
                    <div class="bg-[var(--la-bg-elev-1)] border border-[var(--la-drawer-border)] px-3 py-1.5 rounded-lg flex items-center gap-2">
                      {#if interrupting}
                        <span class="text-[var(--la-semantic-warn,#f59e0b)] text-xs animate-pulse">Interrupting…</span>
                      {:else}
                        <span class="text-[var(--la-text-dim)] text-xs animate-pulse">Thinking…</span>
                      {/if}
                    </div>
                    <button
                      onclick={() => void sendInterrupt()}
                      title="Stop (Esc)"
                      aria-label="Stop generation"
                      class="text-[9px] px-2 py-1 rounded border border-[var(--la-drawer-border)] text-[var(--la-text-dim)] hover:border-[var(--la-semantic-warn,#f59e0b)] hover:text-[var(--la-semantic-warn,#f59e0b)] transition-colors font-mono"
                    >■ stop</button>
                  </div>
                {/if}
              {/if}
            </div>

            <!-- AYIN Traces Panel (Gap 4) — collapsible; mounts panel on open to avoid orphan SSE connection -->
            <div class="border-t border-[var(--la-drawer-border)] shrink-0">
              <button
                class="w-full flex items-center gap-1.5 px-3 py-1 text-[9px] font-mono text-[var(--la-text-dim)] hover:text-[rgba(255,215,0,0.7)] transition-colors"
                onclick={() => { tracesOpen = !tracesOpen; }}
                data-testid="traces-toggle"
                aria-expanded={tracesOpen}
              >
                <span class="text-[rgba(255,215,0,0.5)]">{tracesOpen ? '▾' : '▸'}</span>
                <span>AYIN Lineage Circuit</span>
                {#if tracesOpen}
                  <span class="ml-auto text-[rgba(255,215,0,0.4)]">● live</span>
                {/if}
              </button>
              {#if tracesOpen}
                <div class="h-48 overflow-hidden">
                  <AyinTracesPanel />
                </div>
              {/if}
            </div>

            <!-- Input -->
            <div class="border-t border-[var(--la-drawer-border)] px-3 py-2 relative shrink-0" data-onboarding="copilot-input">
              {#if showSuggestions && matchingCommands.length > 0}
                <div class="absolute bottom-full left-3 right-3 mb-1 bg-[var(--la-bg-void)] border border-[var(--la-drawer-border)] rounded-lg overflow-hidden shadow-xl z-10 max-h-48 overflow-y-auto">
                  {#each matchingCommands as cmd, i}
                    <button
                      class="w-full text-left px-3 py-1.5 text-xs flex items-baseline gap-2 transition-colors
                             {i === slashSuggestionIndex ? 'bg-[var(--la-drawer-border)] text-[var(--la-text-bright)]' : 'hover:bg-[var(--la-drawer-border)]/60'}"
                      onclick={() => selectCommand(cmd.name)}
                    >
                      <span class="text-[var(--la-focus-ring)] font-mono">/{cmd.name}</span>
                      <span class="text-[var(--la-text-dim)] flex-1">{cmd.description}</span>
                      {#if cmd.args}<span class="text-[var(--la-hair-strong)]">{cmd.args}</span>{/if}
                    </button>
                  {/each}
                  <div class="px-3 py-1 border-t border-[var(--la-drawer-border)] flex items-center gap-3 text-[9px] text-[var(--la-hair-strong)] select-none">
                    <span>navigate</span>
                    <span>↵ select</span>
                    <span>Esc dismiss</span>
                  </div>
                </div>
              {/if}
              <!-- Hint bar for current command -->
              {#if input.startsWith('/') && matchingCommands.length > 0}
                {@const hint = matchingCommands[0]}
                <div class="text-[9px] text-[var(--la-text-dim)] mb-1 flex items-center gap-2">
                  <span class="text-[var(--la-focus-ring)]">/{hint.name}</span>
                  <span>{hint.description}</span>
                  {#if hint.args}<span class="text-[var(--la-hair-strong)]">{hint.args}</span>{/if}
                </div>
              {/if}
              <!-- Composite oscilloscope — glows gold when copilot is thinking -->
              <canvas
                bind:this={oscillatorEl}
                width={800}
                height={48}
                class={$copilotLoading ? 'oscilloscope-active' : ''}
                style="width:100%;height:24px;display:block;border-radius:var(--la-radius-sm);margin-bottom:6px;opacity:0.85;"
              ></canvas>
              <CopilotContextTray
                snapshot={contextSnapshot}
                status={$copilotContextStatus}
                onRefresh={() => { contextSnapshot = snapshotContextForCopilot(); }}
                grounding={$copilotGrounding}
              />
              <div class="flex gap-2 relative">
                <!-- Tesseract command palette trigger — left of input, helix gold glow -->
                <button
                  onclick={() => { tesseractOpen = !tesseractOpen; }}
                  class="{$copilotLoading ? 'tesseract-glow-thinking' : 'tesseract-glow'} w-9 h-9 flex items-center justify-center rounded-lg shrink-0 transition-all duration-200 {tesseractOpen ? 'border border-[var(--la-focus-ring)] bg-[var(--la-focus-ring)]/15 shadow-[0_0_14px_rgba(255,215,0,0.5)]' : 'border border-[var(--la-drawer-border)] hover:border-[var(--la-focus-ring)]/50 hover:shadow-[0_0_8px_rgba(255,215,0,0.25)]'}"
                  title="Command palette"
                >
                  <PolytopeIcon type="tesseract" color={tesseractOpen ? '#FFD700' : '#D4A017'} size={22} />
                </button>
                <input
                  type="text"
                  bind:value={input}
                  onkeydown={handleInputKeydownExtended}
                  oninput={handleInputExtended}
                  onpaste={handlePaste}
                  onfocus={() => { if (input.startsWith('/')) showSuggestions = true; }}
                  onblur={() => { setTimeout(() => { showSuggestions = false; atSuggestions = []; }, 200); }}
                  placeholder="Type a message or /command… · @ for files"
                  class="flex-1 bg-[var(--la-bg-elev-1)] border border-[var(--la-drawer-border)] rounded px-3 py-1.5 text-xs text-[var(--la-text-bright)] placeholder-[var(--la-text-dim)] outline-none focus:border-[var(--la-focus-ring)]/60 transition-colors"
                />
                <!-- @-file autocomplete dropdown -->
                {#if atSuggestions.length > 0}
                  <div class="absolute bottom-full left-0 right-0 mb-1 bg-[var(--la-drawer-bg)] border border-[var(--la-focus-ring)]/20 rounded shadow-lg max-h-48 overflow-y-auto z-50">
                    {#each atSuggestions as suggestion, i}
                      <button
                        class="w-full text-left px-3 py-1.5 text-[10px] font-mono transition-colors
                          {i === atSuggestionIndex ? 'bg-[var(--la-focus-ring)]/10 text-[var(--la-focus-ring)]' : 'text-[var(--la-text-label)] hover:bg-[var(--la-drawer-border)]'}"
                        onmousedown={(e) => { e.preventDefault(); acceptAtSuggestion(suggestion); }}
                      >{suggestion}</button>
                    {/each}
                  </div>
                {/if}
                <!-- Voice toggle button — activates EVA voice output after each response -->
                <button
                  onclick={() => voiceEnabled.update(v => !v)}
                  aria-pressed={$voiceEnabled}
                  title={$voiceEnabled ? 'Voice on — click to mute EVA' : 'Click to enable EVA voice'}
                  class="w-9 h-9 flex items-center justify-center rounded-lg shrink-0 border transition-all duration-200
                    {$voiceEnabled
                      ? 'border-[var(--la-focus-ring)] bg-[var(--la-focus-ring)]/15 shadow-[0_0_8px_rgba(255,215,0,0.3)] text-[var(--la-focus-ring)]'
                      : 'border-[var(--la-drawer-border)] text-[var(--la-text-dim)] hover:border-[var(--la-focus-ring)]/40'}"
                >
                  {#if voicePlaying}
                    <svg width="14" height="14" viewBox="0 0 24 24" fill="currentColor"><path d="M3 9v6h4l5 5V4L7 9H3zm13.5 3c0-1.77-1.02-3.29-2.5-4.03v8.05c1.48-.73 2.5-2.25 2.5-4.02z"/></svg>
                  {:else}
                    <svg width="14" height="14" viewBox="0 0 24 24" fill="currentColor"><path d="M3 9v6h4l5 5V4L7 9H3zm13.5 3c0-1.77-1.02-3.29-2.5-4.03v8.05c1.48-.73 2.5-2.25 2.5-4.02zM14 3.23v2.06c2.89.86 5 3.54 5 6.71s-2.11 5.85-5 6.71v2.06c4.01-.91 7-4.49 7-8.77s-2.99-7.86-7-8.77z"/></svg>
                  {/if}
                </button>
                <button
                  onclick={sendMessage}
                  disabled={$copilotLoading}
                  class="px-3 py-1.5 bg-[var(--la-focus-ring)] text-[var(--la-bg-frame)] text-xs font-semibold rounded hover:bg-[var(--la-focus-ring)] disabled:opacity-50 transition-colors"
                >Send</button>

                <!-- Tesseract popover — DISPATCH / CONTEXT / QUICK -->
                {#if tesseractOpen}
                  <div class="absolute bottom-full left-0 mb-2 w-[280px] bg-[var(--la-drawer-bg)] border border-[var(--la-focus-ring)]/20 rounded-lg shadow-[0_0_20px_rgba(255,215,0,0.1)] p-3 flex flex-col gap-3 z-50">
                    <div>
                      <h3 class="text-[9px] font-medium text-[var(--la-text-dim)] mb-2">DISPATCH</h3>
                      <SiblingDispatch onDispatch={(sib, prompt) => { tesseractOpen = false; handleDispatch(sib, prompt); }} selectedSibling={$focusedSibling} />
                    </div>
                    <div>
                      <h3 class="text-[9px] font-medium text-[var(--la-text-dim)] mb-1">CONTEXT</h3>
                      <pre class="text-[8px] text-[var(--la-text-dim)] bg-[var(--la-bg-void)] border border-[var(--la-drawer-border)] rounded p-1.5 whitespace-pre-wrap max-h-28 overflow-y-auto">{contextString}</pre>
                    </div>
                    <div>
                      <h3 class="text-[9px] font-medium text-[var(--la-text-dim)] mb-1">QUICK</h3>
                      <div class="flex flex-wrap gap-1">
                        {#each ['/build', '/secure', '/research', '/review', '/observe'] as cmd}
                          <button onclick={() => { input = cmd + ' '; tesseractOpen = false; }}
                            class="text-[9px] px-2 py-0.5 rounded bg-[var(--la-bg-elev-1)] border border-[var(--la-drawer-border)] hover:border-[var(--la-focus-ring)]/50 text-[var(--la-text-label)] hover:text-[var(--la-focus-ring)] transition-colors">{cmd}</button>
                        {/each}
                      </div>
                    </div>
                  </div>
                {/if}
              </div>
            </div>
          </div>
        </div>
      {/if}

    </div>
  {/if}

  </div><!-- end sidebar flex-col -->

  <!-- Right-edge drag handle -->
  <div
    class="w-1 bg-[var(--la-drawer-border)] hover:bg-[var(--la-focus-ring)] focus:bg-[var(--la-focus-ring)] cursor-ew-resize shrink-0 transition-colors outline-none self-stretch"
    onmousedown={onDragStart}
    onkeydown={onSeparatorKeydown}
    role="slider"
    aria-label="Resize copilot sidebar"
    aria-orientation="vertical"
    aria-valuenow={widthPx}
    aria-valuemin={MIN_WIDTH}
    aria-valuemax={Math.floor(window.innerWidth * MAX_WIDTH_RATIO)}
    tabindex="0"
  ></div>
</div>

{#if showModelPicker}
  <!-- Model picker overlay — ⌘⇧M; uses inert on close to suppress keyboard focus -->
  <div
    class="model-picker-backdrop"
    role="dialog"
    aria-label="Model picker"
    aria-modal="true"
    onkeydown={(e) => { if (e.key === 'Escape') showModelPicker = false; }}
  >
    <div class="model-picker-panel">
      <div class="mp-header">
        <span class="mp-title">Switch Model</span>
        <button class="mp-close" onclick={() => { showModelPicker = false; }} aria-label="Close model picker">✕</button>
      </div>
      {#if modelPickerLoading}
        <div class="mp-loading">Loading…</div>
      {:else}
        <ul class="mp-list" role="listbox" aria-label="Available models">
          {#each modelPickerModels as m}
            <li role="option" aria-selected={$selectedModel === m.id}>
              <button
                class="mp-item"
                class:active={$selectedModel === m.id}
                onclick={() => selectPickerModel(m.id)}
              >
                <span class="mp-model-label">{m.label || m.id}</span>
                <span class="mp-model-tier">{m.tier}</span>
              </button>
            </li>
          {/each}
        </ul>
      {/if}
    </div>
  </div>
{/if}
<OllamaConfigModal isOpen={showOllamaModal} onClose={() => { showOllamaModal = false; }} />
<!-- Hidden audio element for EVA voice playback; aria-hidden prevents AT exposure -->
<audio bind:this={audioEl} aria-hidden="true" style="display:none"></audio>

<style>
  /* ── Copilot header — 3-zone geometric strip ─────────────────────────────── */
  .copilot-header {
    display: flex;
    align-items: stretch;
    height: 32px;
    flex-shrink: 0;
    background: var(--la-bg-void, #08090a);
    border-bottom: 1px solid var(--la-drawer-border, #1c2028);
    font-family: var(--la-font-mono, monospace);
    overflow: hidden;
  }

  /* ZONE A — Mode tabs */
  .hdr-tab {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 0 10px;
    height: 100%;
    font-family: var(--la-font-mono, monospace);
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.1em;
    color: var(--la-text-mute, #6e7681);
    background: transparent;
    border: none;
    border-right: 1px solid var(--la-drawer-border, #1c2028);
    border-bottom: 2px solid transparent;
    cursor: pointer;
    flex-shrink: 0;
    transition: color 80ms, background 80ms, border-color 80ms;
  }
  .hdr-tab:hover {
    color: var(--la-text-base, #c9d1d9);
    background: var(--la-bg-elev-1, #111214);
  }
  .hdr-tab--on {
    color: var(--la-focus-ring, #FFD700);
    border-bottom-color: var(--la-focus-ring, #FFD700);
    background: rgba(255, 215, 0, 0.04);
  }
  .hdr-tab-icon {
    font-size: 9px;
    color: var(--la-text-mute, #6e7681);
    flex-shrink: 0;
  }
  .hdr-tab--on .hdr-tab-icon {
    color: var(--la-focus-ring, #FFD700);
  }

  /* Spacer — pushes ghost actions to the right */
  .hdr-spacer {
    flex: 1;
  }

  /* Ghost actions — invisible at rest, reveal on header hover.
     CSS-only: no JS state needed. focus-within keeps them visible
     for keyboard users once they tab into the group. */
  .hdr-actions {
    display: flex;
    align-items: stretch;
    opacity: 0;
    pointer-events: none;
    transition: opacity 100ms ease;
  }
  .copilot-header:hover .hdr-actions,
  .hdr-actions:focus-within {
    opacity: 1;
    pointer-events: auto;
  }
  .hdr-actions .hdr-action-wrap {
    display: flex;
    align-items: stretch;
    flex-shrink: 0;
  }

  /* Preset chip — inside ghost group */
  .hdr-preset {
    display: flex;
    align-items: center;
    height: 100%;
    padding: 0 8px;
    font-family: var(--la-font-mono, monospace);
    font-size: 8px;
    font-weight: 700;
    letter-spacing: 0.12em;
    text-transform: uppercase;
    color: var(--la-text-mute, #6e7681);
    background: transparent;
    border: none;
    border-left: 1px solid var(--la-drawer-border, #1c2028);
    cursor: pointer;
    flex-shrink: 0;
    transition: color 80ms, background 80ms;
  }
  .hdr-preset:hover {
    color: var(--la-focus-ring, #FFD700);
    background: rgba(255, 215, 0, 0.04);
  }

  /* ZONE C — Icon-only action cells */
  .hdr-action {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 28px;
    height: 100%;
    font-size: 11px;
    color: var(--la-text-mute, #6e7681);
    background: transparent;
    border: none;
    border-left: 1px solid var(--la-drawer-border, #1c2028);
    cursor: pointer;
    flex-shrink: 0;
    transition: color 80ms, background 80ms;
  }
  .hdr-action:hover {
    color: var(--la-text-bright, #f1f5f9);
    background: var(--la-bg-elev-1, #111214);
  }
  .hdr-action:disabled {
    cursor: default;
    opacity: 0.35;
  }
  .hdr-action--on {
    color: var(--la-focus-ring, #FFD700);
    background: rgba(255, 215, 0, 0.06);
  }
  .hdr-action--on:hover {
    background: rgba(255, 215, 0, 0.1);
  }
  .hdr-action--gold {
    color: var(--la-focus-ring, #FFD700);
  }
  .hdr-action--warn {
    color: var(--la-semantic-warn, #f59e0b);
    animation: hdr-warn-pulse 2s ease-in-out infinite;
  }
  @keyframes hdr-warn-pulse {
    0%, 100% { opacity: 1; }
    50%       { opacity: 0.55; }
  }
  .hdr-action--disabled {
    color: var(--la-hair-strong, #44505e);
    cursor: default;
  }
  .hdr-action--ayin {
    color: var(--la-cyan, #1ecbe1);
    font-size: 9px;
    letter-spacing: 0.04em;
  }
  .hdr-action-wrap {
    display: flex;
    align-items: stretch;
    flex-shrink: 0;
  }
  .hdr-collapse {
    border-left: 1px solid var(--la-drawer-border, #1c2028);
    color: var(--la-text-mute, #6e7681);
  }

  @media (prefers-reduced-motion: reduce) {
    .hdr-tab, .hdr-action, .hdr-preset { transition: none; }
    .hdr-action--warn { animation: none; }
  }

  /* Collapsible thinking blocks */
  .thinking-wrap {
    width: 100%;
    margin: 2px 0;
  }
  .thinking-toggle {
    display: flex;
    align-items: center;
    gap: 6px;
    width: 100%;
    background: transparent;
    border: none;
    border-left: 2px solid var(--la-hair-strong);
    padding: 3px 8px;
    cursor: pointer;
    text-align: left;
    color: var(--la-text-mute);
    font-family: var(--la-font-mono, monospace);
    font-size: 9px;
    letter-spacing: 0.05em;
    transition: border-color 0.15s ease, color 0.15s ease;
  }
  .thinking-toggle:hover {
    border-color: var(--la-text-dim);
    color: var(--la-text-dim);
  }
  .thinking-chevron { font-size: 7px; opacity: 0.6; flex-shrink: 0; }
  .thinking-label { font-weight: 700; letter-spacing: 0.14em; text-transform: uppercase; flex: 1; }
  .thinking-chars { font-size: 8px; opacity: 0.4; font-variant-numeric: tabular-nums; }
  .thinking-body {
    border-left: 2px solid var(--la-hair-base);
    padding: 6px 10px 6px 8px;
    font-size: 10px;
    color: var(--la-text-mute);
    font-style: italic;
    line-height: 1.55;
    animation: thinking-expand 0.12s ease both;
  }
  @keyframes thinking-expand {
    from { opacity: 0; transform: translateY(-4px); }
    to   { opacity: 1; transform: translateY(0); }
  }

  /* Markdown rendering inside chat bubbles.
     Scoped via .chat-md-content; paragraphs collapse margins so agent
     responses don't get disproportionate whitespace inside the tight
     text-xs bubble. Code blocks get their own backdrop so they read
     clearly against the #111827 assistant-bubble color. */
  :global(.chat-md-content) {
    display: inline-block;
    width: 100%;
  }
  :global(.chat-md-content p) {
    margin: 0 0 0.35em 0;
    line-height: 1.5;
  }
  :global(.chat-md-content p:last-child) {
    margin-bottom: 0;
  }
  :global(.chat-md-content strong) {
    font-weight: 600;
    color: var(--la-text-stark);
  }
  :global(.chat-md-content em) {
    font-style: italic;
  }
  :global(.chat-md-content code) {
    background: rgba(255, 215, 0, 0.08);
    color: var(--la-focus-ring);
    padding: 0.1em 0.35em;
    border-radius: var(--la-radius-sm);
    font-family: var(--la-font-mono);
    font-size: 0.92em;
    word-break: break-word;
  }
  :global(.chat-md-content pre) {
    background: var(--la-bg-frame);
    border: 1px solid var(--la-drawer-border);
    border-radius: var(--la-radius-sm);
    padding: 8px 10px;
    margin: 0.4em 0;
    overflow-x: auto;
    font-size: 11px;
  }
  :global(.chat-md-content pre code) {
    background: transparent;
    color: var(--la-text-bright);
    padding: 0;
    font-size: inherit;
  }
  :global(.chat-md-content ul),
  :global(.chat-md-content ol) {
    margin: 0.35em 0;
    padding-left: 1.2em;
  }
  :global(.chat-md-content li) {
    margin: 0.1em 0;
  }
  :global(.chat-md-content h1),
  :global(.chat-md-content h2),
  :global(.chat-md-content h3),
  :global(.chat-md-content h4) {
    margin: 0.5em 0 0.3em 0;
    font-weight: 600;
    color: var(--la-text-stark);
    line-height: 1.3;
  }
  :global(.chat-md-content h1) { font-size: 1.15em; }
  :global(.chat-md-content h2) { font-size: 1.05em; }
  :global(.chat-md-content h3) { font-size: 1em; }
  :global(.chat-md-content h4) { font-size: 0.95em; }
  :global(.chat-md-content a) {
    color: var(--la-focus-ring);
    text-decoration: underline;
    text-underline-offset: 2px;
  }
  :global(.chat-md-content a:hover) {
    color: var(--la-focus-ring);
  }
  :global(.chat-md-content blockquote) {
    border-left: 2px solid var(--la-hair-strong);
    padding-left: 0.7em;
    margin: 0.4em 0;
    color: var(--la-text-label);
  }
  :global(.chat-md-content hr) {
    border: none;
    border-top: 1px solid var(--la-drawer-border);
    margin: 0.6em 0;
  }
  :global(.chat-md-content table) {
    border-collapse: collapse;
    margin: 0.4em 0;
    font-size: 11px;
  }
  :global(.chat-md-content th),
  :global(.chat-md-content td) {
    border: 1px solid var(--la-drawer-border);
    padding: 3px 6px;
    text-align: left;
  }
  :global(.chat-md-content th) {
    background: var(--la-bg-frame);
    font-weight: 600;
  }

  /* Tesseract button — ambient gold glow pulse matching the helix energy */
  .tesseract-glow {
    animation: tesseract-pulse 3s ease-in-out infinite;
  }
  /* Faster, brighter pulse when copilot is thinking */
  .tesseract-glow-thinking {
    animation: tesseract-pulse-thinking 1.2s ease-in-out infinite;
  }
  @keyframes tesseract-pulse {
    0%, 100% { box-shadow: 0 0 4px rgba(255, 215, 0, 0.15); }
    50% { box-shadow: 0 0 12px rgba(255, 215, 0, 0.4), 0 0 24px rgba(255, 215, 0, 0.12); }
  }
  @keyframes tesseract-pulse-thinking {
    0%, 100% { box-shadow: 0 0 6px rgba(255, 215, 0, 0.3); }
    50% { box-shadow: 0 0 18px rgba(255, 215, 0, 0.6), 0 0 32px rgba(255, 215, 0, 0.2); }
  }

  /* Oscilloscope active state — bottom border glow frames it as "live" */
  .oscilloscope-active {
    border-bottom: 1px solid rgba(255, 215, 0, 0.25);
    box-shadow: 0 2px 8px rgba(255, 215, 0, 0.15);
    transition: border-color 0.3s, box-shadow 0.3s;
  }

  /* ── Model picker (⌘⇧M) ─────────────────────────────────────────────────── */
  .model-picker-backdrop {
    position: fixed; inset: 0; z-index: 9000;
    background: rgba(0,0,0,0.55);
    display: flex; align-items: flex-start; justify-content: center;
    padding-top: 15vh;
  }
  .model-picker-panel {
    background: #0f172a; border: 1px solid #334155; border-radius: 12px;
    width: 420px; max-height: 55vh; display: flex; flex-direction: column;
    box-shadow: 0 16px 48px rgba(0,0,0,0.6);
    overflow: hidden;
  }
  .mp-header {
    display: flex; align-items: center; justify-content: space-between;
    padding: 0.75rem 1rem; border-bottom: 1px solid #1e293b;
  }
  .mp-title { font-family: 'Raleway', sans-serif; font-size: 0.85rem; font-weight: 700; color: #e2e8f0; }
  .mp-close { background: transparent; border: none; color: #475569; cursor: pointer; font-size: 0.9rem; padding: 0.15rem 0.4rem; border-radius: 4px; }
  .mp-close:hover { color: #94a3b8; }
  .mp-loading { padding: 1rem; font-family: 'IBM Plex Mono', monospace; font-size: 0.75rem; color: #475569; }
  .mp-list { list-style: none; margin: 0; padding: 0.5rem; overflow-y: auto; }
  .mp-item {
    display: flex; align-items: center; justify-content: space-between;
    width: 100%; padding: 0.55rem 0.75rem; border: none; border-radius: 6px;
    background: transparent; cursor: pointer; text-align: left;
  }
  .mp-item:hover { background: #1e293b; }
  .mp-item.active { background: rgba(255,102,0,0.12); }
  .mp-model-label { font-family: 'IBM Plex Mono', monospace; font-size: 0.72rem; color: #94a3b8; }
  .mp-model-tier { font-family: 'IBM Plex Mono', monospace; font-size: 0.6rem; color: #475569; text-transform: uppercase; letter-spacing: 0.08em; }
</style>
