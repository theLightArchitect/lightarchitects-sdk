// Fzf-style client-side fuzzy scorer. No external dependency.
// Score hierarchy: exact (1.0) > prefix (0.85) > consecutive run > scattered.
// Items scoring below THRESHOLD are excluded from results.

const THRESHOLD = 0.1;

/**
 * Score `text` against `query`. Returns a value in [0, 1].
 * Empty query matches everything at score 1.0.
 */
export function score(query: string, text: string): number {
  if (!query) return 1.0;
  const q = query.toLowerCase();
  const t = text.toLowerCase();

  if (t === q) return 1.0;
  if (t.startsWith(q)) return 0.85 + 0.15 * (q.length / t.length);
  if (t.includes(q)) return 0.7 + 0.1 * (q.length / t.length);

  // Consecutive character run scoring
  let qi = 0;
  let consecutiveBonus = 0;
  let lastMatchIdx = -2; // -2 so ti=0 never triggers consecutive bonus on the first char
  let matchCount = 0;

  for (let ti = 0; ti < t.length && qi < q.length; ti++) {
    if (t[ti] === q[qi]) {
      qi++;
      matchCount++;
      if (lastMatchIdx === ti - 1) consecutiveBonus++;
      lastMatchIdx = ti;
    }
  }

  if (qi < q.length) return 0; // not all query chars found

  const coverage = matchCount / q.length;
  const density = matchCount / t.length;
  const consecutive = consecutiveBonus / Math.max(q.length - 1, 1);
  return THRESHOLD + 0.3 * coverage + 0.25 * density + 0.35 * consecutive;
}

/**
 * Rank an array of items by fuzzy score against `query`.
 * Items below threshold are excluded. Preserves stable order for ties.
 */
export function rank<T>(
  query: string,
  items: T[],
  getText: (item: T) => string,
): T[] {
  if (!query) return items;
  return items
    .map(item => ({ item, s: score(query, getText(item)) }))
    .filter(x => x.s >= THRESHOLD)
    .sort((a, b) => b.s - a.s)
    .map(x => x.item);
}
