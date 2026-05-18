import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import {
  FEATURE_FLAGS, FLAG_TOOLTIP, isEnabled, isSafeMode, isGitForestFlagEnabled,
  type FeatureFlag,
} from '$lib/featureFlags';

/** Follow-up-build flags — all default false. */
const FLAG_KEYS: FeatureFlag[] = ['parallelismEnabled', 'commPubSubEnabled', 'multiProjectGateway'];
/** GitForest live-ops flags — default true. */
const GITFOREST_FLAGS: FeatureFlag[] = ['pulseEnabled', 'statsTopbarEnabled'];

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
      expect(Object.keys(FEATURE_FLAGS)).toHaveLength(5);
      expect(FEATURE_FLAGS).toHaveProperty('parallelismEnabled');
      expect(FEATURE_FLAGS).toHaveProperty('commPubSubEnabled');
      expect(FEATURE_FLAGS).toHaveProperty('multiProjectGateway');
      expect(FEATURE_FLAGS).toHaveProperty('pulseEnabled');
      expect(FEATURE_FLAGS).toHaveProperty('statsTopbarEnabled');
    });
  });

  describe('FLAG_TOOLTIP', () => {
    it('has a tooltip for every flag', () => {
      const allFlags = [...FLAG_KEYS, ...GITFOREST_FLAGS];
      for (const key of allFlags) {
        expect(FLAG_TOOLTIP[key]).toBeDefined();
        expect(FLAG_TOOLTIP[key].length).toBeGreaterThan(0);
      }
    });
  });

  describe('GitForest flags', () => {
    it('pulseEnabled and statsTopbarEnabled default to true', () => {
      for (const key of GITFOREST_FLAGS) {
        expect(FEATURE_FLAGS[key]).toBe(true);
      }
    });

    it('isEnabled returns true for GitForest flags when no localStorage override', () => {
      for (const key of GITFOREST_FLAGS) {
        expect(isEnabled(key)).toBe(true);
      }
    });

    it('isEnabled returns false when localStorage overrides GitForest flag to "false"', () => {
      localStorage.setItem('la.feature.pulseEnabled', 'false');
      expect(isEnabled('pulseEnabled')).toBe(false);
    });

    it('isSafeMode returns false when ?safe param is absent', () => {
      expect(isSafeMode()).toBe(false);
    });

    it('isGitForestFlagEnabled mirrors isEnabled when not in safe mode', () => {
      expect(isGitForestFlagEnabled('pulseEnabled')).toBe(true);
      expect(isGitForestFlagEnabled('statsTopbarEnabled')).toBe(true);
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
