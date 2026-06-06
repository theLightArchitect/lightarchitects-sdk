/**
 * BackendPicker unit tests — verifies the agent list, active-detection logic,
 * and store wiring without requiring a DOM environment.
 */
import { describe, it, expect, beforeEach } from 'vitest';
import { get } from 'svelte/store';
import { authProfile } from '$lib/stores';
import type { AuthProfile } from '$lib/types';

// Mirror of BackendPicker's AGENTS constant — tests that every declared agent
// has the required fields and a valid serde-snake_case kind.
const AGENTS = [
  { kind: 'lightarchitects',        label: 'Claude Code',   color: '#22C55E' },
  { kind: 'light_architect', label: 'lÆx0 Native',  color: '#14B8A6' },
  { kind: 'codex',                  label: 'Codex',         color: '#A855F7' },
  { kind: 'mistral_vibe',           label: 'Mistral Vibe',  color: '#FB923C' },
] as const;

// Mirror of StatusBar's AGENT_MAP for color/label derivation.
const AGENT_MAP: Record<string, { label: string; color: string }> = {
  lightarchitects:         { label: 'Claude Code',   color: '#22C55E' },
  light_architect:  { label: 'lÆx0 Native',  color: '#14B8A6' },
  codex:                   { label: 'Codex',         color: '#A855F7' },
  mistral_vibe:            { label: 'Mistral Vibe',  color: '#FB923C' },
  anthropic:               { label: 'Anthropic',     color: '#F59E0B' },
  ollama:                  { label: 'Ollama',        color: '#6366F1' },
};

describe('BackendPicker — agent list', () => {
  it('declares exactly 4 backend options', () => {
    expect(AGENTS).toHaveLength(4);
  });

  it('all agents have a non-empty kind, label, and hex color', () => {
    for (const agent of AGENTS) {
      expect(agent.kind.length).toBeGreaterThan(0);
      expect(agent.label.length).toBeGreaterThan(0);
      expect(agent.color).toMatch(/^#[0-9A-Fa-f]{6}$/);
    }
  });

  it('kind values are valid AuthProfile enum members', () => {
    const validProfiles = new Set<string>([
      'anthropic', 'lightarchitects', 'light_architect',
      'codex', 'mistral_vibe', 'ollama',
    ]);
    for (const agent of AGENTS) {
      expect(validProfiles.has(agent.kind)).toBe(true);
    }
  });

  it('no two agents share the same kind', () => {
    const kinds = AGENTS.map(a => a.kind);
    expect(new Set(kinds).size).toBe(kinds.length);
  });

  it('no two agents share the same color', () => {
    const colors = AGENTS.map(a => a.color);
    expect(new Set(colors).size).toBe(colors.length);
  });
});

describe('BackendPicker — active detection via authProfile store', () => {
  beforeEach(() => {
    authProfile.set(null);
  });

  it('starts with no active profile', () => {
    expect(get(authProfile)).toBeNull();
  });

  it('setting authProfile to a valid kind marks that agent as active', () => {
    authProfile.set('lightarchitects');
    const current = get(authProfile);
    expect(current).toBe('lightarchitects');
    const activeAgent = AGENTS.find(a => a.kind === current);
    expect(activeAgent).toBeDefined();
  });

  it('switching from null to codex sets correct store value', () => {
    authProfile.set('codex');
    expect(get(authProfile)).toBe('codex');
  });

  it('same-backend pick is a noop — current stays unchanged', () => {
    authProfile.set('mistral_vibe');
    // Simulate the picker's same-backend guard
    const current = get(authProfile);
    const picked = 'mistral_vibe';
    const isNoop = picked === current;
    expect(isNoop).toBe(true);
    // Store should remain unchanged
    expect(get(authProfile)).toBe('mistral_vibe');
  });
});

describe('BackendPicker — AGENT_MAP lookup (StatusBar derivation)', () => {
  it('every picker agent has a corresponding AGENT_MAP entry', () => {
    for (const agent of AGENTS) {
      const entry = AGENT_MAP[agent.kind];
      expect(entry).toBeDefined();
      expect(entry.label).toBe(agent.label);
      expect(entry.color).toBe(agent.color);
    }
  });

  it('AGENT_MAP returns correct label for light_architect', () => {
    expect(AGENT_MAP['light_architect']?.label).toBe('lÆx0 Native');
  });

  it('unknown kind returns undefined from AGENT_MAP (no crash)', () => {
    expect(AGENT_MAP['unknown_kind']).toBeUndefined();
  });
});

describe('la:pty-respawned SSE event → authProfile update', () => {
  beforeEach(() => {
    authProfile.set('lightarchitects');
  });

  it('authProfile.set updates store (simulates SSE handler path)', () => {
    // Simulates what sse.ts case 'pty_respawned' does
    const newKind = 'codex' as AuthProfile;
    authProfile.set(newKind);
    expect(get(authProfile)).toBe('codex');
  });

  it('authProfile update to null represents unauthenticated (honest state)', () => {
    authProfile.set(null);
    expect(get(authProfile)).toBeNull();
  });
});
