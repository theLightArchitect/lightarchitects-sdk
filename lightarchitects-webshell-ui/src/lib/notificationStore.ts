/**
 * Typed notification store — push/dismiss/ack API consumed by NotificationStack.
 *
 * Severity ladder:
 *   hitl   → BLOCKING operator gate (§HITL-7); requires_ack=true; non-suppressible
 *   gate   → Phase gate result (pass|fail); auto-dismiss 8s on pass, persistent on fail
 *   wave   → Wave-complete / wave-fail signal
 *   build  → Build-level events (started, complete, aborted)
 *   system → Auth, connection, config (low-priority)
 */

import { writable, derived } from 'svelte/store';

export type NotificationSeverity = 'hitl' | 'gate' | 'wave' | 'build' | 'system';

export interface Notification {
  id:            string;
  severity:      NotificationSeverity;
  title:         string;
  body:          string;
  build_id?:     string;
  call_id?:      string;
  sibling?:      string;
  requires_ack:  boolean;
  /** Wall-clock ms after which the toast auto-dismisses (0 = no auto-dismiss). */
  auto_dismiss_ms: number;
  dispatched_at: number;
  ack_received_at?: number;
  action_label?: string;
  danger_label?: string;
  onAction?: () => void;
  onDanger?: () => void;
}

/** Maximum toasts visible at once — oldest non-ack items scroll off. */
const MAX_VISIBLE = 5;

function createNotificationStore() {
  const { subscribe, update } = writable<Notification[]>([]);

  function push(n: Omit<Notification, 'id' | 'dispatched_at'>): string {
    const id = `notif-${Date.now()}-${Math.random().toString(36).slice(2, 7)}`;
    const item: Notification = { ...n, id, dispatched_at: Date.now() };
    update(list => {
      // Deduplicate by call_id (HITL re-emits on reconnect)
      if (item.call_id && list.some(x => x.call_id === item.call_id)) return list;
      const next = [item, ...list];
      // Evict oldest auto-dismissable items when over cap
      if (next.length > MAX_VISIBLE) {
        const evictable = next.filter(x => !x.requires_ack);
        const toEvict = evictable.slice(MAX_VISIBLE - 1);
        return next.filter(x => !toEvict.includes(x));
      }
      return next;
    });
    return id;
  }

  function dismiss(id: string): void {
    update(list => list.filter(n => n.id !== id));
  }

  function ack(id: string): void {
    update(list =>
      list.map(n =>
        n.id === id ? { ...n, ack_received_at: Date.now(), requires_ack: false } : n,
      ),
    );
    // Remove after short grace so the ack animation plays
    setTimeout(() => dismiss(id), 400);
  }

  function clearBuild(buildId: string): void {
    update(list => list.filter(n => n.build_id !== buildId || n.requires_ack));
  }

  return { subscribe, push, dismiss, ack, clearBuild };
}

export const notifications = createNotificationStore();

/** Unacked HITL count — drives the header badge. */
export const hitlCount = derived(notifications, $n =>
  $n.filter(n => n.severity === 'hitl' && !n.ack_received_at).length,
);
