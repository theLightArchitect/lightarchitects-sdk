/**
 * Git log client for the GitForest view.
 * Wraps GET /api/git/log — returns commit history + branch list for a local repo path.
 */

export interface GitCommit {
  sha: string;
  shortSha: string;
  message: string;
  author: string;
  /** ISO-8601 timestamp with timezone offset */
  timestamp: string;
  parentShas: string[];
  /** Ref decorations: branch names, tag names, HEAD pointer */
  refs: string[];
}

export interface GitBranch {
  name: string;
  headSha: string;
  isCurrent: boolean;
}

export interface GitLogData {
  commits: GitCommit[];
  branches: GitBranch[];
}

export async function fetchGitLog(
  cwd: string,
  limit = 40,
  token?: string,
): Promise<GitLogData> {
  const headers: Record<string, string> = {};
  if (token) headers['Authorization'] = `Bearer ${token}`;
  const qs = new URLSearchParams({ cwd, limit: String(limit) });
  const res = await fetch(`/api/git/log?${qs}`, { headers });
  if (!res.ok) throw new Error(`git log ${res.status}`);
  const raw = await res.json() as { commits: unknown[]; branches: unknown[] };
  return {
    commits: (raw.commits ?? []).map((c: unknown) => {
      const obj = c as Record<string, unknown>;
      return {
        sha: String(obj['sha'] ?? ''),
        shortSha: String(obj['short_sha'] ?? ''),
        message: String(obj['message'] ?? ''),
        author: String(obj['author'] ?? ''),
        timestamp: String(obj['timestamp'] ?? ''),
        parentShas: (obj['parent_shas'] as string[] | undefined) ?? [],
        refs: (obj['refs'] as string[] | undefined) ?? [],
      };
    }),
    branches: (raw.branches ?? []).map((b: unknown) => {
      const obj = b as Record<string, unknown>;
      return {
        name: String(obj['name'] ?? ''),
        headSha: String(obj['head_sha'] ?? ''),
        isCurrent: Boolean(obj['is_current']),
      };
    }),
  };
}

/** Map commit type prefix to a display color (hex). */
export function commitTypeColor(message: string): number {
  if (message.startsWith('feat')) return 0x38bdf8;   // sky-blue
  if (message.startsWith('fix'))  return 0xf97316;   // orange
  if (message.startsWith('chore') || message.startsWith('refactor')) return 0x64748b; // slate
  if (message.startsWith('test'))  return 0x4dff8e;  // green
  if (message.startsWith('docs'))  return 0xa78bfa;  // violet
  if (message.startsWith('Merge')) return 0xfbbf24;  // amber
  return 0x94a3b8; // default slate-400
}

/** Strip remote prefix from branch names (origin/, upstream/, etc.). */
export function localBranchName(name: string): string {
  return name.replace(/^(origin|upstream|github)\//, '');
}
