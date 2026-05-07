// ============================================================================
// Persona Resolver — Multi-tier resolution with platform API + fallback
// ============================================================================
// Resolution order (per OD-5 + WGC persona-and-skill-overlays):
//   1. project/.personas/{sibling}/identity.md (highest priority)
//   2. user/.personas/{sibling}/identity.md (user-tier override)
//   3. platform API cache (platform-tier base)
//   4. plugin cache fallback (offline mode)
// ============================================================================

import { authHeaders } from './auth';

export interface PersonaOverlay {
  name: string;
  sibling: string;
  description?: string;
  version: string;
  identity_text?: string;
  source: 'project' | 'user' | 'platform' | 'plugin-cache';
  is_override: boolean;
}

interface PlatformPersona {
  name: string;
  sibling: string;
  description?: string;
  version: string;
  identity_text?: string;
  content_hash: string;
  published: boolean;
}

const PLATFORM_API_BASE = 'http://localhost:3800';

/**
 * Resolve a persona by sibling name using multi-tier resolution.
 * Returns the highest-priority available version.
 */
export async function resolvePersona(
  sibling: string,
): Promise<PersonaOverlay | null> {
  // Tier 1: Check project/.personas/{sibling}/identity.md (future)
  // const projectPersona = await loadProjectPersona(sibling);
  // if (projectPersona) return projectPersona;

  // Tier 2: Check user/.personas/{sibling}/identity.md
  const userPersona = await loadUserPersona(sibling);
  if (userPersona) {
    return { ...userPersona, source: 'user' as const, is_override: true };
  }

  // Tier 3: Platform API
  try {
    const platformPersona = await fetchPlatformPersona(sibling);
    if (platformPersona) {
      return { ...platformPersona, source: 'platform' as const, is_override: false };
    }
  } catch (e) {
    console.warn('Platform API unavailable, falling back to plugin cache', e);
  }

  // Tier 4: Plugin cache fallback
  const pluginPersona = await loadPluginCachePersona(sibling);
  if (pluginPersona) {
    return { ...pluginPersona, source: 'plugin-cache' as const, is_override: false };
  }

  return null;
}

/**
 * List all available personas from platform API.
 */
export async function listPersonas(limit = 50): Promise<PlatformPersona[]> {
  try {
    const url = `${PLATFORM_API_BASE}/v1/platform/personas?limit=${limit}`;
    const res = await fetch(url, { headers: authHeaders() });
    if (!res.ok) throw new Error(`HTTP ${res.status}`);
    const data = await res.json();
    return data.personas || [];
  } catch (e) {
    console.warn('Failed to list personas from platform API', e);
    return [];
  }
}

// ── Tier loaders ──────────────────────────────────────────────────────────────

async function loadUserPersona(
  sibling: string,
): Promise<Omit<PersonaOverlay, 'source' | 'is_override'> | null> {
  // TODO: Implement filesystem access via backend endpoint
  // For now, return null — user-tier overlays require backend support
  return null;
}

async function fetchPlatformPersona(
  sibling: string,
): Promise<Omit<PersonaOverlay, 'source' | 'is_override'> | null> {
  try {
    const url = `${PLATFORM_API_BASE}/v1/platform/personas/${encodeURIComponent(sibling)}`;
    const res = await fetch(url, { headers: authHeaders() });
    if (res.status === 404) return null;
    if (!res.ok) throw new Error(`HTTP ${res.status}`);
    const data = await res.json();
    return {
      name: data.name,
      sibling: data.sibling,
      description: data.description,
      version: data.version,
      identity_text: data.identity_text,
    };
  } catch {
    return null;
  }
}

async function loadPluginCachePersona(
  sibling: string,
): Promise<Omit<PersonaOverlay, 'source' | 'is_override'> | null> {
  // Fallback: check plugin cache for sibling identity
  // This would require a backend endpoint to read ~/.claude/plugins/cache/
  return null;
}
