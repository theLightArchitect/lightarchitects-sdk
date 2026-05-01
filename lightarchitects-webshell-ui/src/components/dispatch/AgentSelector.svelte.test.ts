import { describe, it, expect, vi } from 'vitest';
import { render } from 'vitest-browser-svelte';
import AgentSelector from './AgentSelector.svelte';
import type { Classification } from '$lib/dispatch';

describe('AgentSelector (browser)', () => {
  it('renders all 8 canonical agent chips with their codes, gates, and perms', async () => {
    const { getByTestId } = render(AgentSelector, {});

    const expected = [
      { id: 'engineer',   code: 'ENG', gate: 'A',  perm: 'W' },
      { id: 'quality',    code: 'QLT', gate: 'Q',  perm: 'W' },
      { id: 'security',   code: 'SEC', gate: 'S',  perm: 'R' },
      { id: 'ops',        code: 'OPS', gate: 'O',  perm: 'W' },
      { id: 'researcher', code: 'RES', gate: 'R',  perm: 'R' },
      { id: 'knowledge',  code: 'KNW', gate: 'K',  perm: 'R' },
      { id: 'testing',    code: 'TST', gate: 'T',  perm: 'W' },
      { id: 'squad',      code: 'SQD', gate: 'SQ', perm: 'R' },
    ];

    for (const { id, code, gate, perm } of expected) {
      const chip = getByTestId(`agent-btn-${id}`);
      await expect.element(chip).toBeInTheDocument();
      await expect.element(chip).toHaveTextContent(code);
      await expect.element(chip).toHaveTextContent(`GATE · ${gate}`);
      await expect.element(chip).toHaveAttribute('data-perm', perm);
    }
  });

  it('shows the empty-state warning when no agent is selected', async () => {
    const { getByText } = render(AgentSelector, {});
    await expect.element(getByText('Select at least one agent to dispatch.')).toBeInTheDocument();
  });

  it('shows aria-pressed=true on selected chips and reflects the count', async () => {
    const { getByTestId, getByText } = render(AgentSelector, {
      selected: ['engineer', 'researcher'],
    });
    await expect.element(getByTestId('agent-btn-engineer')).toHaveAttribute('aria-pressed', 'true');
    await expect.element(getByTestId('agent-btn-researcher')).toHaveAttribute('aria-pressed', 'true');
    await expect.element(getByTestId('agent-btn-quality')).toHaveAttribute('aria-pressed', 'false');
    await expect.element(getByText('2 QUEUED')).toBeInTheDocument();
  });

  it('toggles selection on chip click and notifies via onchange', async () => {
    const onchange = vi.fn();
    const { getByTestId } = render(AgentSelector, {
      selected: [],
      onchange,
    });

    await getByTestId('agent-btn-engineer').click();
    expect(onchange).toHaveBeenLastCalledWith(['engineer']);

    await getByTestId('agent-btn-quality').click();
    expect(onchange).toHaveBeenLastCalledWith(['engineer', 'quality']);

    // Toggle off
    await getByTestId('agent-btn-engineer').click();
    expect(onchange).toHaveBeenLastCalledWith(['quality']);
  });

  it('renders the AUTO·N button only when classification supplies agents', async () => {
    const classification: Classification = {
      agents: ['security', 'researcher'],
      rationale: 'task involves auth + CVE research',
      confidence: 0.82,
    };
    const { getByText } = render(AgentSelector, { classification });
    await expect.element(getByText('AUTO·2')).toBeInTheDocument();
    await expect.element(getByText('task involves auth + CVE research')).toBeInTheDocument();
  });

  it('applies classification to selection when AUTO·N is clicked', async () => {
    const onchange = vi.fn();
    const classification: Classification = {
      agents: ['quality', 'testing'],
      rationale: 'test refactor',
      confidence: 0.7,
    };
    const { getByText } = render(AgentSelector, { classification, onchange });

    await getByText('AUTO·2').click();
    expect(onchange).toHaveBeenCalledWith(['quality', 'testing']);
  });

  it('selects all 8 agents when ALL is clicked', async () => {
    const onchange = vi.fn();
    const { getByText } = render(AgentSelector, { onchange });
    await getByText('ALL').click();
    const lastCall = onchange.mock.calls.at(-1)?.[0] as string[];
    expect(lastCall).toHaveLength(8);
    expect(lastCall).toEqual(expect.arrayContaining(['engineer', 'squad']));
  });

  it('clears selection when CLR is clicked', async () => {
    const onchange = vi.fn();
    const { getByText } = render(AgentSelector, {
      selected: ['engineer', 'security'],
      onchange,
    });
    await getByText('CLR').click();
    expect(onchange).toHaveBeenLastCalledWith([]);
  });

  it('disables all chips and buttons when disabled=true', async () => {
    const onchange = vi.fn();
    const { getByTestId, getByText } = render(AgentSelector, {
      disabled: true,
      onchange,
    });

    await expect.element(getByTestId('agent-btn-engineer')).toBeDisabled();
    await expect.element(getByText('ALL')).toBeDisabled();
    await expect.element(getByText('CLR')).toBeDisabled();
  });
});
