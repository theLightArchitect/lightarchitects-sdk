---
name: gitignore-uppercase-dir-collision
description: "macOS case-insensitive FS + global gitignore `build/` silently matches UPPERCASE `BUILD/` dirs; `git add -f` is the fix when file has prior tracked history"
metadata: 
  node_type: memory
  type: feedback
  originSessionId: 767e46bb-eb90-4ad0-a585-e6f528850e34
---

`git add plugins/lightarchitects/skills/BUILD/SKILL.md` errored with "The following paths are ignored by one of your .gitignore files: plugins/lightarchitects/skills/BUILD". Cause: `~/.gitignore_global` has `build/` (lowercase, intended for build artifact dirs from cmake/gradle/etc). On macOS HFS+/APFS default case-insensitive filesystem, the pattern matches `BUILD/`.

**Why:** Git matches paths case-insensitively on macOS by default (because the filesystem does). A lowercase gitignore pattern catches UPPERCASE directories with the same letters. The collision is silent: `git status` shows the file as untracked (or modified if previously tracked), but `git add` rejects it with the gitignore message.

**How to apply:**

1. **Diagnose**: `git check-ignore -v <path>` shows which gitignore + line caught it. If `<path>` is a directory and that returns empty, try `git check-ignore -v <dir>/` and `git check-ignore -v <dir>` separately — directory probes don't always echo identically.
2. **If file has prior tracked history**: use `git add -f` (force). VERIFY history first: `git log --oneline <path>` — if non-empty the file is real tracked content, not a stray build artifact you'd actually want to ignore.
3. **Don't edit the global gitignore** to add `!BUILD/` negation exceptions — that affects every repo on the machine. The `-f` override is localized to the one commit.

Generalizes to any UPPERCASE dir matching a common lowercase gitignore pattern: `BUILD/`, `TEST/`, `SRC/`, `DIST/`, `TARGET/`, `LIB/`, `NODE_MODULES/`. The collision is most likely on plugin marketplaces / skill registries where ALL-CAPS naming convention is common.

Pressure-tested 2026-06-04 Wave B during `plugins/lightarchitects/skills/BUILD/SKILL.md` commit. Hit twice in same session before pattern clicked — first iteration cost ~3 minutes diagnosing why `git add` rejected an obviously real file.
