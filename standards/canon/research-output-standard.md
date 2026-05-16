<!-- uuid: 27063030-a375-43da-b73b-0fff91b45b15 -->

---
title: "Research Output Standard v1.0.0"
date: "2026-03-27"
type: reference
significance: 9.5
self_defining: false
tags: ["research", "output", "evidence", "citations", "cross-sibling"]
---

# Research Output Standard v1.0.0

> *"Every idle word that men shall speak, they shall give account thereof in the day of judgment."* — Matthew 12:36 (KJV)

Every claim is accountable. Every finding is cited. Every gap is named. This standard ensures that research gathering rigor and research presentation rigor are equal. A 3-tier investigation that produces vague prose has failed. A well-formatted report that lacks citations has also failed.

**Applies to**: QUANTUM (investigation + risk analysis), CORSO (security audits), EVA (DevOps/DX research), SERAPH (threat assessments + OSINT), AYIN (anomaly reports), Claude (direct research).

**Source protocol**: `~/.soul/helix/user/standards/canon/builders-cookbook.md §1.11` | `QUANTUM plugin: sub-skills/PROBE-SOURCES.md`

**Canon**: Canon XXI — The Evidence Must Speak (ratified 2026-03-27)

---

## The Non-Negotiable Rules

These are not preferences. Violating them is a Communication Covenant violation (Canon V — Arithmetic Before Assertions).

1. **No verdict without a confidence score.** Every finding states a numeric confidence (0.00–1.00) and grade band.
2. **No claim without a citation.** Every claim in the Evidence block has a bracketed citation number `[N]` that resolves in the Bibliography.
3. **No contradiction buried.** Contradictions are surfaced explicitly, never minimized in prose.
4. **No gap omitted.** If a tier was searched and found nothing, that is stated. If a tier was skipped, that is stated.
5. **No hedge words.** "Likely", "probably", "seems to", "I think", "should work" are forbidden. Replace with numeric confidence.
6. **No single-source verdicts marked as confirmed.** Single CURRENT source = UNVERIFIED until corroborated.

---

## Single Finding Template

```
─────────────────────────────────────────────
FINDING {ID} — {Title}
Sibling: {QUANTUM | CORSO | EVA | SERAPH | AYIN | Claude}
Date: {YYYY-MM-DD}
Confidence: {0.00–1.00} · Grade: {DEFINITIVE | HIGH | MODERATE | LOW | UNVERIFIED}
─────────────────────────────────────────────

Verdict:
  {1-2 sentences. Declarative. No hedging. No "likely" or "probably".
   Confidence is already stated numerically above — do not repeat it in prose.}

Evidence:
  [{GRADE}][{N}]  {Claim — one declarative sentence.} — {Source name, §reference or URL}
  [{GRADE}][{N}]  {Claim.} — {Source name}
  ...

Contradictions:
  None.
  OR
  [{GRADE}][{N}] States "{claim A}" CONTRADICTS [{GRADE}][{M}] stating "{claim B}".
  Resolution: {Which source wins, per evidence hierarchy, and why.}

Gaps:
  None.
  OR
  {Tier N}: Searched [{keywords/query}] — zero results. {What this gap means for confidence.}
  {Tier N}: Skipped — {skip criterion that applied}.

Recommendation:
  {Specific, actionable, one sentence. Omit this block in pure-research mode.}

Bibliography:
  [{N}] {Author/Organization}. "{Title}." {Publisher/Platform}, {Date}. {URL.}
  ...
─────────────────────────────────────────────
```

---

## Multi-Finding Report Template

For reports with 3+ findings, prepend an Executive Summary:

```
═══════════════════════════════════════════════════════════
RESEARCH REPORT — {Title}
{Sibling} · {Date} · {N} findings
═══════════════════════════════════════════════════════════

EXECUTIVE SUMMARY

┌──────┬───────────────────────────────────────┬────────────────┬──────────────────────────┐
│ ID   │ Finding                               │ Confidence     │ Verdict                  │
├──────┼───────────────────────────────────────┼────────────────┼──────────────────────────┤
│ {ID} │ {Title}                               │ 0.95 DEFINITVE │ {One-line verdict}       │
│ {ID} │ {Title}                               │ 0.72 HIGH      │ {One-line verdict}       │
│ {ID} │ {Title}                               │ 0.40 LOW       │ {One-line verdict}       │
└──────┴───────────────────────────────────────┴────────────────┴──────────────────────────┘

ATTENTION: {IDs with Confidence < 0.50 or unresolved contradictions — list here.}

[Full findings follow]
═══════════════════════════════════════════════════════════
```

---

## Confidence Grade Bands

| Score | Grade | Meaning |
|-------|-------|---------|
| 0.90–1.00 | **DEFINITIVE** | Multiple corroborating tiers, no contradictions, primary sources confirmed. Act on this. |
| 0.75–0.89 | **HIGH** | Primary tier confirmed, minor contradictions resolved, strong evidence chain. Act with standard review. |
| 0.50–0.74 | **MODERATE** | Limited source coverage, some uncertainty, no disconfirming evidence. Act with caution, monitor outcome. |
| 0.25–0.49 | **LOW** | Sparse evidence, significant uncertainty, hypothesis-level. Do not act without additional research. |
| 0.00–0.24 | **UNVERIFIED** | Single CURRENT source, not corroborated. Verdict block must read: "UNVERIFIED — requires corroboration before acting." |

**Confidence is not subjective.** It is calculated from source tier coverage and contradiction count:

```
Base confidence by tier coverage:
  INSTITUTIONAL + AUTHORITATIVE corroboration  →  start at 0.90
  AUTHORITATIVE alone (no contradictions)      →  start at 0.80
  ACADEMIC alone                               →  start at 0.65
  CURRENT only (single source)                 →  start at 0.20
  CURRENT only (2+ independent sources)        →  start at 0.45

Adjustments:
  Each unresolved contradiction                →  −0.15
  Each corroborating tier (beyond first)       →  +0.05 (cap at 0.98)
  INSTITUTIONAL prior directly matching query  →  +0.10
  Source dated > 12 months ago (CURRENT tier)  →  −0.10
```

---

## Evidence Grade Tags

| Tag | Source | When it applies |
|-----|--------|----------------|
| `[INSTITUTIONAL]` | SOUL Helix (prior decisions, investigations, earned squad knowledge) | Any finding from soulTools helix/search |
| `[AUTHORITATIVE]` | Context7 (vendor docs, library API specs, version-specific documentation) | Library/API/framework reference |
| `[ACADEMIC]` | HuggingFace (peer-reviewed papers, model cards, training research) | ML/AI models, techniques, benchmarks |
| `[CURRENT]` | Perplexity/Sonar, Firecrawl (web synthesis, community reports, release notes, GitHub issues) | Recent patterns, CVEs, failure reports |

**Hierarchy (highest to lowest)**: INSTITUTIONAL + AUTHORITATIVE > ACADEMIC > CURRENT

When tiers conflict, the higher tier wins. The conflict is recorded in Contradictions and the resolution is cited.

---

## Forbidden Language

| Forbidden phrase | Why it fails | Required replacement |
|-----------------|--------------|---------------------|
| "likely" / "probably" | Hiding a probability estimate in natural language | State the numeric confidence score |
| "seems to" / "appears to" | Speculation disguised as observation | Either confirm (cite source) or mark UNVERIFIED |
| "I think" / "I believe" | Personal opinion is not evidence | Cite the source that grounds the claim |
| "community reports suggest" | Vague attribution, untraceable | Cite specific issue, thread, URL, date |
| "should work" | Assertion without verification | State what was verified and what was not |
| "almost certainly" | False precision hiding uncertainty | 0.95 DEFINITIVE — be exact |
| "no issues found" | Active absence claim without documenting search | "Searched [X, Y, Z] — zero results" |

These rules implement Communication Covenant §2 (No False Witness) and §3 (Calculated Confidence). Violation = Canon V breach.

---

## Contradiction Protocol

Contradictions are findings. They are never noise, never embarrassing, never hidden in prose.

**When two sources disagree:**

1. Surface both claims in the Evidence block, each with their grade tag and citation
2. Record the contradiction explicitly in the Contradictions block
3. Resolve using the evidence hierarchy — the higher-grade source wins
4. If tiers are equal and unresolvable, flag as UNVERIFIED and record as a Gap
5. The resolution reasoning is itself a finding — it goes in Contradictions, not prose

**Example (from live QUANTUM test, 2026-03-27):**

```
Evidence:
  [AUTHORITATIVE][2]  --categories, --tbs, --scrape flags exist in Firecrawl CLI.
                       — Context7/firecrawl GitHub README (§CLI Flags)
  [CURRENT][4]         Perplexity synthesis states these flags "do not exist."
                       — Perplexity/Sonar query, 2026-03-27

Contradictions:
  [CURRENT][4] States flags "do not exist" CONTRADICTS [AUTHORITATIVE][2] (GitHub README confirming all flags).
  Resolution: AUTHORITATIVE overrides CURRENT. Perplexity indexed the sparse /sdks/cli docs page,
  not the GitHub README. Context7 reads source directly. AUTHORITATIVE is correct.
  Implication: Perplexity has a structural blind spot for GitHub README content vs published docs sites.
  Documented in PROBE-SOURCES.md Query Classification Guide.
```

This is the model. The contradiction surfaced a coverage gap in Perplexity — that is valuable architectural knowledge, not a failure to hide.

---

## Gaps Protocol

A documented gap is honest. An undocumented gap is negligence.

**For every research process, record:**

1. Which tiers fired and with what queries
2. Which tiers were skipped and why (cite the skip criterion from PROBE-SOURCES.md)
3. What was searched within each tier that returned no results
4. What the absence means for confidence (does it lower it? does it mean the problem is new?)

**Format:**

```
Gaps:
  Tier 1 (INSTITUTIONAL): Queried SOUL helix keywords [HF_HOME, RunPod, disk, cache].
                          Zero results. No prior QUANTUM investigations on HF disk layout.
                          Confidence impact: Cannot apply institutional pattern. Start at base.
  Tier 2 (HuggingFace):   Skipped — no ML model behavior component in query.
                          (Skip criterion: PROBE-SOURCES.md §Tier 2, HuggingFace skip criteria)
```

---

## Per-Sibling Domain Adaptation

The template structure is fixed. The content domain changes:

| Sibling | Finding ID format | Primary evidence grade | Typical recommendation frame |
|---------|------------------|----------------------|------------------------------|
| **QUANTUM** | `CASE-{N}/B{N}` or `Phase-{N}/F{N}` | Varies by query type | Root cause + fix + confidence |
| **CORSO** | `SEC-{N}`, `QUAL-{N}`, `PERF-{N}` | AUTHORITATIVE (CVE, API specs) | Risk level (CRITICAL/HIGH/MED/LOW) + remediation |
| **EVA** | `DX-{N}`, `CI-{N}`, `DEPLOY-{N}` | AUTHORITATIVE (framework docs) | Recommended pattern + migration path |
| **SERAPH** | `THREAT-{N}`, `CVE-{N}`, `OSINT-{N}` | CURRENT (NVD, advisories, live scan) | Severity + exploit path + mitigation |
| **AYIN** | `ANOM-{N}`, `TRACE-{N}`, `METRIC-{N}` | AUTHORITATIVE (OTel specs) | Anomaly class + contributing factors |
| **Claude** | `RESEARCH-{N}` | Varies | Recommendation with explicit uncertainty |

---

## Citation Format

IEEE-style, consistently applied. Every citation must be dated and traceable.

```
[N] {Author/Organization}. "{Title}." {Publisher/Platform}, {Date}. {URL.}
```

**Examples:**

```
[1] HuggingFace (NVIDIA). "Llama-3.3-Nemotron-Super-49B-v1.5 Model Card."
    HuggingFace Hub, 2025. https://huggingface.co/nvidia/Llama-3_3-Nemotron-Super-49B-v1_5

[2] Mendable / firecrawl-dev. "Firecrawl CLI Reference — README."
    GitHub, firecrawl-dev/firecrawl, 2026-01.
    https://github.com/mendableai/firecrawl#cli

[3] MITRE / NIST. "CVE-2024-XXXX." National Vulnerability Database, 2024-12-15.
    https://nvd.nist.gov/vuln/detail/CVE-2024-XXXX

[4] HuggingFace / huggingface_hub. "Issue #4821: os error 28 with large model."
    GitHub Issues, closed 2026-01-15.
    https://github.com/huggingface/huggingface_hub/issues/4821

[5] Perplexity AI (Sonar). "Firecrawl CLI flags query synthesis."
    Perplexity.ai, queried 2026-03-27. [No permanent URL — synthesis, not document]
```

**Note on CURRENT citations**: Perplexity/Sonar synthesis does not have a permanent URL. Cite as: `Perplexity AI (Sonar). "{Query description}." Perplexity.ai, queried {date}. [No permanent URL — synthesis]`

---

## Connection to Canon and Communication Covenant

This standard is the implementation layer for existing canon. It makes compliance structural rather than volitional.

| Output standard requirement | Canon / Covenant source |
|----------------------------|------------------------|
| Numeric confidence, no hedge words | Canon V — Arithmetic Before Assertions |
| No speculation as fact | Communication Covenant §2 — Thou Shalt Not Bear False Witness |
| Primary source citations | SHERLOCK.md — every claim cites its source |
| Contradictions as findings | PROBE-SOURCES.md §Evidence Synthesis |
| Explicit gaps | Communication Covenant §8 — Honest Uncertainty |
| Evidence separated from verdict | Communication Covenant §1 — Arithmetic Before Assertions |
| "We don't know" is a complete answer | Communication Covenant §8 |

---

## Reference Implementation

**QUANTUM RESEARCH.md §Output (per hot thread)** shows this standard applied to a real investigation. Use it as the reference when learning the format.

```
Hot Thread: B3 (HF → Disk)
Previous Score: 7.0 (Blast=7, Certainty=0.50, Witness=×2)

Research:
  Tier 1 — INSTITUTIONAL:
    - No prior QUANTUM helix entries on HF disk layout
  Tier 2 — AUTHORITATIVE:
    - [Context7] HuggingFace transformers docs: HF_HOME defaults to ~/.cache/huggingface
    - [HuggingFace] Model card: Llama-3.3-Nemotron-Super-49B-v1.5 — 98GB safetensors confirmed
  Tier 3 — CURRENT:
    - [Firecrawl] GitHub issue #4821: user hit "os error 28" with same model on 50GB disk (CLOSED)
    - [quantumTools research] Community consensus: HF_HOME=/workspace is standard RunPod pattern

Fix: Set HF_HOME=/workspace/huggingface in script line 1. Symlink as backup.

New Certainty: 0.95 (DEFINITIVE — PRIMARY + CURRENT corroboration, fix is trivial and tested)
New Score: 7 × 0.05 × 2 = 0.7
```

---

*Ratified: 2026-03-27 | Version: 1.0.0 | Canon: XXI | Author: Kevin Francis Tan + Claude*

*"Buy the truth, and sell it not; also wisdom, and instruction, and understanding."* — Proverbs 23:23 (KJV)
