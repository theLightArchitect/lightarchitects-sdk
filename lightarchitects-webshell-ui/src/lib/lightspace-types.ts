/**
 * Lightspace type system — shared across all components and stores.
 *
 * @integration src/lib/types.ts — imports CopilotActivityEvent, ImplCompleteEvent, etc.
 * @integration src/lib/stores.ts — stores import these interfaces
 * @integration src/lib/card-eviction.ts — uses CardKind, BentoCardData
 */

// ── Card taxonomy ──────────────────────────────────────────────────────────

export type CardKind =
  | 'monitor'      // Status KPI strip (AgentEvent.StatusUpdate)
  | 'instrument'   // Metrics KPI (AgentEvent.TokenUsage / pillar_update)
  | 'trace'        // Activity stream (CopilotActivity kind=result/assistant/etc.)
  | 'thinking'     // Collapsible reasoning block (CopilotActivity kind=thinking)
  | 'toolcall'     // Tool invocation (CopilotActivity kind=tool_use)
  | 'bash'         // Bash output (CopilotActivity kind=tool_use, Bash tool)
  | 'agentspawn'   // Agent lifecycle (Fleet SSE primary; a2a_envelope secondary)
  | 'diff'         // Unified diff view (impl_complete)
  | 'artifact'     // Shipped file (impl_complete)
  | 'research'     // Research fragment (_audit pending_ — demo only)
  | 'archgallery'  // Diagram thumbnail grid (_audit pending_ — demo only)
  | 'branchlane';  // LASDLC phase ladder (merge_agent_status)

export type CardSpan = 'span-3' | 'span-4' | 'span-6' | 'span-12';

/** Default column span per card kind (replicates mockup's KIND_DEFAULT_SPAN). */
export const KIND_DEFAULT_SPAN: Record<CardKind, CardSpan> = {
  monitor:    'span-3',
  instrument: 'span-3',
  trace:      'span-4',
  thinking:   'span-6',
  toolcall:   'span-4',
  bash:       'span-4',
  agentspawn: 'span-4',
  diff:       'span-12',
  artifact:   'span-3',
  research:   'span-6',
  archgallery:'span-12',
  branchlane: 'span-12',
};

// ── Core card and canvas types ─────────────────────────────────────────────

export interface BentoCardData {
  id: string;
  kind: CardKind;
  span: CardSpan;
  title: string;
  ts: number;              // insertion timestamp — eviction age ordering
  _pinned?: boolean;       // pinned cards are never auto-evicted
  _agentDone?: boolean;    // agentspawn: raises eviction priority when agent completes
  data: unknown;           // kind-specific payload (discriminated by `kind`)
}

export interface TombstoneData {
  id: string;
  kind: CardKind;
  title: string;
  evictedAt: number;
  cardSnapshot: BentoCardData;
  fileTarget?: string;     // set when card was graduated to a drawer file
}

export interface SkeletonData {
  id: string;
  kind: CardKind;
  span: CardSpan;
  tag: string;             // e.g. "awaiting"
}

// ── Drawer file types ──────────────────────────────────────────────────────

export type FileMime = 'md' | 'svg' | 'rs' | 'ts' | 'yaml' | 'json' | 'txt' | 'pdf' | 'png';

export interface FileEntry {
  id: string;
  name: string;
  mime: FileMime;
  meta: string;
  prov: { agent: string; spanId?: string };
  path?: string;
}

// ── Gate types ─────────────────────────────────────────────────────────────

export type GateId = 'A' | 'S' | 'Q' | 'C' | 'O' | 'P' | 'K' | 'D' | 'T' | 'R';
export type GateStatus = 'pending' | 'active' | 'pass' | 'fail' | 'skip';

export interface GateEntry {
  id: GateId;
  status: GateStatus;
}

// ── LASDLC phase types ─────────────────────────────────────────────────────

export interface BranchLane {
  id: string;
  agentKey: string;
  state: 'exploring' | 'committed' | 'rolled_back';
  taskDesc: string;
  progress: number;        // 0–100
  spanId?: string;
}

export interface LasdlcPhaseState {
  id: string;
  name: string;
  status: 'pending' | 'active' | 'complete' | 'failed';
  gates: GateEntry[];
}

export interface LasdlcState {
  phases: LasdlcPhaseState[];
  currentPhaseId: string | null;
  codename: string | null;
}

// ── Conversation types ─────────────────────────────────────────────────────

export interface ConvMessage {
  id: string;
  who: 'operator' | 'copilot' | string;
  text: string;
  ts: number;
  kind?: 'result' | 'assistant' | 'system' | 'error';
}

// ── Materialization phases ─────────────────────────────────────────────────

export type MaterializePhase =
  | 'idle'
  | 'begin'
  | 'rail_collapsed'
  | 'grid_revealed'
  | 'drawer_revealed'
  | 'cards_streaming'
  | 'complete';

// ── Metrics types ──────────────────────────────────────────────────────────

export interface LoopBudgetState {
  turns: number;
  maxTurns: number;
  steps: number;
  costUsd: number;
  status: 'pending' | 'running' | 'halted';
}

export interface DiffEntry {
  lineType: 'add' | 'remove' | 'context';
  content: string;
}

export interface PubSubState {
  seq: number;
  folded: number;
  lag: number;
  producerPhase: string;
  loopStatus: 'pending' | 'running' | 'halted' | 'paused';
  lastTopic: string | null;
  topicHistory: { ts: string; topic: string }[];
}

export interface ReactState {
  currentPhase: number;
  observation: string;
  thought: string;
  action: string;
  stepCount: number;
  turnCount: number;
}

export interface CitationState {
  sources: number;
  verified: number;
  multi: number;
  contras: number;
}

export interface FleetSnapshot {
  nodes: import('$lib/types').FleetNode[];
  captured_at: string;
}

// ── Store interfaces ───────────────────────────────────────────────────────

export interface LightspaceSessionState {
  buildId: string | null;
  runStatus: 'idle' | 'connecting' | 'running' | 'complete' | 'error';
  intent: string;
  lobbyInput: string;
  conv: ConvMessage[];
  mode: 'demo' | 'production';
  northstarState: import('$lib/types').NorthstarEvaluationEvent | null;
  materializePhase: MaterializePhase;
}

export interface LightspaceCanvasState {
  cards: BentoCardData[];
  skeletons: SkeletonData[];
  tombstones: TombstoneData[];
  expandedCardId: string | null;
  highlightCardId: string | null;
}

export interface LightspaceFilesState {
  files: FileEntry[];
  activeFileId: string | null;
  heroFileId: string | null;
  heroTombId: string | null;
}

export interface LightspaceUiState {
  tombFlash: boolean;
  sidebarOpen: boolean;
  schematicOpen: boolean;
  viewPreset: 'default' | 'focus' | 'wide';
  filesDrawerOpen: boolean;
  cacheDrawerOpen: boolean;
}

export interface LightspaceLasdlcState {
  lasdlc: LasdlcState;
  gateMatrix: GateEntry[];
  branchLanes: BranchLane[];
}

export interface LightspaceMetricsState {
  loopBudget: LoopBudgetState;
  diffFeed: DiffEntry[];
  pubsub: PubSubState;
  react: ReactState;
  citation: CitationState;
  mermaidNodes: number;
  fleet: FleetSnapshot | null;
}
