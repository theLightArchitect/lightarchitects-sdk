import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { FEATURE_FLAGS, FLAG_TOOLTIP, isEnabled, type FeatureFlag } from '$lib/featureFlags';

const FLAG_KEYS: FeatureFlag[] = ['parallelismEnabled', 'commPubSubEnabled', 'multiProjectGateway'];

describe('featureFlags', () => {
  beforeEach(() => {
    localStorage.clear();
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  describe('FEATURE_FLAGS defaults', () => {
    it('all three flags default to false', () => {
      for (const key of FLAG_KEYS) {
        expect(FEATURE_FLAGS[key]).toBe(false);
      }
    });

    it('is immutable (const assertion)', () => {
      // TypeScript const assertion — verify the object exists with correct keys
      expect(Object.keys(FEATURE_FLAGS)).toHaveLength(3);
      expect(FEATURE_FLAGS).toHaveProperty('parallelismEnabled');
      expect(FEATURE_FLAGS).toHaveProperty('commPubSubEnabled');
      expect(FEATURE_FLAGS).toHaveProperty('multiProjectGateway');
    });
  });

  describe('FLAG_TOOLTIP', () => {
    it('has a tooltip for every flag', () => {
      for (const key of FLAG_KEYS) {
        expect(FLAG_TOOLTIP[key]).toBeDefined();
        expect(FLAG_TOOLTIP[key].length).toBeGreaterThan(0);
      }
    });
  });

  describe('isEnabled()', () => {
    it('returns false for all flags when no localStorage override', () => {
      for (const key of FLAG_KEYS) {
        expect(isEnabled(key)).toBe(false);
      }
    });

    it('returns true when localStorage override is "true"', () => {
      localStorage.setItem('la.feature.parallelismEnabled', 'true');
      expect(isEnabled('parallelismEnabled')).toBe(true);
    });

    it('returns false when localStorage override is "false" (explicit disable)', () => {
      localStorage.setItem('la.feature.commPubSubEnabled', 'false');
      expect(isEnabled('commPubSubEnabled')).toBe(false);
    });

    it('ignores localStorage override when value is not "true" or "false"', () => {
      // Any non-"true" value means no override → falls through to default
      localStorage.setItem('la.feature.multiProjectGateway', '');
      expect(isEnabled('multiProjectGateway')).toBe(false);
    });

    it('falls back to default when localStorage throws', () => {
      vi.stubGlobal('localStorage', {
        getItem: () => { throw new Error('storage blocked'); },
        setItem: vi.fn(),
        clear: vi.fn(),
        removeItem: vi.fn(),
        length: 0,
        key: vi.fn(),
      });
      expect(isEnabled('parallelismEnabled')).toBe(false);
    });
  });
});
