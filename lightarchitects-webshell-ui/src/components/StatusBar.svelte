<script lang="ts">
  import { ayinStatus, builds, terminalConnected, authProfile } from '$lib/stores';
  import { STATUS_COLORS } from '$lib/design-tokens';

  let status = $derived($ayinStatus);
  let buildCount = $derived($builds.length);
  let ptyColor = $derived($terminalConnected ? STATUS_COLORS.connected : STATUS_COLORS.offline);
  let ptyLabel = $derived($terminalConnected ? 'PTY live' : 'PTY off');

  const statusColor = (s: string) => STATUS_COLORS[s as keyof typeof STATUS_COLORS] ?? '#6b7280';
</script>

<div class="absolute bottom-[12px] left-[12px] flex items-center gap-[6px] pointer-events-none z-10 bg-[#111827]/80 px-2 py-1 rounded backdrop-blur-sm border border-[#1e293b]">
  <!-- AYIN status -->
  <div
    class="w-[7px] h-[7px] rounded-full shrink-0"
    style="background-color: {statusColor(status)}; box-shadow: 0 0 4px {statusColor(status)}"
  ></div>
  <span class="text-[11px] text-[#94a3b8] font-mono leading-none">
    {status === 'connected' ? `AYIN live · ${buildCount} builds` : status === 'reconnecting' ? 'reconnecting…' : 'AYIN offline'}
  </span>

  <div class="w-px h-3 bg-[#334155] mx-1"></div>

  <!-- HELIX indicator -->
  <div class="w-[7px] h-[7px] rounded-full shrink-0 bg-[#FFD700]" style="box-shadow: 0 0 6px #FFD700"></div>
  <span class="text-[11px] text-[#94a3b8] font-mono leading-none">HELIX</span>

  <div class="w-px h-3 bg-[#334155] mx-1"></div>

  <!-- BUILD indicator -->
  <div class="w-[7px] h-[7px] rounded-full shrink-0 bg-[#3B82F6]" style="box-shadow: 0 0 4px #3B82F6"></div>
  <span class="text-[11px] text-[#94a3b8] font-mono leading-none">BUILD</span>

  <div class="w-px h-3 bg-[#334155] mx-1"></div>

  <!-- PTY indicator -->
  <div
    class="w-[7px] h-[7px] rounded-full shrink-0"
    style="background-color: {ptyColor}; box-shadow: 0 0 4px {ptyColor}"
  ></div>
  <span class="text-[11px] text-[#94a3b8] font-mono leading-none">{ptyLabel}</span>

  <div class="w-px h-3 bg-[#334155] mx-1"></div>

  <!-- Auth profile indicator -->
  <div
    class="w-[7px] h-[7px] rounded-full shrink-0"
    style="background-color: {$authProfile === 'anthropic' ? '#F59E0B' : '#6366F1'}; box-shadow: 0 0 4px {$authProfile === 'anthropic' ? '#F59E0B' : '#6366F1'}"
  ></div>
  <span class="text-[11px] text-[#94a3b8] font-mono leading-none">
    {$authProfile === 'anthropic' ? 'Anthropic' : 'Ollama'}
  </span>
</div>