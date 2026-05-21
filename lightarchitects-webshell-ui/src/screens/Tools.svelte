<script lang="ts">
  import { onMount } from 'svelte';
  import { authHeaders } from '$lib/auth';
  import McpToolForm from '$lib/components/McpToolForm.svelte';
  import { listMcpServers, listMcpTools, type McpServerStatus, type McpTool } from '$lib/mcp-client';

  // §O Tool Surface Parity — 4 panels covering MCP servers, squad agents, workspaces, meta-skills.
  // Check 1 + 6: MCP Servers panel shows gap labels for non-invocable servers.
  // Check 3: Meta-skills panel surfaces available meta-skills.
  // Check 4: tab navigable without re-auth (auth headers reused from session).

  interface McpServer {
    id: string;
    name: string;
    status: string;
    command: string | null;
    tool_count: number | null;
    webshell_supported: boolean;
    gap_label: string | null;
  }

  interface SquadAgent {
    id: string;
    status: string;
    binary_present: boolean;
    last_activity: number | null;
  }

  interface Workspace {
    id: string;
    path: string;
    name: string;
  }

  interface MetaSkill {
    id: string;
    label: string;
    description: string;
  }

  let mcpServers      = $state<McpServer[]>([]);
  let squadAgents     = $state<SquadAgent[]>([]);
  let workspaces      = $state<Workspace[]>([]);
  let metaSkills      = $state<MetaSkill[]>([]);
  let loading         = $state(true);
  let mcpError        = $state<string | null>(null);
  let squadError      = $state<string | null>(null);
  let workspacesError = $state<string | null>(null);
  let skillsError     = $state<string | null>(null);

  // MCP Proxy Tools panel state
  let proxyServers    = $state<McpServerStatus[]>([]);
  let proxyTools      = $state<McpTool[]>([]);
  let proxyError      = $state<string | null>(null);
  let activeTool      = $state<McpTool | null>(null);
  let invokeResult    = $state<unknown>(null);
  let filterServer    = $state('');

  async function fetchAll() {
    const h = authHeaders();
    const [mcp, squad, ws, skills, proxySrv, proxyTls] = await Promise.allSettled([
      fetch('/api/mcp-servers', { headers: h }).then(r => r.json()),
      fetch('/api/siblings',    { headers: h }).then(r => r.json()),
      fetch('/api/workspaces',  { headers: h }).then(r => r.json()),
      fetch('/api/meta-skills', { headers: h }).then(r => r.json()),
      listMcpServers(),
      listMcpTools(),
    ]);

    if (mcp.status    === 'fulfilled') mcpServers  = mcp.value;
    else mcpError        = 'MCP servers unavailable';

    if (squad.status  === 'fulfilled') squadAgents = squad.value;
    else squadError      = 'Squad agents unavailable';

    if (ws.status     === 'fulfilled') workspaces  = ws.value;
    else workspacesError = 'Workspaces unavailable';

    if (skills.status === 'fulfilled') metaSkills  = skills.value;
    else skillsError     = 'Meta-skills unavailable';

    if (proxySrv.status === 'fulfilled') proxyServers = proxySrv.value;
    else proxyError = 'MCP proxy not configured — place ~/.lightarchitects/webshell-mcp.json to enable.';

    if (proxyTls.status === 'fulfilled') proxyTools = proxyTls.value;

    loading = false;
  }

  const visibleTools = $derived(
    filterServer ? proxyTools.filter(t => t.server === filterServer) : proxyTools,
  );

  onMount(fetchAll);
</script>

<div class="tools-screen">
  <header class="tools-header">
    <h1>Available Tools</h1>
    <p class="subtitle">Platform capability inventory — MCP servers, agents, workspaces, skills</p>
  </header>

  {#if loading}
    <div class="loading" role="status">Loading tool surface…</div>
  {:else}
    <div class="panels">

      <!-- Panel 1: MCP Servers — §O Checks 1 + 2 + 6 -->
      <section class="panel" aria-label="MCP Servers">
        <h2>MCP Servers</h2>
        {#if mcpError}
          <p class="panel-error" role="alert">{mcpError}</p>
        {/if}
        {#each mcpServers as server (server.id)}
          <div class="server-row" data-supported={server.webshell_supported}>
            <span class="server-name">{server.name}</span>
            <span class="server-status status-{server.status}">{server.status}</span>
            {#if server.tool_count !== null}
              <span class="tool-count">{server.tool_count} tools</span>
            {/if}
            {#if server.webshell_supported}
              <span class="supported-label">invocable via dispatch</span>
            {:else}
              <span class="gap-label" role="status">{server.gap_label}</span>
            {/if}
          </div>
        {:else}
          <p class="empty">No MCP servers configured in ~/.claude/mcp.json</p>
        {/each}
      </section>

      <!-- Panel 2: Squad Agents — agent tool surface -->
      <section class="panel" aria-label="Squad Agents">
        <h2>Squad Agents</h2>
        {#if squadError}
          <p class="panel-error" role="alert">{squadError}</p>
        {/if}
        {#each squadAgents as agent (agent.id)}
          <div class="agent-row">
            <span class="agent-id">{agent.id}</span>
            <span class="agent-status status-{agent.status}">{agent.status}</span>
            {#if !agent.binary_present}
              <span class="gap-label">binary missing</span>
            {/if}
          </div>
        {:else}
          <p class="empty">No agents registered</p>
        {/each}
      </section>

      <!-- Panel 3: Workspaces -->
      <section class="panel" aria-label="Workspaces">
        <h2>Workspaces</h2>
        {#if workspacesError}
          <p class="panel-error" role="alert">{workspacesError}</p>
        {/if}
        {#each workspaces as ws (ws.id)}
          <div class="workspace-row">
            <span class="ws-name">{ws.name}</span>
            <span class="ws-path">{ws.path}</span>
          </div>
        {:else}
          <p class="empty">No workspaces found in ~/Projects/</p>
        {/each}
      </section>

      <!-- Panel 4: Meta-skills — §O Check 3 -->
      <section class="panel" aria-label="Meta-skills">
        <h2>Meta-skills</h2>
        {#if skillsError}
          <p class="panel-error" role="alert">{skillsError}</p>
        {/if}
        {#each metaSkills as skill (skill.id)}
          <div class="skill-row">
            <span class="skill-id">{skill.id}</span>
            <span class="skill-label">{skill.label}</span>
            <span class="skill-desc">{skill.description}</span>
          </div>
        {:else}
          <p class="empty">No meta-skills registered</p>
        {/each}
      </section>

      <!-- Panel 5: MCP Proxy Tools — invoke tools on managed stdio servers -->
      <section class="panel panel-wide" aria-label="MCP Proxy Tools">
        <h2>MCP Proxy Tools</h2>
        {#if proxyError}
          <p class="panel-notice" role="status">{proxyError}</p>
        {:else}
          {#if proxyServers.length > 0}
            <div class="proxy-server-bar">
              <select
                class="server-filter"
                aria-label="Filter by server"
                bind:value={filterServer}
              >
                <option value="">All servers</option>
                {#each proxyServers as srv (srv.name)}
                  <option value={srv.name}>{srv.name} ({srv.state})</option>
                {/each}
              </select>
            </div>
          {/if}
          {#if visibleTools.length === 0}
            <p class="empty">No tools available — servers may still be starting.</p>
          {:else}
            <ul class="tool-list" role="list">
              {#each visibleTools as tool (tool.server + '/' + tool.name)}
                <li class="tool-item">
                  <button
                    class="tool-btn"
                    onclick={() => { activeTool = tool; invokeResult = null; }}
                  >
                    <span class="tool-server">{tool.server}</span>
                    <span class="tool-tool-name">{tool.name}</span>
                    <span class="tool-desc-short">{tool.description}</span>
                  </button>
                </li>
              {/each}
            </ul>
          {/if}
        {/if}
      </section>

    </div>
  {/if}
</div>

<!-- MCP tool invocation modal -->
{#if activeTool !== null}
  <div
    class="modal-backdrop"
    role="presentation"
    onclick={() => { activeTool = null; invokeResult = null; }}
    onkeydown={(e) => { if (e.key === 'Escape') { activeTool = null; invokeResult = null; } }}
  >
    <div
      class="modal-content"
      role="none"
      onclick={(e) => e.stopPropagation()}
    >
      {#if invokeResult !== null}
        <div class="result-panel">
          <header class="result-header">
            <span class="result-title">Result — {activeTool.name}</span>
            <button
              class="close-btn"
              aria-label="Close result"
              onclick={() => { activeTool = null; invokeResult = null; }}
            >✕</button>
          </header>
          <pre class="result-pre">{JSON.stringify(invokeResult, null, 2)}</pre>
          <button
            class="btn-back"
            onclick={() => { invokeResult = null; }}
          >← Back to form</button>
        </div>
      {:else}
        <McpToolForm
          server={activeTool.server}
          toolName={activeTool.name}
          description={activeTool.description}
          inputSchema={null}
          oncancel={() => { activeTool = null; invokeResult = null; }}
          onsuccess={(out) => { invokeResult = out; }}
        />
      {/if}
    </div>
  </div>
{/if}

<style>
  .tools-screen {
    padding: 1.5rem;
    max-width: 1200px;
    margin: 0 auto;
  }

  .tools-header {
    margin-bottom: 1.5rem;
  }

  .tools-header h1 {
    font-size: 1.25rem;
    font-weight: 600;
    margin: 0;
  }

  .subtitle {
    color: var(--text-muted, #888);
    font-size: 0.875rem;
    margin: 0.25rem 0 0;
  }

  .panels {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(340px, 1fr));
    gap: 1rem;
  }

  .panel {
    background: var(--surface-1, #1a1a1a);
    border: 1px solid var(--border, #333);
    border-radius: 6px;
    padding: 1rem;
  }

  .panel h2 {
    font-size: 0.875rem;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    margin: 0 0 0.75rem;
    color: var(--text-muted, #888);
  }

  .server-row,
  .agent-row,
  .workspace-row,
  .skill-row {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0.375rem 0;
    border-bottom: 1px solid var(--border-subtle, #222);
    font-size: 0.875rem;
  }

  .server-row:last-child,
  .agent-row:last-child,
  .workspace-row:last-child,
  .skill-row:last-child {
    border-bottom: none;
  }

  .gap-label {
    margin-left: auto;
    font-size: 0.75rem;
    color: var(--warning, #f59e0b);
    background: var(--warning-bg, rgba(245, 158, 11, 0.1));
    padding: 0.125rem 0.5rem;
    border-radius: 3px;
    white-space: nowrap;
  }

  .supported-label {
    margin-left: auto;
    font-size: 0.75rem;
    color: var(--success, #22c55e);
    white-space: nowrap;
  }

  .status-online,
  .status-configured {
    color: var(--success, #22c55e);
    font-size: 0.75rem;
  }

  .status-active {
    color: var(--info, #60a5fa);
    font-size: 0.75rem;
  }

  .status-offline,
  .status-error {
    color: var(--danger, #ef4444);
    font-size: 0.75rem;
  }

  .ws-path {
    color: var(--text-muted, #888);
    font-size: 0.75rem;
    font-family: monospace;
    margin-left: auto;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 180px;
  }

  .skill-id {
    font-family: monospace;
    color: var(--accent, #818cf8);
    min-width: 80px;
    flex-shrink: 0;
  }

  .skill-desc {
    color: var(--text-muted, #888);
    font-size: 0.75rem;
    margin-left: auto;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 160px;
  }

  .empty {
    color: var(--text-muted, #888);
    font-size: 0.875rem;
    font-style: italic;
    margin: 0;
  }

  .loading {
    color: var(--text-muted, #888);
    font-size: 0.875rem;
    padding: 2rem;
    text-align: center;
  }

  .panel-error {
    color: var(--danger, #ef4444);
    font-size: 0.8rem;
    font-style: italic;
    margin: 0 0 0.5rem;
  }

  /* Panel 5 — MCP Proxy Tools */
  .panel-wide {
    grid-column: 1 / -1;
  }

  .panel-notice {
    font-size: 0.8rem;
    color: var(--text-muted, #888);
    font-style: italic;
    margin: 0;
  }

  .proxy-server-bar {
    margin-bottom: 0.75rem;
  }

  .server-filter {
    font-size: 0.78rem;
    background: var(--surface-1, #1a1a1a);
    border: 1px solid var(--border, #333);
    border-radius: 4px;
    color: inherit;
    padding: 4px 8px;
    font-family: inherit;
    outline: none;
  }

  .server-filter:focus {
    border-color: var(--accent, #818cf8);
  }

  .tool-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(220px, 1fr));
    gap: 0.5rem;
  }

  .tool-btn {
    width: 100%;
    text-align: left;
    background: var(--surface-2, #262626);
    border: 1px solid var(--border, #333);
    border-radius: 5px;
    padding: 10px 12px;
    cursor: pointer;
    color: inherit;
    font-family: inherit;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .tool-btn:hover {
    border-color: var(--accent, #818cf8);
  }

  .tool-server {
    font-size: 0.65rem;
    color: var(--accent, #818cf8);
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }

  .tool-tool-name {
    font-size: 0.82rem;
    font-weight: 600;
    color: var(--text, #e4e4e7);
  }

  .tool-desc-short {
    font-size: 0.72rem;
    color: var(--text-muted, #888);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 100%;
  }

  /* Modal */
  .modal-backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.65);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 200;
    padding: 1rem;
  }

  .modal-content {
    max-width: 540px;
    width: 100%;
    max-height: calc(100vh - 2rem);
    overflow-y: auto;
  }

  /* Invoke result panel */
  .result-panel {
    background: var(--surface-1, #1a1a1a);
    border: 1px solid var(--border, #333);
    border-radius: 8px;
    padding: 16px;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .result-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }

  .result-title {
    font-size: 0.85rem;
    font-weight: 600;
    color: var(--success, #22c55e);
  }

  .close-btn {
    background: none;
    border: none;
    cursor: pointer;
    color: var(--text-muted, #888);
    font-size: 0.9rem;
    padding: 2px 6px;
    border-radius: 4px;
    font-family: inherit;
  }

  .close-btn:hover {
    color: var(--text, #e4e4e7);
    background: var(--surface-2, #262626);
  }

  .result-pre {
    font-family: 'JetBrains Mono Variable', monospace;
    font-size: 0.75rem;
    background: var(--surface-2, #262626);
    border: 1px solid var(--border-subtle, #222);
    border-radius: 4px;
    padding: 10px;
    margin: 0;
    overflow: auto;
    max-height: 320px;
    white-space: pre-wrap;
    word-break: break-all;
    color: var(--text, #e4e4e7);
  }

  .btn-back {
    background: none;
    border: none;
    cursor: pointer;
    color: var(--accent, #818cf8);
    font-size: 0.78rem;
    font-family: inherit;
    padding: 0;
    align-self: flex-start;
  }

  .btn-back:hover {
    text-decoration: underline;
  }

  @media (max-width: 767px) {
    .panels {
      grid-template-columns: 1fr;
    }
    .panel-wide {
      grid-column: 1;
    }
  }
</style>
