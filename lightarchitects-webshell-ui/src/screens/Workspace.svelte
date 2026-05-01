<script lang="ts">
  import { activeBuild, builds, currentBuildId, focusedSibling, spikeSibling, findings, logEntries, selectedPillar, expandedFindings, artifacts, buildNotes, copilotMessages } from '$lib/stores';
  import type { CopilotMessage } from '$lib/types';
  import { PILLAR_COLORS, PILLARS, SIBLING_COLORS, getMetaSkillPolytope, getMetaSkillColor } from '$lib/design-tokens';
  import { SIBLINGS, PILLAR_ACTIONS, type SiblingId, type Pillar } from '$lib/types';
  import { api } from '$lib/api';
  import PillarRail from '$lib/../components/PillarRail.svelte';
  import HierarchyNav from '$lib/../components/HierarchyNav.svelte';
  import PolytopeIcon from '$lib/../components/PolytopeIcon.svelte';
  import PillarDetail from '$lib/../components/PillarDetail.svelte';
  import FindingsPanel from '$lib/../components/FindingsPanel.svelte';
  import LogStream from '$lib/../components/LogStream.svelte';
  import SiblingDispatch from '$lib/../components/SiblingDispatch.svelte';
  import ArtifactPanel from '$lib/../components/ArtifactPanel.svelte';
  import BuildNotes from '$lib/../components/BuildNotes.svelte';
  import PolytopeDecor from '$lib/../components/PolytopeDecor.svelte';
  import PlanView from '$lib/../components/PlanView.svelte';
  import PhaseTimeline from '$lib/../components/PhaseTimeline.svelte';
  import QualityGateDash from '$lib/../components/QualityGateDash.svelte';

  let build = $derived($activeBuild);

  function goBack() {
    currentBuildId.set(null);
    window.location.hash = '/';
  }

  function dispatchSibling(sib: SiblingId, prompt?: string) {
    focusedSibling.set(sib);
    spikeSibling(sib);
    if (!build) return;

    const userPrompt = prompt ?? '';
    // Append user message immediately so the drawer shows intent.
    const userMsg: CopilotMessage = {
      id: `dispatch-user-${Date.now()}`,
      role: 'user',
      content: `[→ ${sib.toUpperCase()}] ${userPrompt}`,
      sibling: sib,
      timestamp: new Date().toISOString(),
    };
    copilotMessages.update(ms => [...ms, userMsg]);
    // Open the Copilot drawer so the response is visible.
    window.dispatchEvent(new CustomEvent('la:open-copilot'));

    api.dispatchSibling(build.id, sib, sib, userPrompt)
      .then(resp => {
        const assistantMsg: CopilotMessage = {
          id: `dispatch-resp-${Date.now()}`,
          role: 'assistant',
          content: (resp as { response?: string }).response ?? '(no response)',
          sibling: sib,
          timestamp: new Date().toISOString(),
        };
        copilotMessages.update(ms => [...ms, assistantMsg]);
      })
      .catch(() => {
        const errMsg: CopilotMessage = {
          id: `dispatch-err-${Date.now()}`,
          role: 'system',
          content: `[${sib.toUpperCase()}] Dispatch failed — agent unavailable`,
          sibling: sib,
          timestamp: new Date().toISOString(),
        };
        copilotMessages.update(ms => [...ms, errMsg]);
      });
  }

  // Derived: findings for current build, optionally filtered by selected pillar
  let buildFindings = $derived(
    build ? $findings.filter(f => f.buildId === build.id) : []
  );
  let filteredFindings = $derived(
    $selectedPillar
      ? buildFindings.filter(f => f.pillar === $selectedPillar)
      : buildFindings
  );

  // Derived: log entries for current build
  let buildLogs = $derived($logEntries);

  // Selected pillar gate
  let selectedGate = $derived(
    build && $selectedPillar
      ? build.pillars.find(p => p.pillar === $selectedPillar) ?? null
      : null
  );

  // Click handler for pillar selection
  function selectPillar(pillar: Pillar) {
    selectedPillar.update(v => v === pillar ? null : pillar);
  }

  // Finding expand/collapse handler
  function toggleFindingExpand(id: string) {
    expandedFindings.update(set => {
      const next = new Set(set);
      if (next.has(id)) next.delete(id); else next.add(id);
      return next;
    });
  }

  // Finding verify handler
  function verifyFinding(id: string) {
    findings.update(f => f.map(finding =>
      finding.id === id ? { ...finding, verified: true } : finding
    ));
  }

  // Open a file in the system editor via /api/control OpenInEditor.
  function handleFileClick(file: string, line?: number) {
    api.control('OpenInEditor', { file, line: line ?? null }).catch(() => {});
  }

  // Reveal a file in Finder via /api/control RevealInFinder.
  function handleFileReveal(file: string) {
    api.control('RevealInFinder', { path: file }).catch(() => {});
  }

  // Derived: build artifacts
  let buildArtifacts = $derived(
    build ? $artifacts.filter(a => a.buildId === build.id) : []
  );

  // Synthesize LASDLC phases from build pillars for PhaseTimeline
  let synthesizedPhases = $derived(
    build ? build.pillars.map((p, i) => ({
      id: i + 1,
      title: p.pillar,
      status: p.status === 'passed' ? 'complete' as const : p.status === 'in_progress' ? 'active' as const : p.status === 'failed' ? 'failed' as const : 'pending' as const
    })) : []
  );
</script>

<div class="h-full flex flex-col relative overflow-hidden">
  <!-- Ambient polytope decoration -->
  <div class="absolute inset-0 overflow-hidden pointer-events-none -z-10">
    <div class="absolute -top-20 -right-20">
      <PolytopeDecor type="hexadecachoron" color="#00BFFF" size={350} opacity={0.03} speed={0.06} />
    </div>
    <div class="absolute -bottom-20 -left-20">
      <PolytopeDecor type="pentachoron" color="#B44AFF" size={300} opacity={0.04} speed={0.08} />
    </div>
  </div>

  <!-- Header with breadcrumb -->
  <header class="flex items-center gap-3 px-6 py-3 border-b border-[var(--la-hair-strong)]">
    {#if build}
      <HierarchyNav crumbs={[
        { id: '', name: 'Builds', type: 'workspaces' },
        { id: build.id, name: build.name, type: 'build', metaSkill: build.metaSkill },
      ]} />
    {:else}
      <button onclick={goBack} class="text-[#64748b] hover:text-white text-xs">
        ← Builds
      </button>
    {/if}
  </header>

  {#if build}
    {@const polyType = getMetaSkillPolytope(build.metaSkill)}
    {@const polyColor = getMetaSkillColor(build.metaSkill)}

    <div class="flex-1 flex overflow-hidden">
      <!-- Left: Pillar rail + detail + panels -->
      <div class="flex-1 overflow-y-auto p-6 space-y-4">
        <!-- CORSO scout plan tracker (renders only when an active plan exists) -->
        <PlanView />

        <!-- Build header with polytope -->
        <div class="flex items-center gap-3">
          <PolytopeIcon type={polyType} color={polyColor} size={36} />
          <div>
            <div class="flex items-center gap-2">
              <h2 class="text-sm font-semibold">{build.name}</h2>
              <span
                class="text-[10px] px-2 py-0.5 rounded-full"
                style="background-color: {polyColor}20; color: {polyColor}"
              >
                {build.metaSkill}
              </span>
            </div>
            <p class="text-xs text-[#64748b]">
              Confidence: {Math.round(build.confidence * 100)}% · Status: {build.status}
            </p>
          </div>
        </div>

        <!-- LASDLC Phase Timeline (primary display) -->
        {#if synthesizedPhases.length > 0}
          <PhaseTimeline phases={synthesizedPhases} />
        {/if}

        <!-- LASDLC Quality Gate Dashboard (7 dimensions) -->
        <QualityGateDash />

        <!-- Pillar rail (clickable) -->
        <PillarRail
          pillars={build.pillars}
          compact={false}
          selected={$selectedPillar}
          onPillarClick={selectPillar}
        />

        <!-- Pillar selector pills -->
        <div class="flex gap-1">
          {#each build.pillars as gate}
            <button
              class="text-[10px] px-2 py-1 rounded border transition-colors
                {$selectedPillar === gate.pillar ? 'border-[#FFD700] bg-[#FFD700]/10 text-white' : 'border-[var(--la-hair-strong)] text-[#64748b] hover:border-[#334155]'}"
              style="{$selectedPillar === gate.pillar ? `color: ${PILLAR_COLORS[gate.pillar]}` : ''}"
              onclick={() => selectPillar(gate.pillar)}
            >
              {gate.pillar}
              {#if PILLAR_ACTIONS[build.metaSkill]?.[gate.pillar]}
                <span class="text-[8px] ml-0.5 opacity-60">{PILLAR_ACTIONS[build.metaSkill][gate.pillar]}</span>
              {/if}
            </button>
          {/each}
        </div>

        <!-- Pillar detail (shown when a pillar is selected) -->
        {#if selectedGate}
          <PillarDetail gate={selectedGate} metaSkill={build.metaSkill} />
        {:else}
          <div class="bg-[#111827] border border-[var(--la-hair-strong)] rounded-lg p-4">
            <p class="text-xs text-[#475569]">Select a pillar above to see phase details</p>
          </div>
        {/if}

        <!-- Findings panel -->
        <FindingsPanel
          findings={filteredFindings}
          expandedIds={$expandedFindings}
          onToggleExpand={toggleFindingExpand}
          onVerify={verifyFinding}
          onFileClick={handleFileClick}
          onReveal={handleFileReveal}
        />

        <!-- Modules -->
        {#if build.modules.length > 0}
          <div class="bg-[#111827] border border-[var(--la-hair-strong)] rounded-lg p-4">
            <h3 class="text-xs font-medium text-[var(--la-text-label)] mb-2">MODULES ({build.modules.length})</h3>
            <ul class="space-y-1">
              {#each build.modules as mod}
                <li class="text-xs text-[var(--la-text-label)] flex items-center gap-2">
                  <span class="text-[#475569] font-mono">{mod.language ?? 'file'}</span>
                  <span class="text-[#e2e8f0]">{mod.name}</span>
                  <span class="text-[#334155] text-[10px]">{mod.path}</span>
                </li>
              {/each}
            </ul>
          </div>
        {/if}

        <!-- Log stream -->
        <LogStream entries={buildLogs} />

        <!-- Build notes -->
        {#if build}
          <BuildNotes
            buildId={build.id}
            onSave={(content) => {
              api.updateNotes(build.id, content).catch(() => {
                // Backend unavailable — local save already happened via store
              });
            }}
          />
        {/if}
      </div>

      <!-- Right: Context panel (collapses on small screens) -->
      <div class="w-[320px] border-l border-[var(--la-hair-strong)] overflow-y-auto p-4 space-y-3 hidden lg:block">
        <div class="bg-[#111827] border border-[var(--la-hair-strong)] rounded-lg p-3">
          <h3 class="text-xs font-medium text-[var(--la-text-label)] mb-2">CONTEXT</h3>
          <p class="text-xs text-[#475569]">Build configuration, CLAUDE.md, active Squad dispatches</p>
        </div>

        <div class="bg-[#111827] border border-[var(--la-hair-strong)] rounded-lg p-3">
          <h3 class="text-xs font-medium text-[var(--la-text-label)] mb-2">SQUAD DISPATCH</h3>
          <SiblingDispatch onDispatch={dispatchSibling} selectedSibling={$focusedSibling} />
        </div>

        <!-- Artifacts -->
        <ArtifactPanel
          artifacts={buildArtifacts}
          onUpload={() => {
            // In production, opens file picker and calls api.uploadArtifact
          }}
        />

        <div class="bg-[#111827] border border-[var(--la-hair-strong)] rounded-lg p-3">
          <h3 class="text-xs font-medium text-[var(--la-text-label)] mb-2">TERMINAL</h3>
          <p class="text-xs text-[#475569]">xterm.js PTY connection</p>
        </div>
      </div>
    </div>
  {:else}
    <div class="flex-1 flex items-center justify-center text-[#475569]">
      <p>Select a build from the queue</p>
    </div>
  {/if}
</div>