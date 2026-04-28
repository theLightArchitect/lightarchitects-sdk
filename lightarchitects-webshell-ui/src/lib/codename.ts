// ============================================================================
// Codename Generator — adjective-gerund-noun pattern
// Derived from 87 existing CORSO build codenames in the helix vault.
// ============================================================================

const ADJECTIVES = [
  'keen', 'swift', 'bold', 'bright', 'steady', 'fierce', 'noble', 'radiant',
  'golden', 'sacred', 'iron', 'sharp', 'calm', 'deep', 'silent', 'vigilant',
  'luminous', 'sovereign', 'precise', 'splendid', 'starry', 'temporal', 'vivid',
  'ethereal', 'gentle', 'logical', 'faithful', 'agile', 'silver', 'dreamy',
  'blazing', 'breezy', 'cheerful', 'clever', 'cozy', 'drifting', 'hardy',
  'misty', 'purrfect', 'quiet', 'sprightly', 'tempered', 'tireless', 'typed',
  'vast', 'zesty', 'abundant', 'binary', 'generic', 'indexed', 'melodic',
  'validated', 'wiggly', 'fortified',
];

const GERUNDS = [
  'forging', 'weaving', 'tracking', 'mining', 'bridging', 'sealing', 'nesting',
  'scribing', 'landing', 'watching', 'proving', 'harvesting', 'plotting',
  'kindling', 'merging', 'binding', 'purging', 'mending', 'sharpening',
  'unifying', 'breaching', 'grafting', 'indexing', 'singing', 'conducting',
  'floating', 'orbiting', 'spiraling', 'teaching', 'driving', 'building',
  'gathering', 'sourcing', 'stitching', 'connecting', 'spanning', 'unearthing',
  'chattering', 'hardening', 'ranging', 'tumbling', 'wiring', 'mapping',
  'dazzling', 'conjuring', 'tending', 'sprouting', 'greeting', 'beaming',
  'guarding', 'sweeping', 'stalking', 'whistling',
];

const NOUNS = [
  'hawk', 'eagle', 'wolf', 'phoenix', 'raven', 'spider', 'falcon', 'viper',
  'lion', 'sentinel', 'anvil', 'nautilus', 'cobra', 'dove', 'panther', 'osprey',
  'heron', 'whale', 'badger', 'bloodhound', 'lark', 'terrier', 'hydra',
  'chimera', 'lynx', 'owl', 'quasar', 'kestrel', 'seal', 'condor', 'ember',
  'wolverine', 'nightingale', 'jay', 'crane', 'scorpion', 'mastiff', 'quill',
  'reef', 'magpie', 'toast', 'urchin', 'boot', 'raptor', 'harrier', 'marble',
  'coral', 'beaver', 'fox', 'chameleon', 'petal', 'snail',
];

function pick<T>(arr: T[]): T {
  return arr[Math.floor(Math.random() * arr.length)];
}

/**
 * Generate a codename in adjective-gerund-noun format.
 * Optionally checks uniqueness against a list of existing codenames.
 *
 * @param existing - Set or array of existing codenames to avoid collisions
 * @param maxAttempts - Maximum retries before giving up (default 100)
 * @returns A unique codename string
 */
export function generateCodename(
  existing?: Set<string> | string[],
  maxAttempts = 100,
): string {
  const existingSet = existing instanceof Set
    ? existing
    : new Set(existing ?? []);

  for (let i = 0; i < maxAttempts; i++) {
    const codename = `${pick(ADJECTIVES)}-${pick(GERUNDS)}-${pick(NOUNS)}`;
    if (!existingSet.has(codename)) return codename;
  }

  // Fallback: append timestamp to guarantee uniqueness
  return `${pick(ADJECTIVES)}-${pick(GERUNDS)}-${pick(NOUNS)}-${Date.now().toString(36)}`;
}

/** Total combination space */
export const CODENAME_SPACE = ADJECTIVES.length * GERUNDS.length * NOUNS.length;
// ~50 * ~50 * ~50 = ~125,000 unique combinations
