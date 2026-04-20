<script lang="ts">
  import {
    copilotMessages, copilotLoading, currentBuildId, activeBuild,
    findings, selectedPillar, focusedSibling, spikeSibling,
    buildBuildContext, authProfile, ollamaConfig, terminalConnected,
    builds, siblingHealth, arenaStats, alertStats, drawerHeightPx, waves,
  } from '$lib/stores';
  import { SIBLING_COLORS } from '$lib/design-tokens';
  import { api } from '$lib/api';
  import { parseCommand, SLASH_COMMANDS } from '$lib/commands';
  import { connectSSE, disconnectSSE } from '$lib/sse';
  import { TerminalWS } from '$lib/ws';
  import SiblingDispatch from './SiblingDispatch.svelte';
  import OllamaConfigModal from './OllamaConfigModal.svelte';
  import SettingsOverlay from './SettingsOverlay.svelte';
  import { settingsOpen, pendingResumeSessionId } from '$lib/setup';
  import { renderMarkdown } from '$lib/markdown';
  import type { CopilotMessage, SiblingId } from '$lib/types';
  import { Terminal } from '@xterm/xterm';
  import { FitAddon } from '@xterm/addon-fit';

  // --- Drawer state ---
  let open = $state(false);
  let heightPx = $state(420);
  const MIN_HEIGHT = 180;
  const MAX_HEIGHT_RATIO = 0.85;

  // Publish drawer height to layout so content area can compensate
  $effect(() => { drawerHeightPx.set(open ? heightPx : 32); });

  // Clamp heightPx on window resize so drawer doesn't overflow small screens
  function onWindowResize() {
    const max = Math.floor(window.innerHeight * MAX_HEIGHT_RATIO);
    if (heightPx > max) heightPx = max;
  }

  // --- Session state ---
  let mode = $state<'chat' | 'terminal'>('chat');
  let terminalEl: HTMLDivElement | undefined = $state();
  let sharedBuildId = $state<string | null>(null);
  let cwd = $state('/tmp');
  let connecting = $state(false);
  let connectError = $state<string | null>(null);
  let showOllamaModal = $state(false);
  let input = $state('');
  let showSuggestions = $state(false);
  let messagesEl: HTMLDivElement | undefined = $state();
  let oscillatorEl: HTMLCanvasElement | undefined = $state();

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
      const color = (SIBLING_COLORS as Record<string, string>)[sid] ?? '#7C3AED';
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
    const resp = await api.createBuild(body) as { build_id: string };
    sharedBuildId = resp.build_id;
    currentBuildId.set(resp.build_id);
    return resp.build_id;
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

  async function connectTerminal() {
    connecting = true;
    connectError = null;
    try { await ensureBuild(); }
    catch (err) { connectError = err instanceof Error ? err.message : 'Failed to create build'; }
    finally { connecting = false; }
  }

  // xterm.js lifecycle — runs only in terminal mode with a live build
  $effect(() => {
    if (mode !== 'terminal' || !terminalEl || !sharedBuildId || !open) return;

    const term = new Terminal({
      cursorBlink: true,
      fontSize: 13,
      fontFamily: 'monospace',
      theme: { background: '#0a0a0a', foreground: '#e2e8f0', cursor: '#7C3AED', selectionBackground: '#7C3AED44' },
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

  // --- Rich context string shown in drawer header ---
  let contextBadge = $derived(() => {
    const b = $activeBuild;
    if (b) return `${b.name.slice(0, 24)} · ${b.metaSkill}`;
    const bCount = $builds.length;
    return bCount > 0 ? `${bCount} build${bCount > 1 ? 's' : ''}` : 'no active build';
  });

  // --- Messages ---
  function addMessage(role: CopilotMessage['role'], content: string, sibling?: SiblingId) {
    const msg: CopilotMessage = { id: crypto.randomUUID(), role, content, sibling, timestamp: new Date().toISOString() };
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
    if (!text) return;
    input = '';
    showSuggestions = false;
    const { command, args } = parseCommand(text);
    if (command) {
      addMessage('system', `/${command.name} ${args}`.trim());
      if (command.name === 'clear') { copilotMessages.set([]); return; }
      try {
        await command.execute(args);
        mockStream(`Dispatched /${command.name}${args ? ` with "${args}"` : ''}. Context: ${$currentBuildId ? `build ${$currentBuildId}` : 'no active build'}.`);
      } catch (err) {
        addMessage('system', `Error: ${err instanceof Error ? err.message : 'Unknown error'}`);
      }
      return;
    }
    addMessage('user', text);
    copilotLoading.set(true);
    let buildId: string | null = null;
    try { buildId = await ensureBuild(); }
    catch { mockStream('Could not create build session. Is the webshell running?'); copilotLoading.set(false); return; }
    try {
      const result = await api.copilotChat(buildId!, `[Context]\n${contextString}\n\n[User]\n${text}`);
      const response = typeof result === 'object' && result !== null && 'response' in result
        ? String((result as Record<string, unknown>).response)
        : 'No response from provider.';
      // If SSE already delivered copilot_response (done: true → loading=false), skip.
      if ($copilotLoading) mockStream(response);
    } catch { mockStream('Could not reach AI provider. Check webshell logs.'); }
  }

  function selectCommand(name: string) { input = `/${name} `; showSuggestions = false; }
  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); sendMessage(); }
    if (e.key === 'Escape') { showSuggestions = false; if (!input) open = false; }
  }
  function handleInput() { showSuggestions = input.startsWith('/'); }

  function handleDispatch(sibling: SiblingId, prompt?: string) {
    focusedSibling.set(sibling);
    spikeSibling(sibling);
    addMessage('system', prompt ? `Dispatching ${sibling.toUpperCase()} with: "${prompt}"` : `Dispatching ${sibling.toUpperCase()}`);
    const buildId = $currentBuildId;
    if (buildId) api.dispatchSibling(buildId, sibling, sibling, prompt ?? '').catch(() => {});
    mockStream(
      `${sibling.toUpperCase()} activated. ${buildId ? `Build ${buildId}` : 'No active build — standing by.'}`,
      sibling,
    );
  }

  // Auto-scroll
  $effect(() => { $copilotMessages; if (messagesEl) requestAnimationFrame(() => { if (messagesEl) messagesEl.scrollTop = messagesEl.scrollHeight; }); });

  // --- Drag to resize ---
  let dragging = false;
  let dragStartY = 0;
  let dragStartH = 0;

  function onDragStart(e: MouseEvent) {
    dragging = true;
    dragStartY = e.clientY;
    dragStartH = heightPx;
    e.preventDefault();
  }

  function onDragMove(e: MouseEvent) {
    if (!dragging) return;
    const delta = dragStartY - e.clientY;
    const maxH = Math.floor(window.innerHeight * MAX_HEIGHT_RATIO);
    heightPx = Math.min(maxH, Math.max(MIN_HEIGHT, dragStartH + delta));
  }

  function onDragEnd() { dragging = false; }

  function onSeparatorKeydown(e: KeyboardEvent) {
    const step = 20;
    const maxH = Math.floor(window.innerHeight * MAX_HEIGHT_RATIO);
    if (e.key === 'ArrowUp') { e.preventDefault(); heightPx = Math.min(maxH, heightPx + step); }
    else if (e.key === 'ArrowDown') { e.preventDefault(); heightPx = Math.max(MIN_HEIGHT, heightPx - step); }
  }

  // Global keyboard shortcut: Ctrl+` to toggle
  function onGlobalKeydown(e: KeyboardEvent) {
    if (e.key === '`' && (e.ctrlKey || e.metaKey)) { e.preventDefault(); open = !open; }
  }
</script>

<svelte:window
  onmousemove={onDragMove}
  onmouseup={onDragEnd}
  onkeydown={onGlobalKeydown}
  onresize={onWindowResize}
/>

<!-- Drawer container -->
<div
  class="fixed bottom-0 left-0 right-0 z-30 flex flex-col"
  style="height: {open ? heightPx + 'px' : '32px'}; transition: height 0.18s ease;"
>
  <!-- Drag handle (only when open) -->
  {#if open}
    <div
      class="h-1 bg-[#1e293b] hover:bg-[#7C3AED] focus:bg-[#7C3AED] cursor-ns-resize shrink-0 transition-colors outline-none focus:ring-1 focus:ring-[#7C3AED]"
      onmousedown={onDragStart}
      onkeydown={onSeparatorKeydown}
      role="separator"
      aria-label="Resize copilot drawer"
      aria-orientation="horizontal"
      aria-valuenow={heightPx}
      aria-valuemin={MIN_HEIGHT}
      aria-valuemax={Math.floor(window.innerHeight * MAX_HEIGHT_RATIO)}
      tabindex="0"
    ></div>
  {/if}

  <!-- Toggle bar / header -->
  <div class="flex items-center gap-2 px-3 bg-[#0d1117] border-t border-[#1e293b] shrink-0 h-8">
    <!-- Mode tabs (only when open) -->
    {#if open}
      <div class="flex rounded overflow-hidden border border-[#1e293b] shrink-0">
        <button
          onclick={() => { mode = 'chat'; }}
          class="text-[9px] px-2 py-0.5 transition-colors {mode === 'chat' ? 'bg-[#7C3AED] text-white' : 'bg-transparent text-[#64748b] hover:text-[#e2e8f0]'}"
        >CHAT</button>
        <button
          onclick={() => { mode = 'terminal'; }}
          class="text-[9px] px-2 py-0.5 transition-colors {mode === 'terminal' ? 'bg-[#7C3AED] text-white' : 'bg-transparent text-[#64748b] hover:text-[#e2e8f0]'}"
        >TERMINAL</button>
      </div>
    {/if}

    <!-- Identity pill -->
    <button
      onclick={() => { open = !open; }}
      aria-expanded={open}
      class="flex items-center gap-1.5 text-[10px] text-[#94a3b8] hover:text-[#e2e8f0] transition-colors"
    >
      <span class="text-[#7C3AED] font-semibold">⌨</span>
      <span>Copilot</span>
      {#if sharedBuildId}
        <span class="text-[#22c55e]">●</span>
        <span class="text-[#475569] font-mono">{sharedBuildId.slice(0, 7)}</span>
      {:else}
        <span class="text-[#475569]">—</span>
      {/if}
    </button>

    <!-- Context badge -->
    {#if open}
      <span class="text-[9px] text-[#475569] truncate max-w-[200px]">{contextBadge()}</span>
      <!-- Platform summary -->
      <div class="flex items-center gap-2 text-[9px] text-[#475569]">
        <span>{$builds.length} builds</span>
        <span>·</span>
        <span>{Object.values($siblingHealth).filter(h => h?.status === 'online').length}/7 siblings</span>
        <span>·</span>
        <span>{$arenaStats.activeAgents} active</span>
        <span>·</span>
        <span class="text-[#ef4444]">{$alertStats.unacknowledged} alerts</span>
      </div>
    {/if}

    <div class="flex-1"></div>

    {#if open}
      {#if mode === 'chat'}
        <button
          onclick={forkToTerminal}
          disabled={!canFork || forking}
          class="text-[9px] px-1.5 py-0.5 rounded border transition-colors
                 {canFork && !forking
                   ? 'text-[#7C3AED] border-[#7C3AED]/40 hover:bg-[#7C3AED]/10'
                   : 'text-[#475569] border-[#1e293b] cursor-not-allowed opacity-50'}"
          title={canFork
            ? 'Fork this conversation to a terminal (claude --resume / codex exec resume)'
            : 'Send at least one message before forking to a terminal'}
        >{forking ? 'Forking…' : '↗ Fork to Terminal'}</button>
        <button
          onclick={() => copilotMessages.set([])}
          class="text-[9px] text-[#475569] hover:text-[#e2e8f0] px-1.5 py-0.5 rounded border border-[#1e293b] transition-colors"
        >Clear</button>
      {/if}
      <div style="position: relative;">
        <button
          onclick={() => settingsOpen.update(v => !v)}
          class="text-[10px] text-[#475569] hover:text-[#94a3b8] px-1.5 py-0.5 rounded border border-[#1e293b] transition-colors"
          title="Switch backend / model (⚙)"
        >⚙</button>
        {#if $settingsOpen}
          <div class="absolute bottom-full right-0 mb-1">
            <SettingsOverlay />
          </div>
        {/if}
      </div>
    {/if}

    <!-- Collapse/expand -->
    <button
      onclick={() => { open = !open; }}
      class="text-[10px] text-[#475569] hover:text-[#e2e8f0] w-5 h-5 flex items-center justify-center transition-colors"
      title="{open ? 'Collapse (Ctrl+`)' : 'Open Copilot (Ctrl+`)'}"
    >{open ? '▾' : '▴'}</button>
  </div>

  <!-- ── BODY (only when open) ───────────────────────────────── -->
  {#if open}
    <div class="flex-1 flex overflow-hidden bg-[#0a0a0f] min-h-0">

      <!-- ── TERMINAL MODE ── -->
      {#if mode === 'terminal'}
        <div class="flex-1 flex flex-col overflow-hidden min-h-0">
          {#if !sharedBuildId}
            <div class="flex items-center gap-3 px-4 py-2 border-b border-[#1e293b] bg-[#0f172a] shrink-0">
              <span class="text-[10px] text-[#64748b]">Profile</span>
              <select
                bind:value={$authProfile}
                class="bg-[#111827] border border-[#1e293b] rounded px-2 py-1 text-[11px] text-[#e2e8f0] outline-none focus:border-[#7C3AED]"
              >
                <option value="anthropic">Anthropic</option>
                <option value="ollama">Ollama Cloud</option>
              </select>
              {#if $authProfile === 'ollama'}
                <button onclick={() => { showOllamaModal = true; }} class="text-[10px] text-[#6366F1] hover:text-[#818CF8]">⚙</button>
              {/if}
              <span class="text-[10px] text-[#64748b]">CWD</span>
              <input type="text" bind:value={cwd} placeholder="/tmp"
                class="w-40 bg-[#111827] border border-[#1e293b] rounded px-2 py-1 text-[11px] text-[#e2e8f0] outline-none focus:border-[#7C3AED]"
              />
              <button
                onclick={connectTerminal}
                disabled={connecting}
                class="px-3 py-1 bg-[#7C3AED] text-white text-[11px] rounded hover:bg-[#6D28D9] disabled:opacity-50"
              >{connecting ? 'Connecting…' : 'Connect'}</button>
              {#if connectError}
                <span class="text-[10px] text-red-400">{connectError}</span>
              {/if}
            </div>
          {:else}
            <div class="flex items-center gap-2 px-4 py-1.5 border-b border-[#1e293b] bg-[#0f172a] shrink-0">
              <div class="w-1.5 h-1.5 rounded-full bg-green-500" style="box-shadow: 0 0 4px #22c55e"></div>
              <span class="text-[9px] text-[#64748b] font-mono">build {sharedBuildId.slice(0, 8)}… · {cwd}</span>
              <div class="flex-1"></div>
              <button onclick={() => { sharedBuildId = null; terminalConnected.set(false); }}
                class="text-[9px] text-[#475569] hover:text-red-400 transition-colors">Disconnect</button>
            </div>
          {/if}
          <div bind:this={terminalEl} class="flex-1 overflow-hidden bg-[#0a0a0a] min-h-0" style="font-family: monospace; contain: strict;"></div>
        </div>

      <!-- ── CHAT MODE ── -->
      {:else}
        <div class="flex-1 flex overflow-hidden">
          <!-- Messages + input -->
          <div class="flex-1 flex flex-col overflow-hidden">
            {#if forkError}
              <div class="px-3 py-1.5 border-b border-red-500/40 bg-red-500/10 flex items-center gap-2 shrink-0">
                <span class="text-[10px] text-red-300">Fork failed: {forkError}</span>
                <div class="flex-1"></div>
                <button onclick={dismissForkResult} class="text-[10px] text-red-300/70 hover:text-red-200">✕</button>
              </div>
            {:else if forkResult}
              {#if forkResult.launched}
                <div class="px-3 py-1.5 border-b border-[#7C3AED]/40 bg-[#7C3AED]/10 flex items-center gap-2 shrink-0">
                  <span class="text-[10px] text-[#A78BFA]">↗ Opened in Terminal ({forkResult.platform}). Conversation continues in both places — same session.</span>
                  <div class="flex-1"></div>
                  <button onclick={dismissForkResult} class="text-[10px] text-[#A78BFA]/70 hover:text-[#A78BFA]">✕</button>
                </div>
              {:else}
                <div class="px-3 py-1.5 border-b border-[#f59e0b]/40 bg-[#f59e0b]/10 flex items-start gap-2 shrink-0">
                  <div class="flex-1">
                    <div class="text-[10px] text-[#f59e0b] mb-1">
                      No native terminal launcher on <span class="font-mono">{forkResult.platform}</span> yet — run this in your terminal:
                    </div>
                    <code class="text-[10px] text-[#e2e8f0] bg-[#0a0a0a] px-2 py-0.5 rounded border border-[#1e293b] font-mono select-all">{forkResult.command}</code>
                  </div>
                  <button onclick={copyForkCommand} class="text-[10px] text-[#f59e0b] hover:text-[#fbbf24] px-1.5 py-0.5 rounded border border-[#f59e0b]/40">Copy</button>
                  <button onclick={dismissForkResult} class="text-[10px] text-[#f59e0b]/70 hover:text-[#f59e0b]">✕</button>
                </div>
              {/if}
            {/if}
            <div bind:this={messagesEl} class="flex-1 overflow-y-auto p-3 space-y-2" role="log" aria-label="Chat messages" aria-live="polite">
              {#if $copilotMessages.length === 0}
                <div class="flex flex-col items-center justify-center h-full text-[#475569] gap-2">
                  <p class="text-xs">Start a conversation · Use <kbd class="bg-[#1e293b] px-1 rounded">/</kbd> for slash commands</p>
                  <div class="flex flex-wrap gap-1.5 justify-center">
                    {#each ['/build', '/secure', '/research', '/deploy', '/quality', '/clear'] as cmd}
                      <button onclick={() => { input = cmd + ' '; }}
                        class="text-[10px] px-2 py-1 rounded bg-[#111827] border border-[#1e293b] hover:border-[#334155] transition-colors">{cmd}</button>
                    {/each}
                  </div>
                </div>
              {:else}
                {#each $copilotMessages as msg (msg.id)}
                  <div class="flex {msg.role === 'user' ? 'justify-end' : msg.role === 'system' ? 'justify-center' : 'justify-start'}">
                    <div class="max-w-[80%] px-3 py-1.5 rounded-lg text-xs chat-bubble
                      {msg.role === 'user' ? 'bg-[#7C3AED] text-white' :
                       msg.role === 'system' ? 'bg-[#1e293b]/50 text-[#64748b] border border-[#1e293b]' :
                       'bg-[#111827] border border-[#1e293b] text-[#e2e8f0]'}">
                      {#if msg.sibling}
                        {@const color = SIBLING_COLORS[msg.sibling] ?? '#6b7280'}
                        <span class="text-[10px] font-medium" style="color: {color}">{msg.sibling.toUpperCase()}</span>
                        <span class="text-[#475569] mx-1">·</span>
                      {/if}
                      {#if msg.role === 'user'}
                        <span class="chat-user-content">{msg.content}</span>
                      {:else}
                        <span class="chat-md-content">{@html renderMarkdown(msg.content)}</span>
                      {/if}
                    </div>
                  </div>
                {/each}
                {#if $copilotLoading}
                  <div class="flex justify-start">
                    <div class="bg-[#111827] border border-[#1e293b] px-3 py-1.5 rounded-lg">
                      <span class="text-[#475569] text-xs animate-pulse">Thinking…</span>
                    </div>
                  </div>
                {/if}
              {/if}
            </div>

            <!-- Input -->
            <div class="border-t border-[#1e293b] px-3 py-2 relative shrink-0">
              {#if showSuggestions && matchingCommands.length > 0}
                <div class="absolute bottom-full left-3 right-3 mb-1 bg-[#0a0a0a] border border-[#1e293b] rounded-lg overflow-hidden shadow-xl z-10 max-h-40 overflow-y-auto">
                  {#each matchingCommands as cmd}
                    <button class="w-full text-left px-3 py-1.5 text-xs hover:bg-[#1e293b] flex items-baseline gap-2" onclick={() => selectCommand(cmd.name)}>
                      <span class="text-[#7C3AED] font-mono">/{cmd.name}</span>
                      <span class="text-[#64748b] flex-1">{cmd.description}</span>
                      {#if cmd.args}<span class="text-[#334155]">{cmd.args}</span>{/if}
                    </button>
                  {/each}
                </div>
              {/if}
              <!-- Hint bar for current command -->
              {#if input.startsWith('/') && matchingCommands.length > 0}
                {@const hint = matchingCommands[0]}
                <div class="text-[9px] text-[#475569] mb-1 flex items-center gap-2">
                  <span class="text-[#7C3AED]">/{hint.name}</span>
                  <span>{hint.description}</span>
                  {#if hint.args}<span class="text-[#334155]">{hint.args}</span>{/if}
                </div>
              {/if}
              <!-- Composite oscilloscope -->
              <canvas
                bind:this={oscillatorEl}
                width={800}
                height={48}
                style="width:100%;height:24px;display:block;border-radius:4px;margin-bottom:6px;opacity:0.85;"
              ></canvas>
              <div class="flex gap-2">
                <input
                  type="text"
                  bind:value={input}
                  onkeydown={handleKeydown}
                  oninput={handleInput}
                  onfocus={() => { if (input.startsWith('/')) showSuggestions = true; }}
                  onblur={() => { setTimeout(() => { showSuggestions = false; }, 200); }}
                  placeholder="Type a message or /command…"
                  class="flex-1 bg-[#111827] border border-[#1e293b] rounded px-3 py-1.5 text-xs text-[#e2e8f0] placeholder-[#475569] outline-none focus:border-[#7C3AED] transition-colors"
                />
                <button
                  onclick={sendMessage}
                  disabled={$copilotLoading}
                  class="px-3 py-1.5 bg-[#7C3AED] text-white text-xs rounded hover:bg-[#6D28D9] disabled:opacity-50 transition-colors"
                >Send</button>
              </div>
            </div>
          </div>

          <!-- Sidebar: dispatch + context (hidden when narrow) -->
          <div class="w-[180px] border-l border-[#1e293b] p-3 flex flex-col gap-3 overflow-y-auto shrink-0 hidden lg:flex">
            <div>
              <h3 class="text-[9px] font-medium text-[#64748b] mb-2">DISPATCH</h3>
              <SiblingDispatch onDispatch={handleDispatch} selectedSibling={$focusedSibling} />
            </div>
            <div>
              <h3 class="text-[9px] font-medium text-[#64748b] mb-1">CONTEXT</h3>
              <pre class="text-[8px] text-[#475569] bg-[#0a0a0a] border border-[#1e293b] rounded p-1.5 whitespace-pre-wrap max-h-28 overflow-y-auto">{contextString}</pre>
            </div>
            <div>
              <h3 class="text-[9px] font-medium text-[#64748b] mb-1">QUICK</h3>
              <div class="space-y-0.5">
                {#each ['/build', '/secure', '/research', '/review', '/observe'] as cmd}
                  <button onclick={() => { input = cmd + ' '; }}
                    class="w-full text-left text-[9px] px-2 py-0.5 rounded bg-[#111827] border border-[#1e293b] hover:border-[#334155] text-[#94a3b8] transition-colors">{cmd}</button>
                {/each}
              </div>
            </div>
          </div>
        </div>
      {/if}

    </div>
  {/if}
</div>

<OllamaConfigModal isOpen={showOllamaModal} onClose={() => { showOllamaModal = false; }} />

<style>
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
    color: #f1f5f9;
  }
  :global(.chat-md-content em) {
    font-style: italic;
  }
  :global(.chat-md-content code) {
    background: rgba(124, 58, 237, 0.15);
    color: #c4b5fd;
    padding: 0.1em 0.35em;
    border-radius: 3px;
    font-family: 'SF Mono', Menlo, Consolas, monospace;
    font-size: 0.92em;
    word-break: break-word;
  }
  :global(.chat-md-content pre) {
    background: #0a0a0f;
    border: 1px solid #1e293b;
    border-radius: 4px;
    padding: 8px 10px;
    margin: 0.4em 0;
    overflow-x: auto;
    font-size: 11px;
  }
  :global(.chat-md-content pre code) {
    background: transparent;
    color: #e2e8f0;
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
    color: #f1f5f9;
    line-height: 1.3;
  }
  :global(.chat-md-content h1) { font-size: 1.15em; }
  :global(.chat-md-content h2) { font-size: 1.05em; }
  :global(.chat-md-content h3) { font-size: 1em; }
  :global(.chat-md-content h4) { font-size: 0.95em; }
  :global(.chat-md-content a) {
    color: #a78bfa;
    text-decoration: underline;
    text-underline-offset: 2px;
  }
  :global(.chat-md-content a:hover) {
    color: #c4b5fd;
  }
  :global(.chat-md-content blockquote) {
    border-left: 2px solid #334155;
    padding-left: 0.7em;
    margin: 0.4em 0;
    color: #94a3b8;
  }
  :global(.chat-md-content hr) {
    border: none;
    border-top: 1px solid #1e293b;
    margin: 0.6em 0;
  }
  :global(.chat-md-content table) {
    border-collapse: collapse;
    margin: 0.4em 0;
    font-size: 11px;
  }
  :global(.chat-md-content th),
  :global(.chat-md-content td) {
    border: 1px solid #1e293b;
    padding: 3px 6px;
    text-align: left;
  }
  :global(.chat-md-content th) {
    background: #0a0a0f;
    font-weight: 600;
  }
</style>
