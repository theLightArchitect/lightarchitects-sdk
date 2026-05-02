import { describe, it, expect, vi } from 'vitest';
import { render } from 'vitest-browser-svelte';
import LiveAgentGrid from './LiveAgentGrid.svelte';
import type { DomainAgent, AgentLiveState } from '$lib/dispatch';

const ALL_AGENTS: DomainAgent[] = [
  'engineer', 'quality', 'security', 'ops',
  'researcher', 'knowledge', 'testing', 'squad',
];

describe('LiveAgentGrid (browser)', () => {
  it('renders the empty state when no agents are dispatched', async () => {
    const { getByText } = render(LiveAgentGrid, { agents: [] });
    await expect.element(getByText('— awaiting dispatch —')).toBeInTheDocument();
  });

  it('renders all 8 canonical agents with their codes and gates', async () => {
    const { getByTestId } = render(LiveAgentGrid, { agents: ALL_AGENTS });

    const expected = [
      { agent: 'engineer',   code: 'ENG', gate: 'A',  label: 'Engineer'   },
      { agent: 'quality',    code: 'QLT', gate: 'Q',  label: 'Quality'    },
      { agent: 'security',   code: 'SEC', gate: 'S',  label: 'Security'   },
      { agent: 'ops',        code: 'OPS', gate: 'O',  label: 'Ops'        },
      { agent: 'researcher', code: 'RES', gate: 'R',  label: 'Researcher' },
      { agent: 'knowledge',  code: 'KNW', gate: 'K',  label: 'Knowledge'  },
      { agent: 'testing',    code: 'TST', gate: 'T',  label: 'Testing'    },
      { agent: 'squad',      code: 'SQD', gate: 'SQ', label: 'Squad'      },
    ];

    for (const { agent, code, gate, label } of expected) {
      const rail = getByTestId(`agent-rail-${agent}`);
      await expect.element(rail).toBeInTheDocument();
      await expect.element(rail).toHaveTextContent(`${code}/`);
      await expect.element(rail).toHaveTextContent(label);
      await expect.element(rail).toHaveTextContent(`GATE · ${gate}`);
    }
  });

  it('renders idle "STANDBY" state when no live data is provided', async () => {
    const { getByTestId } = render(LiveAgentGrid, { agents: ['engineer'] });
    const rail = getByTestId('agent-rail-engineer');
    await expect.element(rail).toHaveTextContent('STANDBY');
  });

  it('renders last_tool metadata (tool name, action, latency) when present', async () => {
    const states = new Map<DomainAgent, AgentLiveState>();
    states.set('researcher', {
      agent: 'researcher',
      state: 'running',
      messages: [],
      last_tool: {
        tool: 'context7',
        action: 'query-docs',
        status: 'fired',
        latency_ms: 142,
      },
    });

    const { getByTestId } = render(LiveAgentGrid, {
      agents: ['researcher'],
      agentStates: states,
    });

    const rail = getByTestId('agent-rail-researcher');
    await expect.element(rail).toHaveTextContent('context7');
    await expect.element(rail).toHaveTextContent('query-docs');
    await expect.element(rail).toHaveTextContent('142ms');
  });

  it('reflects state via data-state attribute (drives CSS)', async () => {
    const states = new Map<DomainAgent, AgentLiveState>();
    states.set('engineer', { agent: 'engineer', state: 'failed', messages: [] });

    const { getByTestId } = render(LiveAgentGrid, {
      agents: ['engineer'],
      agentStates: states,
    });

    const rail = getByTestId('agent-rail-engineer');
    await expect.element(rail).toHaveAttribute('data-state', 'failed');
  });

  it('invokes onSelect with the agent id when a rail is clicked', async () => {
    const onSelect = vi.fn();
    const { getByTestId } = render(LiveAgentGrid, {
      agents: ['security'],
      onSelect,
    });

    await getByTestId('agent-rail-security').click();
    expect(onSelect).toHaveBeenCalledWith('security');
  });

  it('falls back to the latest message text when last_tool is absent', async () => {
    const states = new Map<DomainAgent, AgentLiveState>();
    states.set('quality', {
      agent: 'quality',
      state: 'running',
      messages: ['running clippy --pedantic'],
    });

    const { getByTestId } = render(LiveAgentGrid, {
      agents: ['quality'],
      agentStates: states,
    });

    const rail = getByTestId('agent-rail-quality');
    await expect.element(rail).toHaveTextContent('running clippy --pedantic');
  });
});
