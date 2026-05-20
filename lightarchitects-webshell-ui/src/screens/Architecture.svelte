<script lang="ts">
  import { api } from '$lib/api';
  import { authHeaders } from '$lib/auth';
  import { navigate } from '$lib/routes';

  interface Props {
    params?: Record<string, string>;
  }
  let { params = {} }: Props = $props();

  // ── State ─────────────────────────────────────────────────────────────────

  type OpTab = 'extract' | 'verify' | 'render' | 'emit';
  let activeTab = $state<OpTab>('extract');

  // project root input — pre-filled from route param if present
  let projectRoot = $state<string>(
    params.project ? decodeURIComponent(params.project) : ''
  );
  let siblingId = $state<string>('');

  // render format
  let renderFormat = $state<string>('mermaid');
  const FORMATS = ['mermaid', 'd2', 'likec4', 'markdown', 'html'];

  // blocking threshold for verify
  let blockingThreshold = $state<string>('high');
  const THRESHOLDS = ['info', 'low', 'medium', 'high', 'critical'];

  // result display
  let result = $state<string | null>(null);
  let error = $state<string | null>(null);
  let loading = $state(false);
  let gatewayHealth = $state<'unknown' | 'ok' | 'error'>('unknown');

  // M15 — rate-limit client extract calls ≤1/sec
  let lastExtractMs = 0;
  const EXTRACT_DEBOUNCE_MS = 1_000;

  // planned model JSON for verify tab
  let plannedModelJson = $state<string>('{}');

  // model JSON for render tab (from last extract)
  let renderModelJson = $state<string>('{}');

  // ── Gateway proxy calls ───────────────────────────────────────────────────

  async function postDiagrams(op: string, body: unknown): Promise<string> {
    const res = await fetch(`/api/arch/${op}`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json', ...authHeaders() },
      body: JSON.stringify(body),
    });
    if (!res.ok) {
      const text = await res.text().catch(() => res.statusText);
      throw new Error(`${res.status}: ${text}`);
    }
    const data = await res.json();
    // MCP tool-result envelope: { content: [{ type: "text", text: "..." }] }
    if (data?.content?.[0]?.text) return data.content[0].text as string;
    return JSON.stringify(data, null, 2);
  }

  async function runExtract() {
    const now = Date.now();
    if (now - lastExtractMs < EXTRACT_DEBOUNCE_MS) return;
    lastExtractMs = now;

    if (!projectRoot.trim()) { error = 'Project root is required.'; return; }
    loading = true; error = null; result = null;
    try {
      const text = await postDiagrams('extract', {
        project_root: projectRoot.trim(),
        sibling_id: siblingId || undefined,
      });
      result = text;
      // Try to extract model JSON for use in render/verify tabs
      const modelMatch = text.match(/Model:\n([\s\S]+)/);
      if (modelMatch) {
        try { renderModelJson = modelMatch[1].trim(); } catch { /* ok */ }
      }
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  async function runVerify() {
    if (!projectRoot.trim()) { error = 'Project root is required.'; return; }
    let planned: unknown;
    try { planned = JSON.parse(plannedModelJson); } catch {
      error = 'Planned model JSON is invalid.'; return;
    }
    loading = true; error = null; result = null;
    try {
      result = await postDiagrams('verify', {
        project_root: projectRoot.trim(),
        planned,
        blocking_threshold: blockingThreshold,
        sibling_id: siblingId || undefined,
      });
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  async function runRender() {
    let model: unknown;
    try { model = JSON.parse(renderModelJson); } catch {
      error = 'Model JSON is invalid. Run Extract first.'; return;
    }
    loading = true; error = null; result = null;
    try {
      result = await postDiagrams('render', {
        model,
        format: renderFormat,
        sibling_id: siblingId || undefined,
      });
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  async function runEmit() {
    if (!projectRoot.trim()) { error = 'Project root is required.'; return; }
    loading = true; error = null; result = null;
    try {
      result = await postDiagrams('emit', {
        project_root: projectRoot.trim(),
        sibling_id: siblingId || undefined,
      });
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  async function checkHealth() {
    try {
      const res = await fetch('/api/arch/health', { headers: authHeaders() });
      gatewayHealth = res.ok ? 'ok' : 'error';
    } catch {
      gatewayHealth = 'error';
    }
  }

  // Check health on mount
  import { onMount } from 'svelte';
  onMount(() => { void checkHealth(); });

  function runOp() {
    if (activeTab === 'extract') void runExtract();
    else if (activeTab === 'verify') void runVerify();
    else if (activeTab === 'render') void runRender();
    else void runEmit();
  }
</script>

<!-- Architecture Intelligence screen  -->
<div class="flex flex-col h-full overflow-hidden bg-[var(--la-bg-void)] text-[var(--la-text-bright)]">
  <!-- Header row -->
  <div class="flex items-center justify-between px-4 py-2 border-b border-[#1e293b] shrink-0">
    <div class="flex items-center gap-3">
      <span class="text-[11px] font-mono text-[#FFD700] tracking-wider">Diagrams</span>
      <!-- Gateway health pill -->
      <span class="flex items-center gap-1 text-[10px] font-mono">
        <span class="w-1.5 h-1.5 rounded-full {gatewayHealth === 'ok' ? 'bg-[#22c55e]' : gatewayHealth === 'error' ? 'bg-red-500' : 'bg-[#475569]'}"></span>
        <span class="text-[#475569]">{gatewayHealth === 'ok' ? 'gateway ok' : gatewayHealth === 'error' ? 'gateway unreachable' : 'checking…'}</span>
      </span>
    </div>
    <button
      onclick={() => navigate('/dashboard')}
      class="text-[10px] text-[#475569] hover:text-[#94a3b8] transition-colors px-2 py-1"
    >← Back</button>
  </div>

  <div class="flex flex-1 overflow-hidden">
    <!-- Left: controls -->
    <div class="w-72 shrink-0 border-r border-[#1e293b] flex flex-col overflow-y-auto p-3 gap-3">

      <!-- Project root -->
      <div class="flex flex-col gap-1">
        <label class="text-[10px] text-[#64748b] font-mono uppercase">Project Root</label>
        <input
          type="text"
          bind:value={projectRoot}
          placeholder="/Users/…/myproject"
          class="w-full bg-[#0f1724] border border-[#1e293b] rounded px-2 py-1.5 text-[11px] font-mono text-[#e2e8f0] placeholder-[#334155] focus:outline-none focus:border-[#FFD700]/50"
          data-testid="arch-project-root"
        />
      </div>

      <!-- Sibling ID (optional) -->
      <div class="flex flex-col gap-1">
        <label class="text-[10px] text-[#64748b] font-mono uppercase">Sibling ID <span class="normal-case text-[#334155]">(optional)</span></label>
        <input
          type="text"
          bind:value={siblingId}
          placeholder="corso / eva / unknown"
          class="w-full bg-[#0f1724] border border-[#1e293b] rounded px-2 py-1.5 text-[11px] font-mono text-[#e2e8f0] placeholder-[#334155] focus:outline-none focus:border-[#FFD700]/50"
        />
      </div>

      <!-- Operation tabs -->
      <div class="flex flex-col gap-1">
        <label class="text-[10px] text-[#64748b] font-mono uppercase">Operation</label>
        <div class="grid grid-cols-2 gap-1">
          {#each (['extract', 'verify', 'render', 'emit'] as OpTab[]) as tab}
            <button
              onclick={() => { activeTab = tab; error = null; result = null; }}
              class="px-2 py-1.5 text-[10px] font-mono rounded border transition-colors {activeTab === tab ? 'border-[#FFD700]/60 text-[#FFD700] bg-[#FFD700]/5' : 'border-[#1e293b] text-[#475569] hover:text-[#94a3b8] hover:border-[#334155]'}"
              data-testid="arch-tab-{tab}"
            >{tab.toUpperCase()}</button>
          {/each}
        </div>
      </div>

      <!-- Tab-specific controls -->
      {#if activeTab === 'verify'}
        <div class="flex flex-col gap-1">
          <label class="text-[10px] text-[#64748b] font-mono uppercase">Blocking Threshold</label>
          <select
            bind:value={blockingThreshold}
            class="w-full bg-[#0f1724] border border-[#1e293b] rounded px-2 py-1.5 text-[11px] font-mono text-[#e2e8f0] focus:outline-none focus:border-[#FFD700]/50"
          >
            {#each THRESHOLDS as t}
              <option value={t}>{t}</option>
            {/each}
          </select>
        </div>
        <div class="flex flex-col gap-1">
          <label class="text-[10px] text-[#64748b] font-mono uppercase">Planned Model JSON</label>
          <textarea
            bind:value={plannedModelJson}
            rows={6}
            placeholder='{"{"}...{"}"}'
            class="w-full bg-[#0f1724] border border-[#1e293b] rounded px-2 py-1.5 text-[10px] font-mono text-[#e2e8f0] placeholder-[#334155] focus:outline-none focus:border-[#FFD700]/50 resize-y"
            data-testid="arch-planned-model"
          ></textarea>
        </div>
      {:else if activeTab === 'render'}
        <div class="flex flex-col gap-1">
          <label class="text-[10px] text-[#64748b] font-mono uppercase">Format</label>
          <select
            bind:value={renderFormat}
            class="w-full bg-[#0f1724] border border-[#1e293b] rounded px-2 py-1.5 text-[11px] font-mono text-[#e2e8f0] focus:outline-none focus:border-[#FFD700]/50"
            data-testid="arch-format-select"
          >
            {#each FORMATS as f}
              <option value={f}>{f}</option>
            {/each}
          </select>
        </div>
        <div class="flex flex-col gap-1">
          <label class="text-[10px] text-[#64748b] font-mono uppercase">Model JSON <span class="normal-case text-[#334155]">(from extract)</span></label>
          <textarea
            bind:value={renderModelJson}
            rows={6}
            class="w-full bg-[#0f1724] border border-[#1e293b] rounded px-2 py-1.5 text-[10px] font-mono text-[#e2e8f0] focus:outline-none focus:border-[#FFD700]/50 resize-y"
            data-testid="arch-model-json"
          ></textarea>
        </div>
      {/if}

      <!-- Run button -->
      <button
        onclick={runOp}
        disabled={loading}
        class="mt-auto w-full py-2 rounded text-[11px] font-mono font-medium tracking-wider transition-all {loading ? 'bg-[#1e293b] text-[#475569] cursor-not-allowed' : 'bg-[#FFD700]/10 text-[#FFD700] border border-[#FFD700]/30 hover:bg-[#FFD700]/20 hover:shadow-[0_0_8px_rgba(255,215,0,0.2)]'}"
        data-testid="arch-run-btn"
      >
        {#if loading}
          <span class="flex items-center justify-center gap-2">
            <span class="w-3 h-3 border border-[#FFD700] border-t-transparent rounded-full animate-spin"></span>
            Running…
          </span>
        {:else}
          RUN {activeTab.toUpperCase()}
        {/if}
      </button>

      <!-- Health re-check -->
      <button
        onclick={checkHealth}
        class="w-full py-1 text-[10px] font-mono text-[#334155] hover:text-[#64748b] transition-colors"
        data-testid="arch-health-btn"
      >re-check gateway</button>
    </div>

    <!-- Right: result panel -->
    <div class="flex-1 overflow-auto p-4" data-testid="arch-result-panel">
      {#if error}
        <div class="mb-3 px-3 py-2 rounded bg-red-900/20 border border-red-800/40 text-red-400 text-[11px] font-mono" data-testid="arch-error">
          {error}
        </div>
      {/if}

      {#if result}
        <pre class="text-[10px] font-mono text-[#94a3b8] whitespace-pre-wrap break-all leading-relaxed" data-testid="arch-result">{result}</pre>
      {:else if !loading && !error}
        <div class="flex flex-col items-center justify-center h-full gap-2 opacity-40">
          <div class="text-[10px] font-mono text-[#475569]">
            {#if activeTab === 'extract'}
              Enter a project root and run EXTRACT to analyse the architecture.
            {:else if activeTab === 'verify'}
              Run EXTRACT first to get a current model, then paste a planned model and run VERIFY.
            {:else if activeTab === 'render'}
              Run EXTRACT first, then choose a format and run RENDER.
            {:else}
              Enter a project root and run EMIT to generate all diagram formats.
            {/if}
          </div>
        </div>
      {/if}
    </div>
  </div>
</div>
