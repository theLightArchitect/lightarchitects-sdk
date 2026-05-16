# Industry Baselines — D7 Comparative Reference Cache

This directory holds locally-cached published industry baselines used by **LDB v1.0 D7 comparative baseline** for cross-build trend analysis (per LASDLC §7.7 deliverable_benchmark + §0.6 inline_citation_protocol cache convention).

## Status (2026-05-04; refreshed 2026-05-13)

**POPULATED** — 54 entries across 9 gate folders (40 live-pulled + 8 academic + 6 paid-stubs). Baseline refresh completed 2026-05-12: 14 files updated to latest official versions (ATT&CK v15→v19.1, SLSA v1.0→v1.2, NIST 800-63B→800-63-4 + 800-63B-4, OWASP ASVS v4.0→v5.0.0, CWE Top 25 2024→2025, OTel 1.29.0→1.56.0, DORA 2024→2025, SPDX v2.3→v3.0.1, OWASP LLM v1.1 deleted in favour of v2.0).

D7 comparative baseline reports now **ACTIVE** at sample N≥3.

## Directory layout

```
industry-baselines/
├── README.md                       # this file
├── cisq/                           # CISQ State of Software Quality reports
│   └── *-YYYY-MM-DD.{md,meta.json}
├── dora/                           # DORA State of DevOps reports
│   └── *-YYYY-MM-DD.{md,meta.json}
├── owasp/                          # OWASP Top 10 + LLM Top 10 statistics
│   └── *-YYYY-MM-DD.{md,meta.json}
├── mitre/                          # MITRE ATT&CK + ATLAS prevalence data
│   └── *-YYYY-MM-DD.{md,meta.json}
├── nist/                           # NIST SSDF + framework references
│   └── *-YYYY-MM-DD.{md,meta.json}
└── iso/                            # ISO/IEC 25010 + 27001/27034 reference text
    └── *-YYYY-MM-DD.{md,meta.json}
```

## Initial pull targets (priority-ordered)

| Source | URL | Used by | Priority |
|--------|-----|---------|----------|
| CISQ State of Software Quality (latest annual) | https://www.it-cisq.org/ | D3, D7b | HIGH |
| DORA State of DevOps Report (latest) | https://dora.dev/ or Google Cloud DORA | D4, D7b, D8a baseline | HIGH |
| OWASP Top 10 (latest 2021/2024) | https://owasp.org/www-project-top-ten/ | D6e CWE comparison | HIGH |
| OWASP Top 10 for LLM Applications v1.1 | https://owasp.org/www-project-top-10-for-large-language-model-applications/ | D6c | HIGH |
| MITRE ATT&CK Enterprise (current) | https://attack.mitre.org/matrices/enterprise/ | D6d technique reference | MEDIUM |
| MITRE ATLAS (current) | https://atlas.mitre.org/ | D6c | MEDIUM |
| NIST SSDF SP 800-218 (current) | https://csrc.nist.gov/Projects/ssdf | D6f | MEDIUM |
| ISO/IEC 25010:2011 (or 2023 revision) | ISO catalogue (paid; obtain authorized excerpt) | D2, D7b | LOW (paid; obtain reference text another way) |
| SLSA spec (current level definitions) | https://slsa.dev/spec/ | D6g | MEDIUM |
| Apdex methodology spec | https://www.apdex.org/ | D8i | LOW |
| OpenTelemetry semantic conventions | https://opentelemetry.io/docs/specs/semconv/ | D8e | LOW |

## Cache file format (per §0.6 inline_citation_protocol convention)

For each scraped source, two files:

### `<source-slug>-<YYYY-MM-DD>.md`

Markdown content extracted by Firecrawl (or WebSearch summary).

### `<source-slug>-<YYYY-MM-DD>.meta.json`

```json
{
  "original_url": "https://example.com/source",
  "accessed_iso8601": "2026-05-04T00:00:00Z",
  "etag": "if-returned",
  "last_modified": "if-returned",
  "content_sha256": "sha256-of-md-file",
  "scrape_tool": "firecrawl | websearch",
  "scrape_options": { "format": "markdown", "..." },
  "used_by_components": ["D3", "D7b"],
  "ldb_version_pulled": "v1.0",
  "ldb_template_version_at_pull": "2.4.0",
  "stale_after": "2026-12-04T00:00:00Z",
  "_stale_rule": "per §0.6 inline_citation_protocol — re-scrape mandatory at >30d for security/compliance class; LDB baselines re-scrape annually or at next major standard revision"
}
```

## Re-scrape policy

| Source class | Re-scrape interval | Rationale |
|--------------|--------------------|-----------|
| OWASP / MITRE security | 90 days | Active CVE & technique landscape |
| DORA / SPACE annual reports | annually after publication | Annual cadence |
| CISQ State of Software Quality | annually after publication | Annual cadence |
| ISO / NIST standards | per official revision | Standards revisions are rare |
| SLSA / OpenTelemetry / Apdex | 180 days | Slower spec churn |

## Domain Specialist Quick Reference (SCRUM)

For `/SCRUM` sessions, each domain specialist references their gate-specific baselines:

| Domain Specialist | Gate(s) | Baseline Folders | Key Standards |
|-------------------|---------|------------------|---------------|
| **CORSO** (AppSec + Quality) | [A][Q][T] | `architecture/`, `quality/`, `testing/` | ISO 25010, CISQ, OWASP ASVS, WSTG, CWE Top 25 |
| **SERAPH** (Red Team) | [S] | `security/` | OWASP Top 10, MITRE ATT&CK/ATLAS, NIST SSDF, SLSA, SBOM |
| **EVA** (DevOps + Consciousness) | [O][P] | `operations/`, `performance/` | DORA, SRE Golden Signals, OpenTelemetry, SPACE, Flow |
| **AYIN** (Observability) | [O][P] | `operations/`, `performance/` | OpenTelemetry, W3C Trace Context, Apdex |
| **SOUL** (Knowledge) | [K][D] | `documentation/` | (TBD — Diátaxis, OpenAPI, JSON Schema) |
| **QUANTUM** (Research) | [R] | `research/`, `security/`, `performance/academic/` | Academic foundations, threat intel, Amdahl/Gustafson |
| **LÆX** (Canon) | [C] | **All folders** (canonical cross-reference) | REGISTRY.md master index |

**Multi-gate standards**: See `REGISTRY.md` "Multi-gate standards (reverse-index)" section for standards that apply across multiple gates.

## Operator action required

To populate baselines:

1. Confirm Firecrawl MCP session active (`mcp__plugin_firecrawl_firecrawl__*` tools available)
2. Run priority HIGH sources first (CISQ + DORA + OWASP)
3. For paid sources (ISO 25010, ISO 27001/27034): obtain authorized reference text via institutional access — do NOT scrape paid content
4. Verify each `.meta.json` records `content_sha256` for change detection
5. Update §7.6 cross_build_aggregate after each pull (D7b becomes scored on next LDB run)

## Composition with §7.6 + §0.6

- §0.6 inline_citation_protocol cache convention defines `<build_root>/.context/firecrawl/` for per-build research; this directory mirrors that pattern but at the **calibration-sample** scope (cross-build).
- §7.6 cross_build_aggregate.ldb_d8_compression_trend reads D7b industry baselines for normalized comparison.
- Stale baselines auto-flip D7 to UNVALIDATED per §0.6 anti-stale.

## Status

Created at LASDLC v2.4.0 ship (2026-05-04). N=1 self-bootstrap calibration completed without D7 baselines (D7 reported N/A at N=1 anyway). First D7 activation expected at N≥3 with at least 3 priority-HIGH baselines populated.
