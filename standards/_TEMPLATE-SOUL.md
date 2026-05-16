---
# ============================================================================
# SOUL NOTE TEMPLATE — v1.0.0
# ============================================================================
# Location: ~/lightarchitects/soul/helix/{path}/{slug}.md
# Purpose:  General-purpose structured note for vault storage (non-entry)
# Standard: platform/standards/template-soul-md
# ============================================================================

id: "{full-uuid}"
date: "{YYYY-MM-DD}"
kind: "{note|policy|spec|reference|template}"
title: "{Human-readable title}"
significance: {0.0-10.0}
tags: [{keyword1}, {keyword2}]

# --- OPTIONAL ---
related:
  - "[[{relative-path}]]"
expires: "{ISO-8601-datetime}"         # null = permanent
---

# {Title}

{Body content in plain Markdown. No sibling-specific voice required for general notes.}

## Summary

{1-3 sentence abstract of this note's content and purpose.}

## References

{Links, citations, or related resources if applicable.}

# ============================================================================
# SOUL NOTE RULES (v1.0.0)
# ============================================================================
#
# 1. KIND VOCABULARY:
#    note      — general observation or record
#    policy    — prescriptive rule document
#    spec      — schema or interface specification
#    reference — external resource pointer
#    template  — reusable authoring scaffold
#
# 2. SIGNIFICANCE: follows the platform significance-scoring standard bands.
#    See /v1/platform/standards/significance-scoring for the canonical schema.
#
# 3. DIFFERENCE FROM HELIX ENTRIES:
#    Soul notes do NOT require sibling, strands, epoch, resonance, or themes.
#    They are vault artifacts, not knowledge graph entries.
#    Use _TEMPLATE.md for sibling helix entries.
#
# ============================================================================
