import { describe, it, expect } from 'vitest';
import {
  getMockCommsThreads,
  MOCK_WAVE_STATUS,
  MOCK_DECISION_ENTRIES,
  MOCK_WORKTREES,
  type MockThread,
} from '$lib/mock-surfaces';

describe('getMockCommsThreads()', () => {
  it('returns ≥6 entries (FE-7 temporal variety)', () => {
    expect(getMockCommsThreads().length).toBeGreaterThanOrEqual(6);
  });

  it('every entry has required fields', () => {
    for (const t of getMockCommsThreads()) {
      expect(t.id).toBeTruthy();
      expect(t.from).toBeTruthy();
      expect(t.to).toBeTruthy();
      expect(t.subject).toBeTruthy();
      expect(typeof t.unread).toBe('boolean');
      expect(() => new Date(t.timestamp)).not.toThrow();
    }
  });

  it('FE-7: spans ≥3 temporal ranges (recent, day-old, week-old)', () => {
    const now = Date.now();
    const ages = getMockCommsThreads().map(t => now - new Date(t.timestamp).getTime());
    // within last hour
    expect(ages.some(ms => ms < 60 * 60_000)).toBe(true);
    // day-ish old (12-48h)
    expect(ages.some(ms => ms > 12 * 3600_000 && ms < 48 * 3600_000)).toBe(true);
    // week-ish old (≥5 days)
    expect(ages.some(ms => ms > 5 * 24 * 3600_000)).toBe(true);
  });

  it('FE-5: timestamps recomputed on each call (not frozen at module load)', async () => {
    const first = new Date(getMockCommsThreads()[0].timestamp).getTime();
    await new Promise(r => setTimeout(r, 5));
    const second = new Date(getMockCommsThreads()[0].timestamp).getTime();
    expect(second).toBeGreaterThanOrEqual(first);
  });

  it('SCRUM-3: security-relevant previews carry [MOCK] prefix', () => {
    const threads = getMockCommsThreads();
    const seraphThread = threads.find((t: MockThread) => t.from === 'SERAPH');
    expect(seraphThread).toBeDefined();
    expect(seraphThread!.preview).toContain('[MOCK]');
  });
});

describe('MOCK_WAVE_STATUS', () => {
  it('active_waves matches waves array length', () => {
    expect(MOCK_WAVE_STATUS.waves.length).toBe(MOCK_WAVE_STATUS.active_waves);
  });

  it('total_agents > 0', () => {
    expect(MOCK_WAVE_STATUS.total_agents).toBeGreaterThan(0);
  });

  it('coordinator field is set', () => {
    expect(MOCK_WAVE_STATUS.coordinator).toBeTruthy();
  });
});

describe('MOCK_DECISION_ENTRIES', () => {
  it('has ≥3 entries', () => {
    expect(MOCK_DECISION_ENTRIES.length).toBeGreaterThanOrEqual(3);
  });

  it('levels are valid taxonomy values (L1-L4)', () => {
    const valid = new Set(['L1', 'L2', 'L3', 'L4']);
    for (const e of MOCK_DECISION_ENTRIES) {
      expect(valid.has(e.level)).toBe(true);
    }
  });

  it('line_n is sequential from 0', () => {
    MOCK_DECISION_ENTRIES.forEach((e, i) => {
      expect(e.line_n).toBe(i);
    });
  });

  it('SCRUM-3: every decision string carries [MOCK] prefix', () => {
    for (const e of MOCK_DECISION_ENTRIES) {
      expect(e.decision.startsWith('[MOCK]')).toBe(true);
    }
  });
});

describe('MOCK_WORKTREES', () => {
  it('has ≥1 entry', () => {
    expect(MOCK_WORKTREES.length).toBeGreaterThanOrEqual(1);
  });

  it('every entry has required fields', () => {
    for (const w of MOCK_WORKTREES) {
      expect(w.path).toBeTruthy();
      expect(w.branch).toBeTruthy();
      expect(w.head_sha).toBeTruthy();
      expect(typeof w.locked).toBe('boolean');
      expect(() => new Date(w.created_at)).not.toThrow();
    }
  });

  it('status values are within enum', () => {
    const valid = new Set(['writing', 'gate', 'done', 'failed']);
    for (const w of MOCK_WORKTREES) {
      expect(valid.has(w.status)).toBe(true);
    }
  });
});
