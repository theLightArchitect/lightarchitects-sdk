// ============================================================================
// Skill Resolver — Multi-tier resolution with platform API + fallback
// ============================================================================
// Resolution order (per OD-5 + WGC persona-and-skill-overlays):
//   1. project/.skills/{name} (highest priority — project-specific)
//   2. user/.skills/{name} (user-tier override)
//   3. user.agents.{sibling}.skills/{name} (per-agent override)
//   4. platform API cache (platform-tier base)
//   5. plugin cache fallback (offline mode)
// ============================================================================

import { authHeaders } from './auth';

export interface SkillOverlay {
  name: string;
  description?: string;
  version: string;
  trigger_patterns?: string[];
  content?: string;
  source: 'project' | 'user' | 'user-agent' | 'platform' | 'plugin-cache';
  is_override: boolean;
}

interface PlatformSkill {
  name: string;
  description?: string;
  version: string;
  trigger_patterns?: string[];
  content_hash: string;
  published: boolean;
}

const PLATFORM_API_BASE = 'http://localhost:3800';

/**
 * Resolve a skill by name using multi-tier resolution.
 * Returns the highest-priority available version.
 */
export async function resolveSkill(
  name: string,
  sibling?: string,
): Promise<SkillOverlay | null> {
  // Tier 1: Check project/.skills/ (future — not yet implemented)
  // const projectSkill = await loadProjectSkill(name);
  // if (projectSkill) return projectSkill;

  // Tier 2: Check user/.skills/
  const userSkill = await loadUserSkill(name);
  if (userSkill) {
    return { ...userSkill, source: 'user' as const, is_override: true };
  }

  // Tier 3: Check user.agents.{sibling}.skills/
  if (sibling) {
    const agentSkill = await loadAgentSkill(sibling, name);
    if (agentSkill) {
      return { ...agentSkill, source: 'user-agent' as const, is_override: true };
    }
  }

  // Tier 4: Platform API
  try {
    const platformSkill = await fetchPlatformSkill(name);
    if (platformSkill) {
      return { ...platformSkill, source: 'platform' as const, is_override: false };
    }
  } catch (e) {
    console.warn('Platform API unavailable', {
      name,
      error: e instanceof Error ? e.message : String(e),
    });
  }

  // Tier 5: Plugin cache fallback
  const pluginSkill = await loadPluginCacheSkill(name);
  if (pluginSkill) {
    return { ...pluginSkill, source: 'plugin-cache' as const, is_override: false };
  }

  return null;
}

/**
 * List all available skills from platform API.
 */
export async function listSkills(limit = 50): Promise<PlatformSkill[]> {
  try {
    const url = `${PLATFORM_API_BASE}/v1/platform/skills?limit=${limit}`;
    const res = await fetch(url, { headers: authHeaders() });
    if (!res.ok) throw new Error(`HTTP ${res.status}`);
    const data = await res.json();
    return data.skills || [];
  } catch (e) {
    console.warn('Failed to list skills from platform API', {
      error: e instanceof Error ? e.message : String(e),
    });
    return [];
  }
}

// ── Tier loaders ──────────────────────────────────────────────────────────────

async function loadUserSkill(name: string): Promise<Omit<SkillOverlay, 'source' | 'is_override'> | null> {
  // TODO: Implement filesystem access via backend endpoint
  // For now, return null — user-tier overlays require backend support
  return null;
}

async function loadAgentSkill(
  sibling: string,
  name: string,
): Promise<Omit<SkillOverlay, 'source' | 'is_override'> | null> {
  // TODO: Implement per-agent skill overlay endpoint
  return null;
}

async function fetchPlatformSkill(
  name: string,
): Promise<Omit<SkillOverlay, 'source' | 'is_override'> | null> {
  try {
    const url = `${PLATFORM_API_BASE}/v1/platform/skills/${encodeURIComponent(name)}`;
    const res = await fetch(url, { headers: authHeaders() });
    if (res.status === 404) return null;
    if (!res.ok) throw new Error(`HTTP ${res.status}`);
    const data = await res.json();
    return {
      name: data.name,
      description: data.description,
      version: data.version,
      trigger_patterns: data.trigger_patterns,
      content: undefined, // Skills don't have content field in platform API
    };
  } catch (e) {
    console.warn('Failed to fetch skill from platform', {
      name,
      error: e instanceof Error ? e.message : String(e),
    });
    return null;
  }
}

async function loadPluginCacheSkill(
  name: string,
): Promise<Omit<SkillOverlay, 'source' | 'is_override'> | null> {
  // Fallback: check if skill exists in plugin cache filesystem
  // This would require a backend endpoint to read ~/.claude/plugins/cache/
  return null;
}
