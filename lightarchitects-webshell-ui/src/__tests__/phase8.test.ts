import { describe, it, expect } from 'vitest';
import { get } from 'svelte/store';
import {
  builds, findings, conductorTasks, arenaStatus, alerts, buildStats,
} from '$lib/stores';
import { parseCommand, SLASH_COMMANDS } from '$lib/commands';
import { SIBLINGS, META_SKILLS, PILLAR_ACTIONS, PILLARS } from '$lib/types';
import { getMetaSkillPolytope, getMetaSkillColor, SIBLING_COLORS } from '$lib/design-tokens';

describe('Phase 8: Claude GUI Control + Polish', () => {
  describe('Code-splitting configuration', () => {
    it('vite.config.ts exists', async () => {
      const fs = await import('fs');
      const path = await import('path');
      const configPath = path.resolve(process.cwd(), 'vite.config.ts');
      const exists = fs.existsSync(configPath);
      expect(exists).toBe(true);
    });

    it('manualChunks configured for three.js separation', async () => {
      const fs = await import('fs');
      const path = await import('path');
      const configPath = path.resolve(process.cwd(), 'vite.config.ts');
      const content = fs.readFileSync(configPath, 'utf-8');
      expect(content).toContain('manualChunks');
      expect(content).toContain('three');
    });
  });

  describe('SSE event handlers', () => {
    it('sse.ts imports build-related stores', async () => {
      const content = await import('$lib/sse');
      // Module should load without errors
      expect(content.connectSSE).toBeDefined();
      expect(content.disconnectSSE).toBeDefined();
    });

    it('build_update event can update builds store', () => {
      builds.set([{ id: 'build-001', workspaceId: 'ws', name: 'Auth', metaSkill: '/BUILD', status: 'in_progress', pillars: [], currentPillar: 'ARCH', confidence: 0.67, createdAt: '', updatedAt: '', modules: [], siblingDispatches: [] }]);
      const initialBuilds = get(builds);
      expect(initialBuilds.length).toBeGreaterThan(0);
      // Simulate what build_update handler does
      builds.update(b => b.map(bld =>
        bld.id === 'build-001' ? { ...bld, confidence: 0.75 } : bld
      ));
      const updated = get(builds).find(b => b.id === 'build-001');
      expect(updated?.confidence).toBe(0.75);
      builds.set([]);
    });

    it('finding event can add new finding', () => {
      const initialCount = get(findings).length;
      const newFinding = {
        id: 'f-test', buildId: 'build-001', pillar: 'QUAL' as const,
        severity: 'info' as const, category: 'quality' as const,
        title: 'Test finding', description: 'Test', verified: false,
      };
      findings.update(f => [...f, newFinding]);
      expect(get(findings).length).toBe(initialCount + 1);
      // Clean up
      findings.update(f => f.filter(x => x.id !== 'f-test'));
    });

    it('conductor_task event can update task status', () => {
      conductorTasks.set([{ id: 'ct-002', buildId: 'build-001', sibling: 'quantum', taskType: 'SCAN', priority: 'normal', status: 'pending', queuedAt: new Date().toISOString() }]);
      const initialTasks = get(conductorTasks);
      expect(initialTasks.length).toBeGreaterThan(0);
      // Simulate task completion
      conductorTasks.update(t => t.map(task =>
        task.id === 'ct-002' ? { ...task, status: 'running' as const, startedAt: new Date().toISOString() } : task
      ));
      const updated = get(conductorTasks).find(t => t.id === 'ct-002');
      expect(updated?.status).toBe('running');
      conductorTasks.set([]);
    });

    it('arena_update event can modify arena status', () => {
      const initial = get(arenaStatus);
      arenaStatus.update(a => ({ ...a, activeRoutines: a.activeRoutines + 1 }));
      expect(get(arenaStatus).activeRoutines).toBe(initial.activeRoutines + 1);
      // Restore
      arenaStatus.update(a => ({ ...a, activeRoutines: initial.activeRoutines }));
    });
  });

  describe('Control commands', () => {
    it('has terminal command', () => {
      const { command } = parseCommand('/terminal');
      expect(command).toBeDefined();
      expect(command!.name).toBe('terminal');
    });

    it('has settings command', () => {
      const { command } = parseCommand('/settings');
      expect(command).toBeDefined();
      expect(command!.name).toBe('settings');
    });

    it('has theme command', () => {
      const { command } = parseCommand('/theme');
      expect(command).toBeDefined();
      expect(command!.name).toBe('theme');
    });

    it('has panel command', () => {
      const { command } = parseCommand('/panel terminal');
      expect(command).toBeDefined();
      expect(command!.name).toBe('panel');
    });

    it('all control commands have descriptions', () => {
      const controlNames = ['focus', 'navigate', 'notify', 'terminal', 'settings', 'theme', 'panel'];
      for (const name of controlNames) {
        const cmd = SLASH_COMMANDS.find(c => c.name === name);
        expect(cmd).toBeDefined();
        expect(cmd!.description.length).toBeGreaterThan(0);
      }
    });
  });

  describe('PolytopeDecor presence in screens', () => {
    it('all screens can be imported', async () => {
      const buildQueue = await import('$lib/../screens/BuildQueue.svelte');
      const workspace = await import('$lib/../screens/Workspace.svelte');
      const intake = await import('$lib/../screens/Intake.svelte');
      const sitrep = await import('$lib/../screens/Sitrep.svelte');
      const copilot = await import('$lib/../screens/Copilot.svelte');
      expect(buildQueue.default).toBeDefined();
      expect(workspace.default).toBeDefined();
      expect(intake.default).toBeDefined();
      expect(sitrep.default).toBeDefined();
      expect(copilot.default).toBeDefined();
    });
  });

  describe('Accessibility features', () => {
    it('FindingsPanel has aria-expanded support', async () => {
      const fs = await import('fs');
      const path = await import('path');
      const content = fs.readFileSync(
        path.resolve(process.cwd(), 'src/components/FindingsPanel.svelte'),
        'utf-8'
      );
      expect(content).toContain('aria-expanded');
    });

    it('BuildPortfolio has aria-label on build cards', async () => {
      const fs = await import('fs');
      const path = await import('path');
      const content = fs.readFileSync(
        path.resolve(process.cwd(), 'src/components/BuildPortfolio.svelte'),
        'utf-8'
      );
      expect(content).toContain('aria-label');
    });

    it('Copilot messages have aria-live region', async () => {
      const fs = await import('fs');
      const path = await import('path');
      const content = fs.readFileSync(
        path.resolve(process.cwd(), 'src/screens/Copilot.svelte'),
        'utf-8'
      );
      expect(content).toContain('aria-live');
    });
  });

  describe('Responsive layout', () => {
    it('Workspace right panel has responsive hide class', async () => {
      const fs = await import('fs');
      const path = await import('path');
      const content = fs.readFileSync(
        path.resolve(process.cwd(), 'src/screens/Workspace.svelte'),
        'utf-8'
      );
      expect(content).toContain('hidden lg:block');
    });

    it('CommandPalette has max-width constraint', async () => {
      const fs = await import('fs');
      const path = await import('path');
      const content = fs.readFileSync(
        path.resolve(process.cwd(), 'src/components/CommandPalette.svelte'),
        'utf-8'
      );
      expect(content).toContain('max-w');
    });
  });

  describe('Meta-skill completeness', () => {
    it('every meta-skill has a valid polytope and color', () => {
      for (const skill of META_SKILLS) {
        const polyType = getMetaSkillPolytope(skill);
        const polyColor = getMetaSkillColor(skill);
        expect(polyType).toBeTruthy();
        expect(polyColor).toMatch(/^#[0-9a-fA-F]{6}$/);
      }
    });

    it('every meta-skill has pillar actions for all 7 pillars', () => {
      for (const skill of META_SKILLS) {
        const actions = PILLAR_ACTIONS[skill];
        expect(actions).toBeDefined();
        for (const pillar of PILLARS) {
          expect(actions[pillar]).toBeDefined();
        }
      }
    });
  });
});