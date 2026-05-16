<script lang="ts">
  import { onMount } from 'svelte';
  import { authHeaders } from '$lib/auth';

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

  let mcpServers = $state<McpServer[]>([]);
  let squadAgents = $state<SquadAgent[]>([]);
  let workspaces  = $state<Workspace[]>([]);
  let metaSkills  = $state<MetaSkill[]>([]);
  let loading     = $state(true);
  let fetchError  = $state<string | null>(null);

  async function fetchAll() {
    try {
      const h = authHeaders();
      const [mcp, squad, ws, skills] = await Promise.all([
        fetch('/api/mcp-servers',  { headers: h }).then(r => r.json()),
        fetch('/api/siblings',     { headers: h }).then(r => r.json()),
        fetch('/api/workspaces',   { headers: h }).then(r => r.json()),
        fetch('/api/meta-skills',  { headers: h }).then(r => r.json()),
      ]);
      mcpServers  = mcp;
      squadAgents = squad;
      workspaces  = ws;
      metaSkills  = skills;
    } catch (e) {
      fetchError = e instanceof Error ? e.message : 'Failed to load tool surface';
    } finally {
      loading = false;
    }
  }

  onMount(fetchAll);
</script>

<div class="tools-screen">
  <header class="tools-header">
    <h1>Tool Surface</h1>
    <p class="subtitle">Platform capability inventory — MCP servers, agents, workspaces, skills</p>
  </header>

  {#if loading}
    <div class="loading" role="status">Loading tool surface…</div>
  {:else if fetchError}
    <div class="fetch-error" role="alert">{fetchError}</div>
  {:else}
    <div class="panels">

      <!-- Panel 1: MCP Servers — §O Checks 1 + 6 -->
      <section class="panel" aria-label="MCP Servers">
        <h2>MCP Servers</h2>
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

    </div>
  {/if}
</div>

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

  .fetch-error {
    color: var(--danger, #ef4444);
    font-size: 0.875rem;
    padding: 1rem;
    border: 1px solid var(--danger, #ef4444);
    border-radius: 6px;
  }

  @media (max-width: 767px) {
    .panels {
      grid-template-columns: 1fr;
    }
  }
</style>
