// Lightspace demo timeline — condensed auto-playback sequence.
// Runs once on screen mount to showcase the lobby → workspace → cards flow.
// Replace with real SSE event wiring when the backend is live.

import { ls } from './state.svelte';
import type { MatPhaseId } from './types';

const MAT_PHASES: MatPhaseId[] = [
  'begin', 'rail_collapsed', 'grid_revealed',
  'drawer_revealed', 'cards_streaming', 'complete',
];

/** Run the full demo sequence. Returns a cleanup function. */
export function startDemo(): () => void {
  const timers: ReturnType<typeof setTimeout | typeof setInterval>[] = [];

  function after(ms: number, fn: () => void) {
    timers.push(setTimeout(fn, ms));
  }
  function every(ms: number, fn: () => void) {
    timers.push(setInterval(fn, ms));
    return timers[timers.length - 1] as ReturnType<typeof setInterval>;
  }

  // Phase 0 — typewriter fills lobby intent (~1.8s)
  const INTENT = 'Plan the Lightspace canvas — bento card system + SSE streaming + LASDLC plan drawer';
  let charIdx = 0;
  every(1800 / INTENT.length, () => {
    if (charIdx <= INTENT.length) ls.lobbyInput = INTENT.slice(0, charIdx++);
  });

  // Phase 1 — submit + materialize (2.6s)
  after(2600, () => {
    ls.exitLobby();
    MAT_PHASES.forEach((p, i) => after(i * 220, () => ls.setMatPhase(p)));
  });

  // Phase 2 — intent + first conversation (4s)
  after(4000, () => {
    ls.intentVerb = '/BUILD';
    ls.intentText = 'Lightspace canvas · bento card system + SSE streaming';
    ls.intentClass = 'platform-build';
    ls.currentPhase = 'phase-0-discover';

    ls.addConv({ who: 'operator', text: INTENT, time: '09:14' });
    ls.addConv({ who: 'copilot', text: 'Understood. Running QUANTUM prior-art sweep + SOUL helix search before drafting the plan.', time: '09:14' });
  });

  // Phase 3 — monitor card (5s)
  after(5000, () => {
    ls.addCard({
      id: 'c-monitor', kind: 'monitor', title: 'Session Health',
      body: `<div class="la-mon-grid">
        <div class="la-mon-cell"><span class="k">phase</span><span class="v ok">discover</span></div>
        <div class="la-mon-cell"><span class="k">budget</span><span class="v">$0.00 / $5.00</span></div>
        <div class="la-mon-cell"><span class="k">turns</span><span class="v">1 / 8</span></div>
        <div class="la-mon-cell"><span class="k">spans</span><span class="v ok">3</span></div>
      </div>`,
      prov: { agent: 'copilot' },
    });
    ls.tickSpan('session.health');
  });

  // Phase 4 — QUANTUM agent spawn (6.2s)
  after(6200, () => {
    ls.addCard({
      id: 'c-quantum', kind: 'agentspawn', title: 'QUANTUM · prior-art sweep',
      body: `<div class="la-agent-row">
        <span class="ag-name quantum">QUANTUM</span>
        <span class="ag-state">researching</span>
      </div>
      <div class="la-agent-prog"><div class="fill" style="width:42%"></div></div>
      <div class="ag-task">context7.query-docs(tokio::broadcast) · helix search: lightspace* prior plans</div>`,
      prov: { agent: 'quantum' },
    });
    ls.addConv({ who: 'quantum', agentClass: 'quantum',
      text: 'Found 3 prior-plan entries in helix. context7 returning broadcast::Sender docs now.', time: '09:15' });
    ls.tickSpan('quantum.spawn');
    ls.currentPhase = 'phase-1-plan';
  });

  // Phase 5 — research card (7.8s)
  after(7800, () => {
    ls.addCard({
      id: 'c-research', kind: 'research', title: 'context7 · tokio::broadcast',
      body: `<div class="la-cite-row">
        <span class="src">context7://tokio/broadcast</span>
        <span class="conf ok">0.94 VERIFIED</span>
      </div>
      <div class="la-cite-body">broadcast::Sender&lt;T&gt; ring buffer — cap 256, lag detection via RecvError::Lagged. Each canvas.reduce() subscriber receives events independently with backpressure.</div>`,
      prov: { agent: 'quantum', src: 'context7://tokio' },
      conf: { value: 0.94, tier: 'VERIFIED' },
    });
    ls.tickSpan('context7.broadcast');
  });

  // Phase 6 — trace card (9.5s)
  after(9500, () => {
    ls.addCard({
      id: 'c-trace', kind: 'trace', title: 'QUANTUM · discovery trace',
      body: `<div class="la-trace-list">
        <div class="la-trace-row"><span class="k">obs</span><span class="v">helix has 2 prior lightspace plans (archived+abandoned)</span></div>
        <div class="la-trace-row"><span class="k">thought</span><span class="v">canvas.reduce needs SSE broadcast + Svelte $state bridge</span></div>
        <div class="la-trace-row"><span class="k">action</span><span class="v">emit research card → proceed to plan draft</span></div>
      </div>`,
      prov: { agent: 'quantum' },
    });
    ls.addConv({ who: 'copilot', text: 'Prior art sweep complete. LASDLC template v2.5.1 loaded. Beginning plan draft with Phase 0 (Discover) complete.', time: '09:16' });
    ls.currentPhase = 'phase-2-design';
    ls.tickSpan('quantum.trace');
  });

  // Phase 7 — artifact + files (11.5s)
  after(11500, () => {
    ls.addCard({
      id: 'c-artifact', kind: 'artifact', title: 'lightspace-canvas.md · LASDLC LARGE',
      body: `<div class="la-doc-preview">
        <div class="la-doc-field"><span class="k">project</span><span class="v">lightarchitects-sdk</span></div>
        <div class="la-doc-field"><span class="k">codename</span><span class="v acc">lightarchitects-lightspace</span></div>
        <div class="la-doc-field"><span class="k">tier</span><span class="v">LARGE · 7 phases · [A+S+Q+C+O+P+K+D+T+R]</span></div>
        <div class="la-doc-field"><span class="k">northstar</span><span class="v dim">P1 + P4 — operator completes intent → canvas without terminal…</span></div>
      </div>`,
      prov: { agent: 'copilot', src: 'lightspace-canvas.md' },
    });
    ls.addFile({
      id: 'file-plan', name: 'lightspace-canvas.md', mime: 'md',
      meta: 'copilot · LASDLC LARGE plan draft', prov: { agent: 'copilot' },
    });
    ls.addConv({ who: 'copilot', text: 'Plan drafted: LARGE tier, 7 phases. XEA review queued. /SCRUM pending.', time: '09:17' });
    ls.lasdlcProject  = 'lightarchitects-sdk';
    ls.lasdlcCodename = 'lightarchitects-lightspace';
    ls.currentPhase = 'phase-3-build';
    ls.tickSpan('plan.draft');
  });

  // Budget tween — runs throughout
  let budgetTarget = 0;
  every(600, () => {
    budgetTarget = Math.min(2.34, budgetTarget + 0.08);
    ls.budget = budgetTarget;
    ls.tokens = Math.round(budgetTarget * 41_000);
  });

  return () => timers.forEach(t => { clearTimeout(t as ReturnType<typeof setTimeout>); clearInterval(t as ReturnType<typeof setInterval>); });
}
