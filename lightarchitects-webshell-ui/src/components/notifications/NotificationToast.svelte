<!--
@component
Single notification toast — design-system aligned (zero-radius, corner-bracket,
JetBrains Mono, --la-focus-ring gold for operator actions).

HITL severity (requires_ack=true):
  - Countdown bar when auto_dismiss_ms > 0
  - APPROVE (gold) / DENY (danger) action buttons
  - Non-dismissable via X until ack'd or actioned

Other severities auto-dismiss after auto_dismiss_ms ms (0 = persistent).
-->
<script lang="ts">
  import { onMount } from 'svelte';
  import type { Notification } from '$lib/notificationStore';
  import { notifications } from '$lib/notificationStore';

  let { item }: { item: Notification } = $props();

  let visible = $state(true);

  /** Severity → border color token */
  const SEVERITY_COLOR: Record<string, string> = {
    hitl:   'var(--la-semantic-warn)',
    gate:   'var(--la-struct-primary)',
    wave:   'var(--la-agent-ops)',
    build:  'var(--la-semantic-ok)',
    system: 'var(--la-text-mute)',
  };

  /** Severity → dim background glow token */
  const SEVERITY_BG: Record<string, string> = {
    hitl:   'rgba(245, 158, 11, 0.07)',
    gate:   'rgba(0, 200, 255, 0.05)',
    wave:   'rgba(255, 142, 60, 0.05)',
    build:  'rgba(34, 197, 94, 0.05)',
    system: 'rgba(100, 116, 139, 0.05)',
  };

  const borderColor = $derived(SEVERITY_COLOR[item.severity] ?? SEVERITY_COLOR.system);
  const bgColor     = $derived(SEVERITY_BG[item.severity]    ?? SEVERITY_BG.system);

  let timer: ReturnType<typeof setTimeout> | null = null;

  onMount(() => {
    if (item.auto_dismiss_ms > 0) {
      timer = setTimeout(() => dismiss(), item.auto_dismiss_ms);
    }
    return () => { if (timer) clearTimeout(timer); };
  });

  function dismiss(): void {
    visible = false;
    setTimeout(() => notifications.dismiss(item.id), 220);
  }

  function ack(): void {
    if (item.requires_ack) {
      notifications.ack(item.id);
    } else {
      dismiss();
    }
  }

  function handleAction(): void {
    item.onAction?.();
    ack();
  }

  function handleDanger(): void {
    item.onDanger?.();
    ack();
  }
</script>

{#if visible}
  <div
    class="toast"
    class:hitl={item.severity === 'hitl'}
    style:--border-c={borderColor}
    style:--bg-c={bgColor}
    style:--countdown-ms="{item.auto_dismiss_ms}ms"
    role={item.severity === 'hitl' ? 'alertdialog' : 'status'}
    aria-live={item.severity === 'hitl' ? 'assertive' : 'polite'}
    aria-atomic="true"
  >
    <!-- corner bracket — design system pattern -->
    <span class="bracket tl" aria-hidden="true"></span>
    <span class="bracket br" aria-hidden="true"></span>

    <div class="header">
      <span class="severity-pip" aria-hidden="true"></span>
      <span class="title">{item.title}</span>
      {#if !item.requires_ack}
        <button class="close-btn" onclick={dismiss} aria-label="Dismiss notification" tabindex="0">✕</button>
      {/if}
    </div>

    <p class="body">{item.body}</p>

    {#if item.action_label || item.danger_label}
      <div class="actions">
        {#if item.danger_label}
          <button class="btn-danger" onclick={handleDanger}>{item.danger_label}</button>
        {/if}
        {#if item.action_label}
          <button class="btn-action" onclick={handleAction}>{item.action_label}</button>
        {/if}
      </div>
    {/if}

    {#if item.auto_dismiss_ms > 0}
      <div class="countdown-bar" aria-hidden="true"></div>
    {/if}
  </div>
{/if}

<style>
  .toast {
    position:       relative;
    width:          340px;
    padding:        12px 14px 12px 14px;
    background:     var(--bg-c);
    border:         1px solid var(--border-c);
    border-radius:  0;
    font-family:    'JetBrains Mono Variable', 'JetBrains Mono', monospace;
    font-size:      11px;
    line-height:    1.5;
    color:          var(--la-text-base);
    box-shadow:     0 4px 24px rgba(0, 0, 0, 0.5),
                    0 0 0 1px rgba(0, 0, 0, 0.4);
    overflow:       hidden;
    animation:      toast-in 180ms var(--ease-project) both;

    /* left accent bar */
    border-left-width: 2px;
  }

  .toast:not(:last-child) {
    margin-bottom: 8px;
  }

  /* HITL toasts get a stronger glow to grab attention */
  .toast.hitl {
    box-shadow: 0 4px 24px rgba(0, 0, 0, 0.5),
                0 0 12px rgba(245, 158, 11, 0.25),
                0 0 0 1px rgba(0, 0, 0, 0.4);
  }

  /* ── Corner brackets ────────────────────────────────────────────────── */
  .bracket {
    position:  absolute;
    width:     8px;
    height:    8px;
    pointer-events: none;
    opacity:   0.6;
  }
  .bracket.tl {
    top:    0; left:  0;
    border-top:  1px solid var(--border-c);
    border-left: 1px solid var(--border-c);
  }
  .bracket.br {
    bottom: 0; right: 0;
    border-bottom: 1px solid var(--border-c);
    border-right:  1px solid var(--border-c);
  }

  /* ── Header ────────────────────────────────────────────────────────── */
  .header {
    display:     flex;
    align-items: center;
    gap:         6px;
    margin-bottom: 4px;
  }

  .severity-pip {
    width:         6px;
    height:        6px;
    border-radius: 50%;
    background:    var(--border-c);
    flex-shrink:   0;
    box-shadow:    0 0 6px var(--border-c);
  }

  .title {
    flex:        1;
    font-weight: 700;
    font-size:   11px;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color:       var(--la-text-bright);
    white-space: nowrap;
    overflow:    hidden;
    text-overflow: ellipsis;
  }

  .close-btn {
    background:   none;
    border:       none;
    padding:      0 2px;
    color:        var(--la-text-mute);
    cursor:       pointer;
    font-family:  inherit;
    font-size:    10px;
    line-height:  1;
    transition:   color 120ms;
  }
  .close-btn:hover { color: var(--la-text-bright); }
  .close-btn:focus-visible {
    outline:        2px solid var(--la-focus-ring);
    outline-offset: 2px;
  }

  /* ── Body ───────────────────────────────────────────────────────────── */
  .body {
    margin:     0 0 0 12px;
    font-size:  11px;
    color:      var(--la-text-dim);
    word-break: break-word;
  }

  /* ── Action buttons ─────────────────────────────────────────────────── */
  .actions {
    display:         flex;
    gap:             6px;
    margin-top:      10px;
    margin-left:     12px;
    justify-content: flex-end;
  }

  .btn-action,
  .btn-danger {
    padding:      3px 10px;
    border-radius: 0;
    font-family:   inherit;
    font-size:     10px;
    font-weight:   700;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    cursor:        pointer;
    transition:    background 100ms, color 100ms;
  }

  /* Gold — operator primary action (§HITL-7 approve) */
  .btn-action {
    background: transparent;
    border:     1px solid var(--la-focus-ring);
    color:      var(--la-focus-ring);
  }
  .btn-action:hover {
    background: rgba(255, 215, 0, 0.12);
  }
  .btn-action:focus-visible {
    outline:        2px solid var(--la-focus-ring);
    outline-offset: 2px;
  }

  /* Danger — deny / abort */
  .btn-danger {
    background: transparent;
    border:     1px solid var(--la-danger-stroke);
    color:      var(--la-danger-text);
  }
  .btn-danger:hover {
    background: var(--la-danger-bg);
  }
  .btn-danger:focus-visible {
    outline:        2px solid var(--la-danger-stroke);
    outline-offset: 2px;
  }

  /* ── Countdown bar ──────────────────────────────────────────────────── */
  .countdown-bar {
    position:   absolute;
    bottom:     0;
    left:       0;
    right:      0;
    height:     2px;
    background: var(--border-c);
    transform-origin: left center;
    animation:  countdown-shrink var(--countdown-ms) linear both;
    opacity:    0.5;
  }

  /* ── Animations ─────────────────────────────────────────────────────── */
  @keyframes toast-in {
    from { opacity: 0; transform: translateX(16px); }
    to   { opacity: 1; transform: translateX(0); }
  }

  @keyframes countdown-shrink {
    from { transform: scaleX(1); }
    to   { transform: scaleX(0); }
  }
</style>
