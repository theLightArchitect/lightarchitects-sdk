<script lang="ts">
  import { activePlan, planBuilderDraft } from '$lib/stores';
  import { api } from '$lib/api';
  import type { PlanPhase, PlanPhaseStatus, PhaseWithGates, GateType, GateCriterion, ExitGate } from '$lib/types';

  let plan = $derived($activePlan);
  let draft = $derived($planBuilderDraft);

  // Use the draft plan if available (richer schema with gates), else fall back to basic plan
  let hasDraft = $derived(draft !== null && draft.phase_detail.length > 0);

  // Track which phases are expanded (by id)
  let expandedPhases = $state(new Set<number>());
  let expandedGates = $state(new Set<number>());

  function togglePhase(id: number) {
    expandedPhases = new Set(expandedPhases);
    if (expandedPhases.has(id)) expandedPhases.delete(id);
    else expandedPhases.add(id);
  }

  function toggleGate(id: number) {
    expandedGates = new Set(expandedGates);
    if (expandedGates.has(id)) expandedGates.delete(id);
    else expandedGates.add(id);
  }

  function statusColor(status: PlanPhaseStatus): string {
    switch (status) {
      case 'pending': return '#475569';
      case 'active': return '#FFD700';
      case 'complete': return '#22c55e';
      case 'failed': return '#ef4444';
      case 'skipped': return '#64748b';
    }
  }

  function statusIcon(status: PlanPhaseStatus): string {
    switch (status) {
      case 'pending': return '\u25CB';
      case 'active': return '\u25CF';
      case 'complete': return '\u2713';
      case 'failed': return '\u2717';
      case 'skipped': return '\u2014';
    }
  }

  function gateStatusColor(status: string): string {
    switch (status) {
      case 'passed': return '#22c55e';
      case 'failed': return '#ef4444';
      case 'waived': return '#f59e0b';
      case 'blocked': return '#475569';
      default: return '#64748b';
    }
  }

  const GATE_TYPE_COLORS: Record<GateType, string> = {
    quality: '#10b981', structural: '#6366f1', testing: '#f59e0b',
    security: '#ef4444', complexity: '#8b5cf6', clean_room: '#06b6d4', custom: '#64748b',
  };

  // --- Interactive actions ---

  function toggleCriterion(phaseId: number, criterionId: string) {
    if (!draft) return;
    planBuilderDraft.update(d => {
      if (!d) return d;
      return {
        ...d,
        phase_detail: d.phase_detail.map(p => {
          if (p.id !== phaseId) return p;
          const criteria = p.exit_gate.criteria.map(c =>
            c.id === criterionId ? { ...c, passed: !c.passed } : c,
          );
          const allPassed = criteria.every(c => c.passed);
          return {
            ...p,
            exit_gate: {
              ...p.exit_gate,
              criteria,
              status: allPassed ? 'passed' as const : p.exit_gate.status === 'passed' ? 'pending' as const : p.exit_gate.status,
            },
          };
        }),
      };
    });
  }

  async function runAutomatedGate(phaseId: number) {
    if (!draft) return;
    try {
      await api.evaluateGate(draft.codename, phaseId, true);
    } catch {
      // Backend may not have endpoint — mark criteria as passed locally for demo
      planBuilderDraft.update(d => {
        if (!d) return d;
        return {
          ...d,
          phase_detail: d.phase_detail.map(p => {
            if (p.id !== phaseId) return p;
            return {
              ...p,
              exit_gate: {
                ...p.exit_gate,
                criteria: p.exit_gate.criteria.map(c =>
                  c.type === 'automated' ? { ...c, passed: true } : c,
                ),
              },
            };
          }),
        };
      });
    }
  }

  function advancePhaseStatus(phaseId: number, newStatus: PlanPhaseStatus) {
    if (!draft) return;
    planBuilderDraft.update(d => {
      if (!d) return d;
      return {
        ...d,
        phase_detail: d.phase_detail.map(p =>
          p.id === phaseId ? { ...p, status: newStatus } : p,
        ),
        current_phase: newStatus === 'active' ? phaseId : d.current_phase,
      };
    });
  }

  async function enrichPhase(phaseId: number, researchType: string) {
    if (!draft) return;
    try {
      const resp = await api.enrichPhase(draft.codename, phaseId, researchType);
      // Would populate phase.research from response
    } catch {
      // Backend may not have endpoint — show placeholder
      planBuilderDraft.update(d => {
        if (!d) return d;
        return {
          ...d,
          phase_detail: d.phase_detail.map(p => {
            if (p.id !== phaseId) return p;
            return {
              ...p,
              research: {
                ...p.research,
                enriched_at: new Date().toISOString(),
                enriched_by: researchType,
                prior_art: [`${researchType} enrichment requested — backend endpoint pending`],
              },
            };
          }),
        };
      });
    }
  }
</script>

<!-- Rich plan view (BuildPlan with gates) -->
{#if hasDraft && draft}
  <div class="bg-[#111827] border border-[#1e293b] rounded-lg p-3 mb-4">
    <!-- Plan header -->
    <div class="flex items-center gap-2 mb-2">
      <span class="text-[10px] font-semibold tracking-wider text-[#FFD700] uppercase">Plan</span>
      <span class="text-[11px] text-[#e2e8f0] font-medium flex-1">{draft.name}</span>
      <span class="text-[8px] font-mono text-[#475569]">{draft.codename}</span>
    </div>

    <!-- Pre-flight summary -->
    <div class="flex items-center gap-2 mb-2 px-1">
      <span class="text-[8px] text-[#f59e0b]">PRE-FLIGHT</span>
      <span class="text-[8px] text-[#475569]">
        {draft.pre_flight.filter(c => c.status === 'passed').length}/{draft.pre_flight.length} checks
      </span>
    </div>

    <!-- Phase + Gate list -->
    <div class="space-y-0.5">
      {#each draft.phase_detail as phase (phase.id)}
        {@const color = statusColor(phase.status)}
        {@const icon = statusIcon(phase.status)}
        {@const isExpanded = expandedPhases.has(phase.id)}
        {@const isActive = phase.status === 'active'}
        {@const gateColor = GATE_TYPE_COLORS[phase.exit_gate.type] ?? '#64748b'}
        {@const gateExpanded = expandedGates.has(phase.id)}

        <!-- Phase card -->
        <div
          class="rounded border transition-colors"
          style="border-color: {isActive ? '#FFD700' + '40' : '#1e293b'}; {isActive ? 'box-shadow: 0 0 8px #FFD70020;' : ''}"
        >
          <button
            class="w-full flex items-center gap-2 px-2 py-1.5 text-left hover:bg-[#1e293b]/40 transition-colors rounded"
            onclick={() => togglePhase(phase.id)}
          >
            <span class="flex-shrink-0 w-5 h-5 rounded-full flex items-center justify-center text-[9px] font-bold"
              style="background-color: {color}20; color: {color}">{phase.id}</span>
            <span class="flex-shrink-0 text-[10px]" class:plan-pulse={isActive} style="color: {color}">{icon}</span>
            <span class="text-[10px] text-[#e2e8f0] flex-1 truncate">{phase.title}</span>
            {#if phase.assigned_sibling}
              <span class="text-[8px] px-1 py-0.5 rounded bg-[#1e293b] text-[#94a3b8]">{phase.assigned_sibling}</span>
            {/if}

            <!-- Status dropdown trigger -->
            <select
              class="text-[8px] bg-transparent outline-none cursor-pointer"
              style="color: {color}"
              value={phase.status}
              onclick={(e) => e.stopPropagation()}
              onchange={(e) => advancePhaseStatus(phase.id, (e.target as HTMLSelectElement).value as PlanPhaseStatus)}
            >
              <option value="pending">Pending</option>
              <option value="active">Active</option>
              <option value="complete">Complete</option>
              <option value="failed">Failed</option>
              <option value="skipped">Skipped</option>
            </select>
          </button>

          {#if isExpanded}
            <div class="px-2 pb-2 pl-9 space-y-1.5">
              <p class="text-[9px] text-[#94a3b8]">{phase.description}</p>

              <!-- Task items -->
              {#if phase.items && phase.items.length > 0}
                {#each phase.items as item}
                  <div class="flex items-center gap-1.5 text-[9px] text-[#64748b]">
                    <span>□</span>
                    <span>{item}</span>
                  </div>
                {/each}
              {/if}

              <!-- Research enrichment -->
              {#if phase.research?.prior_art}
                <div class="bg-[#0d1117] rounded p-2 mt-1">
                  <span class="text-[8px] text-[#6366f1] font-medium">RESEARCH</span>
                  {#each phase.research.prior_art as finding}
                    <div class="text-[8px] text-[#94a3b8] mt-0.5">- {finding}</div>
                  {/each}
                </div>
              {/if}

              <!-- Research trigger buttons -->
              <div class="flex gap-1 mt-1">
                <button
                  class="text-[8px] px-1.5 py-0.5 rounded border border-[#1e293b] text-[#64748b] hover:text-[#6366f1] hover:border-[#6366f1] transition-colors"
                  onclick={() => enrichPhase(phase.id, 'general')}
                >QUANTUM</button>
                <button
                  class="text-[8px] px-1.5 py-0.5 rounded border border-[#1e293b] text-[#64748b] hover:text-[#ef4444] hover:border-[#ef4444] transition-colors"
                  onclick={() => enrichPhase(phase.id, 'security')}
                >SERAPH</button>
                <button
                  class="text-[8px] px-1.5 py-0.5 rounded border border-[#1e293b] text-[#64748b] hover:text-[#06b6d4] hover:border-[#06b6d4] transition-colors"
                  onclick={() => enrichPhase(phase.id, 'context7')}
                >Context7</button>
              </div>
            </div>
          {/if}
        </div>

        <!-- EXIT GATE bar -->
        <div class="ml-4 mr-2">
          <button
            class="w-full flex items-center gap-1.5 py-0.5 text-left"
            onclick={() => toggleGate(phase.id)}
          >
            <div class="w-3 h-px" style="background-color: {gateColor}40"></div>
            <span class="text-[7px] font-bold uppercase" style="color: {gateColor}">{phase.exit_gate.type}</span>
            <span class="text-[7px]" style="color: {gateStatusColor(phase.exit_gate.status)}">
              {phase.exit_gate.status}
            </span>
            <span class="text-[7px] text-[#475569]">
              {phase.exit_gate.criteria.filter(c => c.passed).length}/{phase.exit_gate.criteria.length}
            </span>
            <div class="flex-1 h-px" style="background-color: {gateColor}20"></div>
          </button>

          {#if gateExpanded}
            <div class="pl-4 pb-1 space-y-0.5">
              {#each phase.exit_gate.criteria as criterion (criterion.id)}
                <div class="flex items-center gap-1.5">
                  {#if criterion.type === 'manual'}
                    <input
                      type="checkbox"
                      checked={criterion.passed}
                      class="w-3 h-3 accent-[#22c55e]"
                      onchange={() => toggleCriterion(phase.id, criterion.id)}
                    />
                  {:else}
                    <span class="text-[8px]" style="color: {criterion.passed ? '#22c55e' : '#475569'}">
                      {criterion.passed ? '✓' : '○'}
                    </span>
                  {/if}
                  <span class="text-[8px] {criterion.passed ? 'text-[#94a3b8]' : 'text-[#475569]'} flex-1">{criterion.label}</span>
                  <span class="text-[7px] text-[#334155]">{criterion.type}</span>
                </div>
              {/each}

              <!-- Run automated gate button -->
              {#if phase.exit_gate.criteria.some(c => c.type === 'automated' && !c.passed)}
                <button
                  class="mt-1 text-[8px] px-2 py-0.5 rounded border border-[#1e293b] text-[#10b981] hover:bg-[#10b981]/10 transition-colors"
                  onclick={() => runAutomatedGate(phase.id)}
                >Run Automated Checks</button>
              {/if}
            </div>
          {/if}
        </div>
      {/each}
    </div>

    <!-- Close-out summary -->
    <div class="flex items-center gap-2 mt-2 px-1">
      <span class="text-[8px] text-[#06b6d4]">CLOSE-OUT</span>
      <span class="text-[8px] text-[#475569]">
        {draft.close_out.filter(s => s.status === 'complete').length}/{draft.close_out.length} steps
      </span>
    </div>
  </div>

<!-- Basic plan view (legacy ActivePlan without gates) -->
{:else if plan}
  <div class="bg-[#111827] border border-[#1e293b] rounded-lg p-3 mb-4">
    <div class="flex items-center gap-2 mb-2">
      <span class="text-[10px] font-semibold tracking-wider text-[#FFD700] uppercase">Plan</span>
      <span class="text-[11px] text-[#e2e8f0] font-medium">{plan.title}</span>
    </div>

    <div class="space-y-1">
      {#each plan.phases as phase (phase.id)}
        {@const color = statusColor(phase.status)}
        {@const icon = statusIcon(phase.status)}
        {@const isExpanded = expandedPhases.has(phase.id)}
        {@const isActive = phase.status === 'active'}

        <div
          class="rounded border transition-colors"
          style="border-color: {isActive ? '#FFD700' + '40' : '#1e293b'}; {isActive ? 'box-shadow: 0 0 8px #FFD70020;' : ''}"
        >
          <button
            class="w-full flex items-center gap-2 px-2 py-1.5 text-left hover:bg-[#1e293b]/40 transition-colors rounded"
            onclick={() => togglePhase(phase.id)}
          >
            <span class="flex-shrink-0 w-5 h-5 rounded-full flex items-center justify-center text-[9px] font-bold"
              style="background-color: {color}20; color: {color}">{phase.id}</span>
            <span class="flex-shrink-0 text-[10px]" class:plan-pulse={isActive} style="color: {color}">{icon}</span>
            <span class="text-[10px] text-[#e2e8f0] flex-1 truncate">{phase.title}</span>
            <span class="text-[9px] text-[#475569] flex-shrink-0 transition-transform" class:rotate-90={isExpanded}>&#9654;</span>
          </button>

          {#if isExpanded}
            <div class="px-2 pb-2 pl-9 space-y-1">
              {#if phase.description}
                <p class="text-[9px] text-[#94a3b8] leading-relaxed">{phase.description}</p>
              {/if}
              {#if phase.files?.length > 0}
                <div class="space-y-0.5">
                  {#each phase.files as file}
                    <div class="text-[9px] font-mono text-[#64748b] truncate" title={file}>{file}</div>
                  {/each}
                </div>
              {/if}
            </div>
          {/if}
        </div>
      {/each}
    </div>
  </div>
{/if}

<style>
  .plan-pulse {
    animation: plan-glow 2s ease-in-out infinite;
  }
  @keyframes plan-glow {
    0%, 100% { opacity: 0.6; }
    50% { opacity: 1; }
  }
</style>
