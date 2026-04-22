<script lang="ts">
  import {
    copilotMessages, copilotLoading, currentBuildId, activeBuild,
    findings, logEntries, selectedPillar, focusedSibling, spikeSibling,
    buildBuildContext, authProfile, ollamaConfig, terminalConnected,
  } from '$lib/stores';
  import { SIBLING_COLORS } from '$lib/design-tokens';
  import { api } from '$lib/api';
  import { parseCommand, SLASH_COMMANDS } from '$lib/commands';
  import { connectSSE, disconnectSSE } from '$lib/sse';
  import { TerminalWS } from '$lib/ws';
  import SiblingDispatch from '$lib/../components/SiblingDispatch.svelte';
  import PolytopeDecor from '$lib/../components/PolytopeDecor.svelte';
  import OllamaConfigModal from '$lib/../components/OllamaConfigModal.svelte';
  import SettingsOverlay from '$lib/../components/SettingsOverlay.svelte';
  import { settingsOpen } from '$lib/setup';
  import type { CopilotMessage, SiblingId } from '$lib/types';
  import { Terminal } from '@xterm/xterm';
  import { FitAddon } from '@xterm/addon-fit';

  // --- TERMINAL mode state ---
  let mode = $state<'chat' | 'terminal'>('chat');
  let terminalEl: HTMLDivElement | undefined = $state();
  let sharedBuildId = $state<string | null>(null);
  let cwd = $state('/tmp');
  let connecting = $state(false);
  let connectError = $state<string | null>(null);
  let showOllamaModal = $state(false);

  async function ensureBuild(): Promise<string | null> {
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
    const resp = await api.createBuild(body) as { build_id: string };
    sharedBuildId = resp.build_id;
    currentBuildId.set(resp.build_id);
    return resp.build_id;
  }

  // Phase 20 — shared SSE subscription tied to sharedBuildId
  $effect(() => {
    if (!sharedBuildId) return;
    connectSSE(sharedBuildId);
    return () => disconnectSSE();
  });

  async function connectTerminal() {
    connecting = true;
    connectError = null;
    try {
      await ensureBuild();
    } catch (err) {
      connectError = err instanceof Error ? err.message : 'Failed to create build';
    } finally {
      connecting = false;
    }
  }

  // Mount/teardown xterm.js when TERMINAL mode is active and a build is connected.
  $effect(() => {
    if (mode !== 'terminal' || !terminalEl || !sharedBuildId) return;

    const term = new Terminal({
      cursorBlink: true,
      fontSize: 13,
      fontFamily: 'monospace',
      theme: {
        background: '#0a0a0a',
        foreground: '#e2e8f0',
        cursor: '#FFD700',
        selectionBackground: '#FFD70044',
      },
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

    const ro = new ResizeObserver(() => {
      fitAddon.fit();
      ws.sendResize(term.cols, term.rows);
    });
    ro.observe(terminalEl);

    return () => {
      ws.disconnect();
      term.dispose();
      ro.disconnect();
      terminalConnected.set(false);
    };
  });

  let input = $state('');
  let showSuggestions = $state(false);
  let messagesEl: HTMLDivElement | undefined = $state();

  // Derived: matching slash commands for autocomplete
  let matchingCommands = $derived(
    input.startsWith('/')
      ? SLASH_COMMANDS.filter(c =>
          c.name.startsWith(input.slice(1).toLowerCase()) ||
          c.alias?.some(a => a.startsWith(input.slice(1).toLowerCase()))
        ).slice(0, 8)
      : []
  );

  // Derived: context string for current build
  let contextString = $derived(
    buildBuildContext(
      $activeBuild,
      $selectedPillar,
      $activeBuild
        ? $findings.filter(f => f.buildId === $activeBuild!.id)
        : [],
    )
  );

  function addMessage(role: CopilotMessage['role'], content: string, sibling?: SiblingId) {
    const msg: CopilotMessage = {
      id: crypto.randomUUID(),
      role,
      content,
      sibling,
      timestamp: new Date().toISOString(),
    };
    copilotMessages.update(m => [...m, msg]);
    return msg;
  }

  // Mock streaming: simulates an assistant response character by character
  function mockStream(content: string, sibling?: SiblingId) {
    const msg: CopilotMessage = {
      id: crypto.randomUUID(),
      role: 'assistant',
      content: '',
      sibling,
      timestamp: new Date().toISOString(),
    };
    copilotMessages.update(m => [...m, msg]);
    copilotLoading.set(true);

    let i = 0;
    const interval = setInterval(() => {
      if (i < content.length) {
        const chunk = content.slice(0, i + 1);
        copilotMessages.update(msgs => {
          const updated = [...msgs];
          const last = updated[updated.length - 1];
          if (last && last.role === 'assistant') {
            updated[updated.length - 1] = { ...last, content: chunk };
          }
          return updated;
        });
        i++;
      } else {
        clearInterval(interval);
        copilotLoading.set(false);
      }
    }, 15);

    return msg;
  }

  async function sendMessage() {
    const text = input.trim();
    if (!text) return;
    input = '';
    showSuggestions = false;

    // Check for slash command
    const { command, args } = parseCommand(text);
    if (command) {
      addMessage('system', `/${command.name} ${args}`.trim());

      // Special: /clear resets chat
      if (command.name === 'clear') {
        copilotMessages.set([]);
        return;
      }

      try {
        await command.execute(args);
        addMessage('assistant',
          `Dispatched /${command.name}${args ? ` with "${args}"` : ''}. ` +
          `Context: ${$currentBuildId ? `build ${$currentBuildId}` : 'no active build'}.`,
        );
      } catch (err) {
        addMessage('system', `Error: ${err instanceof Error ? err.message : 'Unknown error'}`);
      }
      return;
    }

    // Regular message: add to chat and try API
    addMessage('user', text);
    copilotLoading.set(true);

    let buildId: string | null = null;
    try {
      buildId = await ensureBuild();
    } catch {
      mockStream('Could not create build session. Is the webshell running?');
      copilotLoading.set(false);
      return;
    }

    try {
      const contextMsg = `[Context]\n${contextString}\n\n[User]\n${text}`;
      const result = await api.copilotChat(buildId!, contextMsg);
      const response = typeof result === 'object' && result !== null && 'response' in result
        ? String((result as Record<string, unknown>).response)
        : 'No response from provider.';

      // If SSE already delivered a copilot_response (backend streams), skip.
      // Otherwise inject the single-shot JSON response directly.
      if ($copilotLoading) {
        addMessage('assistant', response);
        copilotLoading.set(false);
      }
    } catch {
      // Offline fallback
      mockStream('Could not reach AI provider. Check webshell logs.');
    }
  }

  function selectCommand(name: string) {
    input = `/${name} `;
    showSuggestions = false;
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      sendMessage();
    }
    if (e.key === 'Escape') {
      showSuggestions = false;
    }
  }

  function handleInput() {
    showSuggestions = input.startsWith('/');
  }

  function handleDispatch(sibling: SiblingId, prompt?: string) {
    focusedSibling.set(sibling);
    spikeSibling(sibling);

    const buildId = $currentBuildId;
    const dispatchMsg = prompt
      ? `Dispatching ${sibling.toUpperCase()} with prompt: "${prompt}"`
      : `Dispatching ${sibling.toUpperCase()}`;

    addMessage('system', dispatchMsg);

    if (buildId) {
      api.dispatchSibling(buildId, sibling, sibling, prompt ?? '').catch(() => {
        // Backend unavailable — mock response
      });
    }

    addMessage('assistant',
      `${sibling.toUpperCase()} sibling activated. ` +
      `${buildId ? `Operating on build ${buildId}` : 'No active build — standing by for assignment.'} ` +
      `I'll coordinate the ${sibling} cycle and report findings here.`,
      sibling,
    );
  }

  function clearChat() {
    copilotMessages.set([]);
  }

  // Auto-scroll on new messages
  $effect(() => {
    $copilotMessages;
    if (messagesEl) {
      requestAnimationFrame(() => {
        if (messagesEl) messagesEl.scrollTop = messagesEl.scrollHeight;
      });
    }
  });
</script>

<div class="h-full flex flex-col relative overflow-hidden">
  <!-- Ambient polytope decoration -->
  <div class="absolute inset-0 overflow-hidden pointer-events-none -z-10">
    <div class="absolute -top-20 -right-20">
      <PolytopeDecor type="rectified5cell" color="#FF1493" size={350} opacity={0.03} speed={0.06} />
    </div>
    <div class="absolute -bottom-20 -left-20">
      <PolytopeDecor type="tesseract" color="#FF6D00" size={300} opacity={0.03} speed={0.07} />
    </div>
  </div>

  <header class="flex items-center gap-3 px-6 py-3 border-b border-[#1e293b]">
    <h1 class="text-lg font-semibold">Copilot</h1>
    <!-- Mode toggle -->
    <div class="flex rounded overflow-hidden border border-[#1e293b]">
      <button
        onclick={() => { mode = 'chat'; }}
        class="text-[10px] px-3 py-1 transition-colors {mode === 'chat' ? 'bg-[#FFD700] text-white' : 'bg-[#111827] text-[#64748b] hover:text-[#e2e8f0]'}"
      >CHAT</button>
      <button
        onclick={() => { mode = 'terminal'; }}
        class="text-[10px] px-3 py-1 transition-colors {mode === 'terminal' ? 'bg-[#FFD700] text-white' : 'bg-[#111827] text-[#64748b] hover:text-[#e2e8f0]'}"
      >TERMINAL</button>
    </div>
    {#if mode === 'chat'}
      <span class="hidden sm:inline text-xs text-[#64748b]">AI Assistant · Routes to sibling via orchestrator</span>
    {:else}
      <span class="hidden sm:inline text-xs text-[#64748b]">PTY · Claude Code in a real terminal</span>
    {/if}
    <div class="flex-1"></div>
    {#if mode === 'chat'}
      <button
        onclick={clearChat}
        class="text-[10px] text-[#475569] hover:text-[#e2e8f0] px-2 py-1 rounded border border-[#1e293b] hover:border-[#334155] transition-colors"
      >
        Clear
      </button>
    {/if}

    <div style="position:relative;">
      <button
        onclick={() => settingsOpen.update(v => !v)}
        class="text-[10px] text-[#475569] hover:text-[#94a3b8] px-2 py-1 rounded border border-[#1e293b] transition-colors"
        title="Switch backend / model"
      >⚙</button>
      {#if $settingsOpen}
        <SettingsOverlay />
      {/if}
    </div>
  </header>

  <!-- ── TERMINAL MODE ──────────────────────────────────── -->
  {#if mode === 'terminal'}
    <div class="flex-1 flex flex-col overflow-hidden">
      <!-- Controls bar -->
      {#if !sharedBuildId}
        <div class="flex items-center flex-wrap gap-2 px-4 py-3 border-b border-[#1e293b] bg-[#0f172a]">
          <!-- Auth profile selector -->
          <span class="text-[10px] text-[#64748b]">Profile</span>
          <select
            bind:value={$authProfile}
            class="bg-[#111827] border border-[#1e293b] rounded px-2 py-1 text-[11px] text-[#e2e8f0] outline-none focus:border-[#FFD700] transition-colors"
          >
            <option value="anthropic">Anthropic</option>
            <option value="ollama">Ollama Cloud</option>
          </select>
          {#if $authProfile === 'ollama'}
            <button
              onclick={() => { showOllamaModal = true; }}
              class="text-[10px] text-[#6366F1] hover:text-[#818CF8] transition-colors"
              title="Configure Ollama"
            >⚙</button>
          {/if}
          <!-- Working directory -->
          <span class="text-[10px] text-[#64748b]">CWD</span>
          <input
            type="text"
            bind:value={cwd}
            placeholder="/tmp"
            class="w-32 sm:w-48 bg-[#111827] border border-[#1e293b] rounded px-2 py-1 text-[11px] text-[#e2e8f0] placeholder-[#475569] outline-none focus:border-[#FFD700] transition-colors"
          />
          <button
            onclick={connectTerminal}
            disabled={connecting}
            class="px-3 py-1 bg-[#FFD700] text-white text-[11px] rounded hover:bg-[#D4A017] disabled:opacity-50 transition-colors"
          >
            {connecting ? 'Connecting…' : 'Connect'}
          </button>
          {#if connectError}
            <span class="text-[10px] text-red-400">{connectError}</span>
          {/if}
        </div>
      {:else}
        <div class="flex items-center gap-2 px-4 py-2 border-b border-[#1e293b] bg-[#0f172a]">
          <div class="w-2 h-2 rounded-full bg-green-500" style="box-shadow: 0 0 4px #22c55e"></div>
          <span class="text-[10px] text-[#64748b] font-mono">build {sharedBuildId.slice(0, 8)}… · {cwd}</span>
          <div class="flex-1"></div>
          <button
            onclick={() => { sharedBuildId = null; terminalConnected.set(false); }}
            class="text-[10px] text-[#475569] hover:text-red-400 transition-colors"
          >Disconnect</button>
        </div>
      {/if}

      <!-- xterm.js mount point -->
      <div
        bind:this={terminalEl}
        class="flex-1 overflow-hidden bg-[#0a0a0a]"
        style="font-family: monospace;"
      ></div>
    </div>
  {:else}

  <!-- ── CHAT MODE ──────────────────────────────────────── -->
  <div class="flex-1 flex overflow-hidden">
    <!-- Chat panel -->
    <div class="flex-1 flex flex-col">
      <!-- Messages -->
      <div bind:this={messagesEl} class="flex-1 overflow-y-auto p-4 space-y-3" role="log" aria-label="Chat messages" aria-live="polite">
        {#if $copilotMessages.length === 0}
          <div class="flex flex-col items-center justify-center h-full text-[#475569]">
            <p class="text-sm mb-1">Start a conversation</p>
            <p class="text-xs">Use <kbd class="bg-[#1e293b] px-1.5 py-0.5 rounded">/</kbd> for slash commands or type a question</p>
            <div class="flex flex-wrap gap-1.5 mt-3">
              {#each ['/build', '/secure', '/research', '/deploy', '/quality', '/clear'] as cmd}
                <button
                  onclick={() => { input = cmd + ' '; }}
                  class="text-[10px] px-2 py-1 rounded bg-[#111827] border border-[#1e293b] hover:border-[#334155] transition-colors"
                >
                  {cmd}
                </button>
              {/each}
            </div>
          </div>
        {:else}
          {#each $copilotMessages as msg (msg.id)}
            <div class="flex {msg.role === 'user' ? 'justify-end' : msg.role === 'system' ? 'justify-center' : 'justify-start'}">
              <div
                class="max-w-[80%] px-3 py-2 rounded-lg text-sm
                  {msg.role === 'user' ? 'bg-[#FFD700] text-white' :
                    msg.role === 'system' ? 'bg-[#1e293b]/50 text-[#64748b] text-xs border border-[#1e293b]' :
                    'bg-[#111827] border border-[#1e293b] text-[#e2e8f0]'}"
              >
                {#if msg.sibling}
                  {@const color = SIBLING_COLORS[msg.sibling] ?? '#6b7280'}
                  <span class="text-[10px] font-medium" style="color: {color}">{msg.sibling.toUpperCase()}</span>
                  <span class="text-[#475569] mx-1">·</span>
                {/if}
                {msg.content}
              </div>
            </div>
          {/each}
          {#if $copilotLoading}
            <div class="flex justify-start">
              <div class="bg-[#111827] border border-[#1e293b] px-3 py-2 rounded-lg">
                <span class="text-[#475569] text-sm animate-pulse">Thinking…</span>
              </div>
            </div>
          {/if}
        {/if}
      </div>

      <!-- Input area with autocomplete -->
      <div class="border-t border-[#1e293b] p-4 relative">
        {#if showSuggestions && matchingCommands.length > 0}
          <div class="absolute bottom-full left-4 right-4 mb-2 bg-[#0a0a0a] border border-[#1e293b] rounded-lg overflow-hidden shadow-xl z-10 max-h-60 overflow-y-auto">
            {#each matchingCommands as cmd}
              <button
                class="w-full text-left px-3 py-2 text-xs hover:bg-[#1e293b] transition-colors flex items-baseline gap-2"
                onclick={() => selectCommand(cmd.name)}
              >
                <span class="text-[#FFD700] font-mono">/{cmd.name}</span>
                {#if cmd.alias}
                  <span class="text-[#475569]">({cmd.alias.join(', ')})</span>
                {/if}
                <span class="text-[#64748b] flex-1">{cmd.description}</span>
                {#if cmd.args}
                  <span class="text-[#334155]">{cmd.args}</span>
                {/if}
              </button>
            {/each}
          </div>
        {/if}

        <div class="flex gap-2">
          <input
            type="text"
            bind:value={input}
            onkeydown={handleKeydown}
            oninput={handleInput}
            onfocus={() => { if (input.startsWith('/')) showSuggestions = true; }}
            onblur={() => { setTimeout(() => showSuggestions = false, 200); }}
            placeholder="Type a message or /command…"
            class="flex-1 bg-[#111827] border border-[#1e293b] rounded px-3 py-2 text-sm text-[#e2e8f0] placeholder-[#475569] outline-none focus:border-[#FFD700] transition-colors"
          />
          <button
            onclick={sendMessage}
            disabled={$copilotLoading}
            class="px-4 py-2 bg-[#FFD700] text-white text-sm rounded hover:bg-[#D4A017] disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
          >
            Send
          </button>
        </div>

        {#if $activeBuild}
          <p class="text-[9px] text-[#334155] mt-1">
            Context: {$activeBuild.name} · {$activeBuild.metaSkill} · Pillar: {$selectedPillar ?? 'none'}
          </p>
        {:else}
          <p class="text-[9px] text-[#334155] mt-1">No active build — context injection disabled</p>
        {/if}
      </div>
    </div>

    <!-- Sibling dispatch sidebar -->
    <div class="w-[220px] border-l border-[#1e293b] p-4 flex flex-col gap-4 hidden lg:flex">
      <div>
        <h3 class="text-xs font-medium text-[#64748b] mb-3">DISPATCH</h3>
        <SiblingDispatch
          onDispatch={handleDispatch}
          selectedSibling={$focusedSibling}
        />
      </div>

      <div>
        <h3 class="text-xs font-medium text-[#64748b] mb-2">CONTEXT</h3>
        <pre class="text-[9px] text-[#475569] bg-[#0a0a0a] border border-[#1e293b] rounded p-2 whitespace-pre-wrap max-h-40 overflow-y-auto">{contextString}</pre>
      </div>

      <div>
        <h3 class="text-xs font-medium text-[#64748b] mb-2">QUICK COMMANDS</h3>
        <div class="space-y-1">
          {#each ['/build', '/secure', '/research', '/review', '/observe'] as cmd}
            <button
              onclick={() => { input = cmd + ' '; }}
              class="w-full text-left text-[10px] px-2 py-1 rounded bg-[#111827] border border-[#1e293b] hover:border-[#334155] text-[#94a3b8] transition-colors"
            >
              {cmd}
            </button>
          {/each}
        </div>
      </div>
    </div>
  </div>
  {/if}
</div>

<OllamaConfigModal isOpen={showOllamaModal} onClose={() => { showOllamaModal = false; }} />