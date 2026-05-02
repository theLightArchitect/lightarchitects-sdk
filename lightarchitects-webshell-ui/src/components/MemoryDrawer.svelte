<script lang="ts">
  import { hotMemory, coldMemory, memoryDrawerOpen, promotionFeed } from '$lib/stores';
  import { api } from '$lib/api';
  import type { ContextMemo, EnrichedHelixEntry } from '$lib/types';
  import { SIBLING_COLORS } from '$lib/design-tokens';
  import { onDestroy } from 'svelte';
  import Drawer from './Drawer.svelte';

  let tab = $state<'hot' | 'cold' | 'convergences'>('cold');
  let query = $state('');
  /** Phase 17a — retrieval strategy selector. Default `bm25` preserves
   *  the Phase-9 baseline. Changing the mode re-runs the current query. */
  type SearchMode = 'bm25' | 'semantic' | 'hybrid';
  let searchMode: SearchMode = $state('bm25');
  let rrfUsed = $state(false);
  let searching = $state(false);
  let searchResults = $state<EnrichedHelixEntry[] | null>(null);

  // Phase 13.3 — cross-sibling convergence view.
  type ConvergenceParticipant = {
    step_id: string;
    title: string | null;
    vault_path: string | null;
    sibling: string;
  };
  type Convergence = {
    id: string;
    weight: number;
    participant_count: number;
    discovered_by: string;
    label: string | null;
    created_at: string;
    participants: ConvergenceParticipant[];
    siblings: string[];
  };
  let convergences = $state<Convergence[] | null>(null);
  let convergencesTotal = $state<number>(0);
  let convergencesLoading = $state(false);

  async function loadConvergences() {
    convergencesLoading = true;
    try {
      const res = await api.getSoulConvergences(2, 50);
      convergences = res.convergences;
      convergencesTotal = res.total;
    } catch {
      convergences = [];
      convergencesTotal = 0;
    } finally {
      convergencesLoading = false;
    }
  }

  // Auto-fetch when the convergences tab becomes active.
  $effect(() => {
    if (tab === 'convergences' && convergences === null) {
      void loadConvergences();
    }
  });
  let selected = $state<{ memo: ContextMemo | null; raw: string | null }>({
    memo: null,
    raw: null,
  });
  let loadingDetail = $state(false);

  // Phase 11.5 — graph relationships for the selected entry.
  // Populated via /api/soul/relationships when a cold memo is selected and
  // Neo4j is attached. Null when not yet fetched; [] when Neo4j unavailable.
  type Neighbor = { id: string; title?: string; helix_id?: string; significance?: number };
  let relatedNeighbors = $state<Neighbor[] | null>(null);
  let relatedTier = $state<'neo4j' | 'none' | null>(null);

  // Phase 10.6 — tier-status pill. Polled every 30s when drawer is open.
  type TierStatus = { filesystem: boolean; sqlite: boolean; neo4j: boolean };
  type HealthResp = { tiers: TierStatus; counts: Record<string, number>; bolt_uri: string };
  let health = $state<HealthResp | null>(null);
  let healthTimer: ReturnType<typeof setInterval> | null = null;

  async function pollHealth() {
    try {
      health = await api.getSoulHealth();
    } catch {
      health = null;
    }
  }

  onDestroy(() => {
    if (healthTimer) clearInterval(healthTimer);
  });

  // Keyboard shortcut: Cmd+M / Ctrl+M toggles the drawer.
  // Escape is handled by the Drawer primitive.
  function onKeydown(e: KeyboardEvent) {
    if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'm') {
      e.preventDefault();
      memoryDrawerOpen.update(v => !v);
    }
  }

  async function refresh() {
    try {
      const [hot, cold] = await Promise.all([
        api.getHotMemory(50),
        api.getColdMemory(undefined, 50),
      ]);
      hotMemory.set(hot);
      coldMemory.set(cold);
    } catch {
      // Server offline — stores keep last-known values.
    }
  }

  async function runSearch() {
    if (!query.trim()) {
      searchResults = null;
      rrfUsed = false;
      return;
    }
    searching = true;
    try {
      const res = await api.searchSoul(query.trim(), 20, searchMode);
      searchResults = res.results;
      rrfUsed = res.rrf_used;
    } catch {
      searchResults = [];
      rrfUsed = false;
    } finally {
      searching = false;
    }
  }

  /** Re-run the active query when the user flips mode. No-op for empty input. */
  function setSearchMode(mode: SearchMode) {
    searchMode = mode;
    if (query.trim()) {
      runSearch();
    }
  }

  async function selectMemo(memo: ContextMemo) {
    selected = { memo, raw: null };
    relatedNeighbors = null;
    relatedTier = null;
    if (memo.tier === 'cold' && memo.source_path) {
      loadingDetail = true;
      try {
        const r = await api.getSoulEntry(memo.source_path);
        selected = { memo, raw: r.raw_markdown };
      } catch {
        selected = { memo, raw: '(failed to load entry)' };
      } finally {
        loadingDetail = false;
      }
      // Fetch graph relationships in parallel — non-blocking; UI renders as
      // soon as it arrives. Neo4j-unavailable case silently yields [].
      api.getSoulRelationships(memo.source_path)
        .then(g => {
          relatedNeighbors = g.neighbors;
          relatedTier = g.tier;
        })
        .catch(() => {
          relatedNeighbors = [];
          relatedTier = 'none';
        });
    }
  }

  async function selectEnrichedEntry(entry: EnrichedHelixEntry) {
    const memo: ContextMemo = {
      id: entry.path,
      tier: 'cold',
      content: entry.content_excerpt ?? '',
      significance: entry.significance ?? 0.5,
      sibling: entry.sibling,
      strands: entry.strands,
      created_at: entry.created_at ?? '',
      source_path: entry.path,
    };
    await selectMemo(memo);
  }

  function siblingColor(sibling: string): string {
    return (SIBLING_COLORS as Record<string, string>)[sibling] ?? 'var(--la-focus-ring)';
  }

  $effect(() => {
    if ($memoryDrawerOpen) {
      refresh();
      pollHealth();
      if (!healthTimer) {
        healthTimer = setInterval(pollHealth, 30_000);
      }
      // T2 — Memory Drawer triad guided tour. Fires on first open only.
      import('$lib/tutorial').then(({ runTutorial }) => runTutorial('t2'));
    } else if (healthTimer) {
      clearInterval(healthTimer);
      healthTimer = null;
    }
  });

  // Phase 14.3 — kind filter set. `null` means "all kinds" (no filter).
  // Kept as a Set of strings so toggling individual kinds in the chip row
  // is O(1). Canonical kinds live in KIND_ORDER (stable display order);
  // unknown kinds discovered in-session append to the tail.
  const KIND_ORDER = [
    'entry',
    'plan',
    'standard',
    'review',
    'lesson',
    'reference',
    'experience',
    'helix',
    'convergence',
    'milestone',
    'meeting',
  ];
  let activeKinds = $state<Set<string> | null>(null);
  const unfilteredList = $derived(tab === 'hot' ? $hotMemory : $coldMemory);
  const availableKinds = $derived.by(() => {
    const seen = new Set<string>();
    for (const m of unfilteredList) seen.add(m.entry_type ?? 'entry');
    // Stable order: canonical first, then alphabetical for unknowns.
    const canon = KIND_ORDER.filter(k => seen.has(k));
    const extras = [...seen].filter(k => !KIND_ORDER.includes(k)).sort();
    return [...canon, ...extras];
  });
  const list = $derived(
    activeKinds === null
      ? unfilteredList
      : unfilteredList.filter(m => activeKinds!.has(m.entry_type ?? 'entry'))
  );

  function toggleKind(k: string) {
    if (activeKinds === null) {
      // First click turns filter on with just this one kind selected.
      activeKinds = new Set([k]);
    } else if (activeKinds.has(k)) {
      const next = new Set(activeKinds);
      next.delete(k);
      activeKinds = next.size === 0 ? null : next;
    } else {
      activeKinds = new Set([...activeKinds, k]);
    }
  }

  function clearKindFilter() {
    activeKinds = null;
  }
  const isSearching = $derived(searchResults !== null);
</script>

<svelte:window onkeydown={onKeydown} />

<Drawer
  open={$memoryDrawerOpen}
  title="Memory"
  subtitle="working ({$hotMemory.length}) · archive ({$coldMemory.length})"
  onclose={() => memoryDrawerOpen.set(false)}
  testId="memory-drawer"
  headerOnboarding="memory-header"
>
  {#snippet actions()}
    {#if health}
      <div
        class="flex items-center gap-1 px-1.5 py-0.5 rounded-full border border-[var(--la-drawer-border)] bg-[var(--la-drawer-bg)]"
        title="Persistence tiers (fs / sqlite / neo4j)"
        data-testid="tier-badge"
        data-onboarding="memory-tier-badge"
      >
        <span
          class="w-1.5 h-1.5 rounded-full"
          style="background: {health.tiers.filesystem ? 'var(--la-agent-researcher)' : 'var(--la-text-dim)'}"
          title="filesystem {health.tiers.filesystem ? 'on' : 'off'}"
        ></span>
        <span
          class="w-1.5 h-1.5 rounded-full"
          style="background: {health.tiers.sqlite ? 'var(--la-agent-researcher)' : 'var(--la-text-dim)'}"
          title="sqlite {health.tiers.sqlite ? 'on' : 'off'}"
        ></span>
        <span
          class="w-1.5 h-1.5 rounded-full"
          style="background: {health.tiers.neo4j ? 'var(--la-agent-researcher)' : 'var(--la-text-dim)'}"
          title="neo4j {health.tiers.neo4j ? 'on' : 'off'}"
        ></span>
      </div>
    {/if}
  {/snippet}

    <!-- Tabs -->
    <div class="flex border-b border-[var(--la-drawer-border)] text-xs" data-onboarding="memory-tabs">
      <button
        class="flex-1 py-2 transition-colors {tab === 'hot' ? 'bg-[var(--la-focus-ring)] text-white' : 'text-[var(--la-text-label)] hover:bg-[var(--la-bg-elev-1)]'}"
        onclick={() => { tab = 'hot'; searchResults = null; }}
        data-testid="memory-tab-hot"
        data-onboarding="memory-tab-hot"
      >Working memory</button>
      <button
        class="flex-1 py-2 transition-colors {tab === 'cold' ? 'bg-[var(--la-focus-ring)] text-white' : 'text-[var(--la-text-label)] hover:bg-[var(--la-bg-elev-1)]'}"
        onclick={() => { tab = 'cold'; searchResults = null; }}
        data-testid="memory-tab-cold"
        data-onboarding="memory-tab-cold"
      >Archive</button>
      <button
        class="flex-1 py-2 transition-colors {tab === 'convergences' ? 'bg-[var(--la-focus-ring)] text-white' : 'text-[var(--la-text-label)] hover:bg-[var(--la-bg-elev-1)]'}"
        onclick={() => { tab = 'convergences'; searchResults = null; }}
        data-testid="memory-tab-convergences"
        data-onboarding="memory-tab-convergences"
      >Consolidations</button>
    </div>

    <!-- Search (Phase 17a: segmented mode control + input + rrf_used badge) -->
    <div class="px-3 py-2 border-b border-[var(--la-drawer-border)]" data-testid="search-block" data-onboarding="memory-search">
      <div class="flex gap-1 mb-1.5" data-testid="search-mode-row">
        {#each (['bm25', 'semantic', 'hybrid'] as const) as m}
          <button
            onclick={() => setSearchMode(m)}
            data-testid={`search-mode-${m}`}
            data-active={searchMode === m}
            class="text-[10px] uppercase tracking-wider px-2 py-0.5 rounded
                   {searchMode === m
                     ? 'bg-[var(--la-focus-ring)] text-white'
                     : 'bg-[var(--la-bg-elev-1)] border border-[var(--la-drawer-border)] text-[var(--la-text-label)] hover:border-[var(--la-focus-ring)]'}"
          >{m}</button>
        {/each}
        {#if rrfUsed}
          <span
            data-testid="rrf-used-badge"
            class="ml-auto text-[10px] uppercase tracking-wider px-2 py-0.5
                   rounded border border-[var(--la-agent-researcher)] text-[var(--la-agent-researcher)]"
          >RRF</span>
        {/if}
      </div>
      <input
        type="text"
        bind:value={query}
        onkeydown={(e) => { if (e.key === 'Enter') runSearch(); }}
        placeholder="Search SOUL vault… (Enter)"
        data-testid="search-input"
        class="w-full bg-[var(--la-bg-elev-1)] border border-[var(--la-drawer-border)] rounded px-2 py-1.5
               text-xs text-[var(--la-text-bright)] placeholder-[var(--la-text-dim)] outline-none
               focus:border-[var(--la-focus-ring)]"
      />
    </div>

    <!-- Phase 14.3 — kind filter chips. Click to restrict the list by
         typed-output classification. Multi-select (click more chips to
         union them). Tap "all" to clear. Hidden on Convergences tab
         (that tab has its own data shape). -->
    {#if tab !== 'convergences' && availableKinds.length > 0}
      <div
        class="px-3 py-1.5 border-b border-[var(--la-drawer-border)] flex flex-wrap gap-1"
        data-testid="kind-filter-row"
      >
        <button
          class="text-[9px] px-1.5 py-0.5 rounded-sm uppercase tracking-wide font-medium transition-colors
                 {activeKinds === null ? 'bg-[var(--la-focus-ring)] text-white' : 'bg-[var(--la-drawer-border)] text-[var(--la-text-dim)] hover:text-white'}"
          onclick={clearKindFilter}
          data-testid="kind-filter-all"
        >all</button>
        {#each availableKinds as k}
          <button
            class="text-[9px] px-1.5 py-0.5 rounded-sm uppercase tracking-wide font-medium transition-colors
                   {activeKinds?.has(k) ? 'bg-[var(--la-focus-ring)] text-white' : 'bg-[var(--la-drawer-border)] text-[var(--la-focus-ring)] hover:text-white'}"
            onclick={() => toggleKind(k)}
            data-testid="kind-filter-{k}"
          >{k}</button>
        {/each}
      </div>
    {/if}

    <!-- List -->
    <div class="flex-1 overflow-y-auto" data-testid="memory-list">
      {#if tab === 'convergences'}
        <!-- Phase 13.3 — cross-sibling SharedExperience view. Strongest convergences first;
             each row groups participants by sibling so the side-by-side-ness of the
             convergence is readable at a glance. -->
        {#if convergencesLoading}
          <div class="px-3 py-6 text-center text-[11px] text-[var(--la-text-dim)]">Loading…</div>
        {:else if convergences && convergences.length > 0}
          <div class="px-3 py-2 text-[10px] text-[var(--la-text-dim)] uppercase" data-testid="convergence-count-label">
            {convergences.length}{convergencesTotal > convergences.length ? ` of ${convergencesTotal}` : ''} convergences
          </div>
          {#each convergences as conv (conv.id)}
            <div
              class="px-3 py-2 border-b border-[var(--la-drawer-border)]/50"
              data-testid="convergence-row"
            >
              <!-- Header: label + weight bar -->
              <div class="flex items-center justify-between mb-1.5">
                <span class="text-[11px] text-[var(--la-text-bright)] truncate">
                  {conv.label ?? `Convergence ${conv.id.slice(0, 8)}`}
                </span>
                <span class="text-[9px] text-[var(--la-focus-ring)] whitespace-nowrap ml-2">
                  w {conv.weight.toFixed(2)}
                </span>
              </div>
              <!-- Sibling chips -->
              <div class="flex flex-wrap gap-1 mb-1">
                {#each conv.siblings as sib}
                  <span
                    class="text-[9px] px-1.5 py-0.5 rounded-sm font-semibold"
                    style="background: {siblingColor(sib)}22; color: {siblingColor(sib)}"
                  >{sib.toUpperCase()}</span>
                {/each}
              </div>
              <!-- Meta: participants + discovered_by -->
              <div class="flex items-center gap-2 text-[9px] text-[var(--la-text-dim)]">
                <span>{conv.participant_count} participants</span>
                <span>·</span>
                <span class="italic">{conv.discovered_by}</span>
              </div>
              <!-- Expandable participant list -->
              <details class="mt-1">
                <summary class="cursor-pointer text-[9px] text-[var(--la-focus-ring)] hover:text-[var(--la-focus-ring)]">
                  participants
                </summary>
                <div class="mt-1 space-y-0.5">
                  {#each conv.participants as p}
                    <div class="flex gap-1.5 text-[10px]">
                      <span
                        class="font-semibold shrink-0"
                        style="color: {siblingColor(p.sibling)}"
                      >{p.sibling}</span>
                      <span class="text-[var(--la-text-label)] truncate">
                        {p.title ?? p.vault_path ?? p.step_id.slice(0, 8)}
                      </span>
                    </div>
                  {/each}
                </div>
              </details>
            </div>
          {/each}
        {:else}
          <div class="px-3 py-6 text-center text-[11px] text-[var(--la-text-dim)]" data-testid="convergence-empty">
            No convergences yet.
            <br />
            <span class="text-[10px] text-[var(--la-hair-strong)]">
              SharedExperience nodes are populated by the nightly<br />
              `soul-consolidator` via Louvain community detection.<br />
              Run it once to see cross-Squad moments here.
            </span>
          </div>
        {/if}
      {:else if isSearching}
        <div class="px-3 py-2 text-[10px] text-[var(--la-text-dim)] uppercase">
          {searching ? 'Searching…' : `${searchResults?.length ?? 0} results`}
        </div>
        {#each searchResults ?? [] as entry (entry.path)}
          <button
            class="w-full text-left px-3 py-2 border-b border-[var(--la-drawer-border)]/50 hover:bg-[var(--la-bg-elev-1)] transition-colors"
            onclick={() => selectEnrichedEntry(entry)}
            data-testid="memory-row"
            data-kind={entry.entry_type ?? 'entry'}
          >
            <div class="flex items-center gap-2 mb-1">
              <span class="text-[10px] font-semibold" style="color: {siblingColor(entry.sibling)}">
                {entry.sibling.toUpperCase()}
              </span>
              {#if entry.significance !== undefined}
                <span class="text-[9px] text-[var(--la-text-dim)]">
                  {(entry.significance * 10).toFixed(1)}
                </span>
              {/if}
            </div>
            <p class="text-[11px] text-[var(--la-text-label)] line-clamp-2">
              {entry.content_excerpt ?? entry.path}
            </p>
          </button>
        {/each}
      {:else}
        {#each list as memo (memo.id)}
          <button
            class="w-full text-left px-3 py-2 border-b border-[var(--la-drawer-border)]/50 hover:bg-[var(--la-bg-elev-1)] transition-colors
                   {selected.memo?.id === memo.id ? 'bg-[var(--la-bg-elev-1)]' : ''}"
            onclick={() => selectMemo(memo)}
            data-testid="memory-row"
            data-kind={memo.entry_type ?? 'entry'}
          >
            <div class="flex items-center gap-2 mb-1 flex-wrap">
              <span class="text-[10px] font-semibold" style="color: {siblingColor(memo.sibling)}">
                {memo.sibling.toUpperCase()}
              </span>
              <span class="text-[9px] text-[var(--la-text-dim)]">
                {(memo.significance * 10).toFixed(1)}
              </span>
              {#if memo.entry_type && memo.entry_type !== 'entry'}
                <!-- Phase 14.1 — typed-output chip. `entry` is the default,
                     so only non-entry kinds get a chip to keep the row quiet. -->
                <span
                  class="text-[8px] px-1 py-0.5 rounded-sm uppercase tracking-wide
                         bg-[var(--la-drawer-border)] text-[var(--la-focus-ring)] font-medium"
                  data-testid="kind-chip"
                >{memo.entry_type}</span>
              {/if}
              {#if memo.strands.length > 0}
                <span class="text-[9px] text-[var(--la-text-dim)]">
                  {memo.strands.slice(0, 2).join(', ')}
                </span>
              {/if}
            </div>
            <p class="text-[11px] text-[var(--la-text-label)] line-clamp-2">{memo.content}</p>
          </button>
        {:else}
          <div class="px-3 py-6 text-center text-[11px] text-[var(--la-text-dim)]">
            {tab === 'hot'
              ? 'No active turnlog entries. Start a session to populate hot memory.'
              : 'No cold entries found. Promoted memos will appear here.'}
          </div>
        {/each}
      {/if}
    </div>

    <!-- Detail pane -->
    {#if selected.memo}
      <div class="border-t border-[var(--la-drawer-border)] bg-[var(--la-drawer-bg)] max-h-[40%] overflow-y-auto">
        <div class="px-3 py-2 border-b border-[var(--la-drawer-border)] flex items-center justify-between">
          <span class="text-[10px] font-semibold text-[var(--la-text-dim)]">ENTRY DETAIL</span>
          <button
            class="text-[var(--la-text-dim)] hover:text-white text-xs"
            onclick={() => { selected = { memo: null, raw: null }; }}
          >clear</button>
        </div>
        <div class="px-3 py-2 space-y-2 text-[11px]">
          <div class="flex items-center gap-2">
            <span class="text-[9px] text-[var(--la-text-dim)]">PATH</span>
            <span class="font-mono text-[10px] text-[var(--la-text-label)]">{selected.memo.source_path ?? selected.memo.id}</span>
          </div>
          <div class="flex items-center gap-2">
            <span class="text-[9px] text-[var(--la-text-dim)]">AGENT</span>
            <span style="color: {siblingColor(selected.memo.sibling)}">{selected.memo.sibling}</span>
            {#if selected.memo.self_defining}
              <span
                class="px-1 py-0 rounded bg-[var(--la-agent-performance)]/20 border border-[var(--la-agent-performance)]/40 text-[var(--la-agent-quality)] text-[9px]"
                title="Self-defining identity entry"
                data-testid="self-defining-badge"
              >★ identity</span>
            {/if}
            {#if selected.memo.entry_type}
              <span
                class="px-1 py-0 rounded bg-[var(--la-drawer-border)] text-[9px] text-[var(--la-text-dim)]"
                data-testid="entry-type-chip"
              >{selected.memo.entry_type}</span>
            {/if}
          </div>
          {#if selected.memo.strands.length > 0}
            <div data-testid="primitive-strands">
              <span class="text-[9px] text-[var(--la-text-dim)]">STRANDS</span>
              <div class="flex flex-wrap gap-1 mt-1">
                {#each selected.memo.strands as strand}
                  <span class="px-1.5 py-0.5 rounded bg-[var(--la-drawer-border)] text-[9px] text-[var(--la-text-label)]">{strand}</span>
                {/each}
              </div>
            </div>
          {/if}
          <!-- Phase 13.2 — zettelkasten primitives: themes, resonance -->
          {#if selected.memo.themes && selected.memo.themes.length > 0}
            <div data-testid="primitive-themes">
              <span class="text-[9px] text-[var(--la-text-dim)]">THEMES</span>
              <div class="flex flex-wrap gap-1 mt-1">
                {#each selected.memo.themes as theme}
                  <span class="px-1.5 py-0.5 rounded bg-[var(--la-agent-testing)]/40 border border-[var(--la-focus-ring)]/30 text-[9px] text-[var(--la-agent-testing)]">{theme}</span>
                {/each}
              </div>
            </div>
          {/if}
          {#if selected.memo.resonance && selected.memo.resonance.length > 0}
            <div data-testid="primitive-resonance">
              <span class="text-[9px] text-[var(--la-text-dim)]">RESONANCE</span>
              <div class="flex flex-wrap gap-1 mt-1">
                {#each selected.memo.resonance as r}
                  <span class="px-1.5 py-0.5 rounded bg-[var(--la-agent-ops)]/40 border border-[var(--la-agent-documentation)]/30 text-[9px] text-[var(--la-agent-documentation)]">{r}</span>
                {/each}
              </div>
            </div>
          {/if}
          {#if loadingDetail}
            <p class="text-[var(--la-text-dim)] italic text-[10px]">Loading…</p>
          {:else if selected.raw}
            <pre class="text-[10px] text-[var(--la-text-label)] whitespace-pre-wrap font-mono leading-relaxed">{selected.raw.slice(0, 2000)}{selected.raw.length > 2000 ? '\n\n…(truncated)' : ''}</pre>
          {:else}
            <p class="text-[var(--la-text-label)] whitespace-pre-wrap">{selected.memo.content}</p>
          {/if}

          <!-- Phase 11.5 — Related Entries via Neo4j graph walk -->
          {#if relatedNeighbors !== null}
            <div class="mt-2 pt-2 border-t border-[var(--la-drawer-border)]" data-testid="related-section">
              <div class="flex items-center gap-2 mb-1">
                <span class="text-[9px] text-[var(--la-text-dim)] uppercase">RELATED</span>
                <span class="text-[9px] text-[var(--la-text-dim)]">
                  {#if relatedTier === 'neo4j'}
                    {relatedNeighbors.length} via graph
                  {:else}
                    (Neo4j offline — no graph data)
                  {/if}
                </span>
              </div>
              {#if relatedNeighbors.length === 0 && relatedTier === 'neo4j'}
                <p class="text-[10px] text-[var(--la-text-dim)] italic">No backlinks found.</p>
              {/if}
              <div class="flex flex-wrap gap-1">
                {#each relatedNeighbors as neighbor (neighbor.id)}
                  <button
                    class="px-1.5 py-0.5 rounded border border-[var(--la-hair-strong)] bg-[var(--la-bg-elev-1)]
                           text-[9px] text-[var(--la-text-label)] hover:border-[var(--la-focus-ring)] transition-colors
                           max-w-[180px] truncate"
                    title={neighbor.id}
                    onclick={() => {
                      // Fetch the neighbor as a cold memo and re-select.
                      if (neighbor.id.includes('/entries/')) {
                        selectMemo({
                          id: neighbor.id,
                          tier: 'cold',
                          content: neighbor.title ?? neighbor.id,
                          significance: (neighbor.significance ?? 5) / 10,
                          sibling: neighbor.id.split('/')[0] ?? '?',
                          strands: [],
                          created_at: '',
                          source_path: neighbor.id,
                        });
                      }
                    }}
                  >
                    {neighbor.title ?? neighbor.id.split('/').pop() ?? neighbor.id}
                  </button>
                {/each}
              </div>
            </div>
          {/if}
        </div>
      </div>
    {/if}

  <!-- Promotion feed strip -->
  {#if $promotionFeed.length > 0}
    <div class="px-3 py-1.5 border-t border-[var(--la-drawer-border)] bg-[var(--la-focus-ring)]/10">
      <span class="text-[9px] text-[var(--la-agent-testing)]">
        ↑ {$promotionFeed.length} recent promotion{$promotionFeed.length === 1 ? '' : 's'}
      </span>
    </div>
  {/if}
</Drawer>
