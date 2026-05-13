<script lang="ts">
  import { ayinStatus, builds, terminalConnected, authProfile, authStatus, agentTokenUsage } from '$lib/stores';
  import { STATUS_COLORS } from '$lib/design-tokens';

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
  // Auth profile indicator
  let profileColor = $derived($authProfile === 'anthropic' ? '#F59E0B' : $authProfile === 'lightarchitects' ? '#22C55E' : '#6366F1');
  let profileLabel = $derived($authProfile === 'anthropic' ? 'Anthropic' : $authProfile === 'lightarchitects' ? 'CLI' : 'Ollama');
</script>

<div class="absolute bottom-[12px] left-[12px] flex items-center gap-[6px] pointer-events-none z-10 bg-[var(--la-bg-elev-1)]/80 px-2 py-1 rounded backdrop-blur-sm border border-[var(--la-drawer-border)]">
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

  <!-- Auth profile indicator -->
  <div
    class="w-[7px] h-[7px] rounded-full shrink-0"
    style="background-color: {profileColor}; box-shadow: 0 0 4px {profileColor}"
  ></div>
  <span class="text-[11px] text-[var(--la-text-label)] font-mono leading-none">{profileLabel}</span>

  {#if $agentTokenUsage.input > 0}
    <div class="w-px h-3 bg-[var(--la-hair-strong)] mx-1"></div>
    <span class="text-[10px] text-[var(--la-text-dim)] font-mono leading-none tabular-nums">
      {$agentTokenUsage.input.toLocaleString()}→{$agentTokenUsage.output.toLocaleString()} tok
    </span>
  {/if}
</div>