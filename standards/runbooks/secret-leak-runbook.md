<!-- uuid: 067dc873-875c-4b5f-90c5-589f79ec27bd -->

> **MARKED FOR DELETION** — Superseded by [`operators-manual.md`](../canon/operators-manual.md) v1.0 (2026-05-12). Content fully absorbed as Part VII §7.1 (Secret-Leak Remediation). Do not edit.

---
id: "secret-leak-runbook"
date: "2026-04-29"
sibling: user
type: runbook
tags: [security, secrets, git, runbook, standards, canonical]
---

# Secret-Leak Remediation Runbook

> **Bug class prevented**: the 2026-04-28 SOUL `HF_TOKEN` leak. Local `git filter-repo`
> rewrote history and force-pushed to GitHub. We forgot that the SOUL repo had a
> **second remote** (`gitlab`) — the rewritten history never reached gitlab, and the
> token remained exposed there for ~30 minutes after we believed remediation was complete.
>
> **Rule**: enumerate **every** remote before rewriting history. Push the rewrite to
> **every** remote. Verify per remote.

---

## Cross-references

- GitHub free-tier constraints (no secret-scanning at push time): `~/.claude/projects/-Users-kft-Projects/memory/project_github_tier_constraints.md`
- Permanent-fixes plan: `~/.claude/plans/permanent-fixes-2026-04-29.md` (Layer 2 → L2.5)
- Backup retention: same plan, L3.5
- Builders Cookbook §11 (secrets management): `~/lightarchitects/soul/helix/user/standards/canon/builders-cookbook.md`

---

## Severity classification

| Token type | Blast radius | Severity |
|------------|-------------|----------|
| `ANTHROPIC_API_KEY` (paid tier) | account-wide spend, cross-project | **CRITICAL** |
| Cloud provider key (AWS, GCP) | infra access, billing | **CRITICAL** |
| `HF_TOKEN` (write scope) | model uploads, dataset modification | **HIGH** |
| `OLLAMA_API_KEY` | quota burn, no infra access | **MEDIUM** |
| `OPENAI_API_KEY` | spend, RBAC depends on org | **HIGH** |
| Personal access token (GitHub PAT) | repo R/W, possibly admin | **CRITICAL** |
| Database password (local-only Neo4j) | read of personal vault | **MEDIUM** |
| HMAC signing secret | tamper-detection bypass | **HIGH** |

If unsure, **escalate up**. Treating MEDIUM as CRITICAL costs nothing; the inverse can be expensive.

---

## Step 1 — ROTATE THE TOKEN FIRST (always, before anything else)

> **Why first**: history rewriting takes minutes. Rotation invalidates the leaked
> token in seconds. Even if the rewrite succeeds and reaches every mirror, an
> attacker who scraped the token between leak and remediation can still use it.
> Rotation is the only **prevention**; rewriting is **clean-up**.

### Rotation locations

| Token | Console |
|-------|---------|
| `ANTHROPIC_API_KEY` | https://console.anthropic.com/settings/keys |
| `HF_TOKEN` | https://hf.co/settings/tokens |
| `OPENAI_API_KEY` | https://platform.openai.com/api-keys |
| `OLLAMA_API_KEY` | https://ollama.com/settings/keys |
| GitHub PAT | https://github.com/settings/tokens |
| GitLab PAT | https://gitlab.com/-/user_settings/personal_access_tokens |
| AWS access key | IAM console — disable old key, create new |

### Update local config

```bash
# 1Password (if managed)
op item edit "Anthropic Production Key" credential[concealed]="<NEW_TOKEN>"

# .env files (search broadly — local checkouts often have stale copies)
grep -rln "OLD_PREFIX" "$HOME/Projects" "$HOME/.config" "$HOME/lightarchitects" 2>/dev/null

# Update .mcp.json envs
grep -rln "OLD_PREFIX" "$HOME/.claude" 2>/dev/null
```

### Confirm rotation took effect

For Anthropic / OpenAI / HF: hit the API with the **old** token and confirm 401.
The provider's UI showing "revoked" is necessary but not sufficient — propagation can lag.

---

## Step 2 — Enumerate ALL remotes

```bash
cd /path/to/affected/repo
git remote -v
```

> **The 2026-04-28 lesson**: SOUL had remotes for `origin` (GitHub) AND `gitlab`
> (a backup mirror). The remediation script only pushed to `origin`. Output of
> `git remote -v` is the **only** authoritative list — checklist memory is unreliable.

For each remote, note:
- Name (`origin`, `gitlab`, `backup`, etc.)
- URL
- Whether it accepts force-push (some hosts reject by default)

```bash
# Capture for the post-mortem record
git remote -v > /tmp/remotes-pre-rewrite.txt
```

---

## Step 3 — Backup before rewriting

```bash
mkdir -p ~/lightarchitects/soul/archive/git-rewrites
git bundle create \
    ~/lightarchitects/soul/archive/git-rewrites/$(date +%Y-%m-%d)-$(basename "$PWD")-pre-filter-repo.bundle \
    --all
```

> **Why a bundle and not a clone**: bundle is a single file you can move off-machine.
> A clone in a sibling directory is convenient but easy to delete by mistake during
> the high-stress remediation. Bundle goes to the canonical archive directory; the
> retention policy keeps it 90 days.

Verify the bundle:

```bash
git bundle verify ~/lightarchitects/soul/archive/git-rewrites/<date>-<repo>-pre-filter-repo.bundle
```

---

## Step 4 — Build the replacements file

```bash
cat > /tmp/replacements.txt <<'EOF'
hf_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx==>***REMOVED-HF-TOKEN***
sk-ant-api03-xxxxxxxxxxxxx==>***REMOVED-ANTHROPIC-KEY***
EOF
```

Format: `LITERAL==>REPLACEMENT` per line, one entry per leaked secret.

If multiple variants of the same token were used (different prefixes, suffix variations),
**list every one**. `git-filter-repo` only replaces literals it sees.

---

## Step 5 — Run filter-repo

```bash
# Install if needed
brew install git-filter-repo  # or: pip install git-filter-repo

# Run inside the repo
cd /path/to/affected/repo
git filter-repo --replace-text /tmp/replacements.txt --force
```

**`--force`** is required: filter-repo refuses to run on a non-fresh clone by default.
The backup bundle from Step 3 is your safety net.

> filter-repo **strips all remotes** as a side effect. This is intentional — it
> prevents accidentally pushing the rewritten history to a remote you forgot about.
> You must re-add every remote in Step 6.

---

## Step 6 — Re-add EVERY remote

Use the captured list from Step 2:

```bash
cat /tmp/remotes-pre-rewrite.txt
# origin  git@github.com:TheLightArchitects/SOUL.git (fetch)
# origin  git@github.com:TheLightArchitects/SOUL.git (push)
# gitlab  git@gitlab.com:lightarchitect/soul.git (fetch)
# gitlab  git@gitlab.com:lightarchitect/soul.git (push)

git remote add origin git@github.com:TheLightArchitects/SOUL.git
git remote add gitlab git@gitlab.com:lightarchitect/soul.git

git remote -v   # confirm
```

---

## Step 7 — Force-push to EVERY remote, every branch

```bash
# Replace BRANCHES with the actual list of refs that contain the leaked token
BRANCHES="main develop feat/some-branch"

for remote in $(git remote); do
    echo "=== Pushing to $remote ==="
    for branch in $BRANCHES; do
        git push --force-with-lease "$remote" "$branch"
    done
    # Tags too if relevant
    git push --force "$remote" --tags
done
```

> Use `--force-with-lease` not `--force` when possible. `--force-with-lease` refuses
> the push if the remote has work the local doesn't — protecting against overwriting
> a teammate's commit. For history rewrites where you're the only contributor,
> `--force` is acceptable but log it explicitly.

---

## Step 8 — Verify per remote

For each remote, fetch a fresh copy and grep for the leaked literal:

```bash
for remote in $(git remote); do
    echo "=== Verifying $remote ==="
    git fetch "$remote"
    # Walk every remote ref's full history. Should print 0.
    git log --all --remotes="$remote" -p | grep -c '<LEAKED-LITERAL>' || echo "0 matches (good)"
done
```

If any remote shows >0 matches, **the rewrite did not reach that remote**. Re-run Step 7
for that specific remote and re-verify.

> The 2026-04-28 gitlab-residue oversight came from skipping this per-remote
> verification. The local `git log --all` showed 0 matches because the local
> refs had been rewritten — but the gitlab remote was unchanged.

---

## Step 9 — Notify consumers

Anyone with a local clone has the **old** history. Until they re-clone or
`git pull --rebase` (which usually fails after a rewrite — they have to reset),
their local copy still contains the token.

For internal-only repos (LA private squad):
- Slack/email: "I rewrote history on `<repo>`. Re-clone or `git fetch && git reset --hard origin/main`."
- Auto-detect: a `.git/config` field comparing local origin's history hash against the rewritten remote.

For public OSS repos: post in the project's discussion / mailing list.

---

## Step 10 — Document the rotation event in helix

```bash
# Create a memory entry for the squad's audit trail
cat > "$HOME/lightarchitects/soul/helix/user/incidents/$(date +%Y-%m-%d)-token-leak-<token-name>.md" <<EOF
---
date: $(date +%Y-%m-%d)
type: incident
severity: <CRITICAL|HIGH|MEDIUM>
token: <NAME, NOT VALUE>
repos_affected: [SOUL, ...]
remotes_remediated: [origin, gitlab]
backup_bundle: ~/lightarchitects/soul/archive/git-rewrites/<file>.bundle
---

# <Token name> leak — <date>

## What happened
...

## Detection
...

## Remediation
1. Rotated at: <UTC time>
2. Rewrote history via filter-repo: <UTC time>
3. Force-pushed to remotes: <UTC time> (origin + gitlab)
4. Verified per remote: <UTC time>

## Consumer notifications
...

## Root cause
...

## Prevention
...
EOF
```

This entry is the post-mortem for the squad's audit trail and feeds future canon updates.

---

## GitHub free-tier specific note

**On GitHub free tier (private repos), secret-scanning at push time is NOT available.**
The org will not block a push containing an obvious credential pattern. There is no
automated detection at the protocol layer.

Compensating controls:

### Pre-commit substitute

```bash
# Install trufflehog
brew install trufflehog

# Run as pre-commit hook
cat > .git/hooks/pre-commit <<'EOF'
#!/usr/bin/env bash
set -e
if ! command -v trufflehog >/dev/null 2>&1; then
    echo "trufflehog not installed — skipping secret scan"
    exit 0
fi
trufflehog filesystem --no-update --fail . || {
    echo "[pre-commit] trufflehog detected secrets. Aborting commit."
    echo "[pre-commit] If false positive, override with: git commit --no-verify"
    exit 1
}
EOF
chmod +x .git/hooks/pre-commit
```

### CI substitute

A `secret-scan` job in GitHub Actions:

```yaml
- name: Secret scan (trufflehog)
  uses: trufflesecurity/trufflehog@v3
  with:
    path: ./
    extra_args: --only-verified
```

This runs **after** push, so it cannot prevent a leak — but it surfaces leaks
within minutes for fast remediation.

### When to revisit

Reconsider GitHub Pro upgrade (~$4/mo/user) when:
- A new credential leak occurs (push-time scanning would have prevented it).
- Org grows past 5 users (per-user cost scales but value scales faster).
- Compliance audit forces the issue.

---

## Anti-patterns (DO NOT DO)

### DO NOT skip rotation because "the leak was only briefly"

Anthropic, HF, and OpenAI all log API key fingerprints in their internal monitoring.
Even a 30-second window is enough for an opportunistic scraper. Rotation is cheap.

### DO NOT push only to `origin`

The 2026-04-28 oversight. Always enumerate via `git remote -v`.

### DO NOT delete the backup bundle until the incident is closed (90+ days)

The bundle is your only recovery path if the rewrite goes wrong. Cheap to keep.

### DO NOT write the new token into a commit message during remediation

It is alarmingly easy to paste the new token into a "rotated to: ..." commit
message intending it as a marker. **Never include the literal in any commit**.
Use the rotation timestamp instead.

### DO NOT trust local `git log --all -p | grep` as a per-remote verification

Local refs were rewritten by filter-repo. The grep is verifying the rewrite worked
**locally**. Per-remote verification (Step 8) requires `git fetch <remote>` first.

---

## Checklist (paste into the incident issue)

- [ ] Step 1 — Token rotated at provider; old token returns 401
- [ ] Step 2 — `git remote -v` captured to /tmp/remotes-pre-rewrite.txt
- [ ] Step 3 — Backup bundle created in ~/lightarchitects/soul/archive/git-rewrites/
- [ ] Step 4 — replacements.txt covers every variant of the leaked literal
- [ ] Step 5 — `git filter-repo` ran successfully (`--force` used; backup verified)
- [ ] Step 6 — Every remote from Step 2 re-added
- [ ] Step 7 — Force-pushed to every remote, every relevant branch, all tags
- [ ] Step 8 — Verified per remote: `git log --all --remotes=<r> -p | grep -c <literal>` returns 0 for each
- [ ] Step 9 — Consumers notified (Slack / mailing list)
- [ ] Step 10 — Incident entry created in helix
- [ ] Bonus — trufflehog pre-commit hook installed in affected repos
- [ ] Bonus — secret-scan CI job added to affected repos

---

## Changelog

- **2026-04-29** — Initial version. Encodes the gitlab-residue lesson and codifies "every remote" as Step 2's checklist item. Adds GitHub free-tier substitutes (trufflehog) per the tier-constraints memory.
