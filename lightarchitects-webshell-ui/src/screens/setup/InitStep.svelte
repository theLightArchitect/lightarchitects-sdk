<script lang="ts">
  import { step, setupComplete } from '$lib/setup';

  const subsystems = [
    { polytope: 'Pentachoron', sibling: 'QUANTUM' },
    { polytope: 'Tesseract', sibling: 'AYIN' },
    { polytope: 'Hexadecachoron', sibling: 'CORSO' },
    { polytope: 'Icositetrachoron', sibling: 'LÆX' },
    { polytope: 'Rectified 5-Cell', sibling: 'EVA' },
    { polytope: 'Duoprism', sibling: 'SERAPH' },
  ];

  let activeIdx = $state(0);
  let doneIdxs = $state<number[]>([]);

  $effect(() => {
    let idx = 0;
    const tick = () => {
      if (idx >= subsystems.length) {
        setupComplete.set(true);
        step.set('done');
        return;
      }
      activeIdx = idx;
      setTimeout(() => {
        doneIdxs = [...doneIdxs, idx];
        idx++;
        setTimeout(tick, 80);
      }, 300);
    };
    setTimeout(tick, 200);
  });
</script>

<div class="step">
  <h2 class="title">Initialising Subsystems</h2>

  <div class="checklist">
    {#each subsystems as s, i}
      <div class="row" class:active={activeIdx === i && !doneIdxs.includes(i)} class:done={doneIdxs.includes(i)}>
        <div class="indicator">
          {#if doneIdxs.includes(i)}
            <span class="checkmark">✓</span>
          {:else if activeIdx === i}
            <span class="spinner">◌</span>
          {:else}
            <span class="pending">○</span>
          {/if}
        </div>
        <div class="row-info">
          <span class="polytope-name">{s.polytope}</span>
          <span class="sibling-name">/ {s.sibling}</span>
        </div>
      </div>
    {/each}
  </div>
</div>

<style>
  .step { display:flex; flex-direction:column; align-items:center; gap:2rem; padding:2rem; height:100vh; justify-content:center; }
  .title { font-family:'Raleway',sans-serif; font-size:1.75rem; font-weight:700; color:#e2e8f0; margin:0; letter-spacing:0.05em; }

  .checklist { display:flex; flex-direction:column; gap:0.6rem; width:320px; }
  .row { display:flex; align-items:center; gap:0.75rem; padding:0.5rem 0.75rem; border-radius:6px; transition:background 0.2s; }
  .row.active { background:rgba(255,102,0,0.08); }
  .row.done { opacity:0.6; }

  .indicator { width:1.2rem; text-align:center; font-size:1rem; }
  .checkmark { color:#00d26a; }
  .spinner { color:#ff6600; animation:spin 0.8s linear infinite; display:inline-block; }
  .pending { color:#334155; }
  @keyframes spin { from { transform:rotate(0deg); } to { transform:rotate(360deg); } }

  .row-info { display:flex; gap:0.5rem; align-items:baseline; }
  .polytope-name { font-family:'IBM Plex Mono',monospace; font-size:0.8rem; color:#94a3b8; }
  .sibling-name { font-family:'IBM Plex Mono',monospace; font-size:0.7rem; color:#475569; }
</style>
