// Lightspace workspace state — Svelte 5 rune-based singleton.
// Import `ls` in any component; reads are reactive automatically.

import type {
  Card, ConvMsg, LightspaceFile, RecentSession,
  LasdlcPhaseId, MatPhaseId, BranchLane, ViewPreset,
} from './types';
import { KIND_TO_TIER } from './types';

/** Maximum canvas cards before auto-eviction. */
const MAX_CANVAS_CARDS = 9;

export class LightspaceState {
  // ── Lifecycle ──────────────────────────────────────────────────────
  inLobby       = $state(true);
  materializing = $state(false);
  isShipped     = $state(false);
  railCollapsed  = $state(false);
  schemCollapsed = $state(false);

  // ── Lobby ──────────────────────────────────────────────────────────
  lobbyInput    = $state('');
  recentSessions = $state<RecentSession[]>([
    { id: 'b704d6e', summary: 'copilot-chatroom-core · L1 + chatroom merge · 3033 tests passing', ago: '2d' },
    { id: 'd783ed2', summary: 'unified-litellm-router · provider switching shipped', ago: '5d' },
    { id: 'e20ab1e', summary: 'webshell-supervisor-autonomy · XEA 92 EXEMPLARY', ago: '9d' },
  ]);

  // ── Materialize phases ─────────────────────────────────────────────
  matPhase      = $state<MatPhaseId | null>(null);
  matPhasesSeen = $state<Set<MatPhaseId>>(new Set());

  // ── Topbar ──────────────────────────────────────────────────────────
  wsState   = $state('idle · awaiting intent');
  budget    = $state(0.00);
  sessionId = $state('39b72c26');
  cwd       = $state('~/Projects/lightarchitects-sdk');

  // ── Intent (canvas header) ─────────────────────────────────────────
  intentVerb  = $state('');
  intentText  = $state('');
  intentClass = $state('—');

  // ── Canvas ──────────────────────────────────────────────────────────
  cards       = $state<Card[]>([]);
  viewPreset  = $state<ViewPreset>('all');
  expandedCardId = $state<string | null>(null);
  highlightCardId = $state<string | null>(null);

  // ── Conversation rail ─────────────────────────────────────────────
  conv = $state<ConvMsg[]>([]);

  // ── Schematic / LASDLC ────────────────────────────────────────────
  currentPhase   = $state<LasdlcPhaseId>('phase-0-discover');
  lasdlcCodename = $state<string | null>(null);
  lasdlcProject  = $state<string | null>(null);
  files          = $state<LightspaceFile[]>([]);
  filesOpen      = $state(false);
  cachedCards    = $state<Card[]>([]);
  cachedOpen     = $state(false);

  // ── Status rail ───────────────────────────────────────────────────
  spans       = $state(0);
  lastEvent   = $state('—');
  throughput  = $state(0.0);
  throughputHistory = $state<number[]>([]);
  contradictions = $state(0);
  tokens      = $state(0);

  // ── Branch lanes ─────────────────────────────────────────────────
  branchLanes = $state<BranchLane[] | null>(null);

  // ── Root CSS class ────────────────────────────────────────────────
  readonly rootClass = $derived([
    'la-root',
    this.inLobby        ? 'in-lobby'             : '',
    this.materializing  ? 'materializing'         : '',
    this.railCollapsed  ? 'rail-collapsed'        : '',
    this.schemCollapsed ? 'schematic-collapsed'   : '',
    this.isShipped      ? 'is-shipped'            : '',
  ].filter(Boolean).join(' '));

  // ── Mutations ─────────────────────────────────────────────────────

  exitLobby() {
    this.inLobby = false;
    this.materializing = true;
    this.wsState = 'materialising';
  }

  setMatPhase(phase: MatPhaseId) {
    this.matPhase = phase;
    this.matPhasesSeen.add(phase);  // Set is a $state proxy — add() is tracked
    if (phase === 'complete') {
      this.materializing = false;
      this.wsState = 'materialised';
    }
  }

  addConv(msg: ConvMsg) {
    // $state arrays are reactive proxies — push() is O(1) and tracked.
    this.conv.push(msg);
  }

  addCard(card: Card) {
    while (this.cards.length >= MAX_CANVAS_CARDS) {
      this._evictOne();
    }
    this.cards.push(card);
  }

  updateCard(id: string, patch: Partial<Card>) {
    // Mutate in-place: $state proxies track property writes on nested objects.
    const card = this.cards.find(c => c.id === id);
    if (card) Object.assign(card, patch);
  }

  removeCard(id: string) {
    const idx = this.cards.findIndex(c => c.id === id);
    if (idx === -1) return;
    this.cachedCards.push(this.cards[idx]);
    this.cards.splice(idx, 1);
  }

  addFile(file: LightspaceFile) {
    if (this.files.some(f => f.id === file.id)) return;
    this.files.push(file);
    if (!this.filesOpen) this.filesOpen = true;
  }

  tickSpan(name: string) {
    this.spans += 1;
    this.lastEvent = name;
    this.throughput = +(this.throughput * 0.85 + 0.6).toFixed(1);
    // shift()+push() is O(1) vs slice()+spread O(n)
    this.throughputHistory.push(this.throughput);
    if (this.throughputHistory.length > 12) this.throughputHistory.shift();
  }

  reset() {
    this.cards = [];
    this.conv = [];
    this.files = [];
    this.cachedCards = [];
    this.filesOpen = false;
    this.cachedOpen = false;
    this.intentVerb = '';
    this.intentText = '';
    this.intentClass = '—';
    this.currentPhase = 'phase-0-discover';
    this.lasdlcCodename = null;
    this.lasdlcProject = null;
    this.spans = 0;
    this.lastEvent = '—';
    this.throughput = 0;
    this.throughputHistory = [];
    this.contradictions = 0;
    this.tokens = 0;
    this.budget = 0;
  }

  // ── Private ───────────────────────────────────────────────────────

  private _evictOne() {
    // Highest eviction priority (least important) card leaves first.
    const EVICT_PRIORITY: Partial<Record<string, number>> = {
      diff: 8, archgallery: 7, bash: 6, toolcall: 6, thinking: 5,
      agentspawn: 5, research: 4, branchlane: 3, artifact: 2, monitor: 1, instrument: 1, trace: 0,
    };
    const ranked = [...this.cards]
      .filter(c => !c.pinned)
      .sort((a, b) => (EVICT_PRIORITY[b.kind] ?? 5) - (EVICT_PRIORITY[a.kind] ?? 5));
    const victim = ranked[0];
    if (!victim) return;
    this.cachedCards.push(victim);
    const idx = this.cards.indexOf(victim);
    this.cards.splice(idx, 1);
    if (!this.cachedOpen) this.cachedOpen = true;
  }
}

// Module-level singleton — import `ls` anywhere in the Lightspace screen.
export const ls = new LightspaceState();
