<script lang="ts">
  import { intakeForm, META_SKILL_CARDS, builds } from '$lib/stores';
  import { getMetaSkillPolytope, getMetaSkillColor, SIBLING_COLORS } from '$lib/design-tokens';
  import { api } from '$lib/api';
  import type { MetaSkill, IntakeSource, Priority, SiblingId } from '$lib/types';
  import PolytopeIcon from '$lib/../components/PolytopeIcon.svelte';
  import PolytopeDecor from '$lib/../components/PolytopeDecor.svelte';

  let form = $derived($intakeForm);
  let submitting = $state(false);
  let previewData = $state<Record<string, string> | null>(null);
  let prefetching = $state(false);

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

  // Meta-skill selection
  function setMetaSkill(skill: MetaSkill) {
    intakeForm.update(f => ({ ...f, metaSkill: skill }));
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
    submitting = true;
    try {
      await api.createBuild({
        cwd: form.repoPath || '.',
        metaSkill: form.metaSkill,
        source: form.source,
        priority: form.priority,
        repoPath: form.repoPath,
        description: form.description,
      });
    } catch {
      // Backend unavailable — local mock creation
      const newBuild = {
        id: `build-${Date.now().toString(36)}`,
        workspaceId: 'ws-001',
        name: form.description || form.repoPath.split('/').pop() || 'New Build',
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
    }
    // Reset form and navigate
    intakeForm.set({
      metaSkill: '/BUILD',
      source: 'manual',
      priority: 'medium',
      repoPath: '',
      description: '',
    });
    submitting = false;
    window.location.hash = '/';
  }

  function formatPillarFlow(actions: Record<string, string>): string {
    return Object.values(actions).join(' → ');
  }
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

  <!-- Header -->
  <header class="flex items-center gap-3 px-6 py-3 border-b border-[#1e293b]">
    <button onclick={() => { window.location.hash = '/'; }} class="text-[#64748b] hover:text-white text-xs">
      ← Queue
    </button>
    <span class="text-[#334155]">/</span>
    <h1 class="text-lg font-semibold">New Build</h1>
    <span class="text-xs text-[#64748b]">Intake</span>
  </header>

  <div class="flex-1 overflow-y-auto p-6">
    <div class="grid grid-cols-1 lg:grid-cols-3 gap-6 max-w-6xl">
      <!-- Left: Form fields -->
      <div class="lg:col-span-2 space-y-6">

        <!-- Source selection -->
        <div>
          <h2 class="text-xs font-medium text-[#64748b] mb-3">SOURCE</h2>
          <div class="grid grid-cols-4 gap-2">
            {#each Object.entries(SOURCE_CONFIG) as [key, cfg]}
              <button
                class="p-3 rounded-lg border text-left transition-colors
                  {form.source === key ? 'border-[#FFD700] bg-[#FFD700]/5' : 'border-[#1e293b] hover:border-[#334155] bg-[#111827]'}"
                onclick={() => setSource(key as IntakeSource)}
              >
                <div class="flex items-center gap-2 mb-1">
                  <div class="w-5 h-5 rounded flex items-center justify-center text-[9px] font-bold bg-[#1e293b] text-[#94a3b8]">
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
          <h2 class="text-xs font-medium text-[#64748b] mb-3">REPOSITORY</h2>
          <div class="flex gap-2">
            <input
              type="text"
              value={form.repoPath}
              oninput={(e) => { intakeForm.update(f => ({ ...f, repoPath: (e.target as HTMLInputElement).value })); }}
              placeholder="org/repo or local path"
              class="flex-1 bg-[#111827] border border-[#1e293b] rounded px-3 py-2 text-sm text-[#e2e8f0] placeholder-[#475569] outline-none focus:border-[#FFD700]"
            />
            <button
              class="px-3 py-2 text-xs rounded border border-[#1e293b] text-[#64748b] hover:border-[#FFD700] hover:text-[#FFD700] transition-colors"
              onclick={prefetchRepo}
              disabled={prefetching || !form.repoPath.trim()}
            >
              {prefetching ? 'Loading...' : 'Prefetch'}
            </button>
          </div>

          <!-- Prefetched metadata preview -->
          {#if previewData}
            <div class="mt-2 bg-[#111827] border border-[#1e293b] rounded-lg p-3">
              <h3 class="text-[10px] font-medium text-[#64748b] mb-2">PREFETCHED METADATA</h3>
              <div class="grid grid-cols-2 gap-2">
                {#each Object.entries(previewData) as [key, value]}
                  <div class="flex items-center gap-2">
                    <span class="text-[9px] text-[#475569] uppercase">{key}:</span>
                    <span class="text-[10px] text-[#94a3b8]">{value}</span>
                  </div>
                {/each}
              </div>
            </div>
          {/if}
        </div>

        <!-- Build description -->
        <div>
          <h2 class="text-xs font-medium text-[#64748b] mb-3">DESCRIPTION</h2>
          <textarea
            value={form.description}
            oninput={(e) => { intakeForm.update(f => ({ ...f, description: (e.target as HTMLTextAreaElement).value })); }}
            placeholder="Describe what this build should accomplish..."
            rows="3"
            class="w-full bg-[#111827] border border-[#1e293b] rounded px-3 py-2 text-sm text-[#e2e8f0] placeholder-[#475569] outline-none focus:border-[#FFD700] resize-y"
          ></textarea>
        </div>

        <!-- Meta-skill selection -->
        <div>
          <h2 class="text-xs font-medium text-[#64748b] mb-3">META-SKILL</h2>
          <div class="grid grid-cols-3 gap-2">
            {#each META_SKILL_CARDS as card (card.skill)}
              {@const polyType = getMetaSkillPolytope(card.skill)}
              {@const polyColor = getMetaSkillColor(card.skill)}
              {@const isSelected = form.metaSkill === card.skill}

              <button
                class="p-3 rounded-lg border text-left transition-colors
                  {isSelected ? 'border-[#FFD700] bg-[#FFD700]/5' : 'border-[#1e293b] hover:border-[#334155] bg-[#111827]'}"
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
          <h2 class="text-xs font-medium text-[#64748b] mb-3">PRIORITY</h2>
          <div class="flex gap-2">
            {#each Object.entries(PRIORITY_CONFIG) as [key, cfg]}
              {@const isActive = form.priority === key}
              <button
                class="px-4 py-2 text-xs rounded border transition-colors
                  {isActive ? `border-current bg-current/10` : 'border-[#1e293b] text-[#64748b] hover:border-[#334155]'}"
                style={isActive ? `color: ${cfg.color}; border-color: ${cfg.color}; background-color: ${cfg.color}10` : ''}
                onclick={() => setPriority(key as Priority)}
              >
                {cfg.label}
              </button>
            {/each}
          </div>
        </div>
      </div>

      <!-- Right: Preview panel -->
      <div class="space-y-4">
        <!-- Selected meta-skill detail -->
        <div class="bg-[#111827] border border-[#1e293b] rounded-lg overflow-hidden">
          <div class="px-4 py-2 border-b border-[#1e293b]">
            <h3 class="text-xs font-medium text-[#64748b]">SELECTED META-SKILL</h3>
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
            <p class="text-[10px] text-[#94a3b8] mb-3">{selectedCard.description}</p>

            <!-- Pillar flow -->
            <div class="bg-[#0d1117] rounded p-2">
              <div class="text-[9px] text-[#475569] mb-1">PILLAR FLOW</div>
              <div class="text-[9px] text-[#94a3b8] font-mono">
                {formatPillarFlow(selectedCard.pillarActions)}
              </div>
            </div>
          </div>
        </div>

        <!-- SQUAD auto-assignment -->
        <div class="bg-[#111827] border border-[#1e293b] rounded-lg overflow-hidden">
          <div class="px-4 py-2 border-b border-[#1e293b]">
            <h3 class="text-xs font-medium text-[#64748b]">SQUAD ASSIGNMENT</h3>
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
        <div class="bg-[#111827] border border-[#1e293b] rounded-lg overflow-hidden">
          <div class="px-4 py-2 border-b border-[#1e293b]">
            <h3 class="text-xs font-medium text-[#64748b]">SUMMARY</h3>
          </div>
          <div class="p-4 space-y-2">
            <div class="flex items-center justify-between text-[10px]">
              <span class="text-[#475569]">Source</span>
              <span class="text-[#94a3b8]">{form.source}</span>
            </div>
            <div class="flex items-center justify-between text-[10px]">
              <span class="text-[#475569]">Repo</span>
              <span class="text-[#94a3b8] font-mono">{form.repoPath || '—'}</span>
            </div>
            <div class="flex items-center justify-between text-[10px]">
              <span class="text-[#475569]">Meta-Skill</span>
              <span class="text-[#94a3b8]">{form.metaSkill}</span>
            </div>
            <div class="flex items-center justify-between text-[10px]">
              <span class="text-[#475569]">Priority</span>
              <span style="color: {PRIORITY_CONFIG[form.priority].color}">
                {PRIORITY_CONFIG[form.priority].label}
              </span>
            </div>
          </div>
        </div>

        <!-- Submit -->
        <button
          class="w-full px-6 py-3 bg-[#FFD700] text-white text-sm rounded-lg hover:bg-[#D4A017] transition-colors font-medium disabled:opacity-50"
          onclick={submit}
          disabled={submitting}
        >
          {submitting ? 'Creating...' : 'Create Build'}
        </button>
      </div>
    </div>
  </div>
</div>