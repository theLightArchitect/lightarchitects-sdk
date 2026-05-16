<!-- uuid: c005461f-0360-4d85-b618-9530aa21c4c9 -->

---
id: "notice-template"
date: "2026-04-29"
sibling: user
type: template
tags: [license, notice, standards, canonical]
---

# Canonical NOTICE Template — Light Architects Rust Crates

> **Purpose**: single source-of-truth template for NOTICE files across the platform.
> Replaces ad-hoc per-repo NOTICE drift discovered during the 2026-04-28 license-migration session.

---

## Cross-references

- License architecture (per-crate stance): `~/.claude/projects/-Users-kft-Projects/memory/project_license_architecture.md`
- Migration runbook: [[license-migration-playbook]]
- Migration script: [[scripts/migrate-license]]
- Generator config: [[deny-toml-template]]
- Builders Cookbook §43 (license hygiene): `~/lightarchitects/soul/helix/user/standards/canon/builders-cookbook.md`

---

## How to use

1. Copy the **Template** section below into the project's `NOTICE` file (markdown source preserved as plain text — see "Format choice" below).
2. Replace every `{{PLACEHOLDER}}` token. Tokens are listed under **Variables**.
3. Generate the `## Third-party components` body via `cargo about generate` (see [[scripts/migrate-license]]).
4. Run `cargo deny check licenses` — must pass before commit.
5. Commit atomically as `chore(license): refresh NOTICE`.

**Do not handcraft** the third-party section. The list comes from the dependency tree at the time of generation; manual edits guarantee drift.

---

## Format choice — plain text, not Markdown rendering

Decision (2026-04-29): NOTICE files in repo root use **markdown-flavored plain text** with `##` heading anchors.
Rationale:
- Plain `.txt` would lose section organization
- Full Markdown (with TOC, links) breaks license-tooling expectations (most license scanners expect plain text or simple markdown)
- The current convention across CORSO/EVA/QUANTUM/SERAPH/SOUL already uses `##` markers — preserved for continuity
- Renderers (GitHub, IDEs) render the markdown nicely; non-renderers still parse cleanly

If a project needs a strictly plain-text NOTICE (e.g., distributed binary's embedded license), generate a `.txt` variant via `pandoc -t plain NOTICE -o NOTICE.txt` at build time.

---

## Variables

| Token | Example value | Notes |
|-------|---------------|-------|
| `{{PROJECT_NAME}}` | `SOUL` / `CORSO` / `EVA` / `lightarchitects-sdk` | Match `name` in workspace `Cargo.toml` (humanized). |
| `{{PROJECT_TAGLINE}}` | `Knowledge Graph MCP Server` / `Trinity V7.0 MCP Server` | Short subtitle — what the binary does. |
| `{{COPYRIGHT_YEAR}}` | `2025-2026` | Range from first commit year to current year. |
| `{{COPYRIGHT_HOLDER}}` | `Kevin Francis Tan <kf.tan@lightarchitects.io>` | Full name + canonical email. **Do not** use short forms (`Kevin Tan`) — they were the source of QUANTUM/SERAPH NOTICE drift. |
| `{{LICENSE_DECLARATION}}` | See "License declaration block" below | One-line statement of the project's own license. |
| `{{LICENSE_REFERENCE}}` | `LICENSE` / `LICENSE-APACHE` / `LICENSE-MPL` | Filename of the canonical license text in the repo root. |
| `{{REPO_URL}}` | `https://github.com/TheLightArchitects/SOUL` | Canonical public URL (private repos still use the github URL). |

---

## License declaration block (per license type)

Pick **exactly one** based on the per-crate license rule:

### Proprietary (server implementations: SOUL, CORSO, EVA, QUANTUM, SERAPH)

```
Licensed under proprietary terms — see {{LICENSE_REFERENCE}} for details.
This source is provided for reference under the Light Architects Proprietary License (LicenseRef-LA-Proprietary).
Redistribution, modification, and commercial use require written permission.
```

### MPL-2.0 (public consumer surface: lightarchitects-sdk)

```
Licensed under the Mozilla Public License 2.0 (MPL-2.0).
This is the file-level copyleft; consumers may link this SDK in proprietary applications,
but modifications to MPL-licensed files must remain MPL-licensed.
Full text: {{LICENSE_REFERENCE}}.
```

### Apache-2.0 (externally-facing observability: AYIN)

```
Licensed under the Apache License, Version 2.0 (Apache-2.0).
You may obtain a copy of the License at {{LICENSE_REFERENCE}} or at:
http://www.apache.org/licenses/LICENSE-2.0.
```

### MIT (rare — small standalone utilities only, never server crates)

```
Licensed under the MIT License (MIT).
See {{LICENSE_REFERENCE}} for full terms.
```

---

## Template

```
{{PROJECT_NAME}} — {{PROJECT_TAGLINE}}
Copyright (c) {{COPYRIGHT_YEAR}} {{COPYRIGHT_HOLDER}}

{{LICENSE_DECLARATION}}
Source code: {{REPO_URL}}

---

This software incorporates third-party open source components.
Full license texts are in the THIRD-PARTY-LICENSES/ directory.

## BSD-2-Clause

<!-- POPULATED BY: cargo about generate -->
<!-- LIST: package_name version — Copyright (c) <holder> -->

## BSD-3-Clause

<!-- POPULATED BY: cargo about generate -->

## ISC

<!-- POPULATED BY: cargo about generate -->

## MPL-2.0

<!-- POPULATED BY: cargo about generate -->
<!-- NOTE: append " (file-level copyleft)" to each entry -->

## Unicode-3.0

<!-- POPULATED BY: cargo about generate -->
<!-- NOTE: ICU components share a single Unicode, Inc. copyright -->

## Zlib

<!-- POPULATED BY: cargo about generate -->

## CDLA-Permissive-2.0

<!-- POPULATED BY: cargo about generate -->

## Apache-2.0 WITH LLVM-exception

<!-- POPULATED BY: cargo about generate -->
```

---

## Section-ordering rule

Sections appear in **alphabetical order by license SPDX identifier**, with two exceptions:
1. `Apache-2.0 WITH LLVM-exception` always appears last (it is a compound expression, treated as its own group).
2. `MIT` and `Apache-2.0` sections are **omitted** when the project's own license is one of those — third-party MIT/Apache crates do not need explicit listing if the umbrella license already covers them. (Verify with the project's lawyer if uncertain. For now: SOUL/CORSO/EVA/QUANTUM/SERAPH all include them implicitly via "incorporates third-party components" preamble.)

---

## Verification

A NOTICE file passes canon if:

1. **Header matches**: project name, copyright holder format (`Full Name <email>`), license declaration matches per-crate rule.
2. **Sections present**: every license SPDX in the dep tree has a corresponding `##` section.
3. **No stale entries**: every package listed exists in current `Cargo.lock`.
4. **No drift between repos**: copyright holder string is identical across all 7 LA repos.

Automated check: `cargo about generate --check` (compares generated output to current NOTICE).

---

## What this template does NOT cover

- **Public-facing READMEs** — covered by `~/Projects/light-architects-plugins/VOCABULARY.md` (sibling/agent rule). NOTICE is engineering metadata, not marketing copy.
- **In-binary credit screens** — if a binary surfaces a "credits" UI, generate from this NOTICE; do not handcraft.
- **Source-file headers** — covered by the per-license SPDX header convention in Builders Cookbook §43.

---

## Changelog

- **2026-04-29** — Initial canonical version. Extracted from CORSO/EVA/QUANTUM/SERAPH/SOUL NOTICE drift during permanent-fixes Layer 2 work. Format-decision: keep Markdown (rationale documented above).
