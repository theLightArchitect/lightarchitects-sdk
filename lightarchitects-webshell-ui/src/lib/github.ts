/**
 * GitHub REST API client for GitForest data.
 *
 * Repos are private (TheLightArchitects org) — a PAT with `repo` scope is required.
 * Token resolution order: argument > localStorage('la_gh_token') > unauthenticated (60 req/hr).
 */

export const GITHUB_ORG = 'TheLightArchitects';

/** The repos visualised in the GitForest by default. */
export const FOREST_REPO_NAMES = [
  'lightarchitects-sdk',
  'SOUL-DEV',
  'CORSO-DEV',
] as const;

// ── Shared types (mirrors GitForest.svelte internal types) ──────────────────

export type GateState = 'clean' | 'hitl_pending' | 'merge_ready' | 'failed' | 'writing' | 'ghost';
export type AgentDomain =
  | 'engineer' | 'quality' | 'security' | 'ops'
  | 'researcher' | 'knowledge' | 'testing' | 'squad';

export interface Worktree {
  domain: AgentDomain;
  commitCount: number;
}

export interface Branch {
  name: string;
  divergeCommit: number;
  commitCount: number;
  filesModified: number;
  gateState: GateState;
  isGhost: boolean;
  worktrees: Worktree[];
}

export interface RepoData {
  id: string;
  name: string;
  commitCount: number;
  fileCount: number;
  branches: Branch[];
}

// ── GitHub REST API response shapes ────────────────────────────────────────

interface GHRepo {
  full_name: string;
  default_branch: string;
  size: number;
}

interface GHBranch {
  name: string;
  commit: { sha: string };
}

interface GHCompare {
  ahead_by: number;
  merge_base_commit: { sha: string };
  commits: { sha: string }[];
  files?: { filename: string; changes: number }[];
}

interface GHPull {
  state: string;
  draft: boolean;
  requested_reviewers: unknown[];
}

interface GHPullList {
  number: number;
  title: string;
  state: string;
  draft: boolean;
  user: { login: string };
  head: { ref: string; sha: string };
  base: { ref: string };
  created_at: string;
  updated_at: string;
  changed_files: number;
  labels: { name: string; color: string }[];
  requested_reviewers: { login: string }[];
}

/** A pull request summary suitable for the Cockpit PR card. */
export interface PullRequest {
  number:             number;
  title:              string;
  repo:               string;
  author:             string;
  headBranch:         string;
  baseBranch:         string;
  createdAt:          string;
  updatedAt:          string;
  changedFiles:       number;
  labels:             string[];
  reviewersRequested: number;
  draft:              boolean;
}

// ── HTTP helpers ────────────────────────────────────────────────────────────

function resolveToken(overrideToken?: string): string | undefined {
  if (overrideToken) return overrideToken;
  try {
    return localStorage.getItem('la_gh_token') ?? undefined;
  } catch {
    return undefined;
  }
}

function makeHeaders(token?: string): HeadersInit {
  const h: Record<string, string> = {
    Accept: 'application/vnd.github+json',
    'X-GitHub-Api-Version': '2022-11-28',
  };
  if (token) h.Authorization = `Bearer ${token}`;
  return h;
}

async function ghFetch<T>(path: string, token?: string): Promise<{ data: T; headers: Headers }> {
  const res = await fetch(`https://api.github.com${path}`, { headers: makeHeaders(token) });
  if (!res.ok) throw new Error(`GitHub API ${res.status}: ${path}`);
  const data = await res.json() as T;
  return { data, headers: res.headers };
}

// ── Commit count via Link header trick ─────────────────────────────────────
// GET /repos/{owner}/{repo}/commits?per_page=1 → parse page=N from Link: last

async function fetchCommitCount(owner: string, repo: string, token?: string): Promise<number> {
  const path = `/repos/${owner}/${repo}/commits?per_page=1&sha=HEAD`;
  const { data, headers } = await ghFetch<unknown[]>(path, token);

  const link = headers.get('link') ?? '';
  const match = link.match(/[?&]page=(\d+)>;\s*rel="last"/);
  if (match) return parseInt(match[1], 10);

  // Fallback: if no last page, count is exactly the items returned (≤1)
  return data.length;
}

// ── File count via recursive tree ──────────────────────────────────────────

async function fetchFileCount(owner: string, repo: string, token?: string): Promise<number> {
  try {
    const { data } = await ghFetch<{ tree: { type: string }[]; truncated: boolean }>(
      `/repos/${owner}/${repo}/git/trees/HEAD?recursive=1`,
      token,
    );
    return data.tree.filter(n => n.type === 'blob').length;
  } catch {
    return 0;
  }
}

// ── Branch list ─────────────────────────────────────────────────────────────

async function fetchBranches(owner: string, repo: string, token?: string): Promise<GHBranch[]> {
  const { data } = await ghFetch<GHBranch[]>(
    `/repos/${owner}/${repo}/branches?per_page=100`,
    token,
  );
  return data;
}

// ── Branch diff vs default branch ──────────────────────────────────────────

async function fetchCompare(
  owner: string,
  repo: string,
  base: string,
  head: string,
  token?: string,
): Promise<GHCompare | null> {
  try {
    const { data } = await ghFetch<GHCompare>(
      `/repos/${owner}/${repo}/compare/${base}...${head}`,
      token,
    );
    return data;
  } catch {
    return null;
  }
}

// ── Open PR for branch → gate state ────────────────────────────────────────

async function fetchGateState(
  owner: string,
  repo: string,
  branch: string,
  token?: string,
): Promise<GateState> {
  try {
    const { data } = await ghFetch<GHPull[]>(
      `/repos/${owner}/${repo}/pulls?state=open&head=${owner}:${branch}&per_page=1`,
      token,
    );
    if (data.length === 0) return 'clean';
    const pr = data[0];
    if (pr.draft) return 'writing';
    if (pr.requested_reviewers.length > 0) return 'hitl_pending';
    return 'merge_ready';
  } catch {
    return 'clean';
  }
}

// ── Map branch name to worktree domain (heuristic) ─────────────────────────

function inferDomain(branchName: string): AgentDomain | null {
  const n = branchName.toLowerCase();
  if (n.includes('security') || n.includes('sec') || n.includes('seraph')) return 'security';
  if (n.includes('fix') || n.includes('quality') || n.includes('qual')) return 'quality';
  if (n.includes('feat') || n.includes('feature') || n.includes('engineer')) return 'engineer';
  if (n.includes('test') || n.includes('spec')) return 'testing';
  if (n.includes('doc') || n.includes('knowledge') || n.includes('readme')) return 'knowledge';
  if (n.includes('ops') || n.includes('deploy') || n.includes('ci')) return 'ops';
  if (n.includes('research') || n.includes('quantum')) return 'researcher';
  return null;
}

// ── Per-repo full fetch ─────────────────────────────────────────────────────

async function fetchRepo(
  org: string,
  repoName: string,
  token?: string,
): Promise<RepoData> {
  const [{ data: meta }, commitCount, fileCount, branches] = await Promise.all([
    ghFetch<GHRepo>(`/repos/${org}/${repoName}`, token),
    fetchCommitCount(org, repoName, token),
    fetchFileCount(org, repoName, token),
    fetchBranches(org, repoName, token),
  ]);

  const defaultBranch = meta.default_branch;
  const nonDefaultBranches = branches.filter(
    b => b.name !== defaultBranch && !b.name.startsWith('dependabot/'),
  ).slice(0, 8); // cap branches for performance

  const branchData = await Promise.all(
    nonDefaultBranches.map(async (b): Promise<Branch> => {
      const [compare, gateState] = await Promise.all([
        fetchCompare(org, repoName, defaultBranch, b.name, token),
        fetchGateState(org, repoName, b.name, token),
      ]);

      const ahead = compare?.ahead_by ?? 0;
      const filesModified = (compare?.files ?? []).reduce((s, f) => s + f.changes, 0);

      // divergeCommit = total commits - how many the branch is ahead (approximate merge-base position)
      const divergeCommit = Math.max(0, commitCount - ahead);

      const domain = inferDomain(b.name);
      const worktrees: Worktree[] = domain && ahead > 1
        ? [{ domain, commitCount: Math.min(ahead, 8) }]
        : [];

      return {
        name: b.name,
        divergeCommit,
        commitCount: ahead,
        filesModified,
        gateState,
        isGhost: false,
        worktrees,
      };
    }),
  );

  return {
    id: repoName.toLowerCase().replace(/[^a-z0-9]/g, '-'),
    name: repoName,
    commitCount,
    fileCount,
    branches: branchData.filter(b => b.commitCount > 0),
  };
}

// ── Public API ──────────────────────────────────────────────────────────────

export interface ForestFetchResult {
  repos: RepoData[];
  fetchedAt: number;
  error?: string;
}

/**
 * Fetch all GitForest repo data from GitHub.
 * Falls back to empty branches (seed geometry still renders) on auth failure.
 */
export async function fetchGitHubForestData(
  org: string,
  repoNames: readonly string[],
  overrideToken?: string,
): Promise<ForestFetchResult> {
  const token = resolveToken(overrideToken);

  try {
    const repos = await Promise.all(
      repoNames.map(name => fetchRepo(org, name, token)),
    );
    return { repos, fetchedAt: Date.now() };
  } catch (err) {
    return {
      repos: [],
      fetchedAt: Date.now(),
      error: err instanceof Error ? err.message : String(err),
    };
  }
}

/** 5-minute cache TTL */
export const FOREST_CACHE_TTL_MS = 5 * 60 * 1000;

/**
 * List open pull requests across the given repos.
 * Uses the list endpoint only (no N+1 per-PR calls) — no additions/deletions.
 * Sort: most-recently-updated first.
 */
export async function listOpenPRs(
  org: string,
  repoNames: readonly string[],
  overrideToken?: string,
): Promise<PullRequest[]> {
  const token = resolveToken(overrideToken);
  const results: PullRequest[] = [];

  await Promise.allSettled(
    repoNames.map(async repo => {
      const { data } = await ghFetch<GHPullList[]>(
        `/repos/${org}/${repo}/pulls?state=open&per_page=20`,
        token,
      );
      for (const pr of data) {
        results.push({
          number:             pr.number,
          title:              pr.title,
          repo,
          author:             pr.user.login,
          headBranch:         pr.head.ref,
          baseBranch:         pr.base.ref,
          createdAt:          pr.created_at,
          updatedAt:          pr.updated_at,
          changedFiles:       pr.changed_files,
          labels:             pr.labels.map(l => l.name),
          reviewersRequested: pr.requested_reviewers.length,
          draft:              pr.draft,
        });
      }
    }),
  );

  return results.sort(
    (a, b) => new Date(b.updatedAt).getTime() - new Date(a.updatedAt).getTime(),
  );
}
