<script lang="ts">
  import { onMount } from 'svelte';
  import { intakeForm, META_SKILL_CARDS, builds, currentBuildId, planBuilderMode, planBuilderDraft } from '$lib/stores';
  import { getMetaSkillPolytope, getMetaSkillColor, SIBLING_COLORS } from '$lib/design-tokens';
  import { api } from '$lib/api';
  import type { MetaSkill, IntakeSource, Priority, SiblingId, PhaseWithGates, GateType, BuildPlan, BuildTier, BuildResponse } from '$lib/types';
  import { generateDefaultPlan, generatePreFlight, generateCloseOut, generateAgenticConfig, suggestDomainGates, DEFAULT_GATE_CRITERIA } from '$lib/plan-templates';
  import { generateCodename } from '$lib/codename';
  import { validateBuildPlan } from '$lib/build-plan-schema';
  import { runTutorial, consumeOnboardingParam } from '$lib/tutorial';
  import PolytopeIcon from '$lib/../components/PolytopeIcon.svelte';
  import PolytopeDecor from '$lib/../components/PolytopeDecor.svelte';
  import PhaseTimeline from '$lib/../components/PhaseTimeline.svelte';

  // Tutorial T1 — auto-fires on first visit; re-trigger via ?onboarding=t1.
  // Runs after a tick so the DOM is settled and Shepherd can attach to the
  // [data-onboarding="..."] target elements.
  onMount(() => {
    const forced = consumeOnboardingParam();
    setTimeout(() => {
      if (forced === 't1') runTutorial('t1', true);
      else runTutorial('t1');
    }, 250);
  });

  let form = $derived($intakeForm);
  let submitting = $state(false);
  let previewData = $state<Record<string, string> | null>(null);
  let prefetching = $state(false);
  let isPlanMode = $derived($planBuilderMode);

  // Per-field inline validation errors — only populated on submit attempt.
  let fieldErrors = $state<{ description?: string; repoPath?: string }>({});

  // Dedupe warning: set when a matching in-flight build is detected pre-submit.
  // null = no duplicate; non-null = show "already queued" warning + "Create anyway" option.
  let dedupeWarning = $state<{ buildId: string; status: string } | null>(null);
  let forceCreate = $state(false);

  // Validate form fields. Returns true if valid.
  function validateForm(): boolean {
    const errs: typeof fieldErrors = {};
    const desc = (form.description ?? '').trim();
    if (!desc) {
      errs.description = 'Description is required.';
    } else if (desc.length < 8) {
      errs.description = 'Description is too short — describe what you want built.';
    }
    fieldErrors = errs;
    return Object.keys(errs).length === 0;
  }

  // Check whether a queued/running build already exists for this repo + meta-skill combo.
  function checkDedupe(): boolean {
    if (forceCreate) return false;
    const repoKey = form.repoPath.trim() || '.';
    const existing = $builds.find(b =>
      b.metaSkill === form.metaSkill &&
      (b.path ?? '.') === repoKey &&
      (b.status === 'queued' || b.status === 'in_progress'),
    );
    if (existing) {
      dedupeWarning = { buildId: existing.id, status: existing.status };
      return true;
    }
    return false;
  }

  // Plan Builder state
  let planTier = $state<BuildTier>('MEDIUM');
  let planPhases = $state<PhaseWithGates[]>([]);
  let planCodename = $state('');
  let planName = $state('');
  let validationErrors = $state<string[]>([]);
  let expandedPhase = $state<number | null>(null);
  let newItemText = $state('');
  // True when the user has edited any phase since the last regenerate. We use
  // it to gate destructive ops (TIER change, meta-skill change) so plan edits
  // aren't silently discarded — see #58.
  let phasesModified = $state(false);

  // Source type config
  const SOURCE_CONFIG: Record<IntakeSource, { label: string; icon: string; desc: string }> = {
    manual:  { label: 'Manual',    icon: 'M', desc: 'Describe the build manually' },
    github:  { label: 'GitHub',    icon: 'GH', desc: 'Import from GitHub issue or PR' },
    audit:   { label: 'Cargo Audit', icon: 'CA', desc: 'Create from cargo audit findings' },
    discovery: { label: 'Discovery', icon: 'D', desc: 'Auto-discovered from project state' },
  };

  // Priority config
  const PRIORITY_CONFIG: Record<Priority, { color: string; label: string }> = {
    high:   { color: '#ef4444', label: 'High' },
    medium: { color: '#f59e0b', label: 'Medium' },
    low:    { color: '#6b7280', label: 'Low' },
  };

  // Derived: selected meta-skill card
  let selectedCard = $derived(
    META_SKILL_CARDS.find(c => c.skill === form.metaSkill) ?? META_SKILL_CARDS[0]
  );

  // Derived: assigned SQUAD member
  let assignedSibling = $derived(selectedCard.sibling);
  let assignedColor = $derived(SIBLING_COLORS[assignedSibling] ?? '#6b7280');

  // Source selection
  function setSource(src: IntakeSource) {
    intakeForm.update(f => ({ ...f, source: src }));
    previewData = null;
  }

  // Meta-skill selection. In plan mode, changing meta-skill regenerates the
  // phase template — confirm first if the user has unsaved edits.
  function setMetaSkill(skill: MetaSkill) {
    if ($planBuilderMode && phasesModified && !confirmDiscardPhases('meta-skill')) {
      return;
    }
    intakeForm.update(f => ({ ...f, metaSkill: skill }));
    if ($planBuilderMode) {
      planPhases = generateDefaultPlan(skill, planTier);
      phasesModified = false;
    }
  }

  // Show a confirm dialog when a destructive plan-template regen is about to
  // run with unsaved phase edits. Returns true on confirm. (window.confirm is
  // a placeholder — a proper modal is a v0.3.1 polish task.)
  function confirmDiscardPhases(trigger: 'tier' | 'meta-skill'): boolean {
    return window.confirm(
      `Changing the ${trigger} will regenerate the phase template and discard your current edits.\n\nContinue?`,
    );
  }

  // Priority selection
  function setPriority(p: Priority) {
    intakeForm.update(f => ({ ...f, priority: p }));
  }

  // Prefetch metadata (simulated)
  async function prefetchRepo() {
    if (!form.repoPath.trim()) return;
    prefetching = true;
    try {
      // In production: api.prefetchMetadata(form.repoPath)
      await new Promise(r => setTimeout(r, 800));
      previewData = {
        language: 'Rust + TypeScript',
        modules: '12 crates',
        lastCommit: '2h ago',
        openIssues: '7',
      };
    } catch {
      previewData = null;
    } finally {
      prefetching = false;
    }
  }

  // Submit build
  async function submit() {
    if (!validateForm()) return;
    if (checkDedupe()) return;
    submitting = true;
    let newBuildId: string | null = null;
    try {
      const resp: BuildResponse = await api.createBuild({
        cwd: form.repoPath || '.',
        metaSkill: form.metaSkill,
        source: form.source,
        priority: form.priority,
        repoPath: form.repoPath,
        description: form.description,
      });
      newBuildId = resp.build_id;
      // Seed the builds store so activeBuild resolves immediately in Workspace.
      const stub = {
        id: resp.build_id,
        workspaceId: 'ws-001',
        name: form.description || form.repoPath.split('/').pop() || 'New Build',
        metaSkill: form.metaSkill,
        path: form.repoPath || '.',
        status: 'queued' as const,
        pillars: [],
        currentPillar: 'ARCH' as const,
        confidence: 0,
        createdAt: new Date().toISOString(),
        updatedAt: new Date().toISOString(),
        modules: [],
        siblingDispatches: [],
      };
      builds.update(b => [stub, ...b.filter(x => x.id !== stub.id)]);
    } catch {
      // Backend unavailable — local mock so the user can still explore Workspace.
      const mockId = `build-${Date.now().toString(36)}`;
      newBuildId = mockId;
      const newBuild = {
        id: mockId,
        workspaceId: 'ws-001',
        name: form.description || form.repoPath.split('/').pop() || 'New Build',
        metaSkill: form.metaSkill,
        path: form.repoPath || '.',
        status: 'queued' as const,
        pillars: [],
        currentPillar: 'ARCH' as const,
        confidence: 0,
        createdAt: new Date().toISOString(),
        updatedAt: new Date().toISOString(),
        modules: [],
        siblingDispatches: [],
      };
      builds.update(b => [newBuild, ...b]);
    }
    // Reset form
    intakeForm.set({
      metaSkill: '/BUILD',
      source: 'manual',
      priority: 'medium',
      repoPath: '',
      description: '',
    });
    fieldErrors = {};
    dedupeWarning = null;
    forceCreate = false;
    submitting = false;
    // Navigate directly to Workspace for the new build.
    if (newBuildId) {
      currentBuildId.set(newBuildId);
      window.location.hash = '/workspace';
    } else {
      window.location.hash = '/';
    }
  }

  function formatPillarFlow(actions: Record<string, string>): string {
    return Object.values(actions).join(' → ');
  }

  // ── Plan Builder functions ──────────────────────────────────────────────

  function togglePlanMode() {
    planBuilderMode.update(v => !v);
    if ($planBuilderMode) {
      // Just entered plan mode — generate defaults
      initPlanFromForm();
      setTimeout(() => import('$lib/tutorial').then(m => m.runTutorial('t3')), 300);
    }
  }

  function initPlanFromForm() {
    planPhases = generateDefaultPlan(form.metaSkill, planTier);
    planCodename = generateCodename();
    planName = form.description || form.repoPath.split('/').pop() || 'New Build Plan';
    phasesModified = false;
  }

  // NOTE: an earlier $effect regenerated planPhases on every meta-skill change,
  // silently discarding user edits (#58). Regen now happens explicitly inside
  // setMetaSkill + the TIER click handler, both gated by confirmDiscardPhases.

  function togglePhaseExpand(id: number) {
    expandedPhase = expandedPhase === id ? null : id;
  }

  function addPhaseItem(phaseId: number) {
    if (!newItemText.trim()) return;
    planPhases = planPhases.map(p =>
      p.id === phaseId ? { ...p, items: [...(p.items ?? []), newItemText.trim()] } : p
    );
    newItemText = '';
    phasesModified = true;
  }

  function removePhaseItem(phaseId: number, idx: number) {
    planPhases = planPhases.map(p =>
      p.id === phaseId ? { ...p, items: (p.items ?? []).filter((_, i) => i !== idx) } : p
    );
    phasesModified = true;
  }

  function changeGateType(phaseId: number, gateType: GateType) {
    planPhases = planPhases.map(p => {
      if (p.id !== phaseId) return p;
      return {
        ...p,
        exit_gate: {
          ...p.exit_gate,
          type: gateType,
          criteria: (DEFAULT_GATE_CRITERIA[gateType] ?? []).map(c => ({ ...c })),
        },
      };
    });
    phasesModified = true;
  }

  function addCustomCriterion(phaseId: number, label: string, type: 'automated' | 'manual') {
    planPhases = planPhases.map(p => {
      if (p.id !== phaseId) return p;
      const id = `custom_${Date.now().toString(36)}`;
      return {
        ...p,
        exit_gate: {
          ...p.exit_gate,
          criteria: [...p.exit_gate.criteria, { id, label, type, passed: false }],
        },
      };
    });
    phasesModified = true;
  }

  function removeCriterion(phaseId: number, criterionId: string) {
    planPhases = planPhases.map(p => {
      if (p.id !== phaseId) return p;
      return {
        ...p,
        exit_gate: {
          ...p.exit_gate,
          criteria: p.exit_gate.criteria.filter(c => c.id !== criterionId),
        },
      };
    });
    phasesModified = true;
  }

  // TIER change handler — gated by confirmDiscardPhases when phasesModified.
  function setPlanTier(tier: BuildTier) {
    if (tier === planTier) return;
    if (phasesModified && !confirmDiscardPhases('tier')) {
      return;
    }
    planTier = tier;
    planPhases = generateDefaultPlan(form.metaSkill, tier);
    phasesModified = false;
  }

  async function submitPlan() {
    if (!validateForm()) return;
    if (checkDedupe()) return;
    submitting = true;
    validationErrors = [];

    const plan: BuildPlan = {
      name: planName,
      codename: planCodename,
      version: '0.3.0',
      description: form.description ?? '',
      meta_skill: form.metaSkill,
      priority: form.priority,
      source: form.source,
      tier: 3,                       // project maturity tier (active.yaml convention: 3=experimental)
      build_tier: planTier,          // LASDLC build complexity: 'SMALL' | 'MEDIUM' | 'LARGE'
      status: 'planned',
      path: form.repoPath || '.',
      language: 'rust+typescript',
      pre_flight: generatePreFlight(),
      phase_detail: planPhases,
      domain_gates: suggestDomainGates('rust+typescript', form.repoPath, form.description ?? ''),
      close_out: generateCloseOut(),
      agentic: generateAgenticConfig(),
      phases: planPhases.length,
      current_phase: 0,
      phase_status: 'PLANNED',
      siblings: [selectedCard.sibling],
    };

    const result = validateBuildPlan(plan);
    if (!result.valid) {
      validationErrors = result.errors.map(e => `${e.path}: ${e.message}`);
      submitting = false;
      return;
    }

    try {
      const resp = await api.createPlan(plan);
      // Navigate to queue on success
      window.location.hash = '/';
    } catch {
      // Backend may not have the endpoint yet — store locally
      const newBuild = {
        id: planCodename,
        workspaceId: 'ws-001',
        name: planName,
        metaSkill: form.metaSkill,
        status: 'queued' as const,
        pillars: [],
        currentPillar: 'ARCH' as const,
        confidence: 0,
        createdAt: new Date().toISOString(),
        updatedAt: new Date().toISOString(),
        modules: [],
        siblingDispatches: [],
      };
      builds.update(b => [...b, newBuild]);
      planBuilderDraft.set(plan);
      window.location.hash = '/';
    }

    submitting = false;
  }

  const GATE_TYPE_LABELS: Record<GateType, string> = {
    quality: 'Quality',
    structural: 'Structural',
    testing: 'Testing',
    security: 'Security',
    complexity: 'Complexity',
    clean_room: 'Clean Room',
    custom: 'Custom',
  };

  const GATE_TYPE_COLORS: Record<GateType, string> = {
    quality: '#10b981',
    structural: '#6366f1',
    testing: '#f59e0b',
    security: '#ef4444',
    complexity: '#8b5cf6',
    clean_room: '#06b6d4',
    custom: '#64748b',
  };
</script>

<div class="h-full flex flex-col relative overflow-hidden">
  <!-- Ambient polytope decoration -->
  <div class="absolute inset-0 overflow-hidden pointer-events-none -z-10">
    <div class="absolute -top-20 -left-20">
      <PolytopeDecor type="icositetrachoron" color="#FFD700" size={350} opacity={0.03} speed={0.05} />
    </div>
    <div class="absolute -bottom-20 -right-20">
      <PolytopeDecor type="duoprism64" color="#FF0040" size={300} opacity={0.03} speed={0.07} />
    </div>
  </div>

  <!-- Header (#38 — fixed 56px band shared across all top-level screens) -->
  <header class="la-screen-header flex items-center gap-3 px-6 border-b border-[var(--la-hair-strong)]">
    <button onclick={() => { window.location.hash = '/'; }} class="text-[#64748b] hover:text-white text-xs">
      ← Queue
    </button>
    <span class="text-[#334155]">/</span>
    <h1 class="text-lg font-semibold">New Build</h1>
    <span class="text-xs text-[#64748b]">Intake</span>
    <!-- Plan Builder mode toggle -->
    <div class="ml-auto flex items-center gap-2" data-onboarding="intake-mode-toggle">
      <button
        class="px-3 py-1 text-[10px] rounded transition-colors
          {!isPlanMode ? 'bg-[#FFD700]/15 text-[#FFD700] border border-[#FFD700]/30' : 'text-[#475569] border border-transparent hover:text-[#FFD700]'}"
        onclick={() => { if (isPlanMode) togglePlanMode(); }}
      >Quick</button>
      <button
        class="px-3 py-1 text-[10px] rounded transition-colors
          {isPlanMode ? 'bg-[#FFD700]/15 text-[#FFD700] border border-[#FFD700]/30' : 'text-[#475569] border border-transparent hover:text-[#FFD700]'}"
        onclick={() => { if (!isPlanMode) togglePlanMode(); }}
      >Plan</button>
    </div>
  </header>

  <div class="flex-1 overflow-y-auto p-6">
    <div class="grid grid-cols-1 lg:grid-cols-3 gap-6 max-w-6xl">
      <!-- Left: Form fields -->
      <div class="lg:col-span-2 space-y-6">

        <!-- Source selection -->
        <div data-onboarding="intake-source">
          <h2 class="text-xs font-medium text-[var(--la-text-label)] mb-3">SOURCE</h2>
          <div class="grid grid-cols-4 gap-2">
            {#each Object.entries(SOURCE_CONFIG) as [key, cfg]}
              <button
                class="p-3 rounded-lg border text-left transition-colors
                  {form.source === key ? 'border-[#FFD700] bg-[#FFD700]/5' : 'border-[var(--la-hair-strong)] hover:border-[#334155] bg-[var(--la-bg-elev-1)]'}"
                onclick={() => setSource(key as IntakeSource)}
              >
                <div class="flex items-center gap-2 mb-1">
                  <div class="w-5 h-5 rounded flex items-center justify-center text-[9px] font-bold bg-[var(--la-bg-elev-2)] text-[var(--la-text-label)]">
                    {cfg.icon}
                  </div>
                  <span class="text-[11px] font-medium text-[#e2e8f0]">{cfg.label}</span>
                </div>
                <p class="text-[9px] text-[#475569]">{cfg.desc}</p>
              </button>
            {/each}
          </div>
        </div>

        <!-- Repository path -->
        <div>
          <h2 class="text-xs font-medium text-[var(--la-text-label)] mb-3">REPOSITORY</h2>
          <div class="flex gap-2">
            <input
              type="text"
              value={form.repoPath}
              oninput={(e) => { intakeForm.update(f => ({ ...f, repoPath: (e.target as HTMLInputElement).value })); }}
              placeholder="org/repo or local path"
              class="flex-1 bg-[var(--la-bg-elev-1)] border border-[var(--la-hair-strong)] rounded px-3 py-2 text-sm text-[#e2e8f0] placeholder-[#475569] outline-none focus:border-[#FFD700]"
            />
            <button
              class="px-3 py-2 text-xs rounded border border-[var(--la-hair-strong)] text-[#64748b] hover:border-[#FFD700] hover:text-[#FFD700] transition-colors"
              onclick={prefetchRepo}
              disabled={prefetching || !form.repoPath.trim()}
            >
              {prefetching ? 'Loading...' : 'Prefetch'}
            </button>
          </div>

          <!-- Prefetched metadata preview -->
          {#if previewData}
            <div class="mt-2 bg-[var(--la-bg-elev-1)] border border-[var(--la-hair-strong)] rounded-lg p-3">
              <h3 class="text-[10px] font-medium text-[#64748b] mb-2">PREFETCHED METADATA</h3>
              <div class="grid grid-cols-2 gap-2">
                {#each Object.entries(previewData) as [key, value]}
                  <div class="flex items-center gap-2">
                    <span class="text-[9px] text-[#475569] uppercase">{key}:</span>
                    <span class="text-[10px] text-[var(--la-text-label)]">{value}</span>
                  </div>
                {/each}
              </div>
            </div>
          {/if}
        </div>

        <!-- Build description -->
        <div>
          <h2 class="text-xs font-medium text-[var(--la-text-label)] mb-3">DESCRIPTION</h2>
          <textarea
            value={form.description}
            oninput={(e) => {
              intakeForm.update(f => ({ ...f, description: (e.target as HTMLTextAreaElement).value }));
              if (fieldErrors.description) fieldErrors = { ...fieldErrors, description: undefined as string | undefined };
            }}
            placeholder="Describe what this build should accomplish..."
            rows="3"
            class="w-full bg-[var(--la-bg-elev-1)] border rounded px-3 py-2 text-sm text-[#e2e8f0] placeholder-[#475569] outline-none resize-y
              {fieldErrors.description ? 'border-[#ef4444] focus:border-[#ef4444]' : 'border-[var(--la-hair-strong)] focus:border-[#FFD700]'}"
            data-testid="intake-description"
          ></textarea>
          {#if fieldErrors.description}
            <p class="mt-1 text-[10px] text-[#ef4444]" data-testid="intake-description-error">{fieldErrors.description}</p>
          {/if}
        </div>

        <!-- Meta-skill selection -->
        <div data-onboarding="intake-meta-skill">
          <h2 class="text-xs font-medium text-[var(--la-text-label)] mb-3">META-SKILL</h2>
          <div class="grid grid-cols-3 gap-2">
            {#each META_SKILL_CARDS as card (card.skill)}
              {@const polyType = getMetaSkillPolytope(card.skill)}
              {@const polyColor = getMetaSkillColor(card.skill)}
              {@const isSelected = form.metaSkill === card.skill}

              <button
                class="p-3 rounded-lg border text-left transition-colors
                  {isSelected ? 'border-[#FFD700] bg-[#FFD700]/5' : 'border-[var(--la-hair-strong)] hover:border-[#334155] bg-[var(--la-bg-elev-1)]'}"
                onclick={() => setMetaSkill(card.skill)}
              >
                <div class="flex items-center gap-2 mb-1.5">
                  <PolytopeIcon type={polyType} color={polyColor} size={20} />
                  <span class="text-[11px] font-semibold" style="color: {polyColor}">{card.label}</span>
                </div>
                <p class="text-[9px] text-[#475569] line-clamp-2">{card.description}</p>
              </button>
            {/each}
          </div>
        </div>

        <!-- Priority -->
        <div>
          <h2 class="text-xs font-medium text-[var(--la-text-label)] mb-3">PRIORITY</h2>
          <div class="flex gap-2">
            {#each Object.entries(PRIORITY_CONFIG) as [key, cfg]}
              {@const isActive = form.priority === key}
              <button
                class="px-4 py-2 text-xs rounded border transition-colors
                  {isActive ? `border-current bg-current/10` : 'border-[var(--la-hair-strong)] text-[#64748b] hover:border-[#334155]'}"
                style={isActive ? `color: ${cfg.color}; border-color: ${cfg.color}; background-color: ${cfg.color}10` : ''}
                onclick={() => setPriority(key as Priority)}
              >
                {cfg.label}
              </button>
            {/each}
          </div>
        </div>
        <!-- Plan Builder: Phase + Gate Editor -->
        {#if isPlanMode}
          <div>
            <div class="flex items-center justify-between mb-3">
              <h2 class="text-xs font-medium text-[var(--la-text-label)]">PHASES + GATES</h2>
              <div class="flex items-center gap-2">
                <input
                  type="text"
                  bind:value={planName}
                  placeholder="Build plan name"
                  class="bg-[var(--la-bg-elev-1)] border border-[var(--la-hair-strong)] rounded px-2 py-1 text-[10px] text-[#e2e8f0] w-40 outline-none focus:border-[#FFD700]"
                />
                <span class="text-[9px] text-[#475569] font-mono">{planCodename}</span>
              </div>
            </div>

            <!-- Tier selector -->
            <div class="flex items-center gap-2 mb-3">
              <span class="text-[9px] text-[#475569]">TIER:</span>
              {#each (['SMALL', 'MEDIUM', 'LARGE'] as const) as tier}
                <button
                  class="px-2 py-0.5 text-[9px] rounded border transition-colors
                    {planTier === tier ? 'border-[#FFD700] bg-[#FFD700]/10 text-[#FFD700]' : 'border-[var(--la-hair-strong)] text-[#475569] hover:border-[#334155]'}"
                  onclick={() => setPlanTier(tier)}
                >
                  {tier} ({tier === 'SMALL' ? '4' : tier === 'MEDIUM' ? '6' : '7'})
                </button>
              {/each}
            </div>

            <div class="space-y-1">
              {#each planPhases as phase, idx (phase.id)}
                <!-- Phase card -->
                <div class="border border-[var(--la-hair-strong)] rounded-lg overflow-hidden bg-[var(--la-bg-elev-1)]">
                  <button
                    class="w-full flex items-center gap-2 px-3 py-2 text-left hover:bg-[var(--la-bg-elev-2)]/50 transition-colors"
                    onclick={() => togglePhaseExpand(phase.id)}
                  >
                    <span class="text-[9px] text-[#475569] w-5">{phase.id}</span>
                    <span class="text-[11px] font-medium text-[#e2e8f0] flex-1">{phase.title}</span>
                    {#if phase.assigned_sibling}
                      <span class="text-[8px] px-1.5 py-0.5 rounded bg-[var(--la-bg-elev-2)] text-[var(--la-text-label)]">{phase.assigned_sibling}</span>
                    {/if}
                    <span class="text-[9px] text-[#475569]">{expandedPhase === phase.id ? '▾' : '▸'}</span>
                  </button>

                  {#if expandedPhase === phase.id}
                    <div class="px-3 pb-3 border-t border-[var(--la-hair-strong)] space-y-2">
                      <p class="text-[9px] text-[#64748b] mt-2">{phase.description}</p>

                      <!-- Task items -->
                      {#if phase.items && phase.items.length > 0}
                        <div class="space-y-1">
                          {#each phase.items as item, itemIdx}
                            <div class="flex items-center gap-2 group">
                              <span class="text-[9px] text-[var(--la-text-label)]">-</span>
                              <span class="text-[10px] text-[var(--la-text-label)] flex-1">{item}</span>
                              <button
                                class="text-[9px] text-[#475569] opacity-0 group-hover:opacity-100"
                                onclick={() => removePhaseItem(phase.id, itemIdx)}
                              >x</button>
                            </div>
                          {/each}
                        </div>
                      {/if}

                      <!-- Add item -->
                      <div class="flex gap-1">
                        <input
                          type="text"
                          bind:value={newItemText}
                          placeholder="Add task item..."
                          class="flex-1 bg-[#0d1117] border border-[var(--la-hair-strong)] rounded px-2 py-1 text-[9px] text-[#e2e8f0] outline-none focus:border-[#FFD700]"
                          onkeydown={(e) => { if (e.key === 'Enter') addPhaseItem(phase.id); }}
                        />
                        <button
                          class="px-2 text-[9px] text-[#FFD700] hover:text-white"
                          onclick={() => addPhaseItem(phase.id)}
                        >+</button>
                      </div>

                      <!-- Deliverables -->
                      {#if phase.deliverables && phase.deliverables.length > 0}
                        <div class="mt-1">
                          <span class="text-[8px] text-[#475569] uppercase">Deliverables:</span>
                          <div class="flex flex-wrap gap-1 mt-0.5">
                            {#each phase.deliverables as d}
                              <span class="text-[8px] px-1.5 py-0.5 rounded bg-[var(--la-bg-elev-2)] text-[#64748b]">{d}</span>
                            {/each}
                          </div>
                        </div>
                      {/if}
                    </div>
                  {/if}
                </div>

                <!-- EXIT GATE bar between phases -->
                <div class="flex items-center gap-2 px-2 py-1">
                  <div class="flex-1 h-px" style="background-color: {GATE_TYPE_COLORS[phase.exit_gate.type]}30"></div>
                  <div class="flex items-center gap-1.5">
                    <span class="text-[8px] font-bold" style="color: {GATE_TYPE_COLORS[phase.exit_gate.type]}">
                      GATE: {GATE_TYPE_LABELS[phase.exit_gate.type]}
                    </span>
                    <select
                      class="bg-transparent text-[8px] text-[#475569] outline-none cursor-pointer"
                      aria-label="Gate type for {phase.title}"
                      value={phase.exit_gate.type}
                      onchange={(e) => changeGateType(phase.id, (e.target as HTMLSelectElement).value as GateType)}
                    >
                      {#each Object.entries(GATE_TYPE_LABELS) as [val, label]}
                        <option value={val}>{label}</option>
                      {/each}
                    </select>
                    <span class="text-[8px] text-[#475569]">({phase.exit_gate.criteria.length} criteria)</span>
                  </div>
                  <div class="flex-1 h-px" style="background-color: {GATE_TYPE_COLORS[phase.exit_gate.type]}30"></div>
                </div>
              {/each}
            </div>

            <!-- Validation errors -->
            {#if validationErrors.length > 0}
              <div class="mt-3 bg-[#ef4444]/10 border border-[#ef4444]/30 rounded-lg p-3">
                <h3 class="text-[10px] font-medium text-[#ef4444] mb-1">Validation Errors</h3>
                {#each validationErrors as err}
                  <div class="text-[9px] text-[#ef4444]/80">- {err}</div>
                {/each}
              </div>
            {/if}
          </div>
        {/if}

      </div>

      <!-- Right: Preview panel -->
      <div class="space-y-4">
        <!-- Selected meta-skill detail -->
        <div class="bg-[var(--la-bg-elev-1)] border border-[var(--la-hair-strong)] rounded-lg overflow-hidden">
          <div class="px-4 py-2 border-b border-[var(--la-hair-strong)]">
            <h3 class="text-xs font-medium text-[var(--la-text-label)]">SELECTED META-SKILL</h3>
          </div>
          <div class="p-4">
            <div class="flex items-center gap-3 mb-3">
              <PolytopeIcon
                type={getMetaSkillPolytope(selectedCard.skill)}
                color={getMetaSkillColor(selectedCard.skill)}
                size={40}
              />
              <div>
                <div class="text-sm font-semibold text-[#e2e8f0]">{selectedCard.label}</div>
                <div class="text-[10px]" style="color: {getMetaSkillColor(selectedCard.skill)}">
                  {selectedCard.skill}
                </div>
              </div>
            </div>
            <p class="text-[10px] text-[var(--la-text-label)] mb-3">{selectedCard.description}</p>

            <!-- Pillar flow -->
            <div class="bg-[#0d1117] rounded p-2">
              <div class="text-[9px] text-[#475569] mb-1">PILLAR FLOW</div>
              <div class="text-[9px] text-[var(--la-text-label)] font-mono">
                {formatPillarFlow(selectedCard.pillarActions)}
              </div>
            </div>
          </div>
        </div>

        <!-- SQUAD auto-assignment -->
        <div class="bg-[var(--la-bg-elev-1)] border border-[var(--la-hair-strong)] rounded-lg overflow-hidden">
          <div class="px-4 py-2 border-b border-[var(--la-hair-strong)]">
            <h3 class="text-xs font-medium text-[var(--la-text-label)]">SQUAD ASSIGNMENT</h3>
          </div>
          <div class="p-4">
            <div class="flex items-center gap-3 mb-3">
              <div
                class="w-8 h-8 rounded flex items-center justify-center text-[10px] font-bold"
                style="background-color: {assignedColor}20; color: {assignedColor}"
              >
                {assignedSibling.slice(0, 2).toUpperCase()}
              </div>
              <div>
                <div class="text-xs font-semibold" style="color: {assignedColor}">
                  {assignedSibling.toUpperCase()}
                </div>
                <div class="text-[9px] text-[#475569]">Primary SQUAD member for {selectedCard.label}</div>
              </div>
            </div>
            <p class="text-[9px] text-[#475569]">
              SQUAD members are auto-assigned based on meta-skill. Override with manual dispatch after build creation.
            </p>
          </div>
        </div>

        <!-- Build summary -->
        <div class="bg-[var(--la-bg-elev-1)] border border-[var(--la-hair-strong)] rounded-lg overflow-hidden">
          <div class="px-4 py-2 border-b border-[var(--la-hair-strong)]">
            <h3 class="text-xs font-medium text-[var(--la-text-label)]">SUMMARY</h3>
          </div>
          <div class="p-4 space-y-2">
            <div class="flex items-center justify-between text-[10px]">
              <span class="text-[#475569]">Source</span>
              <span class="text-[var(--la-text-label)]">{form.source}</span>
            </div>
            <div class="flex items-center justify-between text-[10px]">
              <span class="text-[#475569]">Repo</span>
              <span class="text-[var(--la-text-label)] font-mono">{form.repoPath || '—'}</span>
            </div>
            <div class="flex items-center justify-between text-[10px]">
              <span class="text-[#475569]">Meta-Skill</span>
              <span class="text-[var(--la-text-label)]">{form.metaSkill}</span>
            </div>
            <div class="flex items-center justify-between text-[10px]">
              <span class="text-[#475569]">Priority</span>
              <span style="color: {PRIORITY_CONFIG[form.priority].color}">
                {PRIORITY_CONFIG[form.priority].label}
              </span>
            </div>
          </div>
        </div>

        <!-- Plan preview (plan mode only) -->
        {#if isPlanMode}
          <div class="bg-[var(--la-bg-elev-1)] border border-[var(--la-hair-strong)] rounded-lg overflow-hidden">
            <div class="px-4 py-2 border-b border-[var(--la-hair-strong)]">
              <h3 class="text-xs font-medium text-[var(--la-text-label)]">PLAN LIFECYCLE</h3>
            </div>
            <div class="p-3 space-y-1.5">
              <div class="flex items-center gap-2 text-[9px]">
                <span class="w-2 h-2 rounded-full bg-[#f59e0b]"></span>
                <span class="text-[var(--la-text-label)]">11 pre-flight checks (3 blocking)</span>
              </div>
              <div class="flex items-center gap-2 text-[9px]">
                <span class="w-2 h-2 rounded-full bg-[#10b981]"></span>
                <span class="text-[var(--la-text-label)]">{planPhases.length} work phases</span>
              </div>
              <div class="flex items-center gap-2 text-[9px]">
                <span class="w-2 h-2 rounded-full bg-[#6366f1]"></span>
                <span class="text-[var(--la-text-label)]">{planPhases.length} mandatory exit gates</span>
              </div>
              <div class="flex items-center gap-2 text-[9px]">
                <span class="w-2 h-2 rounded-full bg-[#06b6d4]"></span>
                <span class="text-[var(--la-text-label)]">6 close-out steps</span>
              </div>
              {#if isPlanMode && planPhases.length > 0}
                <div class="mt-2">
                  <PhaseTimeline phases={planPhases.map(p => ({ id: p.id, title: p.title.split(' — ')[0], status: p.status }))} compact={true} />
                </div>
              {/if}
              <div class="text-[8px] text-[#475569] mt-2">
                Codename: <span class="font-mono text-[#FFD700]">{planCodename}</span>
              </div>
            </div>
          </div>
        {/if}

        <!-- Dedupe warning — inline, shown before submit button when a duplicate is detected -->
        {#if dedupeWarning}
          <div class="bg-[#f59e0b]/10 border border-[#f59e0b]/40 rounded-lg p-3" data-testid="intake-dedupe-warning">
            <div class="flex items-start gap-2">
              <span class="text-[#f59e0b] text-sm shrink-0">⚠</span>
              <div class="flex-1">
                <p class="text-[11px] text-[#f59e0b] font-medium mb-1">Duplicate detected</p>
                <p class="text-[10px] text-[var(--la-text-label)]">
                  A <span class="font-mono text-[#e2e8f0]">{form.metaSkill}</span> build
                  {form.repoPath ? `for <span class="font-mono text-[#e2e8f0]">${form.repoPath}</span>` : ''}
                  is already <span class="text-[#f59e0b]">{dedupeWarning.status}</span>
                  (ID: <span class="font-mono">{dedupeWarning.buildId}</span>).
                </p>
                <div class="flex gap-2 mt-2">
                  <button
                    class="px-3 py-1 text-[10px] rounded border border-[#f59e0b]/40 text-[#f59e0b] hover:bg-[#f59e0b]/10 transition-colors"
                    onclick={() => { window.location.hash = '/'; }}
                  >View existing</button>
                  <button
                    class="px-3 py-1 text-[10px] rounded border border-[#64748b]/40 text-[#64748b] hover:border-[#94a3b8] hover:text-[var(--la-text-label)] transition-colors"
                    data-testid="intake-force-create"
                    onclick={() => { forceCreate = true; dedupeWarning = null; }}
                  >Create anyway</button>
                </div>
              </div>
            </div>
          </div>
        {/if}

        <!-- Submit -->
        <button
          data-onboarding="intake-submit"
          data-testid="intake-submit"
          class="w-full px-6 py-3 bg-[#FFD700] text-white text-sm rounded-lg hover:bg-[#D4A017] transition-colors font-medium disabled:opacity-50"
          onclick={isPlanMode ? submitPlan : submit}
          disabled={submitting}
        >
          {submitting ? 'Creating...' : isPlanMode ? 'Create Plan' : 'Create Build'}
        </button>
      </div>
    </div>
  </div>
</div>