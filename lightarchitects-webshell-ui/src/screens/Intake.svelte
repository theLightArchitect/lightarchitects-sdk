<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { intakeForm, META_SKILL_CARDS, builds, currentBuildId, planBuilderMode, planBuilderDraft, planDraftState, resetPlanDraft } from '$lib/stores';
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
  import PreflightPanel from '$lib/../components/PreflightPanel.svelte';
  import MockBadge from '$lib/../components/MockBadge.svelte';
  import { isEnabled } from '$lib/featureFlags';
  import type { PreflightReport } from '$lib/types';

  // Tutorial T1 — auto-fires on first visit; re-trigger via ?onboarding=t1.
  // Runs after a tick so the DOM is settled and Shepherd can attach to the
  // [data-onboarding="..."] target elements.
  onMount(() => {
    // Read query params embedded in the hash (e.g. /intake?return=/builds&prefill=manifest)
    const qs = window.location.hash.split('?')[1] ?? '';
    const params = new URLSearchParams(qs);
    const prefill = params.get('prefill');
    const desc = params.get('desc');
    if (prefill === 'manifest') {
      planBuilderMode.set(true);
      initPlanFromForm();
    } else if (prefill === 'task' && desc) {
      intakeForm.update(f => ({ ...f, description: desc }));
    }

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

  // Build execution mode — wired to POST /api/builds `mode` field.
  type BuildExecMode = 'interactive' | 'autonomous';
  let execMode = $state<BuildExecMode>('interactive');

  // Preflight report — loaded on mount + when mode changes to autonomous.
  let preflightReport = $state<PreflightReport | null>(null);
  let preflightLoading = $state(false);

  async function loadPreflight() {
    if (preflightLoading) return;
    preflightLoading = true;
    try {
      preflightReport = await api.fetchPreflight();
    } catch {
      // Non-fatal — panel shows empty state.
    } finally {
      preflightLoading = false;
    }
  }

  $effect(() => {
    if (execMode === 'autonomous') {
      loadPreflight();
    }
  });

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

  // Inline confirm state — replaces window.confirm() for plan-template regen gating.
  // Async so callers can await the user's decision without blocking the main thread.
  let discardConfirm: { trigger: 'tier' | 'meta-skill' } | null = $state(null);
  let _discardResolve: ((v: boolean) => void) | null = null;

  function confirmDiscardPhases(trigger: 'tier' | 'meta-skill'): Promise<boolean> {
    if (_discardResolve) { _discardResolve(false); _discardResolve = null; }
    discardConfirm = { trigger };
    return new Promise(resolve => { _discardResolve = resolve; });
  }

  function resolveDiscard(confirmed: boolean) {
    discardConfirm = null;
    _discardResolve?.(confirmed);
    _discardResolve = null;
  }

  // Validate ?return= against known in-app route prefixes before hash assignment.
  // Returns null for invalid/missing values so callers can provide a safe fallback.
  const SAFE_RETURN_PREFIXES = ['/builds', '/dispatch', '/ops', '/helix', '/intake', '/project'];
  function safeReturn(raw: string | null | undefined): string | null {
    if (!raw) return null;
    if (raw === '/') return '/';
    return SAFE_RETURN_PREFIXES.some(p => raw === p || raw.startsWith(`${p}/`) || raw.startsWith(`${p}?`))
      ? raw
      : null;
  }

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
  async function setMetaSkill(skill: MetaSkill) {
    if ($planBuilderMode && phasesModified && !(await confirmDiscardPhases('meta-skill'))) {
      return;
    }
    intakeForm.update(f => ({ ...f, metaSkill: skill }));
    if ($planBuilderMode) {
      planPhases = generateDefaultPlan(skill, planTier);
      phasesModified = false;
    }
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
    // Capture before form reset — needed for ?task= return param
    const submittedDesc = form.description;
    let newBuildId: string | null = null;
    try {
      const resp: BuildResponse = await api.createBuild({
        cwd: form.repoPath || '.',
        metaSkill: form.metaSkill,
        source: form.source,
        priority: form.priority,
        repoPath: form.repoPath,
        description: form.description,
        mode: execMode,
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
    // Navigate: use ?return= if present; fall back to the new build's detail page.
    const qs = window.location.hash.split('?')[1] ?? '';
    const params = new URLSearchParams(qs);
    const returnTo = safeReturn(params.get('return'));
    const prefill = params.get('prefill');
    if (newBuildId) {
      currentBuildId.set(newBuildId);
      // When returning to dispatch with a task prefill, carry the description back as ?task=
      if (prefill === 'task' && returnTo && submittedDesc) {
        window.location.hash = `${returnTo}?task=${encodeURIComponent(submittedDesc)}`;
      } else {
        window.location.hash = returnTo || `/builds/${newBuildId}/kanban`;
      }
    } else {
      window.location.hash = returnTo || '/';
    }
  }

  function formatPillarFlow(actions: Record<string, string>): string {
    return Object.values(actions).join(' · ');
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
  async function setPlanTier(tier: BuildTier) {
    if (tier === planTier) return;
    if (phasesModified && !(await confirmDiscardPhases('tier'))) {
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

    const qs = window.location.hash.split('?')[1] ?? '';
    const returnTo = safeReturn(new URLSearchParams(qs).get('return')) ?? '/';

    try {
      const resp = await api.createPlan(plan);
      window.location.hash = returnTo;
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
      window.location.hash = returnTo;
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

  // ── Draft with EVA ────────────────────────────────────────────────────────

  let evaAnchorNorthstar = $state('');
  let evaIncludeResearch = $state(false);
  let evaDrafting = $state(false);
  let evaEventSource = $state<EventSource | null>(null);
  let draft = $derived($planDraftState);

  function cancelEvaDraft() {
    evaEventSource?.close();
    evaEventSource = null;
    evaDrafting = false;
    resetPlanDraft();
  }

  async function draftWithEva() {
    if (!validateForm()) return;
    evaDrafting = true;
    resetPlanDraft();

    let envelope: import('$lib/types').PlanDraftResponseEnvelope;
    try {
      envelope = await api.draftPlan({
        description: form.description ?? '',
        northstar:   evaAnchorNorthstar.trim() || undefined,
        repository:  form.repoPath.trim()      || undefined,
        research:    evaIncludeResearch,
        tier:        planTier,
      });
    } catch (err) {
      planDraftState.update(s => ({
        ...s,
        error: err instanceof Error ? err.message : 'Draft request failed',
      }));
      evaDrafting = false;
      return;
    }

    planDraftState.update(s => ({
      ...s,
      sessionId: envelope.session_id,
      codename:  envelope.codename,
    }));

    const es = api.subscribePlanStream(
      envelope.session_id,
      (ev) => {
        planDraftState.update(s => {
          if (ev.type === 'text_chunk')      return { ...s, text: s.text + ev.text };
          if (ev.type === 'iteration_start') return { ...s, iteration: ev.iteration };
          if (ev.type === 'verdict_block')   return { ...s, verdict: ev.verdict };
          if (ev.type === 'done')            return { ...s, done: true, codename: ev.codename };
          if (ev.type === 'error')           return { ...s, error: ev.message };
          return s;
        });
      },
      () => {
        // SSE error — Phase 3 stub returns 501; show a friendly message.
        planDraftState.update(s => ({
          ...s,
          error: 'Stream unavailable (Phase 4 wires broadcast). EVA is drafting in the background.',
        }));
        evaDrafting = false;
      },
    );
    evaEventSource = es;
  }

  async function commitEvaDraft() {
    if (!draft.sessionId || !draft.codename || !draft.text) return;
    try {
      await api.commitPlan({
        session_id: draft.sessionId,
        codename:   draft.codename,
        body:       draft.text,
      });
      cancelEvaDraft();
      window.location.hash = '/builds';
    } catch (err) {
      planDraftState.update(s => ({
        ...s,
        error: err instanceof Error ? err.message : 'Commit failed',
      }));
    }
  }

  onDestroy(() => {
    evaEventSource?.close();
  });
</script>

<div class="h-full flex flex-col relative overflow-hidden">
  <!-- Ambient polytope decoration -->
  <div class="absolute inset-0 overflow-hidden pointer-events-none -z-10">
    <div class="absolute -top-20 -left-20">
      <PolytopeDecor type="icositetrachoron" color="#FFD700" size={350} opacity={0.04} speed={0.05} />
    </div>
    <div class="absolute -bottom-20 -right-20">
      <PolytopeDecor type="duoprism64" color="#FF0040" size={300} opacity={0.04} speed={0.07} />
    </div>
  </div>

  <!-- Header (#38 — fixed 56px band shared across all top-level screens) -->
  <header class="la-screen-header flex items-center gap-3 px-6 border-b border-[var(--la-hair-strong)]">
    <button onclick={() => { window.location.hash = '/'; }} class="text-[var(--la-text-dim)] hover:text-white text-xs">
      Queue
    </button>
    <span class="text-[var(--la-hair-strong)]">/</span>
    <h1 class="text-lg font-semibold">New Build</h1>
    <span class="text-xs text-[var(--la-text-dim)]">Intake</span>
    <!-- Plan Builder mode toggle -->
    <div class="ml-auto flex items-center gap-2" data-onboarding="intake-mode-toggle">
      <button
        class="px-3 py-1 text-[10px] rounded transition-colors
          {!isPlanMode ? 'bg-[var(--la-focus-ring)]/15 text-[var(--la-focus-ring)] border border-[var(--la-focus-ring)]/30' : 'text-[var(--la-text-dim)] border border-transparent hover:text-[var(--la-focus-ring)]'}"
        onclick={() => { if (isPlanMode) togglePlanMode(); }}
      >Quick Build</button>
      <button
        class="px-3 py-1 text-[10px] rounded transition-colors
          {isPlanMode ? 'bg-[var(--la-focus-ring)]/15 text-[var(--la-focus-ring)] border border-[var(--la-focus-ring)]/30' : 'text-[var(--la-text-dim)] border border-transparent hover:text-[var(--la-focus-ring)]'}"
        onclick={() => { if (!isPlanMode) togglePlanMode(); }}
      >Plan Builder</button>
      <!-- Execution mode toggle (ironclaw-spine Phase 6) -->
      <span class="w-px h-4 bg-[var(--la-hair-strong)]" aria-hidden="true"></span>
      <button
        class="px-3 py-1 text-[10px] rounded transition-colors
          {execMode === 'interactive' ? 'bg-[var(--la-focus-ring)]/15 text-[var(--la-focus-ring)] border border-[var(--la-focus-ring)]/30' : 'text-[var(--la-text-dim)] border border-transparent hover:text-[var(--la-focus-ring)]'}"
        onclick={() => { execMode = 'interactive'; }}
        data-testid="exec-mode-interactive"
        title="Interactive mode — operator-supervised, single-agent"
      >Interactive</button>
      {#if !isEnabled('parallelismEnabled')}
        <button
          class="relative px-3 py-1 text-[10px] rounded transition-colors opacity-60 cursor-not-allowed
            text-[var(--la-text-dim)] border border-transparent"
          disabled
          data-testid="exec-mode-autonomous"
          title="Autonomous mode — wave coordinator pending (webshell-event-bus-redesign)"
        >
          Autonomous
          <MockBadge label="MOCK" detail="wave coord offline" position="top-right" />
        </button>
      {:else}
        <button
          class="px-3 py-1 text-[10px] rounded transition-colors
            {execMode === 'autonomous' ? 'bg-[var(--la-focus-ring)]/15 text-[var(--la-focus-ring)] border border-[var(--la-focus-ring)]/30' : 'text-[var(--la-text-dim)] border border-transparent hover:text-[var(--la-focus-ring)]'}"
          onclick={() => { execMode = 'autonomous'; }}
          data-testid="exec-mode-autonomous"
          title="Autonomous mode — lightsquad conductor with wave-level parallelism"
        >Autonomous</button>
      {/if}
    </div>
  </header>

  <div class="flex-1 overflow-y-auto p-6">
    <div class="grid grid-cols-1 lg:grid-cols-3 gap-6 max-w-6xl">
      <!-- Left: Form fields -->
      <div class="lg:col-span-2 space-y-6">

        <!-- Preflight panel — shown only in autonomous mode (ironclaw-spine Phase 6) -->
        {#if execMode === 'autonomous'}
          <div data-testid="preflight-panel-container">
            <h2 class="text-xs font-medium text-[var(--la-text-label)] mb-3">PREFLIGHT</h2>
            <PreflightPanel
              report={preflightReport ?? { timestamp: '', overall: 'Blocked' as const, checks: [], elapsed_ms: 0 }}
              loading={preflightLoading}
              onRefresh={loadPreflight}
            />
          </div>
        {/if}

        <!-- Source selection -->
        <div data-onboarding="intake-source">
          <h2 class="text-xs font-medium text-[var(--la-text-label)] mb-3">SOURCE</h2>
          <div class="grid grid-cols-4 gap-2">
            {#each Object.entries(SOURCE_CONFIG) as [key, cfg]}
              <button
                class="p-3 rounded-lg border text-left transition-colors
                  {form.source === key ? 'border-[var(--la-focus-ring)] bg-[var(--la-focus-ring)]/5' : 'border-[var(--la-hair-strong)] hover:border-[var(--la-hair-strong)] bg-[var(--la-bg-elev-1)]'}"
                onclick={() => setSource(key as IntakeSource)}
              >
                <div class="flex items-center gap-2 mb-1">
                  <div class="w-5 h-5 rounded flex items-center justify-center text-[9px] font-bold bg-[var(--la-bg-elev-2)] text-[var(--la-text-label)]">
                    {cfg.icon}
                  </div>
                  <span class="text-[11px] font-medium text-[var(--la-text-bright)]">{cfg.label}</span>
                </div>
                <p class="text-[9px] text-[var(--la-text-dim)]">{cfg.desc}</p>
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
              class="flex-1 bg-[var(--la-bg-elev-1)] border border-[var(--la-hair-strong)] rounded px-3 py-2 text-sm text-[var(--la-text-bright)] placeholder-[var(--la-text-dim)] outline-none focus:border-[var(--la-focus-ring)]"
            />
            <button
              class="px-3 py-2 text-xs rounded border border-[var(--la-hair-strong)] text-[var(--la-text-dim)] hover:border-[var(--la-focus-ring)] hover:text-[var(--la-focus-ring)] transition-colors"
              onclick={prefetchRepo}
              disabled={prefetching || !form.repoPath.trim()}
            >
              {prefetching ? 'Loading...' : 'Prefetch'}
            </button>
          </div>

          <!-- Prefetched metadata preview -->
          {#if previewData}
            <div class="mt-2 bg-[var(--la-bg-elev-1)] border border-[var(--la-hair-strong)] rounded-lg p-3">
              <h3 class="text-[10px] font-medium text-[var(--la-text-dim)] mb-2">PREFETCHED METADATA</h3>
              <div class="grid grid-cols-2 gap-2">
                {#each Object.entries(previewData) as [key, value]}
                  <div class="flex items-center gap-2">
                    <span class="text-[9px] text-[var(--la-text-dim)] uppercase">{key}:</span>
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
            class="w-full bg-[var(--la-bg-elev-1)] border rounded px-3 py-2 text-sm text-[var(--la-text-bright)] placeholder-[var(--la-text-dim)] outline-none resize-y
              {fieldErrors.description ? 'border-[var(--la-danger-stroke)] focus:border-[var(--la-danger-stroke)]' : 'border-[var(--la-hair-strong)] focus:border-[var(--la-focus-ring)]'}"
            data-testid="intake-description"
          ></textarea>
          {#if fieldErrors.description}
            <p class="mt-1 text-[10px] text-[var(--la-danger-stroke)]" data-testid="intake-description-error">{fieldErrors.description}</p>
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
                  {isSelected ? 'border-[var(--la-focus-ring)] bg-[var(--la-focus-ring)]/5' : 'border-[var(--la-hair-strong)] hover:border-[var(--la-hair-strong)] bg-[var(--la-bg-elev-1)]'}"
                onclick={() => setMetaSkill(card.skill)}
              >
                <div class="flex items-center gap-2 mb-1.5">
                  <PolytopeIcon type={polyType} color={polyColor} size={20} />
                  <span class="text-[11px] font-semibold" style="color: {polyColor}">{card.label}</span>
                </div>
                <p class="text-[9px] text-[var(--la-text-dim)] line-clamp-2">{card.description}</p>
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
                  {isActive ? `border-current bg-current/10` : 'border-[var(--la-hair-strong)] text-[var(--la-text-dim)] hover:border-[var(--la-hair-strong)]'}"
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
          {#if discardConfirm}
            <div
              class="mb-3 rounded border border-[var(--la-warn)] bg-[var(--la-warn)]/10 px-3 py-2"
              role="alertdialog"
              aria-modal="true"
              aria-label="Confirm discard edits"
            >
              <p class="text-[11px] text-[var(--la-text-bright)] mb-2">
                Changing the {discardConfirm.trigger} will regenerate the phase template and discard your current edits.
              </p>
              <div class="flex gap-2">
                <button
                  class="px-2 py-1 text-[10px] font-mono rounded bg-[var(--la-warn)] text-black"
                  onclick={() => resolveDiscard(true)}
                >Continue</button>
                <button
                  class="px-2 py-1 text-[10px] font-mono rounded border border-[var(--la-hair-strong)] text-[var(--la-text-dim)]"
                  onclick={() => resolveDiscard(false)}
                >Cancel</button>
              </div>
            </div>
          {/if}
          <div>
            <div class="flex items-center justify-between mb-3">
              <h2 class="text-xs font-medium text-[var(--la-text-label)]">PHASES + GATES</h2>
              <div class="flex items-center gap-2">
                <input
                  type="text"
                  bind:value={planName}
                  placeholder="Build plan name"
                  class="bg-[var(--la-bg-elev-1)] border border-[var(--la-hair-strong)] rounded px-2 py-1 text-[10px] text-[var(--la-text-bright)] w-40 outline-none focus:border-[var(--la-focus-ring)]"
                />
                <span class="text-[9px] text-[var(--la-text-dim)] font-mono">{planCodename}</span>
              </div>
            </div>

            <!-- Tier selector -->
            <div class="flex items-center gap-2 mb-3">
              <span class="text-[9px] text-[var(--la-text-dim)]">TIER:</span>
              {#each (['SMALL', 'MEDIUM', 'LARGE'] as const) as tier}
                <button
                  class="px-2 py-0.5 text-[9px] rounded border transition-colors
                    {planTier === tier ? 'border-[var(--la-focus-ring)] bg-[var(--la-focus-ring)]/10 text-[var(--la-focus-ring)]' : 'border-[var(--la-hair-strong)] text-[var(--la-text-dim)] hover:border-[var(--la-hair-strong)]'}"
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
                    <span class="text-[9px] text-[var(--la-text-dim)] w-5">{phase.id}</span>
                    <span class="text-[11px] font-medium text-[var(--la-text-bright)] flex-1">{phase.title}</span>
                    {#if phase.assigned_sibling}
                      <span class="text-[8px] px-1.5 py-0.5 rounded bg-[var(--la-bg-elev-2)] text-[var(--la-text-label)]">{phase.assigned_sibling}</span>
                    {/if}
                    <span class="text-[9px] text-[var(--la-text-dim)]">{expandedPhase === phase.id ? '▾' : '▸'}</span>
                  </button>

                  {#if expandedPhase === phase.id}
                    <div class="px-3 pb-3 border-t border-[var(--la-hair-strong)] space-y-2">
                      <p class="text-[9px] text-[var(--la-text-dim)] mt-2">{phase.description}</p>

                      <!-- Task items -->
                      {#if phase.items && phase.items.length > 0}
                        <div class="space-y-1">
                          {#each phase.items as item, itemIdx}
                            <div class="flex items-center gap-2 group">
                              <span class="text-[9px] text-[var(--la-text-label)]">-</span>
                              <span class="text-[10px] text-[var(--la-text-label)] flex-1">{item}</span>
                              <button
                                class="text-[9px] text-[var(--la-text-dim)] opacity-0 group-hover:opacity-100"
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
                          class="flex-1 bg-[var(--la-drawer-bg)] border border-[var(--la-hair-strong)] rounded px-2 py-1 text-[9px] text-[var(--la-text-bright)] outline-none focus:border-[var(--la-focus-ring)]"
                          onkeydown={(e) => { if (e.key === 'Enter') addPhaseItem(phase.id); }}
                        />
                        <button
                          class="px-2 text-[9px] text-[var(--la-focus-ring)] hover:text-white"
                          onclick={() => addPhaseItem(phase.id)}
                        >+</button>
                      </div>

                      <!-- Deliverables -->
                      {#if phase.deliverables && phase.deliverables.length > 0}
                        <div class="mt-1">
                          <span class="text-[8px] text-[var(--la-text-dim)] uppercase">Deliverables:</span>
                          <div class="flex flex-wrap gap-1 mt-0.5">
                            {#each phase.deliverables as d}
                              <span class="text-[8px] px-1.5 py-0.5 rounded bg-[var(--la-bg-elev-2)] text-[var(--la-text-dim)]">{d}</span>
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
                      class="bg-transparent text-[8px] text-[var(--la-text-dim)] outline-none cursor-pointer"
                      aria-label="Gate type for {phase.title}"
                      value={phase.exit_gate.type}
                      onchange={(e) => changeGateType(phase.id, (e.target as HTMLSelectElement).value as GateType)}
                    >
                      {#each Object.entries(GATE_TYPE_LABELS) as [val, label]}
                        <option value={val}>{label}</option>
                      {/each}
                    </select>
                    <span class="text-[8px] text-[var(--la-text-dim)]">({phase.exit_gate.criteria.length} criteria)</span>
                  </div>
                  <div class="flex-1 h-px" style="background-color: {GATE_TYPE_COLORS[phase.exit_gate.type]}30"></div>
                </div>
              {/each}
            </div>

            <!-- Validation errors -->
            {#if validationErrors.length > 0}
              <div class="mt-3 bg-[var(--la-danger-stroke)]/10 border border-[var(--la-danger-stroke)]/30 rounded-lg p-3">
                <h3 class="text-[10px] font-medium text-[var(--la-danger-stroke)] mb-1">Validation Errors</h3>
                {#each validationErrors as err}
                  <div class="text-[9px] text-[var(--la-danger-stroke)]/80">- {err}</div>
                {/each}
              </div>
            {/if}

            <!-- ── Draft with EVA ────────────────────────────────────────── -->
            <div class="mt-4 border border-[var(--la-focus-ring)]/20 rounded-lg p-3 bg-[var(--la-bg-elev-1)]">
              <h2 class="text-xs font-medium text-[var(--la-focus-ring)] mb-3">DRAFT WITH EVA</h2>

              <!-- Northstar anchor -->
              <div class="mb-3">
                <label class="text-[10px] text-[var(--la-text-dim)] block mb-1">NORTHSTAR (optional)</label>
                <textarea
                  bind:value={evaAnchorNorthstar}
                  placeholder="Describe the ideal end-state this build achieves..."
                  rows="2"
                  class="w-full bg-[var(--la-drawer-bg)] border border-[var(--la-hair-strong)] rounded px-2 py-1.5 text-[11px] text-[var(--la-text-bright)] placeholder-[var(--la-text-dim)] outline-none focus:border-[var(--la-focus-ring)] resize-none"
                ></textarea>
              </div>

              <!-- Research toggle -->
              <label class="flex items-center gap-2 mb-3 cursor-pointer">
                <input
                  type="checkbox"
                  bind:checked={evaIncludeResearch}
                  class="w-3.5 h-3.5 accent-[var(--la-focus-ring)]"
                />
                <span class="text-[10px] text-[var(--la-text-label)]">Include research phase (QUANTUM + SOUL prior-art)</span>
              </label>

              <!-- Action buttons -->
              <div class="flex gap-2">
                <button
                  class="flex-1 px-3 py-2 text-[11px] font-medium rounded border transition-colors
                    {evaDrafting
                      ? 'border-[var(--la-focus-ring)]/40 text-[var(--la-focus-ring)]/60 cursor-not-allowed'
                      : 'border-[var(--la-focus-ring)] text-[var(--la-focus-ring)] hover:bg-[var(--la-focus-ring)]/10'}"
                  onclick={draftWithEva}
                  disabled={evaDrafting}
                  data-testid="intake-draft-with-eva"
                >
                  {evaDrafting ? 'EVA drafting...' : 'Draft with EVA'}
                </button>
                {#if draft.sessionId}
                  <button
                    class="px-3 py-2 text-[11px] rounded border border-[var(--la-hair-strong)] text-[var(--la-text-dim)] hover:border-[var(--la-text-label)] hover:text-[var(--la-text-label)] transition-colors"
                    onclick={cancelEvaDraft}
                  >Cancel</button>
                {/if}
              </div>

              <!-- Streaming output + iteration indicator -->
              {#if draft.sessionId}
                <div class="mt-3">
                  <div class="flex items-center gap-2 mb-1.5">
                    <span class="text-[9px] font-mono text-[var(--la-text-dim)]">ITERATION {draft.iteration}</span>
                    {#if draft.verdict}
                      <span class="text-[9px] px-1.5 py-0.5 rounded
                        {draft.verdict.validation_status === 'VALIDATED' ? 'bg-[#22c55e]/10 text-[#22c55e]' : 'bg-[#f59e0b]/10 text-[#f59e0b]'}">
                        {draft.verdict.validation_status}
                      </span>
                    {/if}
                    {#if draft.done}
                      <span class="text-[9px] text-[#22c55e]">✓ Done — {draft.codename}</span>
                    {/if}
                  </div>

                  {#if draft.error}
                    <div class="text-[9px] text-[var(--la-danger-stroke)] bg-[var(--la-danger-stroke)]/10 rounded px-2 py-1.5 mb-2">
                      {draft.error}
                    </div>
                  {/if}

                  {#if draft.text}
                    <div class="bg-[var(--la-drawer-bg)] border border-[var(--la-hair-strong)] rounded p-2 max-h-64 overflow-y-auto font-mono text-[9px] text-[var(--la-text-label)] whitespace-pre-wrap leading-relaxed" data-testid="eva-draft-stream">
                      {draft.text}
                    </div>
                  {:else if !draft.error}
                    <div class="text-[9px] text-[var(--la-text-dim)] italic">Waiting for EVA to begin streaming...</div>
                  {/if}

                  {#if draft.done}
                    <button
                      class="mt-2 w-full px-3 py-2 text-[11px] font-medium rounded bg-[#22c55e] text-black hover:bg-[#16a34a] transition-colors disabled:opacity-50"
                      onclick={commitEvaDraft}
                      disabled={draft.verdict?.validation_status !== 'VALIDATED'}
                      data-testid="eva-commit-plan"
                    >
                      Commit Plan to ~/.claude/plans/{draft.codename}.md
                    </button>
                    {#if draft.verdict?.validation_status !== 'VALIDATED'}
                      <p class="mt-1 text-[9px] text-[var(--la-text-dim)]">
                        Commit unlocks when EVA marks the plan VALIDATED.
                      </p>
                    {/if}
                  {/if}
                </div>
              {/if}
            </div>
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
                <div class="text-sm font-semibold text-[var(--la-text-bright)]">{selectedCard.label}</div>
                <div class="text-[10px]" style="color: {getMetaSkillColor(selectedCard.skill)}">
                  {selectedCard.skill}
                </div>
              </div>
            </div>
            <p class="text-[10px] text-[var(--la-text-label)] mb-3">{selectedCard.description}</p>

            <!-- Pillar flow -->
            <div class="bg-[var(--la-drawer-bg)] rounded p-2">
              <div class="text-[9px] text-[var(--la-text-dim)] mb-1">PILLAR FLOW</div>
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
                <div class="text-[9px] text-[var(--la-text-dim)]">Primary SQUAD member for {selectedCard.label}</div>
              </div>
            </div>
            <p class="text-[9px] text-[var(--la-text-dim)]">
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
              <span class="text-[var(--la-text-dim)]">Source</span>
              <span class="text-[var(--la-text-label)]">{form.source}</span>
            </div>
            <div class="flex items-center justify-between text-[10px]">
              <span class="text-[var(--la-text-dim)]">Repo</span>
              <span class="text-[var(--la-text-label)] font-mono">{form.repoPath || '—'}</span>
            </div>
            <div class="flex items-center justify-between text-[10px]">
              <span class="text-[var(--la-text-dim)]">Meta-Skill</span>
              <span class="text-[var(--la-text-label)]">{form.metaSkill}</span>
            </div>
            <div class="flex items-center justify-between text-[10px]">
              <span class="text-[var(--la-text-dim)]">Priority</span>
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
                <span class="w-2 h-2 rounded-full bg-[var(--la-agent-performance)]"></span>
                <span class="text-[var(--la-text-label)]">11 pre-flight checks (3 blocking)</span>
              </div>
              <div class="flex items-center gap-2 text-[9px]">
                <span class="w-2 h-2 rounded-full bg-[var(--la-agent-researcher)]"></span>
                <span class="text-[var(--la-text-label)]">{planPhases.length} work phases</span>
              </div>
              <div class="flex items-center gap-2 text-[9px]">
                <span class="w-2 h-2 rounded-full bg-[var(--la-agent-testing)]"></span>
                <span class="text-[var(--la-text-label)]">{planPhases.length} mandatory exit gates</span>
              </div>
              <div class="flex items-center gap-2 text-[9px]">
                <span class="w-2 h-2 rounded-full bg-[var(--la-agent-knowledge)]"></span>
                <span class="text-[var(--la-text-label)]">6 close-out steps</span>
              </div>
              {#if isPlanMode && planPhases.length > 0}
                <div class="mt-2">
                  <PhaseTimeline phases={planPhases.map(p => ({ id: p.id, title: p.title.split(' — ')[0], status: p.status }))} compact={true} />
                </div>
              {/if}
              <div class="text-[8px] text-[var(--la-text-dim)] mt-2">
                Codename: <span class="font-mono text-[var(--la-focus-ring)]">{planCodename}</span>
              </div>
            </div>
          </div>
        {/if}

        <!-- Dedupe warning — inline, shown before submit button when a duplicate is detected -->
        {#if dedupeWarning}
          <div class="bg-[var(--la-agent-performance)]/10 border border-[var(--la-agent-performance)]/40 rounded-lg p-3" data-testid="intake-dedupe-warning">
            <div class="flex items-start gap-2">
              <span class="text-[var(--la-agent-performance)] text-sm shrink-0">⚠</span>
              <div class="flex-1">
                <p class="text-[11px] text-[var(--la-agent-performance)] font-medium mb-1">Duplicate detected</p>
                <p class="text-[10px] text-[var(--la-text-label)]">
                  A <span class="font-mono text-[var(--la-text-bright)]">{form.metaSkill}</span> build
                  {form.repoPath ? `for <span class="font-mono text-[var(--la-text-bright)]">${form.repoPath}</span>` : ''}
                  is already <span class="text-[var(--la-agent-performance)]">{dedupeWarning.status}</span>
                  (ID: <span class="font-mono">{dedupeWarning.buildId}</span>).
                </p>
                <div class="flex gap-2 mt-2">
                  <button
                    class="px-3 py-1 text-[10px] rounded border border-[var(--la-agent-performance)]/40 text-[var(--la-agent-performance)] hover:bg-[var(--la-agent-performance)]/10 transition-colors"
                    onclick={() => { window.location.hash = '/'; }}
                  >View existing</button>
                  <button
                    class="px-3 py-1 text-[10px] rounded border border-[var(--la-text-dim)]/40 text-[var(--la-text-dim)] hover:border-[var(--la-text-label)] hover:text-[var(--la-text-label)] transition-colors"
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
          class="w-full px-6 py-3 bg-[var(--la-focus-ring)] text-white text-sm rounded-lg hover:bg-[var(--la-focus-ring)] transition-colors font-medium disabled:opacity-50"
          onclick={isPlanMode ? submitPlan : submit}
          disabled={submitting}
        >
          {submitting ? 'Creating...' : isPlanMode ? 'Create Plan' : 'Create Build'}
        </button>
      </div>
    </div>
  </div>
</div>