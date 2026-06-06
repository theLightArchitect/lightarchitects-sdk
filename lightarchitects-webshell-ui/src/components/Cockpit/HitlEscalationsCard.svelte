<script lang="ts">
  import { activeBuild, isNativeAgent } from '$lib/stores';
  import { AgentWS } from '$lib/ws';
  import ConductorHitlPanel from './ConductorHitlPanel.svelte';
  import { authHeaders } from '$lib/auth';
  import { lastWaveId } from '$lib/cockpit/stores';
  import type { EscalationEvent, IronclawHitlEscalationEvent, AgentEvent } from '$lib/types';

  interface PendingPermission {
    callId:      string;
    buildId:     string;
    tool:        string;
    summary:     string;
    deadline:    number;
    timeoutSecs: number;
  }

  const IRONCLAW_LAYER_LABEL: Record<number, string> = {
    0: 'CAT', 1: 'L1', 2: 'L2', 3: 'L3', 4: 'FULL',
  };

  let pendingPermissions        = $state<PendingPermission[]>([]);
  let pendingEscalations        = $state<EscalationEvent[]>([]);
  let pendingIronclawEscalations = $state<IronclawHitlEscalationEvent[]>([]);
  let ironclawResolveErr        = $state<Record<string, string>>({});
  let now                       = $state(Date.now());
  let ws                        = $state<AgentWS | null>(null);

  $effect(() => {
    const build = $activeBuild;
    if (!build || !$isNativeAgent) { ws?.disconnect(); ws = null; return; }
    const instance = new AgentWS(
      build.id,
      (_ev: AgentEvent) => {},
      () => {}, () => {},
    );
    instance.connect();
    ws = instance;
    return () => { instance.disconnect(); ws = null; };
  });

  $effect(() => {
    const timer = setInterval(() => {
      now = Date.now();
      const expired = pendingPermissions.filter(p => now >= p.deadline);
      for (const p of expired) ws?.sendDeny(p.callId, 'timeout');
      if (expired.length) pendingPermissions = pendingPermissions.filter(p => now < p.deadline);
    }, 1000);
    return () => clearInterval(timer);
  });

  function secsLeft(p: PendingPermission): number { return Math.max(0, Math.ceil((p.deadline - now) / 1000)); }
  function approve(p: PendingPermission) { ws?.sendApprove(p.callId); pendingPermissions = pendingPermissions.filter(x => x.callId !== p.callId); }
  function deny(p: PendingPermission)   { ws?.sendDeny(p.callId, 'operator-denied'); pendingPermissions = pendingPermissions.filter(x => x.callId !== p.callId); }

  function onPermissionRequest(e: Event) {
    const detail = (e as CustomEvent).detail as { call_id: string; build_id?: string; dispatch_id?: string; tool: string; summary: string; timeout_secs: number; };
    pendingPermissions = [{ callId: detail.call_id, buildId: detail.build_id ?? detail.dispatch_id ?? '', tool: detail.tool, summary: detail.summary, deadline: Date.now() + detail.timeout_secs * 1000, timeoutSecs: detail.timeout_secs }, ...pendingPermissions].slice(0, 6);
  }
  function onEscalation(e: Event) { pendingEscalations = [(e as CustomEvent).detail as EscalationEvent, ...pendingEscalations].slice(0, 4); }
  function onIronclawEscalation(e: Event) { pendingIronclawEscalations = [(e as CustomEvent<IronclawHitlEscalationEvent>).detail, ...pendingIronclawEscalations].slice(0, 8); }
  function onIronclawResolution(e: Event) { pendingIronclawEscalations = pendingIronclawEscalations.filter(x => x.nonce !== (e as CustomEvent<{ nonce: string }>).detail.nonce); }

  async function resolveIronclawEscalation(esc: IronclawHitlEscalationEvent, decision: 'approve' | 'reject') {
    try {
      const res = await fetch('/api/control', { method: 'POST', headers: { 'Content-Type': 'application/json', ...authHeaders() },
        // SECURITY: escalation_nonce must not be logged; resolution via /api/control only (nonce validated server-side)
        body: JSON.stringify({ kind: 'ironclaw_hitl_resolution', escalation_nonce: esc.nonce, decision }) });
      if (!res.ok) { ironclawResolveErr = { ...ironclawResolveErr, [esc.nonce]: (await res.text().catch(() => res.statusText)).slice(0, 80) }; return; }
      pendingIronclawEscalations = pendingIronclawEscalations.filter(x => x.nonce !== esc.nonce);
      const errs = { ...ironclawResolveErr }; delete errs[esc.nonce]; ironclawResolveErr = errs;
    } catch (err) {
      ironclawResolveErr = { ...ironclawResolveErr, [esc.nonce]: err instanceof Error ? err.message : 'request failed' };
    }
  }

  $effect(() => {
    window.addEventListener('la:permission-request', onPermissionRequest);
    window.addEventListener('la:escalation', onEscalation);
    window.addEventListener('la:ironclaw_hitl_escalation', onIronclawEscalation);
    window.addEventListener('la:ironclaw_hitl_resolution', onIronclawResolution);
    return () => {
      window.removeEventListener('la:permission-request', onPermissionRequest);
      window.removeEventListener('la:escalation', onEscalation);
      window.removeEventListener('la:ironclaw_hitl_escalation', onIronclawEscalation);
      window.removeEventListener('la:ironclaw_hitl_resolution', onIronclawResolution);
    };
  });
</script>

<div class="card-label">
  ESCALATIONS
  {#if pendingPermissions.length > 0}
    <span class="badge-count">{pendingPermissions.length}</span>
  {/if}
</div>

{#if pendingPermissions.length === 0 && pendingEscalations.length === 0}
  <div class="empty-state">no pending requests</div>
{/if}

{#each pendingPermissions as p (p.callId)}
  <div class="perm-card">
    <div class="perm-top">
      <span class="perm-tool">{p.tool}</span>
      <span class="perm-timer" class:perm-timer-warn={secsLeft(p) < 10}>{secsLeft(p)}s</span>
    </div>
    <div class="perm-summary">{p.summary.slice(0, 120)}{p.summary.length > 120 ? '…' : ''}</div>
    <div class="perm-actions">
      <button class="btn-approve" onclick={() => approve(p)}>APPROVE</button>
      <button class="btn-deny"    onclick={() => deny(p)}>DENY</button>
    </div>
  </div>
{/each}

{#each pendingEscalations as esc (esc.call_id)}
  <div class="esc-card">
    <span class="esc-badge">L4 ESC</span>
    <span class="esc-reason">{esc.reason}</span>
    {#if esc.canon_ref}<span class="esc-canon">{esc.canon_ref}</span>{/if}
  </div>
{/each}

{#each pendingIronclawEscalations as esc (esc.nonce)}
  <div class="esc-card esc-ironclaw" data-testid="ironclaw-esc-{esc.nonce.slice(0, 8)}">
    <div class="esc-ironclaw-header">
      <span class="esc-badge esc-badge-ironclaw">{IRONCLAW_LAYER_LABEL[esc.layer_failed] ?? `L${esc.layer_failed}`}</span>
      <span class="esc-ironclaw-topic">{esc.decision_topic}</span>
      {#if $lastWaveId && esc.build_id === $lastWaveId}
        <span class="esc-from-composer" data-testid="esc-from-composer">FROM COMPOSER</span>
      {/if}
    </div>
    <div class="esc-ironclaw-question">{esc.escalation_question}</div>
    {#if ironclawResolveErr[esc.nonce]}
      <div class="esc-ironclaw-err">{ironclawResolveErr[esc.nonce]}</div>
    {/if}
    <div class="perm-actions">
      <button class="btn-approve" data-testid="ironclaw-approve-{esc.nonce.slice(0, 8)}" onclick={() => resolveIronclawEscalation(esc, 'approve')}>APPROVE</button>
      <button class="btn-deny"    data-testid="ironclaw-deny-{esc.nonce.slice(0, 8)}"    onclick={() => resolveIronclawEscalation(esc, 'reject')}>REJECT</button>
    </div>
  </div>
{/each}

<ConductorHitlPanel />

<style>
  .card-label { font-size: 9px; font-weight: 700; letter-spacing: var(--la-tk-loose); color: var(--la-text-mute); display: flex; align-items: center; gap: 6px; flex-shrink: 0; }
  .badge-count { background: var(--la-semantic-error); color: #fff; font-size: 8px; padding: 1px 4px; border-radius: 8px; min-width: 14px; text-align: center; }
  .empty-state { color: var(--la-text-mute); font-size: 10px; }
  .perm-card { border: 1px solid var(--la-hair-strong); padding: 8px; display: flex; flex-direction: column; gap: 6px; }
  .perm-top { display: flex; align-items: center; gap: 6px; }
  .perm-tool { font-weight: 600; font-size: 10px; color: var(--la-struct-primary); }
  .perm-timer { font-size: 10px; color: var(--la-semantic-warn); margin-left: auto; }
  .perm-timer-warn { color: var(--la-semantic-error); }
  .perm-summary { font-size: 9px; color: var(--la-text-dim); word-break: break-word; }
  .perm-actions { display: flex; gap: 6px; }
  .btn-approve { background: var(--la-semantic-ok); color: #000; border: none; padding: 4px 10px; font-size: 9px; font-weight: 700; cursor: pointer; letter-spacing: 0.05em; }
  .btn-deny { background: var(--la-semantic-error); color: #fff; border: none; padding: 4px 10px; font-size: 9px; font-weight: 700; cursor: pointer; letter-spacing: 0.05em; }
  .esc-card { border: 1px solid var(--la-semantic-error); padding: 8px; display: flex; flex-direction: column; gap: 4px; }
  .esc-badge { font-size: 8px; font-weight: 700; color: var(--la-semantic-error); letter-spacing: 0.1em; }
  .esc-badge-ironclaw { color: var(--la-semantic-warn); }
  .esc-reason { font-size: 9px; color: var(--la-text-dim); }
  .esc-canon { font-size: 8px; color: var(--la-text-mute); }
  .esc-ironclaw-header { display: flex; align-items: center; gap: 6px; }
  .esc-ironclaw-topic { font-size: 9px; font-weight: 600; color: var(--la-text-secondary); flex: 1; }
  .esc-from-composer { font-size: 8px; color: var(--la-struct-primary); border: 1px solid var(--la-struct-primary); padding: 1px 4px; }
  .esc-ironclaw-question { font-size: 9px; color: var(--la-text-dim); }
  .esc-ironclaw-err { font-size: 9px; color: var(--la-semantic-error); }
</style>
