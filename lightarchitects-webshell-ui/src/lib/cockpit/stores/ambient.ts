import { writable } from 'svelte/store';

export type SiblingId = 'CORSO' | 'EVA' | 'SOUL' | 'QUANTUM' | 'SERAPH' | 'AYIN' | 'LAEX';
export type SiblingAvailability = 'online' | 'offline' | 'saturated' | 'down';

export interface SlotEconomy {
  write_used:  number;
  write_cap:   number;  // platform constant: 7
  read_used:   number;
  read_cap:    number;  // platform constant: 16
  queue_depth: number;
}

export interface AmbientState {
  slot_economy:         SlotEconomy;
  sibling_availability: Record<SiblingId, SiblingAvailability>;
  /** null when cost-accounting mechanism is disabled — UI shows `—` */
  cost_per_hour_usd:    number | null;
  northstar_pulse:      Record<'P1' | 'P2' | 'P3' | 'P4' | 'P5' | 'P6' | 'P7', number>;
  unread_alerts:        number;
}

const OFFLINE_SIBLINGS = Object.fromEntries(
  (['CORSO', 'EVA', 'SOUL', 'QUANTUM', 'SERAPH', 'AYIN', 'LAEX'] as SiblingId[]).map(
    id => [id, 'offline' as SiblingAvailability],
  ),
) as Record<SiblingId, SiblingAvailability>;

export const DEFAULT_AMBIENT: AmbientState = {
  slot_economy: { write_used: 0, write_cap: 7, read_used: 0, read_cap: 16, queue_depth: 0 },
  sibling_availability: OFFLINE_SIBLINGS,
  cost_per_hour_usd: null,
  northstar_pulse: { P1: 0, P2: 0, P3: 0, P4: 0, P5: 0, P6: 0, P7: 0 },
  unread_alerts: 0,
};

/** Platform-wide ambient state — updated by SSE handlers in Phase 5 Wave C. */
export const ambient = writable<AmbientState>(DEFAULT_AMBIENT);

/** Partial update — merges top-level keys only. */
export function patchAmbient(patch: Partial<AmbientState>): void {
  ambient.update(s => ({ ...s, ...patch }));
}
