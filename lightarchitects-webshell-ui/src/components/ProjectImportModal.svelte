<script lang="ts">
  import { authHeaders } from '$lib/auth';
  import { goto } from '$app/navigation';
  import type { ProjectCandidate, ProjectInitRequest } from '$lib/types';

  interface Props {
    onclose: () => void;
    onimported: (slug: string, path: string) => void;
  }

  let { onclose, onimported }: Props = $props();

  let candidates = $state<ProjectCandidate[]>([]);
  let loading = $state(true);
  let error = $state<string | null>(null);
  let filter = $state('');
  let selected = $state<ProjectCandidate | null>(null);
  let selectedSlug = $state('');
  let importing = $state(false);
  let importError = $state<string | null>(null);
  let imported = $state<{ slug: string; name: string; path: string } | null>(null);
  let slugInputEl = $state<HTMLInputElement | null>(null);

  let visible = $derived(
    candidates.filter((c) => {
      if (!filter) return true;
      return (
        c.name.toLowerCase().includes(filter.toLowerCase()) ||
        c.slug.toLowerCase().includes(filter.toLowerCase())
      );
    }),
  );

  let slugValid = $derived(/^[a-z0-9][a-z0-9-]{0,62}$/.test(selectedSlug));

  async function loadCandidates() {
    loading = true;
    error = null;
    try {
      const res = await fetch('/api/projects/browse', { headers: authHeaders() });
      if (!res.ok) throw new Error(`${res.status}`);
      candidates = await res.json();
    } catch (e) {
      error = e instanceof Error ? e.message : 'failed to load';
    } finally {
      loading = false;
    }
  }

  $effect(() => { loadCandidates(); });

  // Focus slug input when a new folder is selected
  $effect(() => {
    if (selected && slugInputEl) {
      setTimeout(() => slugInputEl?.focus(), 50);
    }
  });

  function select(c: ProjectCandidate) {
    if (c.initialized) return;
    selected = c;
    selectedSlug = c.slug;
    importError = null;
  }

  async function confirmImport() {
    if (!selected || !slugValid || importing) return;
    importing = true;
    importError = null;
    try {
      const body: ProjectInitRequest = { slug: selectedSlug, name: selected.name };
      const res = await fetch('/api/projects/init', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json', ...authHeaders() },
        body: JSON.stringify(body),
      });
      if (res.status === 409) {
        importError = `"${selectedSlug}" is already imported`;
        return;
      }
      if (!res.ok) {
        const data = await res.json().catch(() => ({}));
        importError = (data as { message?: string }).message ?? `error ${res.status}`;
        return;
      }
      imported = { slug: selectedSlug, name: selected.name, path: selected.path };
      onimported(selectedSlug, selected.path);
    } catch (e) {
      importError = e instanceof Error ? e.message : 'import failed';
    } finally {
      importing = false;
    }
  }

  function langBadges(c: ProjectCandidate): string[] {
    const out: string[] = [];
    if (c.has_git)          out.push('GIT');
    if (c.has_cargo_toml)   out.push('RUST');
    if (c.has_package_json) out.push('NODE');
    if (c.has_python)       out.push('PY');
    if (c.has_claude_md)    out.push('CC');
    return out;
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') { e.preventDefault(); onclose(); }
    if (e.key === 'Enter' && selected && !selected.initialized) {
      e.preventDefault();
      confirmImport();
    }
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div
  class="modal-backdrop"
  role="dialog"
  aria-modal="true"
  aria-label="Import project"
  onkeydown={handleKeydown}
>
  <div class="modal-panel">
    <!-- Header -->
    <div class="modal-header">
      <span class="modal-title">{imported ? 'PROJECT IMPORTED' : 'IMPORT PROJECT'}</span>
      <span class="modal-sub">~/Projects/ — click folder to select</span>
      <button class="modal-close" onclick={onclose} aria-label="Close">✕</button>
    </div>

    {#if imported}
      <!-- ── Success state ──────────────────────────────────────────────── -->
      <div class="success-body">
        <div class="success-icon">✓</div>
        <p class="success-name">{imported.name}</p>
        <p class="success-slug">slug: {imported.slug}</p>
        <p class="success-path">{imported.path}</p>
        <div class="success-actions">
          <button
            class="action-btn action-btn--primary"
            onclick={() => { onclose(); goto('/editor'); }}
          >Explore in Editor</button>
          <button
            class="action-btn action-btn--secondary"
            onclick={() => { onclose(); goto('/dispatch'); }}
          >Dispatch Agent</button>
          <button class="action-btn action-btn--ghost" onclick={onclose}>Done</button>
        </div>
        <p class="success-hint">
          No prerequisites required — any folder imports as a project.
          Run an engineer or researcher agent to analyse it.
        </p>
      </div>

    {:else}
      <!-- ── Browser state ───────────────────────────────────────────────── -->
      <div class="modal-search">
        <input
          class="search-input"
          type="search"
          placeholder="Filter folders…"
          bind:value={filter}
          autofocus
          aria-label="Filter project folders"
        />
        <button class="refresh-btn" onclick={loadCandidates} aria-label="Refresh">↺</button>
      </div>

      <div class="modal-body">
        {#if loading}
          <p class="state-msg">scanning ~/Projects/…</p>
        {:else if error}
          <p class="state-msg err">{error}</p>
        {:else if visible.length === 0}
          <p class="state-msg">no folders found{filter ? ' matching filter' : ''}</p>
        {:else}
          <ul class="candidate-list" role="listbox" aria-label="Project folders">
            {#each visible as c (c.name)}
              {@const badges = langBadges(c)}
              {@const isSelected = selected?.name === c.name}
              <li
                class="candidate-row"
                class:is-selected={isSelected}
                class:is-initialized={c.initialized}
                role="option"
                aria-selected={isSelected}
                aria-disabled={c.initialized}
                tabindex={c.initialized ? -1 : 0}
                onclick={() => select(c)}
                onkeydown={(e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); select(c); } }}
              >
                <div class="row-left">
                  {#if c.initialized}
                    <span class="row-check">✓</span>
                  {:else if isSelected}
                    <span class="row-check row-check--selected">◆</span>
                  {:else}
                    <span class="row-check row-check--empty">◇</span>
                  {/if}
                  <span class="candidate-name">{c.name}</span>
                </div>
                <div class="row-right">
                  {#each badges as badge}
                    <span class="lang-badge" data-lang={badge}>{badge}</span>
                  {/each}
                  {#if c.initialized}
                    <span class="badge-done">imported</span>
                  {:else if badges.length === 0}
                    <span class="badge-bare">bare</span>
                  {/if}
                </div>
              </li>
            {/each}
          </ul>
        {/if}
      </div>

      <!-- ── Confirm bar (shown when a folder is selected) ─────────────── -->
      {#if selected}
        <div class="confirm-bar" class:has-error={!!importError}>
          {#if importError}
            <span class="confirm-error" role="alert">{importError}</span>
          {/if}
          <div class="confirm-row">
            <span class="confirm-label">slug</span>
            <input
              bind:this={slugInputEl}
              class="confirm-slug-input"
              class:invalid={!slugValid}
              type="text"
              bind:value={selectedSlug}
              aria-label="Project slug"
              aria-invalid={!slugValid}
            />
            {#if !slugValid}
              <span class="confirm-slug-err">[a-z0-9-] only</span>
            {/if}
            <button
              class="confirm-btn"
              disabled={!slugValid || importing}
              onclick={confirmImport}
              aria-busy={importing}
            >
              {importing ? 'Importing…' : 'Init'}
            </button>
          </div>
          <p class="confirm-hint">
            No prerequisites checked — any folder can be initialised.
            Press Enter or click Init.
          </p>
        </div>
      {:else}
        <div class="footer-bar">
          <span class="footer-note">
            badges: GIT · RUST · NODE · PY · CC · bare = no markers (still importable)
          </span>
        </div>
      {/if}
    {/if}
  </div>
</div>

<style>
  /* ── Success ──────────────────────────────────────────────────────────── */

  .success-body {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
    padding: 32px 24px 28px;
    text-align: center;
  }

  .success-icon { font-size: 28px; color: #22c55e; line-height: 1; margin-bottom: 4px; }

  .success-name {
    font-size: 14px; font-weight: 700;
    color: var(--la-text-stark, #e2e8f0);
    letter-spacing: 0.04em; margin: 0;
  }

  .success-slug { font-size: 9px; color: var(--la-text-mute, #555570); margin: 0; letter-spacing: 0.08em; }

  .success-path {
    font-size: 9px; color: var(--la-text-dim, #8b949e);
    margin: 0 0 8px; word-break: break-all;
  }

  .success-actions { display: flex; gap: 8px; flex-wrap: wrap; justify-content: center; margin-top: 4px; }

  .action-btn {
    font-size: 9px; font-weight: 700; letter-spacing: 0.1em;
    text-transform: uppercase; padding: 6px 14px;
    font-family: inherit; cursor: pointer;
    transition: background 80ms, color 80ms, border-color 80ms;
  }

  .action-btn--primary {
    background: rgba(88,166,255,0.12); border: 1px solid var(--la-focus-ring, #58a6ff);
    color: var(--la-focus-ring, #58a6ff);
  }
  .action-btn--primary:hover { background: rgba(88,166,255,0.22); }

  .action-btn--secondary {
    background: rgba(167,139,250,0.1); border: 1px solid var(--la-agent-researcher, #a78bfa);
    color: var(--la-agent-researcher, #a78bfa);
  }
  .action-btn--secondary:hover { background: rgba(167,139,250,0.2); }

  .action-btn--ghost {
    background: none; border: 1px solid var(--la-hair-base, #1e1e2e);
    color: var(--la-text-mute, #555570);
  }
  .action-btn--ghost:hover { border-color: var(--la-hair-strong, #2a2a3a); color: var(--la-text-base, #c9d1d9); }

  .success-hint {
    font-size: 9px; color: var(--la-text-mute, #555570);
    font-style: italic; max-width: 380px; line-height: 1.6; margin: 8px 0 0;
  }

  /* ── Modal shell ──────────────────────────────────────────────────────── */

  .modal-backdrop {
    position: fixed; inset: 0; z-index: 500;
    background: rgba(0,0,0,0.72);
    display: flex; align-items: center; justify-content: center;
    animation: fade-in 120ms ease-out;
  }

  @keyframes fade-in { from { opacity: 0; } to { opacity: 1; } }

  .modal-panel {
    width: min(580px, 92vw); max-height: 80vh;
    display: flex; flex-direction: column;
    background: var(--la-bg-elev-1, #0f0f1a);
    border: 1px solid var(--la-hair-strong, #2a2a3a);
    box-shadow: 0 24px 64px rgba(0,0,0,0.7);
    font-family: var(--la-font-mono, monospace);
    animation: slide-up 140ms ease-out;
  }

  @keyframes slide-up {
    from { transform: translateY(12px); opacity: 0; }
    to   { transform: translateY(0);    opacity: 1; }
  }

  .modal-header {
    display: flex; align-items: baseline; gap: 10px;
    padding: 12px 14px 10px;
    border-bottom: 1px solid var(--la-hair-faint, rgba(255,255,255,0.06));
    flex-shrink: 0;
  }

  .modal-title { font-size: 10px; font-weight: 700; letter-spacing: 0.14em; color: var(--la-text-stark, #e2e8f0); }
  .modal-sub   { flex: 1; font-size: 9px; color: var(--la-text-mute, #555570); letter-spacing: 0.06em; }

  .modal-close {
    background: none; border: none; color: var(--la-text-mute, #555570);
    cursor: pointer; font-size: 11px; padding: 2px 4px; line-height: 1;
  }
  .modal-close:hover { color: var(--la-text-base, #c9d1d9); }

  .modal-search {
    display: flex; align-items: center; gap: 6px;
    padding: 8px 14px;
    border-bottom: 1px solid var(--la-hair-faint, rgba(255,255,255,0.06));
    flex-shrink: 0;
  }

  .search-input {
    flex: 1; font-size: 11px; font-family: inherit; padding: 5px 8px;
    background: var(--la-bg-frame, #0a0a14);
    border: 1px solid var(--la-hair-base, #1e1e2e);
    color: var(--la-text-primary, #e2e8f0); outline: none;
  }
  .search-input:focus { border-color: var(--la-focus-ring, #58a6ff); }

  .refresh-btn {
    background: none; border: 1px solid var(--la-hair-base, #1e1e2e);
    color: var(--la-text-mute, #555570); cursor: pointer;
    font-size: 12px; padding: 4px 7px; font-family: inherit;
  }
  .refresh-btn:hover { color: var(--la-text-base, #c9d1d9); }

  /* ── Candidate list ───────────────────────────────────────────────────── */

  .modal-body { flex: 1; overflow-y: auto; min-height: 0; }

  .state-msg { font-size: 10px; color: var(--la-text-mute, #555570); padding: 20px 14px; font-style: italic; }
  .state-msg.err { color: var(--la-agent-security, #ef4444); }

  .candidate-list { list-style: none; margin: 0; padding: 4px 0; }

  .candidate-row {
    display: flex; align-items: center; justify-content: space-between;
    padding: 7px 14px; gap: 12px;
    border-bottom: 1px solid var(--la-hair-faint, rgba(255,255,255,0.04));
    cursor: pointer; outline: none;
    transition: background 60ms;
  }

  .candidate-row:hover:not(.is-initialized) { background: var(--la-bg-elev-2, rgba(255,255,255,0.04)); }
  .candidate-row:focus-visible { box-shadow: inset 0 0 0 1px var(--la-focus-ring, #58a6ff); }

  .candidate-row.is-selected {
    background: rgba(88,166,255,0.07);
    border-bottom-color: rgba(88,166,255,0.15);
  }

  .candidate-row.is-initialized { cursor: default; opacity: 0.45; }

  .row-left  { display: flex; align-items: center; gap: 8px; min-width: 0; }
  .row-right { display: flex; align-items: center; gap: 4px; flex-shrink: 0; }

  .row-check { font-size: 10px; width: 12px; flex-shrink: 0; color: var(--la-text-mute, #555570); }
  .row-check--selected { color: var(--la-focus-ring, #58a6ff); }
  .row-check--empty    { color: var(--la-hair-strong, #2a2a3a); }
  .is-initialized .row-check { color: #22c55e; }

  .candidate-name {
    font-size: 11px; font-weight: 600;
    color: var(--la-text-stark, #e2e8f0); letter-spacing: 0.04em;
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }

  .lang-badge {
    font-size: 7px; font-weight: 700; letter-spacing: 0.1em;
    padding: 1px 4px; border-radius: 2px;
    background: rgba(255,255,255,0.06); color: var(--la-text-dim, #8b949e);
  }

  .lang-badge[data-lang="GIT"]  { color: #f97316; background: rgba(249,115,22,0.1); }
  .lang-badge[data-lang="RUST"] { color: #f87171; background: rgba(248,113,113,0.1); }
  .lang-badge[data-lang="NODE"] { color: #4ade80; background: rgba(74,222,128,0.1); }
  .lang-badge[data-lang="PY"]   { color: #facc15; background: rgba(250,204,21,0.1); }
  .lang-badge[data-lang="CC"]   { color: #a78bfa; background: rgba(167,139,250,0.1); }

  .badge-done { font-size: 7px; font-weight: 700; letter-spacing: 0.1em; color: #22c55e; }
  .badge-bare { font-size: 7px; letter-spacing: 0.06em; color: var(--la-text-mute, #555570); font-style: italic; }

  /* ── Confirm bar ──────────────────────────────────────────────────────── */

  .confirm-bar {
    display: flex; flex-direction: column; gap: 6px;
    padding: 10px 14px;
    border-top: 1px solid var(--la-focus-ring, #58a6ff);
    background: rgba(88,166,255,0.04);
    flex-shrink: 0;
  }

  .confirm-bar.has-error { border-top-color: var(--la-agent-security, #ef4444); }

  .confirm-error {
    font-size: 9px; color: var(--la-agent-security, #ef4444);
    letter-spacing: 0.06em;
  }

  .confirm-row { display: flex; align-items: center; gap: 8px; }

  .confirm-label {
    font-size: 8px; font-weight: 700; letter-spacing: 0.12em;
    color: var(--la-text-mute, #555570); text-transform: uppercase;
    width: 28px; flex-shrink: 0;
  }

  .confirm-slug-input {
    flex: 1; max-width: 240px; font-size: 11px; font-family: inherit;
    padding: 4px 7px;
    background: var(--la-bg-frame, #0a0a14);
    border: 1px solid var(--la-hair-base, #1e1e2e);
    color: var(--la-text-primary, #e2e8f0); outline: none;
  }
  .confirm-slug-input:focus { border-color: var(--la-focus-ring, #58a6ff); }
  .confirm-slug-input.invalid { border-color: var(--la-agent-security, #ef4444); }

  .confirm-slug-err { font-size: 8px; color: var(--la-agent-security, #ef4444); }

  .confirm-btn {
    font-size: 9px; font-weight: 700; letter-spacing: 0.12em;
    text-transform: uppercase; padding: 5px 16px;
    background: var(--la-focus-ring, #58a6ff);
    border: none; color: #000; cursor: pointer; font-family: inherit;
    transition: opacity 80ms;
  }
  .confirm-btn:hover:not(:disabled) { opacity: 0.85; }
  .confirm-btn:disabled { opacity: 0.35; cursor: not-allowed; }

  .confirm-hint {
    font-size: 8px; color: var(--la-text-mute, #555570);
    font-style: italic; margin: 0; letter-spacing: 0.04em;
  }

  /* ── Footer (no selection) ────────────────────────────────────────────── */

  .footer-bar {
    padding: 8px 14px;
    border-top: 1px solid var(--la-hair-faint, rgba(255,255,255,0.06));
    flex-shrink: 0;
  }

  .footer-note { font-size: 8px; color: var(--la-text-mute, #555570); letter-spacing: 0.06em; font-style: italic; }
</style>
