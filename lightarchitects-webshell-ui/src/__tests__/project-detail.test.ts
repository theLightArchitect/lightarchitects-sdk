/**
 * Tests for webshell-project-ingestion Phase 4 frontend additions:
 *  - ApiError class (api.ts)
 *  - projects store + projectsBySlug derived (stores.ts)
 *  - project_update SSE dispatch (sse.ts)
 *  - ProjectUpdateEvent type shape (types.ts)
 */
import { describe, it, expect, beforeEach, vi, afterEach } from 'vitest';
import { get } from 'svelte/store';
import { projects, projectsBySlug } from '$lib/stores';
import { _handleEvent } from '$lib/sse';
import { ApiError } from '$lib/api';
import type { EventType, Project, ProjectMeta, ProjectUpdateEvent } from '$lib/types';

function makeEvent(type: EventType, data: unknown) {
  return { type, data };
}

// ── ApiError ─────────────────────────────────────────────────────────────────

describe('ApiError', () => {
  it('carries status, code, and message', () => {
    const err = new ApiError(404, 'MANIFEST_MISSING', 'project not initialized');
    expect(err.status).toBe(404);
    expect(err.code).toBe('MANIFEST_MISSING');
    expect(err.message).toBe('project not initialized');
    expect(err.name).toBe('ApiError');
  });

  it('is an instance of Error', () => {
    const err = new ApiError(500, 'IO_ERROR', 'disk full');
    expect(err instanceof Error).toBe(true);
    expect(err instanceof ApiError).toBe(true);
  });

  it('distinguishes MANIFEST_MISSING from other 404s by code', () => {
    const manifestMissing = new ApiError(404, 'MANIFEST_MISSING', 'no toml');
    const rootMissing = new ApiError(404, 'PROJECT_ROOT_MISSING', 'no dir');
    expect(manifestMissing.code).toBe('MANIFEST_MISSING');
    expect(rootMissing.code).toBe('PROJECT_ROOT_MISSING');
    expect(manifestMissing.code).not.toBe(rootMissing.code);
  });
});

// ── projects store ────────────────────────────────────────────────────────────

const FIXTURE_PROJECT: Project = {
  id: '019501e0-0000-7000-8000-000000000001',
  slug: 'lightarchitects-sdk',
  name: 'Light Architects SDK',
  kind: 'git_repo',
  created_at: '2026-05-21T00:00:00Z',
  helix_link: '/home/kft/lightarchitects/soul/helix/corso/projects/lightarchitects-sdk',
};

const FIXTURE_META: ProjectMeta = {
  project: FIXTURE_PROJECT,
  git: { remote: 'git@github.com:TheLightArchitects/lightarchitects-sdk.git', branch: 'main' },
  agents: {},
};

describe('projects store', () => {
  beforeEach(() => {
    projects.set([]);
  });

  it('starts empty', () => {
    expect(get(projects)).toHaveLength(0);
  });

  it('projectsBySlug is empty when projects is empty', () => {
    expect(get(projectsBySlug).size).toBe(0);
  });

  it('projectsBySlug rebuilds when projects store updates', () => {
    projects.set([FIXTURE_META]);
    const map = get(projectsBySlug);
    expect(map.size).toBe(1);
    expect(map.get('lightarchitects-sdk')).toBe(FIXTURE_META);
  });

  it('projectsBySlug lookup returns undefined for unknown slugs', () => {
    projects.set([FIXTURE_META]);
    expect(get(projectsBySlug).get('unknown-project')).toBeUndefined();
  });
});

// ── SSE project_update dispatch ───────────────────────────────────────────────

describe('SSE project_update', () => {
  let dispatchedEvents: CustomEvent[] = [];
  const origDispatch = window.dispatchEvent.bind(window);

  beforeEach(() => {
    dispatchedEvents = [];
    vi.spyOn(window, 'dispatchEvent').mockImplementation((e: Event) => {
      if (e instanceof CustomEvent && e.type === 'la:project-update') {
        dispatchedEvents.push(e);
      }
      return true;
    });
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('dispatches la:project-update DOM event with slug and kind', () => {
    const payload: ProjectUpdateEvent = {
      project_id: '019501e0-0000-7000-8000-000000000001',
      slug: 'lightarchitects-sdk',
      kind: 'created',
    };
    _handleEvent(makeEvent('project_update', payload));
    expect(dispatchedEvents).toHaveLength(1);
    expect(dispatchedEvents[0].detail).toMatchObject({
      slug: 'lightarchitects-sdk',
      kind: 'created',
    });
  });

  it('does not dispatch when slug is missing', () => {
    _handleEvent(makeEvent('project_update', { project_id: 'x', kind: 'created' }));
    expect(dispatchedEvents).toHaveLength(0);
  });

  it('dispatches for kind "updated" as well', () => {
    _handleEvent(makeEvent('project_update', { project_id: 'x', slug: 'my-proj', kind: 'updated' }));
    expect(dispatchedEvents).toHaveLength(1);
    expect(dispatchedEvents[0].detail.kind).toBe('updated');
  });
});
