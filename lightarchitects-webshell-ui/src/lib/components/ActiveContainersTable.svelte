<script lang="ts">
  /**
   * ActiveContainersTable — live view of running agent containers.
   *
   * Auto-refreshes every 5 s via GET /api/container/active.
   * Shows hardening_actual columns so operators can see when Hardened mode
   * is degraded (e.g., userns remapping unavailable on the host kernel).
   */
  import { onMount, onDestroy } from 'svelte';

  // ── Types ────────────────────────────────────────────────────────────────

  interface HardeningActual {
    seccomp: boolean;
    cap_drop: boolean;
    userns: 'Remapped' | 'Host' | 'Unsupported';
  }

  type ContainerKind = { type: 'Pty' } | { type: 'WorkerTask'; task_id: string; wave_index: number };

  interface ActiveContainer {
    container_id: string;
    kind: ContainerKind;
    iso_mode_at_spawn: string;
    network_policy_at_spawn: string;
    hardening_actual: HardeningActual;
    spawned_at: string;
    age_secs: number;
  }

  // ── State ────────────────────────────────────────────────────────────────

  let containers = $state<ActiveContainer[]>([]);
  let loadError = $state<string | null>(null);
  let lastRefreshed = $state<Date | null>(null);

  let timer: ReturnType<typeof setInterval> | null = null;

  // ── Fetch ────────────────────────────────────────────────────────────────

  async function refresh() {
    try {
      const res = await fetch('/api/container/active', {
        headers: { Authorization: `Bearer ${localStorage.getItem('la_token') ?? ''}` },
      });
      if (!res.ok) {
        loadError = `HTTP ${res.status}`;
        return;
      }
      containers = (await res.json()) as ActiveContainer[];
      loadError = null;
      lastRefreshed = new Date();
    } catch (e) {
      loadError = e instanceof Error ? e.message : 'network error';
    }
  }

  onMount(() => {
    refresh();
    timer = setInterval(refresh, 5000);
  });

  onDestroy(() => {
    if (timer != null) clearInterval(timer);
  });

  // ── Helpers ──────────────────────────────────────────────────────────────

  function truncateId(id: string): string {
    return id.slice(0, 12);
  }

  function formatAge(secs: number): string {
    if (secs < 60) return `${secs}s`;
    if (secs < 3600) return `${Math.floor(secs / 60)}m`;
    return `${Math.floor(secs / 3600)}h ${Math.floor((secs % 3600) / 60)}m`;
  }

  function usernsClass(userns: HardeningActual['userns']): string {
    return { Remapped: 'userns-remapped', Host: 'userns-host', Unsupported: 'userns-unsup' }[userns];
  }

  function kindLabel(kind: ContainerKind): string {
    return kind.type === 'WorkerTask' ? 'worker' : 'pty';
  }

  function workerTaskTooltip(kind: ContainerKind): string | undefined {
    if (kind.type !== 'WorkerTask') return undefined;
    return `task: ${kind.task_id} · wave: ${kind.wave_index}`;
  }
</script>

<section class="active-containers" aria-label="Active containers">
  <header class="section-header">
    <h3>Active Containers</h3>
    {#if lastRefreshed}
      <span class="refresh-ts">refreshed {lastRefreshed.toLocaleTimeString()}</span>
    {/if}
  </header>

  {#if loadError}
    <p class="load-error">{loadError}</p>
  {:else if containers.length === 0}
    <p class="empty-state">No containers running.</p>
  {:else}
    <div class="table-scroll">
      <table>
        <thead>
          <tr>
            <th>ID</th>
            <th>Kind</th>
            <th>ISO mode</th>
            <th>Network</th>
            <th title="seccomp profile applied">seccomp</th>
            <th title="capabilities dropped">cap_drop</th>
            <th title="user-namespace remapping">userns</th>
            <th>Age</th>
          </tr>
        </thead>
        <tbody>
          {#each containers as c (c.container_id)}
            <tr data-kind={c.kind.type}>
              <td class="mono" title={c.container_id}>{truncateId(c.container_id)}</td>
              <td>
                <span
                  class="kind-badge kind-{kindLabel(c.kind)}"
                  title={workerTaskTooltip(c.kind)}
                >
                  {kindLabel(c.kind)}
                </span>
              </td>
              <td>{c.iso_mode_at_spawn}</td>
              <td>{c.network_policy_at_spawn}</td>
              <td class="check-cell">{c.hardening_actual.seccomp ? '✓' : '✗'}</td>
              <td class="check-cell">{c.hardening_actual.cap_drop ? '✓' : '✗'}</td>
              <td>
                <span class="userns-badge {usernsClass(c.hardening_actual.userns)}">
                  {c.hardening_actual.userns}
                </span>
              </td>
              <td class="mono">{formatAge(c.age_secs)}</td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  {/if}
</section>

<style>
  .active-containers {
    margin-top: 2rem;
  }

  .section-header {
    display: flex;
    align-items: baseline;
    gap: 0.75rem;
    margin-bottom: 0.5rem;
  }

  .section-header h3 {
    font-size: 0.8rem;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--color-text-secondary, #aaa);
    margin: 0;
  }

  .refresh-ts {
    font-size: 0.7rem;
    color: var(--color-text-secondary, #888);
  }

  .load-error,
  .empty-state {
    font-size: 0.8rem;
    color: var(--color-text-secondary, #aaa);
    padding: 0.5rem 0;
  }

  .load-error {
    color: var(--color-error, #e55);
  }

  .table-scroll {
    overflow-x: auto;
  }

  table {
    width: 100%;
    border-collapse: collapse;
    font-size: 0.8rem;
  }

  th {
    text-align: left;
    padding: 0.4rem 0.6rem;
    color: var(--color-text-secondary, #888);
    border-bottom: 1px solid var(--color-border, #333);
    white-space: nowrap;
  }

  td {
    padding: 0.4rem 0.6rem;
    border-bottom: 1px solid var(--color-border-subtle, #222);
  }

  .mono {
    font-family: monospace;
  }

  .check-cell {
    text-align: center;
  }

  .userns-badge {
    padding: 0.1rem 0.4rem;
    border-radius: 3px;
    font-size: 0.72rem;
  }

  .userns-remapped {
    background: rgba(91, 155, 213, 0.2);
    color: #5b9bd5;
  }

  .userns-host {
    background: rgba(230, 168, 23, 0.2);
    color: #e6a817;
  }

  .userns-unsup {
    background: rgba(200, 80, 80, 0.2);
    color: #e55;
  }

  /* ── Kind column ─────────────────────────────────────────────────────── */

  .kind-badge {
    padding: 0.1rem 0.4rem;
    border-radius: 3px;
    font-size: 0.72rem;
    font-family: monospace;
  }

  .kind-pty {
    background: rgba(120, 200, 120, 0.15);
    color: #7cc87c;
  }

  .kind-worker {
    background: rgba(160, 120, 230, 0.15);
    color: #a078e6;
  }

  /* Row-level tinting: worker-task rows get a faint purple stripe */
  tr[data-kind='WorkerTask'] {
    background: rgba(160, 120, 230, 0.04);
  }
</style>
