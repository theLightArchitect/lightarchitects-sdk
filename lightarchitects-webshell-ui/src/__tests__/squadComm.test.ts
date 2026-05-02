import { describe, it, expect } from 'vitest';
import {
  DEFAULT_IMPORTANCE,
  MESSAGE_STYLE,
  importanceForFinding,
  wrapAsProgressUpdate,
  type MessageType,
  type DomainAgent,
  type FindingSeverity,
} from '$lib/squadComm';

const ALL_TYPES: MessageType[] = [
  'commit.completed',
  'decision.made',
  'gap.discovered',
  'blocker.raised',
  'assumption.flagged',
  'risk.surfaced',
  'convention.established',
  'review.requested',
  'finding.classified',
  'handoff.completed',
  'context.shared',
  'progress.updated',
];

const ALL_AGENTS: DomainAgent[] = [
  'engineer', 'quality', 'security', 'ops',
  'researcher', 'knowledge', 'testing', 'squad',
];

describe('squadComm', () => {
  describe('MessageType coverage', () => {
    it('has exactly 12 message types', () => {
      expect(ALL_TYPES).toHaveLength(12);
    });
  });

  describe('DomainAgent coverage', () => {
    it('has exactly 9 domain agents', () => {
      expect(ALL_AGENTS).toHaveLength(8);
    });
  });

  describe('DEFAULT_IMPORTANCE', () => {
    it('covers every message type', () => {
      for (const type of ALL_TYPES) {
        expect(DEFAULT_IMPORTANCE[type]).toBeDefined();
        expect(['low', 'normal', 'high', 'critical']).toContain(DEFAULT_IMPORTANCE[type]);
      }
    });

    it('blockers are critical, context/progress are low', () => {
      expect(DEFAULT_IMPORTANCE['blocker.raised']).toBe('critical');
      expect(DEFAULT_IMPORTANCE['context.shared']).toBe('low');
      expect(DEFAULT_IMPORTANCE['progress.updated']).toBe('low');
    });

    it('decisions and gaps are high importance', () => {
      expect(DEFAULT_IMPORTANCE['decision.made']).toBe('high');
      expect(DEFAULT_IMPORTANCE['gap.discovered']).toBe('high');
    });
  });

  describe('MESSAGE_STYLE', () => {
    it('covers every message type with an edge color', () => {
      for (const type of ALL_TYPES) {
        expect(MESSAGE_STYLE[type]).toBeDefined();
        expect(MESSAGE_STYLE[type].edge).toMatch(/^#[0-9a-fA-F]{3,6}$/);
      }
    });

    it('high-severity types have an icon', () => {
      const typesWithIcons: MessageType[] = [
        'commit.completed', 'decision.made', 'gap.discovered',
        'blocker.raised', 'assumption.flagged', 'risk.surfaced',
        'convention.established', 'review.requested', 'finding.classified',
        'handoff.completed',
      ];
      for (const type of typesWithIcons) {
        expect(MESSAGE_STYLE[type].icon).toBeDefined();
      }
    });
  });

  describe('importanceForFinding()', () => {
    const cases: [FindingSeverity, string][] = [
      ['CRITICAL', 'critical'],
      ['HIGH', 'high'],
      ['MEDIUM', 'normal'],
      ['LOW', 'low'],
    ];
    it.each(cases)('%s severity → %s importance', (severity, expected) => {
      expect(importanceForFinding(severity)).toBe(expected);
    });
  });

  describe('wrapAsProgressUpdate()', () => {
    it('returns a valid progress.updated message', () => {
      const msg = wrapAsProgressUpdate('Compiling sources');
      expect(msg.type).toBe('progress.updated');
      expect(msg.from).toBe('coordinator');
      expect(msg.to).toBeNull();
      expect(msg.topic).toBe('build');
      expect(msg.importance).toBe('low');
      expect(msg.payload.current_step).toBe('Compiling sources');
      expect(msg.payload.progress_pct).toBe(0);
    });

    it('passes through progress_pct', () => {
      const msg = wrapAsProgressUpdate('Running tests', 75);
      expect(msg.payload.progress_pct).toBe(75);
      expect(msg.payload.current_step).toBe('Running tests');
    });

    it('generates a unique id each call', () => {
      const a = wrapAsProgressUpdate('a');
      const b = wrapAsProgressUpdate('b');
      expect(a.id).not.toBe(b.id);
    });

    it('sets a valid ISO 8601 timestamp', () => {
      const msg = wrapAsProgressUpdate('step');
      expect(new Date(msg.timestamp).getTime()).toBeGreaterThan(0);
      expect(msg.timestamp).toMatch(/^\d{4}-\d{2}-\d{2}T/);
    });
  });
});
