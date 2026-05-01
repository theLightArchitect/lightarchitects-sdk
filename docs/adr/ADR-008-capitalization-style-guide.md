# ADR-008 — Capitalization Style Guide for Domain Terms

**Status**: Accepted  
**Date**: 2026-04-30  
**Build**: unifying-rolling-aegis  
**Tasks**: #24  

## Context

The lightarchitects ecosystem has a set of domain acronyms (sibling names, system names) that appear across multiple surfaces: UI labels, copy text, code identifiers, component names, and documentation. Without a canonical rule, these drifted between `CORS0`, `Corso`, `corso`, and `CORSO` across different surfaces during the aegis build wave. A grep audit on 2026-04-30 found zero violations in rendered text — all public-facing labels already use ALL-CAPS — but the rule needs to be documented to prevent future drift.

## Decision

### Rule by surface

| Surface | Rule | Examples |
|---------|------|---------|
| Public UI text (titles, labels, badges, copy) | ALL-CAPS for domain acronyms | `CORSO`, `EVA`, `SOUL`, `QUANTUM`, `SERAPH`, `AYIN` |
| TypeScript/JavaScript identifiers | camelCase or PascalCase (per JS convention) | `corsoClient`, `SoulStore`, `QuantumRunner` |
| Svelte component names | PascalCase | `MemoryDrawer.svelte`, `AgentTopology.svelte` |
| Rust identifiers | snake_case (types/structs: PascalCase) | `soul_client`, `CorsoBuilder`, `SoulStore` |
| Rust module names | snake_case | `mod copilot;`, `mod dispatch;` |
| Documentation / ADRs | ALL-CAPS for domain acronyms in prose | "CORSO handles build quality." |
| Git branch names | lowercase-kebab | `feat/squad-dispatch` |

### Canonical domain acronyms (ALL-CAPS on public surfaces)

`CORSO` · `EVA` · `SOUL` · `QUANTUM` · `SERAPH` · `AYIN` · `LÆX` · `LASDLC` · `ASQPTDO`

### Enforcement

No automated ESLint/Stylelint rules exist today — no ESLint config is present in `lightarchitects-webshell-ui`. The grep audit confirmed 0 violations at time of writing. A future tooling task can add a custom lint rule if drift recurs.

## Consequences

- All public-facing UI text uses ALL-CAPS for domain acronyms. Code identifiers follow language-idiomatic casing as above.
- No changes to existing code required (audit confirmed compliance).
- Future contributors: check this ADR before naming new components or labels that reference domain terms.
- Tooling automation is explicitly deferred — document the rule first, automate only if violations accumulate.
