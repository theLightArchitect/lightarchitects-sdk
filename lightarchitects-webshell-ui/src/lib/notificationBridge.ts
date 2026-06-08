/**
 * Notification bridge — translates DOM CustomEvents from sse.ts into
 * typed Notification pushes.
 *
 * §HITL-7 compliance:
 *   Every l4 escalation fires: webshell SSE toast + osascript (via /api/os-notify)
 *   MTTA is tracked via dispatched_at → ack_received_at on the Notification object.
 *   Non-suppressible: hitl severity items are not evicted from the stack.
 *
 * Call `mountNotificationBridge()` once in app.svelte's onMount.
 * Call the returned teardown function in onDestroy.
 */

import { notifications } from './notificationStore';
import type { EscalationEvent } from './types';
import { goto } from '$app/navigation';

interface PermissionRequestDetail {
  type:        'permission_request';
  dispatch_id: string;
  call_id:     string;
  tool:        string;
  summary:     string;
  agent_id:    string;
  timeout_secs: number;
}

type GateResultDetail = {
  build_id: string;
  phase:    string;
  verdict:  'pass' | 'fail' | 'blocked';
  reason?:  string;
};

type WaveCompleteDetail = {
  build_id:   string;
  wave_index: number;
  passed:     number;
  failed:     number;
};

type BuildCompleteDetail = {
  build_id: string;
  status:   'complete' | 'aborted' | 'error';
  reason?:  string;
};

/** Fires OS-level notification via gateway endpoint (best-effort; non-blocking). */
function fireOsNotify(title: string, body: string): void {
  fetch('/api/os-notify', {
    method:  'POST',
    headers: { 'Content-Type': 'application/json' },
    body:    JSON.stringify({ title, body }),
  }).catch(() => {
    // Gateway endpoint may not exist yet — degrade silently
  });
}

function onPermissionRequest(e: Event): void {
  const detail = (e as CustomEvent<PermissionRequestDetail>).detail;
  notifications.push({
    severity:        'hitl',
    title:           'Permission required',
    body:            detail.summary ?? `Tool: ${detail.tool}`,
    build_id:        detail.dispatch_id,
    call_id:         detail.call_id,
    requires_ack:    true,
    auto_dismiss_ms: 0,
    action_label:    'APPROVE',
    danger_label:    'DENY',
    onAction: () => resolvePermission(detail.call_id, 'approve', detail.dispatch_id),
    onDanger: () => resolvePermission(detail.call_id, 'deny',    detail.dispatch_id),
  });
  fireOsNotify('Permission required', detail.summary ?? detail.tool);
}

function onEscalation(e: Event): void {
  const detail = (e as CustomEvent<EscalationEvent>).detail;
  notifications.push({
    severity:        'hitl',
    title:           'HITL escalation',
    body:            detail.reason,
    build_id:        detail.build_id,
    call_id:         detail.call_id,
    requires_ack:    true,
    auto_dismiss_ms: 0,
    action_label:    'ACKNOWLEDGE',
  });
  fireOsNotify('Build escalation', detail.reason);
}

function onGateResult(e: Event): void {
  const detail = (e as CustomEvent<GateResultDetail>).detail;
  const pass = detail.verdict === 'pass';
  notifications.push({
    severity:        pass ? 'gate' : 'gate',
    title:           pass ? `Gate passed — ${detail.phase}` : `Gate FAILED — ${detail.phase}`,
    body:            detail.reason ?? (pass ? 'All checks green.' : 'See gate report for details.'),
    build_id:        detail.build_id,
    requires_ack:    !pass,
    auto_dismiss_ms: pass ? 8000 : 0,
  });
}

function onWaveComplete(e: Event): void {
  const detail = (e as CustomEvent<WaveCompleteDetail>).detail;
  const allPass = detail.failed === 0;
  notifications.push({
    severity:        allPass ? 'wave' : 'wave',
    title:           `Wave ${detail.wave_index} ${allPass ? 'complete' : 'failed'}`,
    body:            `${detail.passed} passed · ${detail.failed} failed`,
    build_id:        detail.build_id,
    requires_ack:    !allPass,
    auto_dismiss_ms: allPass ? 6000 : 0,
  });
}

function onBuildComplete(e: Event): void {
  const detail = (e as CustomEvent<BuildCompleteDetail>).detail;
  const ok = detail.status === 'complete';
  notifications.push({
    severity:        'build',
    title:           ok ? 'Build complete' : `Build ${detail.status}`,
    body:            detail.reason ?? (ok ? 'All phases shipped.' : 'Check build log.'),
    build_id:        detail.build_id,
    requires_ack:    false,
    auto_dismiss_ms: ok ? 10000 : 0,
  });
}

function onConnectionLost(): void {
  notifications.push({
    severity:        'system',
    title:           'Connection lost',
    body:            'Reconnecting to gateway…',
    requires_ack:    false,
    auto_dismiss_ms: 0,
  });
}

function onAuthFail(): void {
  notifications.push({
    severity:        'system',
    title:           'Authentication failed',
    body:            'Session expired — re-authenticate in Settings.',
    requires_ack:    false,
    auto_dismiss_ms: 0,
    action_label:    'SETTINGS',
    onAction: () => { void goto('/settings'); },
  });
}

async function resolvePermission(callId: string, verdict: 'approve' | 'deny', buildId: string): Promise<void> {
  try {
    await fetch(`/api/builds/${buildId}/hitl/resolve`, {
      method:  'POST',
      headers: { 'Content-Type': 'application/json' },
      body:    JSON.stringify({ call_id: callId, verdict }),
    });
  } catch {
    // Swallow — the HITL toast remains until gateway confirms
  }
}

const EVENTS: [string, EventListener][] = [
  ['la:permission-request', onPermissionRequest as EventListener],
  ['la:escalation',         onEscalation         as EventListener],
  ['la:gate-result',        onGateResult          as EventListener],
  ['la:wave-complete',      onWaveComplete        as EventListener],
  ['la:build-complete',     onBuildComplete       as EventListener],
  ['la:connection-lost',    onConnectionLost      as EventListener],
  ['la:auth-fail',          onAuthFail            as EventListener],
];

export function mountNotificationBridge(): () => void {
  for (const [type, handler] of EVENTS) {
    window.addEventListener(type, handler);
  }
  return () => {
    for (const [type, handler] of EVENTS) {
      window.removeEventListener(type, handler);
    }
  };
}
