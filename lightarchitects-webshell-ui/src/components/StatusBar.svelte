<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { ayinStatus, builds, terminalConnected, authProfile, authStatus, agentTokenUsage, connectedProviders } from '$lib/stores';
  import { authHeaders } from '$lib/auth';
  import { STATUS_COLORS } from '$lib/design-tokens';
  import { api } from '$lib/api';
  import type { OverallStatus } from '$lib/types';
  import { subscribeByTopic, type WebEventV2 } from '$lib/sse';
  import BackendPicker from './BackendPicker.svelte';
  import RespawnConfirmModal from './RespawnConfirmModal.svelte';

  // Auth state takes precedence over connection state — a 401/403 is a more
  // urgent operator signal than "reconnecting" (#13 second-half: AuthBanner
  // already covers the loud surface; this chip is the persistent at-a-glance).
  let auth = $derived($authStatus);
  let status = $derived($ayinStatus);
  let buildCount = $derived($builds.length);
  let ptyColor = $derived($terminalConnected ? STATUS_COLORS.connected : STATUS_COLORS.offline);
  let ptyLabel = $derived($terminalConnected ? 'PTY live' : 'PTY off');

  const statusColor = (s: string) => STATUS_COLORS[s as keyof typeof STATUS_COLORS] ?? '#6b7280';

  // Effective AYIN-chip color + label, factoring in auth state.
  let ayinColor = $derived(auth !== 'ok' ? '#DC2626' : statusColor(status));
  let ayinLabel = $derived(
    auth === 'unauthorized' ? 'auth: expired' :
    auth === 'forbidden'    ? 'auth: denied' :
    status === 'connected'  ? `AYIN live · ${buildCount} builds` :
    status === 'reconnecting' ? 'reconnecting…' :
                                'AYIN offline'
  );
  // Active agent backend chip — mirrors server-side AgentKind enum 1:1.
  // NULL = unauthenticated/unknown (NEVER guess a backend).
  // Color palette is canon-aligned (Builders Cookbook design-tokens):
  //   anthropic           → amber  (Claude brand)
  //   lightarchitects     → green  (Claude Code CLI default)
  //   light_architect → teal (lÆx0 native)
  //   codex               → magenta (OpenAI)
  //   mistral_vibe        → orange (Mistral brand)
  //   ollama              → indigo (legacy model-backend chip)
  //   null                → gray   (unauthenticated)
  let profileColor = $derived(
    $authProfile === null                    ? '#6b7280' :  // gray — unauthenticated
    $authProfile === 'anthropic'             ? '#F59E0B' :  // amber
    $authProfile === 'lightarchitects'       ? '#22C55E' :  // green
    $authProfile === 'light_architect' ? '#14B8A6' : // teal
    $authProfile === 'codex'                 ? '#A855F7' :  // magenta
    $authProfile === 'mistral_vibe'          ? '#FB923C' :  // mistral orange
                                                '#6366F1'    // ollama (legacy) — indigo
  );
  let profileLabel = $derived(
    $authProfile === null                    ? 'unauthenticated' :
    $authProfile === 'anthropic'             ? 'Anthropic' :
    $authProfile === 'lightarchitects'       ? 'Claude Code' :
    $authProfile === 'light_architect' ? 'lÆx0 CLI' :
    $authProfile === 'codex'                 ? 'Codex' :
    $authProfile === 'mistral_vibe'          ? 'Mistral Vibe' :
                                                'Ollama'
  );

  // Connected credential providers (separate from active backend kind).
  // Source of truth is server-side `state.credential_store`; this chip surfaces
  // it without requiring the operator to open Settings.
  let connectedLabel = $derived(
    $connectedProviders.length === 0
      ? 'no providers'
      : `${$connectedProviders.length} provider${$connectedProviders.length === 1 ? '' : 's'}: ${$connectedProviders.map(p => p[0].toUpperCase() + p.slice(1)).join(' · ')}`
  );
  let connectedColor = $derived($connectedProviders.length > 0 ? '#22C55E' : '#6b7280');

  // AYIN status subscription — live topic events replace polling-only fallback.
  let unsubscribeAyin: (() => void) | null = null;

  function handleAyinEvent(event: WebEventV2): void {
    if (event.topic.endsWith('.connected')) ayinStatus.set('connected');
    else if (event.topic.endsWith('.disconnected')) ayinStatus.set('offline');
    else if (event.topic.endsWith('.reconnecting')) ayinStatus.set('reconnecting');
  }

  // Preflight dot — unauthenticated poll every 30 s so the badge stays current
  // without requiring the operator to open the panel.
  let preflightOverall = $state<OverallStatus | null>(null);

  function preflightDotColor(o: OverallStatus | null): string {
    if (o === 'Ready')    return '#22c55e';
    if (o === 'Degraded') return '#f59e0b';
    if (o === 'Blocked')  return '#ef4444';
    return '#6b7280';
  }

  function preflightDotLabel(o: OverallStatus | null): string {
    if (o === 'Ready')    return 'infra: ready';
    if (o === 'Degraded') return 'infra: degraded';
    if (o === 'Blocked')  return 'infra: blocked';
    return 'infra: …';
  }

  async function pollPreflight() {
    try {
      const r = await api.fetchPreflight();
      preflightOverall = r.overall;
    } catch {
      // Silently ignore — the dot stays at its last-known state.
    }
  }

  // Poll /api/agent/current — single endpoint returns active backend kind +
  // list of connected credential providers. Replaces the prior "authProfile
  // store default = anthropic" lie with server-truth.
  async function pollAgentCurrent() {
    try {
      const r = await fetch('/api/agent/current', {
        credentials: 'same-origin',
        headers: authHeaders(),
      });
      if (r.status === 401) {
        // Definitive unauthenticated — reflect honestly. Operator sees
        // "unauthenticated" chip rather than a stale-but-misleading provider name.
        authProfile.set(null);
        connectedProviders.set([]);
        return;
      }
      if (!r.ok) return;  // 5xx / network glitch — keep prior state
      const ct = r.headers.get('content-type') ?? '';
      if (!ct.includes('application/json')) return;  // Vite SPA fallback guard
      const { kind, connected_providers } = await r.json();

      // Map server AgentKind → AuthProfile 1:1 (no collapsing — each kind has
      // its own chip color + label per the StatusBar palette above).
      const profile: import('$lib/types').AuthProfile | null =
        kind === 'lightarchitects'        ? 'lightarchitects' :
        kind === 'light_architect' ? 'light_architect' :
        kind === 'codex'                  ? 'codex' :
        kind === 'mistral_vibe'           ? 'mistral_vibe' :
        kind === 'anthropic'              ? 'anthropic' :
        kind === 'ollama'                 ? 'ollama' :
                                            null;   // Unknown kind — stay honest
      authProfile.set(profile);
      connectedProviders.set(Array.isArray(connected_providers) ? connected_providers : []);
    } catch {
      // Network error — keep previous state; we don't downgrade to null on
      // transient failures, only on definitive 401 above.
    }
  }

  // Backend picker state
  let pickerOpen = $state(false);

  type ConfirmTarget = { kind: string; label: string; color: string } | null;
  let confirmTarget = $state<ConfirmTarget>(null);

  const AGENT_MAP: Record<string, { label: string; color: string }> = {
    lightarchitects:         { label: 'Claude Code',   color: '#22C55E' },
    light_architect:  { label: 'lÆx0 Native',  color: '#14B8A6' },
    codex:                   { label: 'Codex',         color: '#A855F7' },
    mistral_vibe:            { label: 'Mistral Vibe',  color: '#FB923C' },
    anthropic:               { label: 'Anthropic',     color: '#F59E0B' },
    ollama:                  { label: 'Ollama',        color: '#6366F1' },
  };

  function openPicker() { pickerOpen = true; }
  function closePicker() { pickerOpen = false; }

  function handlePickerSelect(kind: string) {
    const meta = AGENT_MAP[kind] ?? { label: kind, color: '#6b7280' };
    confirmTarget = { kind, ...meta };
    pickerOpen = false;
  }

  function handleConfirm() {
    confirmTarget = null;
    // authProfile is updated via la:pty-respawned SSE event (no extra action needed)
  }

  function handleCancel() {
    confirmTarget = null;
  }

  function handlePtyRespawned() {
    pickerOpen = false;
    confirmTarget = null;
  }

  let pollInterval: ReturnType<typeof setInterval> | undefined;
  let agentPollInterval: ReturnType<typeof setInterval> | undefined;
  onMount(() => {
    void pollPreflight();
    void pollAgentCurrent();
    pollInterval = setInterval(() => { void pollPreflight(); }, 30_000);
    agentPollInterval = setInterval(() => { void pollAgentCurrent(); }, 30_000);
    unsubscribeAyin = subscribeByTopic('v1.agent.ayin.*', handleAyinEvent);
    window.addEventListener('la:pty-respawned', handlePtyRespawned);
  });
  onDestroy(() => {
    clearInterval(pollInterval);
    clearInterval(agentPollInterval);
    unsubscribeAyin?.();
    window.removeEventListener('la:pty-respawned', handlePtyRespawned);
  });
</script>

<div class="fixed bottom-[12px] left-1/2 -translate-x-1/2 flex items-center gap-[6px] pointer-events-auto z-10 bg-[var(--la-bg-elev-1)]/80 px-2 py-1 rounded backdrop-blur-sm border border-[var(--la-drawer-border)]">
  <!-- AYIN status (also surfaces auth failures from sse.ts; banner offers recovery) -->
  <div
    class="w-[7px] h-[7px] rounded-full shrink-0"
    style="background-color: {ayinColor}; box-shadow: 0 0 4px {ayinColor}"
  ></div>
  <span
    class="text-[11px] font-mono leading-none {auth !== 'ok' ? 'text-[var(--la-danger-text)]' : 'text-[var(--la-text-label)]'}"
  >
    {ayinLabel}
  </span>

  <div class="w-px h-3 bg-[var(--la-hair-strong)] mx-1"></div>

  <!-- HELIX indicator -->
  <div class="w-[7px] h-[7px] rounded-full shrink-0 bg-[var(--la-focus-ring)]" style="box-shadow: 0 0 6px #FFD700"></div>
  <span class="text-[11px] text-[var(--la-text-label)] font-mono leading-none">HELIX</span>

  <div class="w-px h-3 bg-[var(--la-hair-strong)] mx-1"></div>

  <!-- BUILD indicator -->
  <div class="w-[7px] h-[7px] rounded-full shrink-0 bg-[var(--la-agent-engineer)]" style="box-shadow: 0 0 4px #3B82F6"></div>
  <span class="text-[11px] text-[var(--la-text-label)] font-mono leading-none">BUILD</span>

  <div class="w-px h-3 bg-[var(--la-hair-strong)] mx-1"></div>

  <!-- PTY indicator -->
  <div
    class="w-[7px] h-[7px] rounded-full shrink-0"
    style="background-color: {ptyColor}; box-shadow: 0 0 4px {ptyColor}"
  ></div>
  <span class="text-[11px] text-[var(--la-text-label)] font-mono leading-none">{ptyLabel}</span>

  <div class="w-px h-3 bg-[var(--la-hair-strong)] mx-1"></div>

  <!-- Auth profile indicator — clickable chip opens BackendPicker -->
  <div class="relative flex items-center gap-[6px]">
    <button
      class="flex items-center gap-[6px] rounded px-1 py-0.5 -mx-1
             hover:bg-[var(--la-surface-hover)] transition-colors cursor-pointer"
      title="Active backend: {profileLabel} — click to switch"
      onclick={openPicker}
    >
      <span
        class="w-[7px] h-[7px] rounded-full shrink-0"
        style="background-color: {profileColor}; box-shadow: 0 0 4px {profileColor}"
      ></span>
      <span class="text-[11px] text-[var(--la-text-label)] font-mono leading-none">{profileLabel}</span>
    </button>

    {#if pickerOpen}
      <BackendPicker
        onselect={handlePickerSelect}
        onclose={closePicker}
      />
    {/if}
  </div>

  <div class="w-px h-3 bg-[var(--la-hair-strong)] mx-1"></div>

  <!-- Connected credential providers (live from /api/agent/current) -->
  <div
    class="w-[7px] h-[7px] rounded-full shrink-0"
    style="background-color: {connectedColor}; box-shadow: 0 0 4px {connectedColor}"
    title="{connectedLabel}"
  ></div>
  <span class="text-[11px] text-[var(--la-text-label)] font-mono leading-none whitespace-nowrap">{connectedLabel}</span>

  <div class="w-px h-3 bg-[var(--la-hair-strong)] mx-1"></div>

  <!-- Preflight readiness dot -->
  <div
    class="w-[7px] h-[7px] rounded-full shrink-0"
    style="background-color: {preflightDotColor(preflightOverall)}; box-shadow: 0 0 4px {preflightDotColor(preflightOverall)}"
  ></div>
  <span class="text-[11px] text-[var(--la-text-label)] font-mono leading-none">{preflightDotLabel(preflightOverall)}</span>

  {#if $agentTokenUsage.input > 0}
    <div class="w-px h-3 bg-[var(--la-hair-strong)] mx-1"></div>
    <span class="text-[10px] text-[var(--la-text-dim)] font-mono leading-none tabular-nums">
      {$agentTokenUsage.input.toLocaleString()}/{$agentTokenUsage.output.toLocaleString()} tok
    </span>
  {/if}
</div>

{#if confirmTarget}
  <RespawnConfirmModal
    targetKind={confirmTarget.kind}
    targetLabel={confirmTarget.label}
    targetColor={confirmTarget.color}
    currentKind={$authProfile ?? 'lightarchitects'}
    currentLabel={AGENT_MAP[$authProfile ?? 'lightarchitects']?.label ?? $authProfile ?? 'unknown'}
    currentColor={profileColor}
    onconfirm={handleConfirm}
    oncancel={handleCancel}
  />
{/if}