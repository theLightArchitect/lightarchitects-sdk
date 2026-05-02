import { describe, it, expect, vi } from 'vitest';
import { render } from 'vitest-browser-svelte';
import AgentToolConfig from './AgentToolConfig.svelte';
import { DEPTH_CONTRACT } from '$lib/dispatch';

describe('AgentToolConfig (browser)', () => {
  it('renders nothing when no agents are passed', async () => {
    const { container } = render(AgentToolConfig, { agents: [] });
    // Wait a tick for $derived to settle
    await new Promise((r) => setTimeout(r, 50));
    expect(container.querySelector('.tool-config-root')).toBeNull();
  });

  it('renders one panel per configurable agent with their canonical name', async () => {
    const { getByText } = render(AgentToolConfig, {
      agents: ['engineer', 'researcher', 'security'],
    });
    await expect.element(getByText('Engineer')).toBeInTheDocument();
    await expect.element(getByText('Researcher')).toBeInTheDocument();
    await expect.element(getByText('Security')).toBeInTheDocument();
  });

  it('shows the default depth badge per agent (engineer=standard, security=deep)', async () => {
    const { getByText } = render(AgentToolConfig, {
      agents: ['engineer', 'security'],
    });
    await expect.element(getByText('standard').first()).toBeInTheDocument();
    await expect.element(getByText('deep').first()).toBeInTheDocument();
  });

  it('panel header is collapsed by default (aria-expanded=false)', async () => {
    const { getByRole } = render(AgentToolConfig, { agents: ['engineer'] });
    await expect.element(getByRole('button', { name: /Engineer/ })).toHaveAttribute('aria-expanded', 'false');
  });

  it('expands on header click and reveals ACTIVE / OPTIONAL / DEPTH sections', async () => {
    const { getByText, getByRole } = render(AgentToolConfig, { agents: ['engineer'] });
    await getByRole('button', { name: /Engineer/ }).click();

    await expect.element(getByText('ACTIVE')).toBeInTheDocument();
    await expect.element(getByText('OPTIONAL')).toBeInTheDocument();
    await expect.element(getByText('DEPTH')).toBeInTheDocument();
  });

  it('renders all 3 depth option buttons (standard / deep / exhaustive)', async () => {
    const { getByRole } = render(AgentToolConfig, { agents: ['engineer'] });
    await getByRole('button', { name: /Engineer/ }).click();

    await expect.element(getByRole('button', { name: 'standard', exact: true })).toBeInTheDocument();
    await expect.element(getByRole('button', { name: 'deep', exact: true })).toBeInTheDocument();
    await expect.element(getByRole('button', { name: 'exhaustive', exact: true })).toBeInTheDocument();
  });

  it('changes depth and surfaces the behavioral contract', async () => {
    const onchange = vi.fn();
    const { getByText, getByRole } = render(AgentToolConfig, {
      agents: ['engineer'],
      onchange,
    });

    await getByRole('button', { name: /Engineer/ }).click();
    await getByRole('button', { name: 'exhaustive', exact: true }).click();

    expect(onchange).toHaveBeenCalled();
    const lastCall = onchange.mock.calls.at(-1)?.[0];
    expect(lastCall.engineer.depth).toBe('exhaustive');

    // Contract text appears on screen — proves no label-without-effect
    await expect.element(getByText(DEPTH_CONTRACT.exhaustive)).toBeInTheDocument();
  });

  it('toggles optional tools when their pill is clicked', async () => {
    const onchange = vi.fn();
    const { getByRole } = render(AgentToolConfig, {
      agents: ['researcher'],
      onchange,
    });

    await getByRole('button', { name: /Researcher/ }).click();
    await getByRole('button', { name: 'HuggingFace' }).click();

    expect(onchange).toHaveBeenCalled();
    const lastCall = onchange.mock.calls.at(-1)?.[0];
    // HuggingFace was already in researcher's default optional_tools — first click removes it
    expect(lastCall.researcher.optional_tools).not.toContain('HuggingFace');
  });

  it('renders ACTIVE pills for engineer agent (SOUL, CORSO, rust-analyzer-lsp, coderabbit)', async () => {
    const { getByText, getByRole } = render(AgentToolConfig, { agents: ['engineer'] });
    await getByRole('button', { name: /Engineer/ }).click();

    await expect.element(getByText('SOUL').first()).toBeInTheDocument();
    await expect.element(getByText('CORSO').first()).toBeInTheDocument();
    await expect.element(getByText('rust-analyzer-lsp')).toBeInTheDocument();
    await expect.element(getByText('coderabbit')).toBeInTheDocument();
  });

  it('disables all interactions when disabled=true', async () => {
    const { getByRole } = render(AgentToolConfig, {
      agents: ['engineer'],
      disabled: true,
    });
    await expect.element(getByRole('button', { name: /Engineer/ })).toBeDisabled();
  });
});
