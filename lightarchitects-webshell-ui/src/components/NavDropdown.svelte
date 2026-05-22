<script lang="ts">
  import { navigate } from '$lib/routes';
  import { currentRoute } from '$lib/stores';

  interface NavItem {
    label: string;
    hash: string;
    hint: string;
  }

  interface NavGroup {
    tier: string;
    icon: string;
    items: NavItem[];
  }

  const GROUPS: NavGroup[] = [
    {
      tier: 'Dashboard',
      icon: '⬡',
      items: [
        { label: 'Overview',        hash: '/dashboard', hint: 'Mission control — live agent activity, alerts, squad health' },
        { label: 'Cockpit',         hash: '/activity',  hint: 'Build cockpit — live health, escalations, fleet and decisions' },
      ],
    },
    {
      tier: 'Workspace',
      icon: '◈',
      items: [
        { label: 'Build Studio',    hash: '/builds',    hint: 'All builds — past, in-flight, and queued' },
        { label: 'Dispatch',        hash: '/run',       hint: 'Dispatch agents by domain — Engineer, Security, Ops (Cmd+K)' },
        { label: 'Intake',          hash: '/intake',    hint: 'Author plans and submit new builds' },
        { label: 'Editor',          hash: '/editor',    hint: 'Embedded code editor with agent copilot' },
        { label: 'Git',             hash: '/git',       hint: 'Git history, worktree state, branch operations' },
      ],
    },
    {
      tier: 'Knowledge',
      icon: '⬢',
      items: [
        { label: 'Helix',           hash: '/helix',     hint: 'Knowledge graph — agent memory strands and quality gates' },
        { label: 'Architecture',    hash: '/diagrams',  hint: 'Architecture intelligence — extract, verify, render diagrams' },
        { label: 'Diagram Library', hash: '/library',   hint: 'Pre-built diagram templates and architecture compositions' },
        { label: 'Roadmap',         hash: '/roadmap',   hint: 'Portfolio-level roadmap — active builds, phases, blockers' },
      ],
    },
    {
      tier: 'Settings',
      icon: '⬧',
      items: [
        { label: 'Tools',           hash: '/tools',     hint: 'MCP servers, squad agents, workspaces, meta-skills' },
      ],
    },
  ];

  const ALL_ITEMS = GROUPS.flatMap(g => g.items);

  let open = $state(false);
  let rootEl = $state<HTMLDivElement | null>(null);

  let activeRoute = $derived($currentRoute);

  function isActive(hash: string): boolean {
    if (hash === '/dashboard') return activeRoute.startsWith('/dashboard') || activeRoute.startsWith('/monitor') || activeRoute.startsWith('/ops');
    if (hash === '/run')       return activeRoute === '/' || activeRoute === '' || activeRoute.startsWith('/run') || activeRoute.startsWith('/dispatch');
    if (hash === '/builds')    return activeRoute.startsWith('/builds') || activeRoute.startsWith('/manage');
    if (hash === '/activity')  return activeRoute.startsWith('/activity') || activeRoute.startsWith('/comms');
    if (hash === '/diagrams')  return (activeRoute.startsWith('/diagrams') || activeRoute.startsWith('/arch')) && !activeRoute.includes('/library');
    if (hash === '/library')   return activeRoute.startsWith('/library') || activeRoute.startsWith('/diagrams/library');
    if (hash === '/helix')     return activeRoute.startsWith('/helix') || activeRoute.startsWith('/knowledge') || activeRoute.startsWith('/memory');
    if (hash === '/roadmap')   return activeRoute.startsWith('/roadmap');
    if (hash === '/intake')    return activeRoute.startsWith('/intake');
    if (hash === '/editor')    return activeRoute.startsWith('/editor');
    if (hash === '/git')       return activeRoute.startsWith('/git') || activeRoute.startsWith('/pull');
    if (hash === '/tools')     return activeRoute.startsWith('/tools');
    return activeRoute.startsWith(hash);
  }

  function activeTierIcon(): string {
    for (const g of GROUPS) {
      if (g.items.some(item => isActive(item.hash))) return g.icon;
    }
    return '⬡';
  }

  function activeTierLabel(): string {
    for (const g of GROUPS) {
      if (g.items.some(item => isActive(item.hash))) return g.tier;
    }
    return 'Dashboard';
  }

  function activeScreenLabel(): string {
    for (const item of ALL_ITEMS) {
      if (isActive(item.hash)) return item.label;
    }
    return 'Overview';
  }

  function pick(hash: string) {
    navigate(hash);
    open = false;
  }

  function onOutsideClick(e: MouseEvent) {
    if (!rootEl) return;
    if (!rootEl.contains(e.target as Node)) open = false;
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') open = false;
  }

  $effect(() => {
    if (open) {
      window.addEventListener('mousedown', onOutsideClick, { capture: true });
      window.addEventListener('keydown', onKeydown);
      return () => {
        window.removeEventListener('mousedown', onOutsideClick, { capture: true });
        window.removeEventListener('keydown', onKeydown);
      };
    }
  });
</script>

<div class="root" bind:this={rootEl}>
  <!-- ── Trigger ────────────────────────────────────────────────────────── -->
  <button
    onclick={() => { open = !open; }}
    aria-haspopup="true"
    aria-expanded={open}
    aria-label="Navigate screens"
    class="trigger"
    class:trigger--open={open}
  >
    <span class="t-icon" aria-hidden="true">{activeTierIcon()}</span>
    <span class="t-tier">{activeTierLabel()}</span>
    <span class="t-slash" aria-hidden="true">/</span>
    <span class="t-screen">{activeScreenLabel()}</span>
    <span class="t-chevron" class:t-chevron--open={open} aria-hidden="true">▾</span>
  </button>

  <!-- ── Dropdown panel ────────────────────────────────────────────────── -->
  {#if open}
    <div class="panel" role="menu" aria-label="Screen navigation">
      {#each GROUPS as group, gi}
        <div class="col" role="group" aria-label={group.tier}>
          <!-- Column header -->
          <div class="col-head">
            <span class="col-icon" aria-hidden="true">{group.icon}</span>
            <span class="col-label">{group.tier}</span>
          </div>
          <!-- Screen items -->
          {#each group.items as item}
            <button
              onclick={() => pick(item.hash)}
              title={item.hint}
              role="menuitem"
              class="item"
              class:item--active={isActive(item.hash)}
            >
              {item.label}
            </button>
          {/each}
          <!-- Fill remaining height so columns are equal length visually -->
          <div class="col-fill" aria-hidden="true"></div>
        </div>
      {/each}
    </div>
  {/if}
</div>

<style>
  /* ── Root container ──────────────────────────────────────────────────── */
  .root {
    position: relative;
    display: flex;
    align-items: stretch;
    flex-shrink: 0;
  }

  /* ── Trigger button ──────────────────────────────────────────────────── */
  .trigger {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 0 16px;
    height: 100%;
    font-family: var(--la-font-mono);
    font-size: 11px;
    border: none;
    border-right: 1px solid var(--la-hair-base, #2c3140);
    background: transparent;
    cursor: pointer;
    white-space: nowrap;
    color: var(--la-text-base, #c8d4de);
    transition: background 80ms, color 80ms;
  }

  .trigger:hover,
  .trigger--open {
    background: var(--la-bg-elev-1, #111214);
    color: var(--la-text-bright, #f1f5f9);
  }

  .t-icon {
    font-size: 9px;
    color: var(--la-text-mute, #6e7f8f);
    flex-shrink: 0;
  }

  .t-tier {
    font-size: 9px;
    letter-spacing: 0.12em;
    text-transform: uppercase;
    color: var(--la-text-mute, #6e7f8f);
    flex-shrink: 0;
  }

  .t-slash {
    color: var(--la-hair-strong, #44505e);
    font-size: 10px;
    flex-shrink: 0;
  }

  .t-screen {
    font-size: 11px;
    color: var(--la-text-bright, #f1f5f9);
    flex-shrink: 0;
  }

  .trigger--open .t-screen {
    color: var(--la-focus-ring, #FFD700);
  }

  .t-chevron {
    font-size: 8px;
    color: var(--la-text-mute, #6e7f8f);
    margin-left: 2px;
    flex-shrink: 0;
    display: inline-block;
    transition: transform 100ms var(--la-ease-mech, cubic-bezier(0.2, 0, 0.4, 1));
  }

  .t-chevron--open {
    transform: rotate(180deg);
    color: var(--la-focus-ring, #FFD700);
  }

  /* ── Dropdown panel ──────────────────────────────────────────────────── */
  .panel {
    position: absolute;
    top: 100%;
    left: 0;
    z-index: 50;
    display: flex;
    flex-direction: row;
    background: var(--la-bg-frame, #0c0d0e);
    border: 1px solid var(--la-hair-base, #2c3140);
    border-top: none;  /* flush with nav bottom border */
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.5);
    /* Geometric clip-path reveal from top */
    animation: panel-reveal 100ms var(--la-ease-mech, cubic-bezier(0.2, 0, 0.4, 1)) forwards;
  }

  @keyframes panel-reveal {
    from { clip-path: inset(0 0 100% 0); }
    to   { clip-path: inset(0 0 0% 0); }
  }

  /* ── Column ──────────────────────────────────────────────────────────── */
  .col {
    display: flex;
    flex-direction: column;
    min-width: 152px;
    border-right: 1px solid var(--la-hair-faint, #1c2028);
    flex-shrink: 0;
  }

  .col:last-child {
    border-right: none;
  }

  /* ── Column header ───────────────────────────────────────────────────── */
  .col-head {
    display: flex;
    align-items: center;
    gap: 6px;
    height: 32px;
    padding: 0 16px;
    border-bottom: 1px solid var(--la-hair-faint, #1c2028);
    flex-shrink: 0;
  }

  .col-icon {
    font-size: 9px;
    color: var(--la-hair-strong, #44505e);
  }

  .col-label {
    font-family: var(--la-font-mono);
    font-size: 9px;
    letter-spacing: 0.14em;
    text-transform: uppercase;
    color: var(--la-text-mute, #6e7f8f);
  }

  /* ── Screen item ─────────────────────────────────────────────────────── */
  .item {
    display: flex;
    align-items: center;
    height: 32px;
    padding: 0 16px;
    font-family: var(--la-font-mono);
    font-size: 11px;
    color: var(--la-text-dim, #a4b4c2);
    background: transparent;
    border: none;
    border-left: 2px solid transparent;
    cursor: pointer;
    text-align: left;
    width: 100%;
    transition: background 60ms, color 60ms;
    flex-shrink: 0;
  }

  .item:hover {
    background: var(--la-bg-elev-1, #111214);
    color: var(--la-text-bright, #f1f5f9);
  }

  .item--active {
    border-left-color: var(--la-focus-ring, #FFD700);
    background: rgba(255, 215, 0, 0.04);
    color: var(--la-focus-ring, #FFD700);
  }

  .item--active:hover {
    background: rgba(255, 215, 0, 0.08);
  }

  /* ── Column fill (structural spacer) ────────────────────────────────── */
  .col-fill {
    flex: 1;
    min-height: 8px;
  }

  /* ── Reduced motion ──────────────────────────────────────────────────── */
  @media (prefers-reduced-motion: reduce) {
    .panel { animation: none; clip-path: none; }
    .t-chevron { transition: none; }
    .trigger, .item { transition: none; }
  }
</style>
