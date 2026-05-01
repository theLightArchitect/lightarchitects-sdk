<script lang="ts">
  import {
    DOMAIN_AGENT_COLORS,
    DOMAIN_AGENT_LABELS,
    DEPTH_CONTRACT,
    type DomainAgent,
    type AgentToolConfig,
    type ResearchDepth,
  } from '$lib/dispatch';

  // Per-agent tool configuration defaults
  const AGENT_TOOL_DEFAULTS: Partial<Record<DomainAgent, AgentToolConfig>> = {
    engineer:      { tools: ['SOUL', 'CORSO', 'rust-analyzer-lsp', 'coderabbit'], depth: 'standard', optional_tools: ['Context7', 'Firecrawl', 'HuggingFace'] },
    quality:       { tools: ['SOUL', 'CORSO', 'coderabbit'], depth: 'standard', optional_tools: ['Context7', 'Firecrawl'] },
    security:      { tools: ['SOUL', 'SERAPH', 'CORSO'], depth: 'deep', optional_tools: ['Context7', 'Firecrawl', 'Playwright', 'HuggingFace'] },
    ops:           { tools: ['SOUL', 'EVA', 'AYIN'], depth: 'standard', optional_tools: ['Playwright', 'Firecrawl'] },
    researcher:    { tools: ['SOUL', 'QUANTUM', 'Context7'], depth: 'deep', optional_tools: ['Firecrawl', 'HuggingFace'] },
    knowledge:     { tools: ['SOUL'], depth: 'standard', optional_tools: ['Context7', 'Firecrawl'] },
    performance:   { tools: ['SOUL', 'CORSO', 'AYIN'], depth: 'standard', optional_tools: ['Context7'] },
    testing:       { tools: ['SOUL', 'CORSO', 'EVA', 'Playwright'], depth: 'standard', optional_tools: ['Firecrawl'] },
    documentation: { tools: ['SOUL', 'EVA', 'Context7'], depth: 'standard', optional_tools: ['Firecrawl'] },
  };

  const DEPTHS: ResearchDepth[] = ['standard', 'deep', 'exhaustive'];

  interface Props {
    agents?: DomainAgent[];
    toolConfig?: Partial<Record<DomainAgent, AgentToolConfig>>;
    disabled?: boolean;
    onchange?: (config: Partial<Record<DomainAgent, AgentToolConfig>>) => void;
  }

  let {
    agents = [],
    toolConfig = $bindable({}),
    disabled = false,
    onchange,
  }: Props = $props();

  // Track which agent panels are expanded
  let expanded = $state<Partial<Record<DomainAgent, boolean>>>({});

  // Configurable agents — only those with default tool configs
  const configurableAgents = $derived(
    agents.filter((a) => a in AGENT_TOOL_DEFAULTS),
  );

  // Ensure toolConfig has defaults for all selected agents
  $effect(() => {
    let changed = false;
    for (const agent of configurableAgents) {
      if (!(agent in toolConfig)) {
        const defaults = AGENT_TOOL_DEFAULTS[agent];
        if (defaults) {
          toolConfig = { ...toolConfig, [agent]: { ...defaults } };
          changed = true;
        }
      }
    }
    if (changed) onchange?.(toolConfig);
  });

  function toggleOptional(agent: DomainAgent, tool: string) {
    if (disabled) return;
    const cfg = toolConfig[agent];
    if (!cfg) return;
    const had = cfg.optional_tools.includes(tool);
    const next = had
      ? cfg.optional_tools.filter((t) => t !== tool)
      : [...cfg.optional_tools, tool];
    toolConfig = { ...toolConfig, [agent]: { ...cfg, optional_tools: next } };
    onchange?.(toolConfig);
  }

  function setDepth(agent: DomainAgent, depth: ResearchDepth) {
    if (disabled) return;
    const cfg = toolConfig[agent];
    if (!cfg) return;
    toolConfig = { ...toolConfig, [agent]: { ...cfg, depth } };
    onchange?.(toolConfig);
  }

  function toggleExpanded(agent: DomainAgent) {
    expanded = { ...expanded, [agent]: !expanded[agent] };
  }

  function agentColor(agent: DomainAgent): string {
    return DOMAIN_AGENT_COLORS[agent] ?? '#888';
  }
</script>

{#if configurableAgents.length > 0}
  <div class="tool-config-root">
    {#each configurableAgents as agent (agent)}
      {@const cfg = toolConfig[agent]}
      {@const color = agentColor(agent)}
      {@const isOpen = !!expanded[agent]}

      <div class="agent-panel" style="--agent-color: {color}">
        <button
          class="panel-header"
          class:open={isOpen}
          onclick={() => toggleExpanded(agent)}
          {disabled}
          aria-expanded={isOpen}
        >
          <span class="agent-dot" style="background: {color}"></span>
          <span class="agent-name">{DOMAIN_AGENT_LABELS[agent]}</span>
          {#if cfg}
            <span class="depth-badge">{cfg.depth}</span>
          {/if}
          <span class="chevron" class:rotated={isOpen}>›</span>
        </button>

        {#if isOpen && cfg}
          <div class="panel-body">
            <!-- Always-on tools -->
            <div class="tool-section">
              <span class="section-label">ACTIVE</span>
              <div class="pill-row">
                {#each cfg.tools as tool (tool)}
                  <span class="pill pill-active">{tool}</span>
                {/each}
              </div>
            </div>

            <!-- Optional tools -->
            {#if AGENT_TOOL_DEFAULTS[agent]?.optional_tools?.length}
              <div class="tool-section">
                <span class="section-label">OPTIONAL</span>
                <div class="pill-row">
                  {#each AGENT_TOOL_DEFAULTS[agent]!.optional_tools as tool (tool)}
                    {@const active = cfg.optional_tools.includes(tool)}
                    <button
                      class="pill pill-optional"
                      class:active
                      onclick={() => toggleOptional(agent, tool)}
                      {disabled}
                      title={active ? 'Click to disable' : 'Click to enable'}
                    >{tool}</button>
                  {/each}
                </div>
              </div>
            {/if}

            <!-- Research depth -->
            <div class="tool-section">
              <span class="section-label">DEPTH</span>
              <div class="depth-control">
                {#each DEPTHS as d (d)}
                  <button
                    class="depth-btn"
                    class:selected={cfg.depth === d}
                    onclick={() => setDepth(agent, d)}
                    {disabled}
                    title={DEPTH_CONTRACT[d]}
                  >{d}</button>
                {/each}
              </div>
              <p class="depth-contract">{DEPTH_CONTRACT[cfg.depth]}</p>
            </div>
          </div>
        {/if}
      </div>
    {/each}
  </div>
{/if}

<style>
  .tool-config-root {
    display: flex;
    flex-direction: column;
    gap: 2px;
    width: 100%;
  }

  .agent-panel {
    border: 1px solid color-mix(in srgb, var(--agent-color) 25%, transparent);
    border-radius: 4px;
    overflow: hidden;
  }

  .panel-header {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    padding: 6px 10px;
    background: color-mix(in srgb, var(--agent-color) 6%, transparent);
    border: none;
    cursor: pointer;
    font-family: inherit;
    font-size: 11px;
    color: #aaa;
    text-align: left;
    transition: background 120ms;
  }

  .panel-header:hover:not(:disabled) {
    background: color-mix(in srgb, var(--agent-color) 12%, transparent);
  }

  .panel-header:disabled {
    cursor: default;
    opacity: 0.5;
  }

  .agent-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    flex-shrink: 0;
  }

  .agent-name {
    flex: 1;
    font-weight: 600;
    color: #ddd;
    letter-spacing: 0.05em;
    text-transform: uppercase;
    font-size: 10px;
  }

  .depth-badge {
    font-size: 9px;
    padding: 1px 5px;
    border-radius: 3px;
    background: rgba(255,255,255,0.07);
    color: #888;
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }

  .chevron {
    font-size: 14px;
    color: #555;
    transition: transform 180ms ease;
    line-height: 1;
  }

  .chevron.rotated {
    transform: rotate(90deg);
  }

  .panel-body {
    padding: 8px 10px 10px;
    display: flex;
    flex-direction: column;
    gap: 8px;
    background: rgba(0, 0, 0, 0.2);
  }

  .tool-section {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .section-label {
    font-size: 9px;
    letter-spacing: 0.1em;
    color: #555;
    font-weight: 600;
  }

  .pill-row {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }

  .pill {
    padding: 2px 7px;
    border-radius: 3px;
    font-size: 10px;
    letter-spacing: 0.04em;
    font-family: 'JetBrains Mono', monospace;
  }

  .pill-active {
    background: rgba(255,255,255,0.06);
    color: #888;
    border: none;
  }

  .pill-optional {
    background: transparent;
    border: 1px solid #333;
    color: #555;
    cursor: pointer;
    transition: all 120ms;
  }

  .pill-optional:hover:not(:disabled) {
    border-color: #555;
    color: #999;
  }

  .pill-optional.active {
    background: rgba(255,255,255,0.08);
    border-color: color-mix(in srgb, var(--agent-color) 60%, #333);
    color: #ddd;
  }

  .pill-optional:disabled {
    cursor: default;
    opacity: 0.5;
  }

  .depth-control {
    display: flex;
    gap: 2px;
  }

  .depth-btn {
    flex: 1;
    padding: 3px 6px;
    border: 1px solid #2a2a2a;
    background: transparent;
    color: #555;
    font-size: 10px;
    font-family: inherit;
    letter-spacing: 0.06em;
    cursor: pointer;
    text-transform: uppercase;
    transition: all 120ms;
    border-radius: 3px;
  }

  .depth-btn:hover:not(:disabled) {
    border-color: #444;
    color: #999;
  }

  .depth-btn.selected {
    background: color-mix(in srgb, var(--agent-color) 15%, transparent);
    border-color: color-mix(in srgb, var(--agent-color) 50%, transparent);
    color: #ddd;
  }

  .depth-btn:disabled {
    cursor: default;
    opacity: 0.5;
  }

  .depth-contract {
    font-size: 9px;
    color: #444;
    margin: 0;
    font-style: italic;
    line-height: 1.4;
  }
</style>
