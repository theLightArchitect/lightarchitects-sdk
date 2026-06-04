---
name: empty-discriminator-gap
description: "When introducing a new schema discriminator kind / enum variant / typed category, author at least one exemplar IN THE SAME COMMIT — empty discriminator dirs are silent gaps"
metadata: 
  node_type: memory
  type: feedback
  originSessionId: 767e46bb-eb90-4ad0-a585-e6f528850e34
---

`standards/canon/contracts/agent.skill/` existed as a directory and `agent.skill` was a valid `kind` enum value in `la-contracts.schema.json` for some time before 2026-06-04 — but the directory was empty. Silent gap: validation passes (empty dir = 0 invalid files), cross-references against agent.skill kind don't fire (none to fire), and downstream tools can't enforce any agent.skill rules until exemplars exist.

**Why:** Adding a discriminator kind / enum variant / type tag without authoring an exemplar instance creates a category that exists in name only. Validators don't catch it because there's nothing TO validate against. Cross-references don't catch it because there's no body of work to cross-reference. The gap surfaces only when someone tries to apply rules against the empty category — by that point the schema has been in production for arbitrary time and the wiring assumptions accumulated.

**How to apply:** When adding a new schema discriminator kind (or `enum` variant, or agent role, or sibling identity, or Rust trait that's meant to be implemented elsewhere):

1. **Author the first exemplar IN THE SAME COMMIT** as the schema change that introduces the kind.
2. **If you can't author an exemplar yet**: don't add the kind. Wait until the first real consumer drives the schema change.
3. **Audit after the fact**: after any schema discriminator addition, `ls <category-dir>` (or grep for instances of the new enum variant) — if empty after the commit, the gap is open.

Generalizes beyond JSON Schema:
- Rust enum variants used only as type tags with no constructor sites
- Empty subdirectories in classified directory trees
- Empty agent roles in role registries
- Trait declarations with no impl blocks anywhere
- New skill names in plugin marketplaces without SKILL.md

The category that exists without instances is a code smell — silent because nothing fails, dangerous because every later consumer assumes the category is populated.

Pressure-tested 2026-06-04 Wave A: agent.skill/ was empty for an unknown duration; the wave's primary work was filling it (24 contracts). Without the fill, /CODE-VERIFY + /SECURE cross-references against agent.skill would have silently passed for any uncontracted skill — the recursive gap identified in the cross-exam audit. See related: [[la-contracts-schema-v1-2-shipped]].
