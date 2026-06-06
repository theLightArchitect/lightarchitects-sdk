import { writable } from 'svelte/store';
import type { RouteScope } from './scope';

/**
 * Polymorphic right-drawer selection.  Cleared on every scope navigation.
 * Drives FocusRouter content dispatch via type-narrowed Svelte 5 $derived.
 *
 * 'pr' is invalid at d3 (file scope).
 * 'crate' is invalid at d0 (platform scope).
 */
export type Selection =
  | { kind: 'none' }
  | { kind: 'build';       codename: string }
  | { kind: 'worker';      worker_id: string; build_codename: string }
  | { kind: 'escalation';  source: 'pr' | 'conductor' | 'ironclaw'; id: string }
  | { kind: 'span';        turn_span_id: string }
  | { kind: 'gate';        codename: string; phase: number; gate: GateDim }
  | { kind: 'decision';    decision_id: string; build_codename: string }
  | { kind: 'pr';          owner: string; repo: string; number: number }
  | { kind: 'crate';       name: string };

export type GateDim = 'A' | 'S' | 'Q' | 'C' | 'O' | 'P' | 'K' | 'D' | 'T' | 'R';

const _selection = writable<Selection>({ kind: 'none' });

/** Read-only export for components to subscribe. */
export const selection = { subscribe: _selection.subscribe };

/** Select a new item. Scope guards enforced here — invalid combinations no-op. */
export function select(s: Selection, currentScope: RouteScope | null): void {
  if (s.kind === 'pr'    && currentScope?.depth === 3) return; // invalid at d3
  if (s.kind === 'crate' && currentScope?.depth === 0) return; // invalid at d0
  _selection.set(s);
}

/** Clear the selection back to 'none'. */
export function clearSelection(): void {
  _selection.set({ kind: 'none' });
}

/** Called by CockpitShell on every scope navigation — clears stale selection. */
export function clearOnScopeChange(): void {
  _selection.set({ kind: 'none' });
}
