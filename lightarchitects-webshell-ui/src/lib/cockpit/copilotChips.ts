import { selectedTarget } from './stores';

export interface CopilotChip {
  id: string;
  label: string;
  action: () => void;
}

const PR_URL_RE = /https:\/\/github\.com\/([A-Za-z0-9_.-]+)\/([A-Za-z0-9_.-]+)\/pull\/(\d+)/g;
const PR_REF_RE = /\bPR #(\d+)\b/gi;

/**
 * Scan an assistant message for actionable cockpit targets (PR URLs, bare PR refs).
 * Returns deduplicated chips the user can click to set `selectedTarget`.
 *
 * @param content  Raw text content of the assistant message.
 * @param repoHint Optional (owner, repo) pair to resolve bare `PR #N` references.
 */
export function parseChips(
  content: string,
  repoHint?: { owner: string; repo: string },
): CopilotChip[] {
  const chips: CopilotChip[] = [];
  const seen = new Set<string>();

  function add(chip: CopilotChip) {
    if (!seen.has(chip.id)) { seen.add(chip.id); chips.push(chip); }
  }

  for (const m of content.matchAll(PR_URL_RE)) {
    const [url, owner, repo, num] = m;
    add({
      id: `pr-${owner}-${repo}-${num}`,
      label: `→ ${owner}/${repo}#${num}`,
      action: () => selectedTarget.set({ type: 'pr', id: url, label: `#${num} (${repo})` }),
    });
  }

  if (repoHint && chips.length === 0) {
    for (const m of content.matchAll(PR_REF_RE)) {
      const num = m[1];
      const url = `https://github.com/${repoHint.owner}/${repoHint.repo}/pull/${num}`;
      add({
        id: `pr-ref-${num}`,
        label: `→ #${num} (${repoHint.repo})`,
        action: () => selectedTarget.set({ type: 'pr', id: url, label: `#${num} (${repoHint.repo})` }),
      });
    }
  }

  return chips;
}
