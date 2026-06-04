# Memory Mirror

Git-tracked mirror of selected entries from the auto-memory system at
`~/.claude/projects/-Users-kft-Projects/memory/`.

## Purpose

The auto-memory system is file-based but NOT version-controlled. This mirror
provides:

1. **Durability** — memory entries that would otherwise live only on one
   machine's `~/.claude/` directory are checked into git and reviewable in PRs.
2. **Discoverability** — peers can browse the entries without access to the
   author's local Claude Code session.
3. **Canon-adjacency** — memory entries that are candidates for canon promotion
   (Cookbook / Platform Canon / etc.) sit next to the canon docs they may extend.

## Scope

This is a **selective mirror**, not a full backup. Entries land here when they:

- Pressure-test a learning that future builds will reference (across sessions
  on potentially different machines)
- Capture patterns related to the contract canon, Cookbook, or other
  versioned standards
- Were promoted via /REFLECT with HIGH confidence and operator approval

Entries that stay in `~/.claude/...` (not mirrored): session-local gotchas,
personal preference notes, ephemeral debugging context.

## Layout

- `MEMORY.md` — copy of the index at the time entries were mirrored. May be
  out of sync with the live index; the live index at
  `~/.claude/projects/-Users-kft-Projects/memory/MEMORY.md` is authoritative.
- `feedback_*.md` — individual memory entries, frontmatter intact.

## Update protocol

When the live memory in `~/.claude/...` updates, do NOT auto-sync. Mirror
selectively: copy only the entries that meet the scope criteria above. The
mirror is curated, not a one-way replication.
