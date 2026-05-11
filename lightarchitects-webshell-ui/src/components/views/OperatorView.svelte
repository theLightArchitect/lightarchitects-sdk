<script lang="ts">
  import { activeBuild, logEntries, artifacts, isNativeAgent } from '$lib/stores';
  import LogStream from '$lib/../components/LogStream.svelte';
  import SiblingDispatch from '$lib/../components/SiblingDispatch.svelte';
  import AgentConsole from '$lib/../components/AgentConsole.svelte';
  import ArtifactPanel from '$lib/../components/ArtifactPanel.svelte';

  let build = $derived($activeBuild);

  // LogEntry has no buildId — show the global log stream
  let buildLogs = $derived($logEntries);

  let buildArtifacts = $derived(
    $artifacts.filter(a => !build || a.buildId === build.id)
  );
</script>

<div class="operator-wrap" data-testid="operator-view">
  {#if !build}
    <div class="operator-empty">— no build selected —</div>
  {:else}
    <div class="operator-main">
      <!-- Log stream — primary panel -->
      <div class="log-panel">
        <div class="panel-head">
          <span class="panel-label">LOG STREAM</span>
          <span class="panel-meta">{buildLogs.length} entries</span>
        </div>
        <div class="panel-body">
          <LogStream entries={buildLogs} />
        </div>
      </div>

      <!-- Right column: agent dispatch + artifacts -->
      <div class="operator-side">
        <div class="side-panel">
          {#if $isNativeAgent}
            <AgentConsole />
          {:else}
            <div class="panel-head">
              <span class="panel-label">AGENT DISPATCH</span>
            </div>
            <div class="panel-body">
              <SiblingDispatch />
            </div>
          {/if}
        </div>

        {#if buildArtifacts.length > 0}
          <div class="side-panel">
            <div class="panel-head">
              <span class="panel-label">ARTIFACTS</span>
              <span class="panel-meta">{buildArtifacts.length}</span>
            </div>
            <div class="panel-body">
              <ArtifactPanel artifacts={buildArtifacts} />
            </div>
          </div>
        {/if}
      </div>
    </div>
  {/if}
</div>

<style>
  .operator-wrap {
    display: flex;
    flex-direction: column;
    height: 100%;
    overflow: hidden;
  }

  .operator-empty {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--la-text-mute);
    font-size: 11px;
    letter-spacing: 0.12em;
    font-style: italic;
  }

  .operator-main {
    display: flex;
    flex: 1;
    min-height: 0;
    overflow: hidden;
    gap: 0;
  }

  .log-panel {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    border-right: 1px solid var(--la-hair-faint);
    overflow: hidden;
  }

  .operator-side {
    width: 320px;
    flex-shrink: 0;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  .side-panel {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    border-bottom: 1px solid var(--la-hair-faint);
    overflow: hidden;
  }
  .side-panel:last-child { border-bottom: none; }

  .panel-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 6px 14px;
    border-bottom: 1px solid var(--la-hair-faint);
    flex-shrink: 0;
  }

  .panel-label {
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.14em;
    text-transform: uppercase;
    color: var(--la-text-mute);
  }

  .panel-meta {
    font-size: 9px;
    color: var(--la-text-mute);
    font-variant-numeric: tabular-nums;
  }

  .panel-body {
    flex: 1;
    overflow: hidden;
    min-height: 0;
  }
</style>
