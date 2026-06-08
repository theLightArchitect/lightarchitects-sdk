<script lang="ts">
  // LASDLC build plan schematic — middle panel.
  // Shows the phase ladder, graduated files, and cached cards.
  import { ls } from '$lib/lightspace/state.svelte';
  import { LASDLC_PHASES } from '$lib/lightspace/types';

  const phaseIndex = $derived(LASDLC_PHASES.findIndex(p => p.id === ls.currentPhase));

  function phaseState(idx: number): 'done' | 'active' | 'pending' {
    if (idx < phaseIndex) return 'done';
    if (idx === phaseIndex) return 'active';
    return 'pending';
  }
</script>

<aside class="la-schematic">
  <div class="la-schem-head">
    <span class="glyph">⧉</span>
    <span class="label">LA · Schematic</span>
    <span class="sub">build plan · {phaseIndex + 1}/{LASDLC_PHASES.length} phases</span>
    <button class="la-schem-collapse" onclick={() => ls.schemCollapsed = !ls.schemCollapsed} aria-label="Collapse schematic">‹</button>
  </div>

  <!-- Collapsed icon strip -->
  <div class="la-schem-iconstrip">
    <div class="la-schem-icon plan" title="LASDLC build plan">⧉</div>
    <div class="la-schem-icon" title="docs">md</div>
    <div class="la-schem-icon" title="diagrams">svg</div>
    <div class="la-schem-icon" title="contracts">yml</div>
  </div>

  <div class="la-schem-body">

    <!-- ── LASDLC phase ladder ─────────────────────────────────────── -->
    <div class="la-plan">
      {#if ls.lasdlcCodename}
        <div class="la-plan-meta">
          <span class="codename">{ls.lasdlcCodename}</span>
          <span class="tier">LARGE</span>
        </div>
      {:else}
        <div class="la-plan-empty">awaiting intent…</div>
      {/if}

      {#each LASDLC_PHASES as phase, idx}
        {@const ps = phaseState(idx)}
        <div class="la-plan-phase" data-state={ps}>
          <div class="la-plan-phase-head">
            <span class="la-phase-dot" class:done={ps === 'done'} class:active={ps === 'active'}></span>
            <span class="la-phase-name">{phase.name}</span>
            <div class="la-phase-gates">
              {#each phase.gates as gate}
                <span class="la-gate" class:pass={ps === 'done'}>{gate}</span>
              {/each}
            </div>
          </div>
        </div>
      {/each}
    </div>

    <!-- ── Files & Diagrams sub-drawer ───────────────────────────────── -->
    <div class="la-subdrawer" class:open={ls.filesOpen} class:closed={!ls.filesOpen}>
      <button class="la-subdrawer-head" onclick={() => ls.filesOpen = !ls.filesOpen}>
        <span class="chev">{ls.filesOpen ? '▾' : '▸'}</span>
        <span>Files &amp; Diagrams</span>
        <span class="count">{ls.files.length} file{ls.files.length === 1 ? '' : 's'}</span>
      </button>
      {#if ls.filesOpen && ls.files.length > 0}
        <div class="la-subdrawer-body">
          {#each ls.files as file (file.id)}
            <div class="la-file-row">
              <span class="la-file-mime {file.mime}">{file.mime.toUpperCase()}</span>
              <div class="la-file-info">
                <div class="la-file-name">{file.name}</div>
                <div class="la-file-meta">{file.meta}</div>
              </div>
            </div>
          {/each}
        </div>
      {/if}
    </div>

    <!-- ── Cached Cards sub-drawer ───────────────────────────────────── -->
    {#if ls.cachedCards.length > 0}
      <div class="la-subdrawer" class:open={ls.cachedOpen} class:closed={!ls.cachedOpen}>
        <button class="la-subdrawer-head" onclick={() => ls.cachedOpen = !ls.cachedOpen}>
          <span class="chev">{ls.cachedOpen ? '▾' : '▸'}</span>
          <span>Cached Cards</span>
          <span class="count">{ls.cachedCards.length} cached</span>
        </button>
        {#if ls.cachedOpen}
          <div class="la-subdrawer-body">
            {#each ls.cachedCards as card (card.id)}
              <div class="la-cache-row">
                <span class="la-cache-kind {card.kind}">{card.kind.slice(0, 5).toUpperCase()}</span>
                <div class="la-cache-info">
                  <div class="la-cache-title">{card.title}</div>
                </div>
                <button class="la-cache-restore" onclick={() => {
                  ls.addCard(card);
                  ls.cachedCards = ls.cachedCards.filter(c => c.id !== card.id);
                }} title="Restore to canvas">↩</button>
              </div>
            {/each}
          </div>
        {/if}
      </div>
    {/if}

  </div>
</aside>

<style>
.la-schematic {
  grid-area: schematic;
  display: flex; flex-direction: column;
  background: var(--la-bg-panel);
  border-right: 1px solid var(--la-hair-base);
  overflow: hidden; transition: opacity var(--la-slow);
}

.la-schem-head {
  display: flex; align-items: center; gap: 8px;
  padding: 9px 12px; border-bottom: 1px solid var(--la-hair-base);
  font-family: var(--la-font-display); font-weight: 700;
  font-size: 10px; letter-spacing: var(--la-tk-loose);
  color: var(--la-text-bright); text-transform: uppercase;
}
.la-schem-head .glyph { color: var(--la-acc2); }
.la-schem-head .sub {
  font-family: var(--la-font-mono); font-weight: 400;
  font-size: 9px; color: var(--la-text-mute);
  text-transform: none; letter-spacing: var(--la-tk-tight);
  margin-left: auto;
}
.la-schem-collapse {
  background: transparent; border: 0; color: var(--la-text-mute);
  cursor: pointer; font-family: var(--la-font-mono); font-size: 11px;
  transition: color var(--la-fast);
}
.la-schem-collapse:hover { color: var(--la-text-bright); }

.la-schem-iconstrip {
  display: none; flex-direction: column; gap: 8px;
  padding: 14px 8px; align-items: center;
}
:global(.la-root.schematic-collapsed) .la-schem-head .label,
:global(.la-root.schematic-collapsed) .la-schem-head .sub,
:global(.la-root.schematic-collapsed) .la-schem-body { display: none; }
:global(.la-root.schematic-collapsed) .la-schem-iconstrip { display: flex; }

.la-schem-icon {
  font-family: var(--la-font-mono); font-size: 8px;
  letter-spacing: var(--la-tk-mid); text-transform: uppercase;
  color: var(--la-text-dim); width: 28px; height: 22px;
  display: flex; align-items: center; justify-content: center;
  border: 1px solid var(--la-hair-base); background: var(--la-bg-card);
  cursor: pointer; border-radius: 2px;
}
.la-schem-icon.plan { color: var(--la-acc2); }

.la-schem-body { flex: 1; overflow-y: auto; scrollbar-width: thin; }
.la-schem-body::-webkit-scrollbar { width: 4px; }
.la-schem-body::-webkit-scrollbar-thumb { background: var(--la-hair-base); }

/* LASDLC plan */
.la-plan { padding: 10px 12px; }
.la-plan-empty {
  font-size: 10px; color: var(--la-text-ghost);
  font-family: var(--la-font-mono); letter-spacing: var(--la-tk-mid);
  text-transform: uppercase; padding: 12px 0;
}
.la-plan-meta {
  display: flex; gap: 8px; align-items: center;
  margin-bottom: 10px; padding-bottom: 8px;
  border-bottom: 1px solid var(--la-hair-faint);
}
.la-plan-meta .codename {
  font-family: var(--la-font-mono); font-size: 10px;
  color: var(--la-acc2); letter-spacing: var(--la-tk-tight);
}
.la-plan-meta .tier {
  font-family: var(--la-font-display); font-weight: 700;
  font-size: 8px; letter-spacing: var(--la-tk-loose);
  color: var(--la-text-mute); text-transform: uppercase;
}

.la-plan-phase { padding: 2px 0; }
.la-plan-phase-head {
  display: flex; align-items: center; gap: 8px;
  font-size: 10px; letter-spacing: var(--la-tk-tight);
  cursor: pointer; padding: 5px 6px; border-radius: 2px;
  transition: background var(--la-fast);
}
.la-plan-phase[data-state="active"] .la-plan-phase-head {
  background: rgba(77,142,255,0.08); color: var(--la-text-bright);
}
.la-plan-phase[data-state="done"]    .la-plan-phase-head { color: var(--la-text-mute); }
.la-plan-phase[data-state="pending"] .la-plan-phase-head { color: var(--la-text-ghost); }

.la-phase-dot {
  width: 6px; height: 6px; border-radius: 50%;
  background: var(--la-hair-strong); flex-shrink: 0;
}
.la-phase-dot.done   { background: var(--la-ok); box-shadow: 0 0 4px var(--la-ok); }
.la-phase-dot.active { background: var(--la-acc); box-shadow: 0 0 6px var(--la-acc); animation: ph-pulse 1.5s infinite; }
@keyframes ph-pulse { 50% { transform: scale(1.4); } }

.la-phase-name { flex: 1; font-size: 10px; }
.la-phase-gates { display: flex; gap: 3px; }
.la-gate {
  font-family: var(--la-font-display); font-weight: 700;
  font-size: 7px; letter-spacing: var(--la-tk-loose);
  color: var(--la-text-ghost);
  padding: 1px 4px; border: 1px solid var(--la-hair-faint); border-radius: 1px;
}
.la-gate.pass { color: var(--la-ok); border-color: rgba(57,255,138,0.25); }

/* Sub-drawers */
.la-subdrawer { border-top: 1px solid var(--la-hair-base); }
.la-subdrawer-head {
  display: flex; align-items: center; gap: 8px; padding: 8px 12px;
  font-size: 10px; letter-spacing: var(--la-tk-mid); text-transform: uppercase;
  color: var(--la-text-dim); cursor: pointer; width: 100%;
  background: transparent; border: 0; text-align: left;
  font-family: var(--la-font-mono); transition: color var(--la-fast);
}
.la-subdrawer-head:hover { color: var(--la-text-bright); }
.la-subdrawer-head .chev { font-size: 9px; color: var(--la-text-mute); }
.la-subdrawer-head .count { margin-left: auto; color: var(--la-text-mute); font-size: 9px; }
.la-subdrawer-body { padding: 6px 10px 10px; }

/* File rows */
.la-file-row {
  display: flex; align-items: center; gap: 8px;
  padding: 5px 7px; border-radius: 2px; cursor: pointer;
  transition: background var(--la-fast);
}
.la-file-row:hover { background: rgba(77,142,255,0.08); }
.la-file-mime {
  font-family: var(--la-font-display); font-weight: 700;
  font-size: 7px; letter-spacing: var(--la-tk-loose);
  color: var(--la-acc2); padding: 2px 4px;
  border: 1px solid rgba(169,138,255,0.3); border-radius: 1px;
}
.la-file-mime.rs { color: var(--la-err); border-color: rgba(255,77,106,0.3); }
.la-file-mime.ts { color: var(--la-info); border-color: rgba(138,169,255,0.3); }
.la-file-mime.svg { color: var(--la-acc3); border-color: rgba(255,209,102,0.3); }
.la-file-info { flex: 1; min-width: 0; }
.la-file-name { font-size: 10px; color: var(--la-text-bright); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.la-file-meta { font-size: 9px; color: var(--la-text-mute); margin-top: 1px; }

/* Cache rows */
.la-cache-row {
  display: flex; align-items: center; gap: 8px;
  padding: 5px 7px; border-radius: 2px;
}
.la-cache-kind {
  font-family: var(--la-font-display); font-weight: 700;
  font-size: 7px; letter-spacing: var(--la-tk-loose); color: var(--la-text-mute);
  padding: 2px 4px; border: 1px solid var(--la-hair-faint); border-radius: 1px;
}
.la-cache-info { flex: 1; min-width: 0; }
.la-cache-title { font-size: 10px; color: var(--la-text-dim); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.la-cache-restore {
  background: transparent; border: 1px solid var(--la-hair-base);
  color: var(--la-text-mute); font-family: var(--la-font-mono);
  font-size: 10px; cursor: pointer; padding: 2px 6px; border-radius: 2px;
  transition: all var(--la-fast);
}
.la-cache-restore:hover { color: var(--la-text-bright); border-color: var(--la-acc); }
</style>
