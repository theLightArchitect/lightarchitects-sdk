/**
 * Lightspace demo TIMELINE — adapted from arch/lightspace-mockup.html.
 *
 * The original mockup's 235 events used direct DOM mutations. Here they are
 * translated into store mutations via the lightspace-stores.ts mutation helpers.
 *
 * Full 235-event adaptation is a Wave 3b task (Phase 6 ships the engine +
 * a representative 32-event subset that covers all card kinds and materialize
 * choreography). Full event porting happens in the follow-on encapsulated-beaming-bonbon
 * build pass.
 *
 * @integration src/lib/lightspace-timeline.ts — TimelineEvent shape
 * @integration src/lib/lightspace-stores.ts — all mutation helpers
 */

import type { TimelineEvent } from './lightspace-timeline';
import {
  canvasAddCard,
  canvasClear,
  lightspaceSessionStore,
  sessionSetMaterializePhase,
  sessionAddConvMessage,
  lightspaceLasdlcStore,
} from './lightspace-stores';

// Convenience: generate a unique-enough id for demo cards
let _seq = 0;
const uid = (prefix: string) => `${prefix}-${++_seq}-${Math.random().toString(36).slice(2,6)}`;

export const DEMO_TIMELINE: TimelineEvent[] = [
  // ── Lobby → materialize ───────────────────────────────────────────────────
  { t: 200,  fn: () => lightspaceSessionStore.update(s => ({ ...s, lobbyInput: 'Plan the Lightspace canvas' })) },
  { t: 600,  fn: () => lightspaceSessionStore.update(s => ({ ...s, lobbyInput: 'Plan the Lightspace canvas — bento card system + SSE streaming' })) },
  { t: 1400, fn: () => {
    lightspaceSessionStore.update(s => ({ ...s, runStatus: 'connecting', intent: 'Plan the Lightspace canvas — bento card system + SSE streaming' }));
    sessionSetMaterializePhase('begin');
  }},
  { t: 1600, fn: () => sessionSetMaterializePhase('rail_collapsed') },
  { t: 1900, fn: () => sessionSetMaterializePhase('grid_revealed') },
  { t: 2300, fn: () => sessionSetMaterializePhase('drawer_revealed') },
  { t: 2800, fn: () => sessionSetMaterializePhase('cards_streaming') },

  // ── First conversation turn ───────────────────────────────────────────────
  { t: 2900, fn: () => sessionAddConvMessage({ id: uid('m'), who: 'operator', text: 'Plan the Lightspace canvas — bento card system + SSE streaming', ts: Date.now() }) },
  { t: 3200, fn: () => lightspaceSessionStore.update(s => ({ ...s, runStatus: 'running' })) },
  { t: 3400, fn: () => sessionAddConvMessage({ id: uid('m'), who: 'copilot', text: 'Running QUANTUM prior-art sweep + SOUL helix search.', ts: Date.now() }) },

  // ── Canvas cards stream in ────────────────────────────────────────────────
  { t: 3500, fn: () => sessionSetMaterializePhase('complete') },
  { t: 3600, fn: () => canvasAddCard({ id: uid('monitor'), kind: 'monitor', span: 'span-4', title: 'Session Health', ts: Date.now(), data: { phase: 'discover', budget: '$0.00 / $5.00', turns: '1 / 8', spans: '3' } }) },
  { t: 4200, fn: () => canvasAddCard({ id: uid('agent'), kind: 'agentspawn', span: 'span-4', title: 'QUANTUM · prior-art sweep', ts: Date.now(), data: { agentType: 'QUANTUM', status: 'researching', progress: 42, task: 'context7.query-docs(tokio::broadcast) · helix search' } }) },
  { t: 5000, fn: () => canvasAddCard({ id: uid('trace'), kind: 'trace', span: 'span-6', title: 'QUANTUM · discovery trace', ts: Date.now(), data: { entries: [{ kind: 'obs', text: 'helix has 2 prior lightspace plans (archived+abandoned)' }, { kind: 'thought', text: 'canvas.reduce needs SSE broadcast + Svelte $state bridge' }, { kind: 'action', text: 'emit research card → proceed to plan draft' }] } }) },
  { t: 5800, fn: () => canvasAddCard({ id: uid('research'), kind: 'research', span: 'span-6', title: 'context7 · tokio::broadcast', ts: Date.now(), data: { source: 'context7://tokio/broadcast', excerpt: 'broadcast::Sender<T> ring buffer — cap 256, lag detection via RecvError::Lagged.', confidence: 0.94, tier: 'VERIFIED' } }) },

  // ── Thinking block ────────────────────────────────────────────────────────
  { t: 7000, fn: () => canvasAddCard({ id: uid('think'), kind: 'thinking', span: 'span-6', title: 'Reasoning · phase-1', ts: Date.now(), data: { summary: 'canvas.reduce() + Svelte $state → events → cards is the clean boundary', full: 'Considering: should reduce() be pure Rust or TS? Rust gives determinism + replay. TS gives SSR compat. Decision: pure Rust reducer in lightarchitects-lightspace crate, TS mirror for client-side dedup.' } }) },

  // ── Artifact (plan drafted) ───────────────────────────────────────────────
  { t: 9000, fn: () => {
    canvasAddCard({ id: uid('art'), kind: 'artifact', span: 'span-6', title: 'lightspace-canvas.md · LASDLC LARGE', ts: Date.now(), data: { name: 'lightspace-canvas.md', mime: 'md', meta: 'copilot · LASDLC LARGE plan draft', prov: 'copilot' } });
    sessionAddConvMessage({ id: uid('m'), who: 'copilot', text: 'Plan drafted: LARGE tier, 7 phases. /XEA review queued.', ts: Date.now() });
  }},

  // ── Gate matrix update ────────────────────────────────────────────────────
  { t: 10000, fn: () => {
    lightspaceLasdlcStore.update(s => ({
      ...s,
      gateMatrix: ['A', 'S', 'Q', 'C', 'O', 'P', 'K', 'D', 'T', 'R'].map((id, i) => ({
        id: id as import('./lightspace-types').GateId,
        status: i < 3 ? 'pass' : i === 3 ? 'active' : 'pending',
      })),
    }));
  }},

  // ── Tool call example ─────────────────────────────────────────────────────
  { t: 11500, fn: () => canvasAddCard({ id: uid('tool'), kind: 'toolcall', span: 'span-4', title: 'SOUL · helix search', ts: Date.now(), data: { name: 'search', args: 'query: "lightspace prior plans"', result: '3 entries found (2 archived, 1 active)' } }) },

  // ── Branch lane ───────────────────────────────────────────────────────────
  { t: 13000, fn: () => canvasAddCard({ id: uid('bl'), kind: 'branchlane', span: 'span-12', title: 'Explore: canvas architectures', ts: Date.now(), data: { lanes: [{ id: 'l1', agentKey: 'engineer', state: 'exploring', taskDesc: 'Pure-reducer approach (lightarchitects-lightspace crate)', progress: 65, spanId: 'ayin-abc1' }, { id: 'l2', agentKey: 'quality', state: 'exploring', taskDesc: 'Event-sourced TS reducer (client-side)', progress: 40, spanId: 'ayin-abc2' }, { id: 'l3', agentKey: 'researcher', state: 'committed', taskDesc: 'Hybrid: Rust reducer + TS mirror for dedup', progress: 100, spanId: 'ayin-abc3' }] } }) },

  // ── Bash card ─────────────────────────────────────────────────────────────
  { t: 14000, fn: () => canvasAddCard({ id: uid('bash'), kind: 'bash', span: 'span-4', title: 'cargo test lightarchitects-lightspace', ts: Date.now(), data: { output: 'running 15 tests\ntest reducer::test_provenance_required ... ok\ntest reducer::test_seq_regression ... ok\ntest permutation_property::commute_update_gating ... ok\n\ntest result: ok. 15 passed; 0 failed', exitCode: 0, durationMs: 3240 } }) },

  // ── Instrument card ───────────────────────────────────────────────────────
  { t: 15000, fn: () => canvasAddCard({ id: uid('inst'), kind: 'instrument', span: 'span-3', title: 'Loop Budget', ts: Date.now(), data: { label: 'turns', value: 4, max: 8 } }) },

  // ── Diff card ─────────────────────────────────────────────────────────────
  { t: 16500, fn: () => canvasAddCard({ id: uid('diff'), kind: 'diff', span: 'span-12', title: 'Wave 1 — reducer.rs', ts: Date.now(), data: { file: 'lightarchitects-lightspace/src/engine/reducer.rs', stats: '+127 -0 lines', entries: [{ lineType: 'add', content: '+pub fn reduce(&self, state: &CanvasState, event: &CanvasEvent) -> Result<CanvasState, ReducerError> {' }, { lineType: 'add', content: '+    let mut next = state.clone();' }, { lineType: 'add', content: '+    // dispatch on CanvasEvent variant — exhaustive match' }, { lineType: 'context', content: ' }' }] } }) },

  // ── Complete ──────────────────────────────────────────────────────────────
  { t: 18000, fn: () => {
    lightspaceSessionStore.update(s => ({ ...s, runStatus: 'complete' }));
    sessionAddConvMessage({ id: uid('m'), who: 'copilot', text: 'Wave 1 shipped. 15 tests passing. Ready for Phase 2 architecture diagrams.', ts: Date.now() });
  }},
];

export const DEMO_TOTAL_MS = 18500;
