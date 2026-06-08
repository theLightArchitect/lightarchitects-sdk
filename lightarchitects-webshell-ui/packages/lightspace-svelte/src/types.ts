// TypeScript mirrors of lightarchitects-lightspace/src/types.rs
// serde rename strategies: rename_all="lowercase" → lowercase literals;
// EvidenceTier: rename_all="UPPERCASE" → uppercase literals.

export type CardKind =
  | 'monitor' | 'instrument' | 'trace' | 'thinking' | 'toolcall'
  | 'bash' | 'agentspawn' | 'diff' | 'artifact' | 'research'
  | 'archgallery' | 'branchlane';

export type CardState       = 'attached' | 'detached';
export type CardTransition  = 'attach'   | 'detach';
export type Actor           = 'copilot'  | 'operator';
export type UpdateMode      = 'replace'  | 'append' | 'patch';
export type DrawerFileAction = 'attach'  | 'detach' | 'update';
export type EvidenceTier    = 'HIGH' | 'MEDIUM' | 'LOW';
export type MaterializePhase = 'idle' | 'begin' | 'canvas' | 'drawer' | 'complete';

export interface Provenance {
  agent: string;
  source: string;
  span_id?: string;
}

export interface CardData {
  id: string;
  kind: CardKind;
  title: string;
  state: CardState;
  content: unknown;
  provenance: Provenance;
  attribution?: string;
}

export interface DrawerFileData {
  id: string;
  mime_type: string;
  content_uri: string;
  size_bytes: number;
  provenance: Provenance;
}

// CanvasState serialised from IndexMap → JSON object (Record, not array)
export interface CanvasSnapshot {
  session_id: string;
  cards: Record<string, CardData>;
  drawer_files: Record<string, DrawerFileData>;
  materialize_phase: number | null;
  snapshot_seq: number;
}

// ── SSE event payloads (flat wire format; WebEventV2 uses #[serde(flatten)]) ──

export interface LightspaceCardEvent {
  session_id: string;
  card: CardData;
}
export interface LightspaceLifecycleEvent {
  session_id: string;
  card_id: string;
  transition: CardTransition;
  actor: Actor;
  ghost: boolean;
}
export interface LightspaceUpdateEvent {
  session_id: string;
  card_id: string;
  seq: number;
  mode: UpdateMode;
  path?: string;
  payload: unknown;
}
export interface LightspaceGraduateEvent {
  session_id: string;
  card_id: string;
  file_id: string;
  content_uri: string;
  content_mime: string;
}
export interface LightspaceMaterializeEvent {
  session_id: string;
  phase: number;
}
export interface LightspaceGatingEvent {
  session_id: string;
  card_id: string;
  gate: string;
  satisfied: boolean;
  reason?: string;
}
export interface LightspaceBranchLaneEvent {
  session_id: string;
  card_id: string;
  lanes: unknown;
  fork_span_id?: string;
  committed_lane_id?: string;
}
export interface LightspaceConfidenceEvent {
  session_id: string;
  target_id: string;
  target_kind: string;
  value: number;
  basis: string;
  evidence_tier: EvidenceTier;
  contradicts?: string[];
}
export interface LightspaceDrawerFileEvent {
  session_id: string;
  file: DrawerFileData;
}
export interface LightspaceDrawerEventPayload {
  session_id: string;
  file_id: string;
  action: DrawerFileAction;
  actor: Actor;
  new_content_uri?: string;
}

// ── Visual layer content schemas ──────────────────────────────────────────────

export interface GateMatrixContent {
  instrument_kind: 'gate_matrix';
  dimensions: string[];
  phase_id: string;
  cells: Record<string, { status: 'pass' | 'fail' | 'running' | 'pending' }>;
}

export interface ContextBurnContent {
  instrument_kind: 'context_burn';
  samples: Array<{ t: string; used: number; budget: number }>;
  current_pct: number;
  level: 'l1' | 'l2' | 'l3' | null;
}

export interface BuildTopologyContent {
  monitor_kind: 'build_topology';
  phases: Array<{ id: string; status: 'pending' | 'active' | 'completed' | 'failed'; label: string }>;
  test_ratchet: Array<{ wave: string; count: number }>;
}

// ── HITL queue item ───────────────────────────────────────────────────────────

export interface HitlItem {
  id: string;
  gate?: string;
  label: string;
  inserted_at: number;  // ms epoch
}
