// ============================================================================
// Slash command system — /build, /deploy, /quality, etc.
// ============================================================================

import { api } from './api';
import { currentBuildId, commandPaletteOpen, copilotMessages } from './stores';
import type { MetaSkill, SiblingId } from './types';
import { META_SKILLS, SIBLINGS } from './types';

export interface SlashCommand {
  name: string;
  alias?: string[];
  description: string;
  args?: string;
  execute: (args: string) => Promise<void>;
}

export const SLASH_COMMANDS: SlashCommand[] = [
  // --- Meta-skills ---
  ...META_SKILLS.map(skill => ({
    name: skill.slice(1).toLowerCase(), // e.g., 'build', 'research', 'secure'
    description: `Start ${skill} meta-skill cycle`,
    args: '[project]',
    execute: async (args: string) => {
      await api.createBuild({ cwd: '/tmp', metaSkill: skill, target: args });
      currentBuildId.set(null); // will be set by SSE
    },
  })),

  // --- Sibling dispatch ---
  ...SIBLINGS.map(sib => ({
    name: sib,
    description: `Dispatch ${sib.toUpperCase()} sibling agent`,
    args: '[prompt]',
    execute: async (args: string) => {
      const $buildId = await new Promise<string | null>(resolve =>
        currentBuildId.subscribe(v => { resolve(v); })?.()
      );
      if ($buildId) {
        await api.dispatchSibling($buildId, sib, sib, args);
      }
    },
  })),

  // --- Control commands ---
  {
    name: 'focus',
    description: 'Focus a specific panel or view',
    args: '<panel>',
    execute: async (args) => { await api.control('FocusPanel', { panel: args }); },
  },
  {
    name: 'navigate',
    description: 'Navigate to a build or view',
    args: '<build-id>',
    execute: async (args) => { await api.control('NavigateTo', { buildId: args }); },
  },
  {
    name: 'notify',
    description: 'Send a toast notification',
    args: '<message>',
    execute: async (args) => { await api.control('Notify', { message: args }); },
  },
  {
    name: 'terminal',
    description: 'Open or focus terminal pane',
    execute: async () => { await api.control('OpenTerminal'); },
  },
  {
    name: 'settings',
    description: 'Open settings panel',
    execute: async () => { await api.control('OpenSettings'); },
  },
  {
    name: 'theme',
    description: 'Toggle theme (dark/light)',
    execute: async () => { await api.control('ToggleTheme'); },
  },
  {
    name: 'panel',
    description: 'Show or hide a specific panel',
    args: '<panel-name>',
    execute: async (args) => { await api.control('TogglePanel', { panel: args }); },
  },
  {
    name: 'clear',
    description: 'Clear copilot chat',
    execute: async () => { copilotMessages.set([]); },
  },
];

export function parseCommand(input: string): { command: SlashCommand | null; args: string } {
  const trimmed = input.trim();
  if (!trimmed.startsWith('/')) return { command: null, args: trimmed };

  const parts = trimmed.slice(1).split(/\s+/);
  const name = parts[0].toLowerCase();
  const args = parts.slice(1).join(' ');

  const cmd = SLASH_COMMANDS.find(c => c.name === name || c.alias?.includes(name));
  return { command: cmd ?? null, args };
}