# Architecture Intelligence Substrate — v1

## Purpose and Northstar

The lightarchitects-arch crate is the Architecture Intelligence Substrate for
the Light Architects platform. It provides three capabilities that together close
Northstar Pillar 1 / E3 (operators see living architecture diagrams that drift when
code diverges) and E5 (webshell Architecture screen):

Extraction — tree-sitter parsers for Rust, TypeScript, and Python produce
an ArchModel (nodes + relations) from source trees without running the code.
Emission — the model is rendered to Likec4, Mermaid, D2, and an HTML/Markdown
pair. The HTML pair is the canonical operator-facing artifact.
Verification — Verifier::diff compares a planned model (committed spec) against
the current extraction, capping findings at 50 per run (M7) to prevent alert fatigue.

Security is first-class: security::path prevents path traversal (H1/CWE-22) and
security::cmd_exec hard-codes an allowlist for compliance-check binaries (B1/CWE-78),
rejecting any invocation that uses shell expansion.

## Nodes

| ID | Label | Level | Language | Location |
|----|-------|-------|----------|----------|
| `extractor` | extractor | Module | Rust | lightarchitects-arch/src/extractor/mod.rs |
| `emitter` | emitter | Module | Rust | lightarchitects-arch/src/emitter/mod.rs |
| `narrative` | narrative | Module | Rust | lightarchitects-arch/src/narrative/mod.rs |
| `verifier` | verifier | Module | Rust | lightarchitects-arch/src/verifier/mod.rs |
| `security` | security | Module | Rust | lightarchitects-arch/src/security/mod.rs |
| `model` | model | Module | Rust | lightarchitects-arch/src/model/mod.rs |

## Relations

| From | Kind | To |
|------|------|----|
| `extractor` | Uses | `model` |
| `emitter` | Uses | `model` |
| `emitter` | Uses | `narrative` |
| `verifier` | Uses | `model` |
| `security` | Uses | `extractor` |
| `security` | Uses | `emitter` |

## Glossary

**ArchModel**: The normalized representation of a project's architecture: nodes (modules, structs, functions) + directed relations (calls, imports, depends_on) + findings.

**ExtractedFacts**: Raw output of a single extraction pass: nodes, relations, and parse warnings. Converted to ArchModel by the caller.

**NarrativeSeed**: Architect-authored TOML file providing section text, glossary entries, and source anchors. The emitter merges this with the deterministic skeleton — it never invents narrative.

**Drift Finding**: A discrepancy between the planned ArchModel (committed spec) and the current extraction. Findings are capped at 50 per run (M7) to prevent alert fatigue.

**M6 Capability Check**: Per-sibling gate at the gateway: the requesting sibling must have arch_extract in its capability set, and the target project must be on the per-project allowlist.

**M17 Isolation**: The webshell crate has no compile-time dependency on lightarchitects-arch; it proxies via HTTP to the gateway. This keeps the webshell compilation surface clean and allows the arch crate to evolve independently.

**Bootstrap Convergence**: The three-axis test that the tool-generated arch-substrate-v1 HTML matches the hand-drafted L0–L2: (a) DOM-tree structural diff ≤5%, (b) exact container/component IDs, (c) exact relation source→target tuples (modulo ordering).

