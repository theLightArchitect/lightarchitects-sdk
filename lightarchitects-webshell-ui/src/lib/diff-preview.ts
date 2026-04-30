// ============================================================================
// Diff-Preview API + types — operator-gated FS mutation flow (#47)
// ============================================================================
//
// When an agent invokes Edit/Write/MultiEdit during a dispatch, the backend
// (planned, mantis-rebase pending) intercepts the call, computes a unified
// diff, broadcasts an FsMutationPending event over SSE, and HOLDS the agent's
// response until the operator approves or rejects via this client.
//
// The frontend scaffold lands first so the component is ready to integrate
// the moment the backend wiring is in place. Until then, the
// `triggerMockDiffPreview()` helper exists for development verification.

import { authHeaders } from './auth';

/** A single pending FS mutation awaiting operator approval. */
export interface FsMutationPending {
  /** Unique mutation id — use to approve/reject the specific mutation. */
  mutation_id: string;
  /** Dispatch id that originated this mutation. */
  dispatch_id: string;
  /** Domain agent that requested the mutation (drives badge color). */
  agent: string;
  /** Vault-relative or absolute path being mutated. */
  file_path: string;
  /** Tool name that triggered the gate (e.g. 'Edit', 'Write', 'MultiEdit'). */
  tool: string;
  /** Unified-diff text — pre-formatted by the backend with file headers. */
  diff_unified: string;
  /** ISO-8601 timestamp the mutation was queued. */
  queued_at: string;
}

/**
 * SSE event variant — backend sends this when an agent's FS-mutating tool
 * call needs operator approval. Subscribed to in app.svelte; the FsMutationModal
 * component opens on receipt.
 */
export interface FsMutationPendingEvent extends FsMutationPending {
  type: 'fs_mutation_pending';
}

// ── Approve / reject API ────────────────────────────────────────────────────

/** Approve a pending mutation; backend releases the held tool call. */
export async function approveMutation(
  dispatchId: string,
  mutationId: string,
): Promise<void> {
  const res = await fetch(
    `/api/dispatch/${encodeURIComponent(dispatchId)}/fs-approve`,
    {
      method: 'POST',
      headers: { 'content-type': 'application/json', ...authHeaders() },
      body: JSON.stringify({ mutation_id: mutationId }),
    },
  );
  if (!res.ok) {
    throw new Error(`approve mutation: ${res.status} ${res.statusText}`);
  }
}

/** Reject a pending mutation; backend returns a synthetic error to the agent. */
export async function rejectMutation(
  dispatchId: string,
  mutationId: string,
  reason?: string,
): Promise<void> {
  const res = await fetch(
    `/api/dispatch/${encodeURIComponent(dispatchId)}/fs-reject`,
    {
      method: 'POST',
      headers: { 'content-type': 'application/json', ...authHeaders() },
      body: JSON.stringify({ mutation_id: mutationId, reason: reason ?? '' }),
    },
  );
  if (!res.ok) {
    throw new Error(`reject mutation: ${res.status} ${res.statusText}`);
  }
}

// ── Mock trigger (dev verification) ──────────────────────────────────────────
//
// Until the backend wires the interception layer (mantis-rebase pending), this
// helper lets developers fire the modal locally to verify rendering.
//
// Usage from devtools console:
//   import('/src/lib/diff-preview.ts').then(m => m.triggerMockDiffPreview())

export function triggerMockDiffPreview(): void {
  const mockEvent: FsMutationPendingEvent = {
    type: 'fs_mutation_pending',
    mutation_id: `mock-${Math.random().toString(36).slice(2, 10)}`,
    dispatch_id: 'mock-dispatch',
    agent: 'engineer',
    file_path: 'src/lib/example.ts',
    tool: 'Edit',
    queued_at: new Date().toISOString(),
    diff_unified: `--- a/src/lib/example.ts
+++ b/src/lib/example.ts
@@ -1,5 +1,7 @@
 export function greet(name: string): string {
-  return 'Hello, ' + name;
+  return \`Hello, \${name}!\`;
 }

+export const DEFAULT_GREETING = greet('world');
+
 // EOF
`,
  };
  window.dispatchEvent(
    new CustomEvent<FsMutationPendingEvent>('la:fs-mutation-pending', {
      detail: mockEvent,
    }),
  );
}
