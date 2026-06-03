---
name: owasp-corpus-gcs-index
description: GCS corpus index — full OWASP document set in la-platform-helix, mirroring this helix path
type: reference
updated: 2026-06-02
---

# OWASP GCS Corpus Index

Full OWASP document corpus is indexed in Vertex AI Search for QUANTUM investigations.

## GCS Location

```
gs://la-platform-helix/user/standards/industry-baselines/security/owasp/
├── cheat-sheets/   # OWASP CheatSheet Series — 118 files (.md source + .txt import copy)
├── wstg/           # Web Security Testing Guide — 142+ files (.md source + .txt import copy)
└── llm-top10/      # OWASP LLM Top 10 v2.0 — 12 files (.md source + .txt import copy)
```

Mirrors this helix path: `$HELIX/user/standards/industry-baselines/security/owasp/`

> **Import note**: Discovery Engine detects MIME from file extension; `.md` → `text/markdown`
> (not in allowed list). Each `.md` has a sibling `.txt` copy used for indexing.
> Source `.md` files are kept for helix parity. If re-uploading: copy `.md` → `.txt` first,
> then import `*.txt`. Command: `gsutil cp gs://.../file.md gs://.../file.txt`

## Vertex AI Search

- **Project**: `webshell-497114`
- **Data store**: `la-security-baselines`
- **Engine**: `la-search`
- **SDK**: `lightarchitects::vertex_search::VertexSearchClient` (feature `vertex-search`)

## QUANTUM Usage

```rust
// Via ResearchSource::VertexAi in research/mod.rs
// Requires: gcloud auth application-default login
// Feature flag: --features vertex-search
```

## Helix Summary Files (locally indexed, no API cost)

The files in this directory (`owasp-*.md`) are scraped summaries cached locally for
SOUL search and LDB D7 baseline comparisons. The GCS corpus contains the full source
documents for semantic retrieval during QUANTUM investigations.
