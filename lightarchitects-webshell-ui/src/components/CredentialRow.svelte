<script lang="ts">
  import { authHeaders } from '$lib/auth';
  import ApiKeyInputModal from './ApiKeyInputModal.svelte';
  import DeviceFlowModal from './DeviceFlowModal.svelte';

  let {
    provider,
    label,
    prompt = '',
    flow,
  }: {
    /** Provider identifier matching the backend route segment. */
    provider: string;
    /** Human-readable provider label. */
    label: string;
    /** Prompt shown in the API key input modal (ApiKey flows only). */
    prompt?: string;
    /** Credential acquisition flow type. */
    flow: 'oauth' | 'device' | 'apikey' | 'cli';
  } = $props();

  type Status = 'loading' | 'connected' | 'not_connected';

  let status = $state<Status>('loading');
  let revoking = $state(false);
  let cliConnecting = $state(false);
  let showApiKeyModal = $state(false);
  let showDeviceModal = $state(false);
  let error = $state<string | null>(null);

  const flowMeta: Record<'oauth' | 'device' | 'apikey' | 'cli', { label: string }> = {
    oauth:  { label: 'OAuth'  },
    device: { label: 'Device' },
    apikey: { label: 'Key'    },
    cli:    { label: 'Local'  },
  };

  $effect(() => {
    void loadStatus();
  });

  async function loadStatus() {
    status = 'loading';
    try {
      const res = await fetch(`/api/auth/credential/${provider}/status`, {
        headers: authHeaders(),
      });
      if (res.ok) {
        const data = await res.json() as { state: string };
        status = data.state === 'connected' ? 'connected' : 'not_connected';
      } else {
        status = 'not_connected';
      }
    } catch {
      status = 'not_connected';
    }
  }

  async function connect() {
    error = null;
    switch (flow) {
      case 'apikey':
        showApiKeyModal = true;
        break;
      case 'device':
        showDeviceModal = true;
        break;
      case 'oauth': {
        try {
          const res = await fetch(`/api/auth/credential/${provider}/init`, {
            method: 'POST',
            headers: authHeaders(),
          });
          if (res.ok) {
            const data = await res.json() as { redirect_url: string };
            window.open(data.redirect_url, '_blank', 'noopener,noreferrer');
          } else {
            error = `Failed to start OAuth flow (HTTP ${res.status})`;
          }
        } catch (e) {
          error = e instanceof Error ? e.message : 'Network error';
        }
        break;
      }
      case 'cli': {
        cliConnecting = true;
        try {
          const res = await fetch(`/api/auth/credential/${provider}/connect`, {
            method: 'POST',
            headers: authHeaders(),
          });
          if (res.ok || res.status === 201) {
            status = 'connected';
          } else {
            const body = await res.json().catch(() => ({})) as { error?: string };
            error = body.error ?? `HTTP ${res.status}`;
          }
        } catch (e) {
          error = e instanceof Error ? e.message : 'Network error';
        } finally {
          cliConnecting = false;
        }
        break;
      }
    }
  }

  async function revoke() {
    revoking = true;
    error = null;
    try {
      const res = await fetch(`/api/auth/credential/${provider}`, {
        method: 'DELETE',
        headers: authHeaders(),
      });
      if (res.ok || res.status === 204) {
        status = 'not_connected';
      } else {
        error = `Revoke failed (HTTP ${res.status})`;
      }
    } catch (e) {
      error = e instanceof Error ? e.message : 'Network error';
    } finally {
      revoking = false;
    }
  }

  function onConnected() {
    status = 'connected';
  }
</script>

<div class="flex items-center gap-3 py-2 px-1">
  <!-- Status dot -->
  <span
    class="w-2 h-2 rounded-full flex-shrink-0 transition-colors"
    class:bg-green-500={status === 'connected'}
    class:bg-slate-600={status === 'not_connected'}
    class:bg-yellow-400={status === 'loading'}
    class:animate-pulse={status === 'loading'}
    aria-hidden="true"
  ></span>

  <!-- Label + flow badge -->
  <span class="text-sm text-[var(--la-text-bright)] flex-1 min-w-0 flex items-center gap-1.5">
    <span class="truncate">{label}</span>
    <span
      class="flow-pill"
      class:flow-oauth={flow === 'oauth'}
      class:flow-device={flow === 'device'}
      class:flow-apikey={flow === 'apikey'}
      class:flow-cli={flow === 'cli'}
    >{flowMeta[flow].label}</span>
  </span>

  <!-- Status text -->
  <span class="text-[11px] text-[var(--la-text-dim)] w-20 text-right flex-shrink-0">
    {#if status === 'loading'}Checking…
    {:else if status === 'connected'}Connected
    {:else}Not connected
    {/if}
  </span>

  <!-- Action button -->
  {#if status === 'connected'}
    <button
      onclick={() => void revoke()}
      disabled={revoking}
      class="text-[11px] px-2 py-1 rounded border border-[var(--la-drawer-border)] text-[var(--la-text-dim)] hover:text-red-400 hover:border-red-400/40 transition-colors disabled:opacity-40"
      aria-label="Disconnect {label}"
    >
      {revoking ? '…' : 'Disconnect'}
    </button>
  {:else if status === 'not_connected'}
    <button
      onclick={() => void connect()}
      disabled={cliConnecting}
      class="text-[11px] px-2 py-1 rounded border border-[var(--la-drawer-border)] text-[var(--la-text-dim)] hover:text-[var(--la-text-bright)] hover:border-[var(--la-hair-strong)] transition-colors disabled:opacity-40"
      aria-label="Connect {label}"
    >
      {cliConnecting ? 'Connecting…' : 'Connect'}
    </button>
  {:else}
    <span class="w-[66px]"></span>
  {/if}
</div>

{#if error}
  <p class="text-[11px] text-red-400 pl-5 pb-1">{error}</p>
{/if}

<!-- Modals rendered at component level (portaled by z-index) -->
<ApiKeyInputModal
  {provider}
  {label}
  {prompt}
  isOpen={showApiKeyModal}
  onClose={() => { showApiKeyModal = false; }}
  {onConnected}
/>

<DeviceFlowModal
  isOpen={showDeviceModal}
  onClose={() => { showDeviceModal = false; }}
  {onConnected}
/>

<style>
  .flow-pill {
    font-family: 'IBM Plex Mono', monospace;
    font-size: 0.58rem;
    font-weight: 600;
    border: 1px solid;
    border-radius: 3px;
    padding: 0.1rem 0.3rem;
    white-space: nowrap;
    flex-shrink: 0;
    letter-spacing: 0.04em;
  }
  .flow-oauth  { color: #60a5fa; border-color: rgba(96,165,250,0.35);  background: rgba(96,165,250,0.08);  }
  .flow-device { color: #c084fc; border-color: rgba(192,132,252,0.35); background: rgba(192,132,252,0.08); }
  .flow-apikey { color: #fbbf24; border-color: rgba(251,191,36,0.35);  background: rgba(251,191,36,0.08);  }
  .flow-cli    { color: #4ade80; border-color: rgba(74,222,128,0.35);  background: rgba(74,222,128,0.08);  }
</style>
