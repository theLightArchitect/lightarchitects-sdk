<script lang="ts">
  import { authHeaders } from '$lib/auth';

  let {
    isOpen,
    onClose,
    onConnected,
  }: {
    isOpen: boolean;
    onClose: () => void;
    onConnected: () => void;
  } = $props();

  type Phase = 'loading' | 'polling' | 'connected' | 'denied' | 'expired' | 'error';

  let phase = $state<Phase>('loading');
  let userCode = $state('');
  let verificationUri = $state('');
  let deviceCode = $state('');
  let pollIntervalSecs = $state(5);
  let error = $state<string | null>(null);
  let pollTimer = $state<ReturnType<typeof setInterval> | null>(null);

  $effect(() => {
    if (isOpen) {
      void initFlow();
    } else {
      clearPoll();
      phase = 'loading';
      userCode = '';
      verificationUri = '';
      deviceCode = '';
      error = null;
    }
    return () => clearPoll();
  });

  function clearPoll() {
    if (pollTimer !== null) {
      clearInterval(pollTimer);
      pollTimer = null;
    }
  }

  async function initFlow() {
    phase = 'loading';
    error = null;
    try {
      const res = await fetch('/api/auth/credential/github/device', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json', ...authHeaders() },
      });
      if (!res.ok) {
        const body = await res.json().catch(() => ({})) as { error?: string };
        error = body.error ?? `HTTP ${res.status}`;
        phase = 'error';
        return;
      }
      const data = await res.json() as {
        user_code: string;
        verification_uri: string;
        device_code: string;
        interval: number;
      };
      userCode = data.user_code;
      verificationUri = data.verification_uri;
      deviceCode = data.device_code;
      pollIntervalSecs = data.interval ?? 5;
      phase = 'polling';
      window.open(verificationUri, '_blank', 'noopener,noreferrer');
      startPolling();
    } catch (e) {
      error = e instanceof Error ? e.message : 'Network error';
      phase = 'error';
    }
  }

  function startPolling() {
    clearPoll();
    pollTimer = setInterval(() => { void poll(); }, pollIntervalSecs * 1000);
  }

  async function poll() {
    try {
      const res = await fetch('/api/auth/credential/github/poll', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json', ...authHeaders() },
        body: JSON.stringify({ device_code: deviceCode }),
      });
      if (!res.ok) return;
      const data = await res.json() as { status: string };
      switch (data.status) {
        case 'connected':
          clearPoll();
          phase = 'connected';
          setTimeout(() => { onConnected(); onClose(); }, 1500);
          break;
        case 'slow_down':
          pollIntervalSecs = Math.min(pollIntervalSecs * 2, 30);
          clearPoll();
          startPolling();
          break;
        case 'denied':
          clearPoll();
          phase = 'denied';
          break;
        case 'expired':
          clearPoll();
          phase = 'expired';
          break;
      }
    } catch {
      // transient network error — keep polling
    }
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') { clearPoll(); onClose(); }
  }

  function copyCode() {
    void navigator.clipboard.writeText(userCode);
  }
</script>

{#if isOpen}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm"
    role="dialog"
    aria-modal="true"
    aria-label="GitHub Device Flow"
    onkeydown={handleKeydown}
  >
    <div class="w-[440px] bg-[var(--la-bg-frame)] border border-[var(--la-drawer-border)] rounded-lg shadow-2xl p-6 flex flex-col gap-5">
      <div class="flex items-center justify-between">
        <h2 class="text-sm font-semibold text-[var(--la-text-bright)]">Connect GitHub</h2>
        <button
          onclick={() => { clearPoll(); onClose(); }}
          class="text-[var(--la-text-dim)] hover:text-[var(--la-text-bright)] transition-colors text-lg leading-none"
          aria-label="Close"
        >×</button>
      </div>

      {#if phase === 'loading'}
        <p class="text-sm text-[var(--la-text-dim)]">Starting Device Flow…</p>

      {:else if phase === 'polling'}
        <div class="flex flex-col gap-4">
          <p class="text-xs text-[var(--la-text-dim)]">
            A browser tab opened to <span class="text-[var(--la-text-bright)] font-mono text-[11px]">{verificationUri}</span>.
            Enter the code below to authorize:
          </p>
          <div class="flex items-center gap-3">
            <span class="font-mono text-2xl font-bold tracking-widest text-[var(--la-text-bright)] bg-[var(--la-bg-elev-1)] px-4 py-2 rounded border border-[var(--la-drawer-border)] select-all">
              {userCode}
            </span>
            <button
              onclick={copyCode}
              class="text-xs text-[var(--la-text-dim)] hover:text-[var(--la-text-bright)] border border-[var(--la-drawer-border)] rounded px-3 py-2 transition-colors"
              aria-label="Copy code to clipboard"
            >Copy</button>
          </div>
          <p class="text-[11px] text-[var(--la-text-dim)]">Waiting for authorization… polling every {pollIntervalSecs}s</p>
        </div>

      {:else if phase === 'connected'}
        <div class="flex items-center gap-2 text-green-400">
          <span class="text-lg">✓</span>
          <p class="text-sm font-medium">GitHub connected!</p>
        </div>

      {:else if phase === 'denied'}
        <div class="flex flex-col gap-3">
          <p class="text-sm text-red-400">Authorization denied by GitHub.</p>
          <button
            onclick={() => void initFlow()}
            class="self-start px-4 py-2 text-sm border border-[var(--la-drawer-border)] rounded hover:border-[var(--la-hair-strong)] text-[var(--la-text-dim)] hover:text-[var(--la-text-bright)] transition-colors"
          >Retry</button>
        </div>

      {:else if phase === 'expired'}
        <div class="flex flex-col gap-3">
          <p class="text-sm text-yellow-400">Device code expired. Please restart the flow.</p>
          <button
            onclick={() => void initFlow()}
            class="self-start px-4 py-2 text-sm border border-[var(--la-drawer-border)] rounded hover:border-[var(--la-hair-strong)] text-[var(--la-text-dim)] hover:text-[var(--la-text-bright)] transition-colors"
          >Restart</button>
        </div>

      {:else if phase === 'error'}
        <div class="flex flex-col gap-3">
          <p class="text-sm text-red-400">{error ?? 'Unknown error'}</p>
          <button
            onclick={() => void initFlow()}
            class="self-start px-4 py-2 text-sm border border-[var(--la-drawer-border)] rounded hover:border-[var(--la-hair-strong)] text-[var(--la-text-dim)] hover:text-[var(--la-text-bright)] transition-colors"
          >Retry</button>
        </div>
      {/if}

      {#if phase !== 'connected'}
        <div class="flex justify-end">
          <button
            onclick={() => { clearPoll(); onClose(); }}
            class="px-4 py-2 text-sm text-[var(--la-text-dim)] hover:text-[var(--la-text-bright)] rounded border border-[var(--la-drawer-border)] hover:border-[var(--la-hair-strong)] transition-colors"
          >Cancel</button>
        </div>
      {/if}
    </div>
  </div>
{/if}
