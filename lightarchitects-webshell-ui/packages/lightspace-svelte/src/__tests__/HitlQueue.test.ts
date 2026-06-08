import { describe, it, expect } from 'vitest';
import { formatAge } from '../components/HitlQueue.svelte';

describe('HitlQueue — formatAge', () => {
  it('returns "< 1 min" for t=0 (0 seconds)', () => {
    expect(formatAge(0)).toBe('< 1 min');
  });

  it('returns "< 1 min" for 59 seconds', () => {
    expect(formatAge(59)).toBe('< 1 min');
  });

  it('returns "1 min" for exactly 60 seconds', () => {
    expect(formatAge(60)).toBe('1 min');
  });

  it('returns "8 min" for t=8min (480 seconds)', () => {
    expect(formatAge(480)).toBe('8 min');
  });

  it('returns "59 min" for 3599 seconds', () => {
    expect(formatAge(3599)).toBe('59 min');
  });

  it('returns "1 hr" for exactly 3600 seconds', () => {
    expect(formatAge(3600)).toBe('1 hr');
  });

  it('returns "2 hr" for 7200 seconds', () => {
    expect(formatAge(7200)).toBe('2 hr');
  });
});
