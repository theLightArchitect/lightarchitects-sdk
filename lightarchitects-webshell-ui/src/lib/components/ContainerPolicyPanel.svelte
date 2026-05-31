<script lang="ts">
  /** ContainerPolicyPanel — /#/security — live container spawn policy controls. */
  import { onDestroy } from 'svelte';
  import { api } from '$lib/api';
  import { PHASE_2_DISCLOSURE } from '$lib/types';
  import type { ContainerPolicyResponse, IsoMode, NetworkPolicy } from '$lib/types';
  import ActiveContainersTable from './ActiveContainersTable.svelte';

  // ── State ────────────────────────────────────────────────────────────────

  let policy = $state<ContainerPolicyResponse | null>(null);
  let loading = $state(true);
  let error = $state<string | null>(null);
  let toast = $state<string | null>(null);
  let saving = $state(false);
  let advancedOpen = $state(false);

  // Pending patch values (debounced before sending)
  let pendingPatch = $state<Partial<ContainerPolicyResponse>>({});
  let debounceTimer: ReturnType<typeof setTimeout> | null = null;

  // ── ISO mode ordering (tighter = higher index) ───────────────────────────

  const ISO_MODES: IsoMode[] = ['standard', 'hardened', 'airgapped'];
  const NETWORK_POLICIES: NetworkPolicy[] = ['bridge', 'host', 'none'];

  // ── Lifecycle ────────────────────────────────────────────────────────────

  async function loadPolicy() {
    loading = true;
    error = null;
    try {
      policy = await api.getContainerPolicy();
    } catch (e) {
      error = e instanceof Error ? e.message : 'Failed to load policy';
    } finally {
      loading = false;
    }
  }

  loadPolicy();

  onDestroy(() => {
    if (debounceTimer != null) clearTimeout(debounceTimer);
  });

  // ── Patch dispatch (500 ms debounced) ────────────────────────────────────

  function scheduleUpdate(patch: Partial<ContainerPolicyResponse>) {
    pendingPatch = { ...pendingPatch, ...patch };
    if (debounceTimer != null) clearTimeout(debounceTimer);
    debounceTimer = setTimeout(sendPatch, 500);
  }

  async function sendPatch() {
    if (Object.keys(pendingPatch).length === 0) return;
    const patch = { ...pendingPatch };
    pendingPatch = {};
    saving = true;
    try {
      policy = await api.updateContainerPolicy(patch);
      showToast('Policy saved.');
    } catch (e: unknown) {
      const msg = e instanceof Error ? e.message : String(e);
      if (msg.includes('412') || msg.includes('Precondition')) {
        // Concurrent mutation — re-fetch and notify
        await loadPolicy();
        showToast('Someone else changed the policy — refreshed.');
      } else {
        showToast(`Error: ${msg}`);
      }
    } finally {
      saving = false;
    }
  }

  function showToast(msg: string) {
    toast = msg;
    setTimeout(() => { toast = null; }, 3500);
  }

  // ── ISO mode helpers ─────────────────────────────────────────────────────

  function canSelectIso(mode: IsoMode): boolean {
    if (policy == null) return false;
    // Monotonic-tighten: can only select current or tighter
    return ISO_MODES.indexOf(mode) >= ISO_MODES.indexOf(policy.iso_mode);
  }

  function selectIso(mode: IsoMode) {
    if (!canSelectIso(mode) || policy == null) return;
    policy = { ...policy, iso_mode: mode };
    scheduleUpdate({ iso_mode: mode });
  }

  function selectNetwork(net: NetworkPolicy) {
    if (net === 'balanced' || policy == null) return;
    policy = { ...policy, network: net };
    scheduleUpdate({ network: net });
  }

  function isoLabel(m: IsoMode): string {
    return { standard: 'Standard', hardened: 'Hardened', airgapped: 'Airgapped' }[m];
  }

  function networkLabel(n: NetworkPolicy): string {
    return { bridge: 'Bridge', host: 'Host', none: 'None', balanced: 'Balanced' }[n];
  }
</script>

<div class="policy-panel">
  <header class="panel-header">
    <h2>Container Spawn Policy</h2>
    <p class="subtitle">
      Controls applied to every agent container at spawn time. Changes take effect
      immediately and are monotonic — you can only tighten, not loosen, during a session.
    </p>
  </header>

  {#if loading}
    <div class="state-placeholder">Loading…</div>
  {:else if error}
    <div class="state-placeholder error">{error}</div>
  {:else if policy}
    <section class="control-section" aria-label="Isolation mode">
      <h3>Isolation Mode</h3>
      <div class="segment-group" role="group" aria-label="Isolation mode selection">
        {#each ISO_MODES as mode}
          {@const active = policy.iso_mode === mode}
          {@const allowed = canSelectIso(mode)}
          <button
            class="segment"
            class:active
            class:locked={!allowed}
            aria-pressed={active}
            aria-disabled={!allowed}
            disabled={!allowed}
            onclick={() => selectIso(mode)}
            title={allowed ? isoLabel(mode) : 'Cannot loosen isolation mode'}
          >
            {isoLabel(mode)}
          </button>
        {/each}
      </div>
      <p class="hint">
        {#if policy.iso_mode === 'standard'}
          Resource limits only — memory, CPU, pids, <code>no-new-privileges</code>.
        {:else if policy.iso_mode === 'hardened'}
          Standard + <code>--cap-drop ALL</code> + seccomp + non-root user + read-only root fs.
        {:else}
          Hardened + <code>--network none</code> — agents have no outbound network access.
        {/if}
      </p>
    </section>

    <section class="control-section" aria-label="Network policy">
      <h3>Network Policy</h3>
      <div class="segment-group" role="group" aria-label="Network policy selection">
        {#each NETWORK_POLICIES as net}
          {@const active = policy.network === net}
          <button
            class="segment"
            class:active
            aria-pressed={active}
            onclick={() => selectNetwork(net)}
          >
            {networkLabel(net)}
          </button>
        {/each}
        <!-- Balanced — Phase 2 placeholder, always disabled -->
        <button
          class="segment phase2"
          aria-pressed={false}
          aria-disabled="true"
          disabled
          title={PHASE_2_DISCLOSURE}
        >
          Balanced
          <span class="phase2-badge">Phase 2</span>
        </button>
      </div>
      {#if policy.iso_mode === 'airgapped'}
        <p class="hint warning">Network policy locked to None in Airgapped mode.</p>
      {/if}
    </section>

    <!-- Advanced (resources + tier) — collapsible -->
    <details class="advanced-section" bind:open={advancedOpen}>
      <summary>Advanced — Resources &amp; Tier</summary>

      <div class="slider-grid">
        <label>
          Memory cap
          <span class="slider-value">{policy.memory_mb} MiB</span>
          <input
            type="range"
            min="128"
            max={policy.memory_mb}
            step="64"
            value={policy.memory_mb}
            oninput={(e) => {
              const v = parseInt((e.target as HTMLInputElement).value, 10);
              policy = policy != null ? { ...policy, memory_mb: v } : policy;
              scheduleUpdate({ memory_mb: v });
            }}
          />
        </label>

        <label>
          CPU quota
          <span class="slider-value">{policy.cpus.toFixed(1)} cores</span>
          <input
            type="range"
            min="0.1"
            max={policy.cpus}
            step="0.1"
            value={policy.cpus}
            oninput={(e) => {
              const v = parseFloat((e.target as HTMLInputElement).value);
              policy = policy != null ? { ...policy, cpus: v } : policy;
              scheduleUpdate({ cpus: v });
            }}
          />
        </label>

        <label>
          PID limit
          <span class="slider-value">{policy.pids_limit} pids</span>
          <input
            type="range"
            min="16"
            max={policy.pids_limit}
            step="16"
            value={policy.pids_limit}
            oninput={(e) => {
              const v = parseInt((e.target as HTMLInputElement).value, 10);
              policy = policy != null ? { ...policy, pids_limit: v } : policy;
              scheduleUpdate({ pids_limit: v });
            }}
          />
        </label>

        <label>
          Concurrent cap
          <span class="slider-value">{policy.max_concurrent} containers</span>
          <input
            type="range"
            min="1"
            max={policy.max_concurrent}
            step="1"
            value={policy.max_concurrent}
            oninput={(e) => {
              const v = parseInt((e.target as HTMLInputElement).value, 10);
              policy = policy != null ? { ...policy, max_concurrent: v } : policy;
              scheduleUpdate({ max_concurrent: v });
            }}
          />
        </label>
      </div>

      <!-- Phase 2 callout for credential_strategy -->
      <div class="phase2-callout" role="note">
        <strong>Credential Strategy</strong> — {PHASE_2_DISCLOSURE}
      </div>
    </details>

    <ActiveContainersTable />
  {/if}

  {#if toast}
    <div class="toast" role="status" aria-live="polite">{toast}</div>
  {/if}

  {#if saving}
    <div class="saving-indicator" aria-label="Saving policy…">Saving…</div>
  {/if}
</div>

<style>
  .policy-panel {
    padding: 1.5rem 2rem;
    max-width: 720px;
    position: relative;
  }

  .panel-header h2 {
    font-size: 1.25rem;
    font-weight: 600;
    margin: 0 0 0.25rem;
  }

  .subtitle {
    color: var(--color-text-secondary, #aaa);
    font-size: 0.875rem;
    margin: 0 0 1.5rem;
  }

  .control-section {
    margin-bottom: 1.5rem;
  }

  h3 {
    font-size: 0.8rem;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--color-text-secondary, #aaa);
    margin: 0 0 0.5rem;
  }

  .segment-group {
    display: flex;
    gap: 0;
    border-radius: 6px;
    overflow: hidden;
    border: 1px solid var(--color-border, #333);
    width: fit-content;
  }

  .segment {
    padding: 0.4rem 1rem;
    background: var(--color-surface, #1a1a1a);
    color: var(--color-text, #eee);
    border: none;
    border-right: 1px solid var(--color-border, #333);
    cursor: pointer;
    font-size: 0.875rem;
    transition: background 0.15s;
  }

  .segment:last-child {
    border-right: none;
  }

  .segment.active {
    background: var(--color-accent, #5b9bd5);
    color: #fff;
  }

  .segment.locked {
    opacity: 0.4;
    cursor: not-allowed;
  }

  .segment.phase2 {
    opacity: 0.5;
    cursor: not-allowed;
    display: flex;
    align-items: center;
    gap: 0.4rem;
  }

  .phase2-badge {
    font-size: 0.65rem;
    background: var(--color-border, #444);
    border-radius: 3px;
    padding: 0 0.3rem;
  }

  .hint {
    font-size: 0.8rem;
    color: var(--color-text-secondary, #aaa);
    margin: 0.4rem 0 0;
  }

  .hint.warning {
    color: var(--color-warning, #e6a817);
  }

  .advanced-section {
    margin-bottom: 1.5rem;
    border: 1px solid var(--color-border, #333);
    border-radius: 6px;
    padding: 0.75rem 1rem;
  }

  summary {
    cursor: pointer;
    font-size: 0.875rem;
    font-weight: 500;
    color: var(--color-text, #eee);
  }

  .slider-grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 1rem;
    margin-top: 1rem;
  }

  label {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
    font-size: 0.8rem;
    color: var(--color-text-secondary, #aaa);
  }

  .slider-value {
    font-variant-numeric: tabular-nums;
    color: var(--color-text, #eee);
    font-size: 0.85rem;
  }

  input[type='range'] {
    width: 100%;
    accent-color: var(--color-accent, #5b9bd5);
  }

  .phase2-callout {
    margin-top: 1rem;
    padding: 0.6rem 0.75rem;
    border-left: 3px solid var(--color-border, #444);
    font-size: 0.8rem;
    color: var(--color-text-secondary, #aaa);
  }

  .toast {
    position: fixed;
    bottom: 1.5rem;
    left: 50%;
    transform: translateX(-50%);
    background: var(--color-surface-elevated, #2a2a2a);
    border: 1px solid var(--color-border, #444);
    padding: 0.5rem 1.25rem;
    border-radius: 6px;
    font-size: 0.875rem;
    z-index: 100;
  }

  .saving-indicator {
    position: absolute;
    top: 1.5rem;
    right: 2rem;
    font-size: 0.75rem;
    color: var(--color-text-secondary, #aaa);
  }

  .state-placeholder {
    padding: 2rem;
    text-align: center;
    color: var(--color-text-secondary, #aaa);
  }

  .state-placeholder.error {
    color: var(--color-error, #e55);
  }
</style>
