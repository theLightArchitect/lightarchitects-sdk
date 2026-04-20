<script lang="ts">
  import { getMetaSkillPolytope, getMetaSkillColor } from '$lib/design-tokens';
  import PolytopeIcon from './PolytopeIcon.svelte';

  interface Crumb {
    id: string;
    name: string;
    type: 'workspaces' | 'workspace' | 'build' | 'module';
    metaSkill?: string;
  }

  interface Props {
    crumbs: Crumb[];
  }

  let { crumbs }: Props = $props();

  function navigate(crumb: Crumb) {
    if (crumb.type === 'workspaces') {
      window.location.hash = '/';
    } else if (crumb.type === 'workspace') {
      window.location.hash = '/';
    } else if (crumb.type === 'build') {
      window.location.hash = `/workspace/${crumb.id}`;
    }
  }
</script>

<nav class="flex items-center gap-2 text-sm" aria-label="Breadcrumb">
  {#each crumbs as crumb, i}
    {#if i > 0}
      <span class="text-[#334155] select-none">/</span>
    {/if}

    {#if i === crumbs.length - 1}
      <!-- Active crumb (current location) -->
      <div class="flex items-center gap-2">
        {#if crumb.metaSkill}
          {@const polyType = getMetaSkillPolytope(crumb.metaSkill)}
          {@const polyColor = getMetaSkillColor(crumb.metaSkill)}
          <PolytopeIcon type={polyType} color={polyColor} size={20} />
        {/if}
        <span class="font-medium text-[#e2e8f0]">{crumb.name}</span>
      </div>
    {:else}
      <!-- Clickable crumb -->
      <!-- svelte-ignore a11y_click_events_have_key_events -->
      <span
        class="text-[#64748b] hover:text-white transition-colors cursor-pointer"
        onclick={() => navigate(crumb)}
        role="link"
        tabindex={0}
      >
        {crumb.name}
      </span>
    {/if}
  {/each}
</nav>