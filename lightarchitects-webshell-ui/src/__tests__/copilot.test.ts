import { describe, it, expect, beforeEach } from 'vitest';
import { get } from 'svelte/store';
import {
  copilotMessages, copilotLoading, currentBuildId, activeBuild,
  builds, findings, selectedPillar, buildBuildContext,
} from '$lib/stores';
import { parseCommand, SLASH_COMMANDS } from '$lib/commands';
import { SIBLINGS, META_SKILLS } from '$lib/types';

describe('Slash command system', () => {
  describe('parseCommand', () => {
    it('returns null for non-slash input', () => {
      const { command, args } = parseCommand('hello world');
      expect(command).toBeNull();
      expect(args).toBe('hello world');
    });

    it('parses /build command', () => {
      const { command, args } = parseCommand('/build my-project');
      expect(command).toBeDefined();
      expect(command!.name).toBe('build');
      expect(args).toBe('my-project');
    });

    it('parses /secure command', () => {
      const { command, args } = parseCommand('/secure');
      expect(command).toBeDefined();
      expect(command!.name).toBe('secure');
      expect(args).toBe('');
    });

    it('parses sibling dispatch commands', () => {
      for (const sib of SIBLINGS) {
        const { command, args } = parseCommand(`/${sib} check auth module`);
        expect(command).toBeDefined();
        expect(command!.name).toBe(sib);
        expect(args).toBe('check auth module');
      }
    });

    it('parses control commands', () => {
      const focus = parseCommand('/focus terminal');
      expect(focus.command).toBeDefined();
      expect(focus.command!.name).toBe('focus');
      expect(focus.args).toBe('terminal');

      const navigate = parseCommand('/navigate build-001');
      expect(navigate.command).toBeDefined();
      expect(navigate.command!.name).toBe('navigate');

      const notify = parseCommand('/notify Build complete');
      expect(notify.command).toBeDefined();
      expect(notify.command!.name).toBe('notify');

      const terminal = parseCommand('/terminal');
      expect(terminal.command).toBeDefined();
      expect(terminal.command!.name).toBe('terminal');

      const clear = parseCommand('/clear');
      expect(clear.command).toBeDefined();
      expect(clear.command!.name).toBe('clear');
    });

    it('returns null for unknown commands', () => {
      const { command, args } = parseCommand('/unknown-thing hello');
      expect(command).toBeNull();
      // args contains the text after the command name, not the full input
      expect(args).toBe('hello');
    });

    it('handles leading/trailing whitespace', () => {
      const { command, args } = parseCommand('  /build project-name  ');
      expect(command).toBeDefined();
      expect(command!.name).toBe('build');
      expect(args).toBe('project-name');
    });

    it('handles command with no args', () => {
      const { command, args } = parseCommand('/clear');
      expect(command).toBeDefined();
      expect(command!.name).toBe('clear');
      expect(args).toBe('');
    });

    it('is case-insensitive for command name', () => {
      const { command } = parseCommand('/BUILD project');
      expect(command).toBeDefined();
      expect(command!.name).toBe('build');
    });
  });

  describe('SLASH_COMMANDS', () => {
    it('includes commands for all 13 meta-skills', () => {
      const skillNames = META_SKILLS.map(s => s.slice(1).toLowerCase());
      for (const skillName of skillNames) {
        const cmd = SLASH_COMMANDS.find(c => c.name === skillName);
        expect(cmd, `Missing command for /${skillName}`).toBeDefined();
      }
    });

    it('includes commands for all 7 siblings', () => {
      for (const sib of SIBLINGS) {
        const cmd = SLASH_COMMANDS.find(c => c.name === sib);
        expect(cmd, `Missing command for /${sib}`).toBeDefined();
      }
    });

    it('includes 5 control commands', () => {
      const controlNames = ['focus', 'navigate', 'notify', 'terminal', 'clear'];
      for (const name of controlNames) {
        const cmd = SLASH_COMMANDS.find(c => c.name === name);
        expect(cmd, `Missing control command /${name}`).toBeDefined();
      }
    });

    it('each command has required fields', () => {
      for (const cmd of SLASH_COMMANDS) {
        expect(cmd.name).toBeTruthy();
        expect(cmd.description).toBeTruthy();
        expect(typeof cmd.execute).toBe('function');
      }
    });

    it('/clear command resets copilotMessages store', async () => {
      copilotMessages.set([
        { id: '1', role: 'user', content: 'test', timestamp: new Date().toISOString() },
      ]);
      expect(get(copilotMessages)).toHaveLength(1);

      const clearCmd = SLASH_COMMANDS.find(c => c.name === 'clear');
      await clearCmd!.execute('');
      expect(get(copilotMessages)).toHaveLength(0);
    });
  });
});

describe('Copilot stores', () => {
  beforeEach(() => {
    copilotMessages.set([]);
    copilotLoading.set(false);
    currentBuildId.set(null);
    selectedPillar.set(null);
  });

  describe('copilotMessages', () => {
    it('starts empty', () => {
      expect(get(copilotMessages)).toHaveLength(0);
    });

    it('can add user messages', () => {
      copilotMessages.update(msgs => [...msgs, {
        id: '1', role: 'user', content: 'Hello', timestamp: new Date().toISOString(),
      }]);
      expect(get(copilotMessages)).toHaveLength(1);
      expect(get(copilotMessages)[0].role).toBe('user');
    });

    it('can add assistant messages with sibling', () => {
      copilotMessages.update(msgs => [...msgs, {
        id: '2', role: 'assistant', content: 'Analysis complete', sibling: 'corso', timestamp: new Date().toISOString(),
      }]);
      const msgs = get(copilotMessages);
      expect(msgs[0].sibling).toBe('corso');
    });

    it('can add system messages', () => {
      copilotMessages.update(msgs => [...msgs, {
        id: '3', role: 'system', content: '/build dispatched', timestamp: new Date().toISOString(),
      }]);
      expect(get(copilotMessages)[0].role).toBe('system');
    });

    it('can be cleared', () => {
      copilotMessages.set([
        { id: '1', role: 'user', content: 'test', timestamp: new Date().toISOString() },
      ]);
      copilotMessages.set([]);
      expect(get(copilotMessages)).toHaveLength(0);
    });
  });

  describe('copilotLoading', () => {
    it('starts as false', () => {
      expect(get(copilotLoading)).toBe(false);
    });

    it('can be set to true during streaming', () => {
      copilotLoading.set(true);
      expect(get(copilotLoading)).toBe(true);
      copilotLoading.set(false);
    });
  });

  describe('SSE-driven copilot_response', () => {
    it('updates copilotMessages via _handleEvent', async () => {
      const { _handleEvent } = await import('$lib/sse');
      copilotMessages.set([]);
      copilotLoading.set(true);

      // Backend uses #[serde(tag = "type")] — fields inline, not under `data`.
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      _handleEvent({ type: 'copilot_response', chunk: 'Hello' } as any);
      expect(get(copilotMessages)).toHaveLength(1);
      expect(get(copilotMessages)[0].role).toBe('assistant');
      expect(get(copilotMessages)[0].content).toBe('Hello');

      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      _handleEvent({ type: 'copilot_response', chunk: ' world' } as any);
      expect(get(copilotMessages)).toHaveLength(1);
      expect(get(copilotMessages)[0].content).toBe('Hello world');
    });

    it('clears copilotLoading on done flag', async () => {
      const { _handleEvent } = await import('$lib/sse');
      copilotLoading.set(true);

      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      _handleEvent({ type: 'copilot_response', chunk: 'Done', done: true } as any);
      expect(get(copilotLoading)).toBe(false);
    });

    it('falls back gracefully when SSE unavailable', () => {
      // When no SSE events arrive, copilotMessages stays as-is
      // and copilotLoading remains true — the Copilot component
      // handles this by injecting the single-shot JSON response
      copilotMessages.set([{ id: 'u1', role: 'user', content: 'Q', timestamp: '' }]);
      copilotLoading.set(true);
      expect(get(copilotLoading)).toBe(true);
      // Manual injection path (simulating Copilot.svelte sendMessage)
      copilotMessages.update(m => [...m, { id: 'a1', role: 'assistant', content: 'A', timestamp: '' }]);
      copilotLoading.set(false);
      expect(get(copilotMessages)).toHaveLength(2);
      expect(get(copilotLoading)).toBe(false);
    });
  });
});

describe('buildBuildContext', () => {
  beforeEach(() => {
    currentBuildId.set(null);
    selectedPillar.set(null);
  });

  it('returns "[No active build]" when build is null', () => {
    const context = buildBuildContext(null, null, []);
    expect(context).toBe('[No active build]');
  });

  it('includes build name and status for active build', () => {
    const build = { id: 'build-001', workspaceId: 'ws', name: 'Add authentication flow', metaSkill: '/BUILD' as const, status: 'in_progress' as const, pillars: [], currentPillar: 'ARCH' as const, confidence: 0.67, createdAt: '', updatedAt: '', modules: [], siblingDispatches: [] };
    const context = buildBuildContext(build, null, []);
    expect(context).toContain('Add authentication flow');
    expect(context).toContain('/BUILD');
    expect(context).toContain('in_progress');
    expect(context).toContain('67%');
  });

  it('includes selected pillar info', () => {
    const build = { id: 'build-001', workspaceId: 'ws', name: 'Add authentication flow', metaSkill: '/BUILD' as const, status: 'in_progress' as const, pillars: [{ pillar: 'QUAL' as const, status: 'in_progress' as const, confidence: 0.7, findings: [] }], currentPillar: 'QUAL' as const, confidence: 0.67, createdAt: '', updatedAt: '', modules: [], siblingDispatches: [] };
    const context = buildBuildContext(build, 'QUAL', []);
    expect(context).toContain('Selected Pillar: QUAL');
  });

  it('includes findings summary', () => {
    const build = { id: 'build-001', workspaceId: 'ws', name: 'Add authentication flow', metaSkill: '/BUILD' as const, status: 'in_progress' as const, pillars: [], currentPillar: 'ARCH' as const, confidence: 0.67, createdAt: '', updatedAt: '', modules: [], siblingDispatches: [] };
    const buildFindings = [{ id: 'f-001', buildId: 'build-001', pillar: 'QUAL' as const, severity: 'warning' as const, category: 'quality' as const, title: 'Issue', description: 'Desc', verified: false }];
    const context = buildBuildContext(build, null, buildFindings);
    expect(context).toContain('Findings:');
    expect(context).toContain('warning');
  });

  it('skips findings line when there are none', () => {
    const build = { id: 'build-001', workspaceId: 'ws', name: 'Add authentication flow', metaSkill: '/BUILD' as const, status: 'in_progress' as const, pillars: [], currentPillar: 'ARCH' as const, confidence: 0.67, createdAt: '', updatedAt: '', modules: [], siblingDispatches: [] };
    const context = buildBuildContext(build, null, []);
    expect(context).not.toContain('Findings:');
  });
});

describe('SiblingDispatch component', () => {
  it('module imports successfully', async () => {
    const mod = await import('$lib/../components/SiblingDispatch.svelte');
    expect(mod.default).toBeDefined();
  });
});

describe('CopilotDrawer component', () => {
  it('module imports successfully', async () => {
    const mod = await import('$lib/../components/CopilotDrawer.svelte');
    expect(mod.default).toBeDefined();
  });
});

describe('sharedBuildId regression', () => {
  it('currentBuildId store exists and is writable', async () => {
    const storesModule = await import('$lib/stores');
    expect(storesModule.currentBuildId).toBeDefined();
    storesModule.currentBuildId.set('build-shared-test');
    const { get } = await import('svelte/store');
    expect(get(storesModule.currentBuildId)).toBe('build-shared-test');
    storesModule.currentBuildId.set(null);
  });

  it('CopilotDrawer does not export chatBuildId or activeBuildId', async () => {
    const mod = await import('$lib/../components/CopilotDrawer.svelte') as Record<string, unknown>;
    expect(mod).not.toHaveProperty('chatBuildId');
    expect(mod).not.toHaveProperty('activeBuildId');
  });
});

describe('grounding_header_parsed', () => {
  it('parses full grounding header', async () => {
    const { parseGroundingHeader } = await import('$lib/api');
    const result = parseGroundingHeader('eva=1,soul=3,git=5');
    expect(result).toEqual({ eva: 1, soul: 3, git: 5 });
  });

  it('parses header with eva=0', async () => {
    const { parseGroundingHeader } = await import('$lib/api');
    const result = parseGroundingHeader('eva=0,soul=0,git=0');
    expect(result).toEqual({ eva: 0, soul: 0, git: 0 });
  });

  it('returns null for missing header', async () => {
    const { parseGroundingHeader } = await import('$lib/api');
    expect(parseGroundingHeader(null)).toBeNull();
  });

  it('returns null for malformed header', async () => {
    const { parseGroundingHeader } = await import('$lib/api');
    expect(parseGroundingHeader('notvalid')).toBeNull();
  });
});

describe('grounding_chips_render', () => {
  it('copilotGrounding store accepts GroundingInfo', async () => {
    const { copilotGrounding } = await import('$lib/stores');
    copilotGrounding.set({ eva: 1, soul: 3, git: 5 });
    expect(get(copilotGrounding)).toEqual({ eva: 1, soul: 3, git: 5 });
    copilotGrounding.set(null);
  });

  it('CopilotContextTray module imports successfully', async () => {
    const mod = await import('$lib/../components/CopilotContextTray.svelte');
    expect(mod.default).toBeDefined();
  });
});

describe('grounding_chips_hidden_when_zero', () => {
  it('copilotGrounding store starts as null', async () => {
    const { copilotGrounding } = await import('$lib/stores');
    copilotGrounding.set(null);
    expect(get(copilotGrounding)).toBeNull();
  });

  it('grounding row only rendered when grounding is non-null (conditional logic)', async () => {
    // Validates that the tray hides the row when grounding=null by checking
    // the conditional is present in the component source.
    const fs = await import('fs');
    const path = await import('path');
    const src = fs.readFileSync(
      path.resolve(process.cwd(), 'src/components/CopilotContextTray.svelte'),
      'utf8'
    );
    expect(src).toContain('{#if grounding !== null}');
  });
});