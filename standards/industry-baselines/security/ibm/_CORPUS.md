---
name: ibm-corpus-index
description: IBM Anthropic-verified industry baselines for AI agent security (ADLC + MCP architecture)
type: reference
authority_rating: HIGH
updated: 2026-06-02
---

# IBM Industry Baselines — Corpus Index

IBM's Anthropic-verified guides on enterprise AI agent architecture. Authoritative for the LASDLC + MCP Gateway patterns we implement.

## GCS Location

```
gs://la-platform-helix/user/standards/industry-baselines/security/agent-and-pentest/
└── ibm-guide-to-architecting-secure-enterprise-ai-agents-with-mcp-techxchange-2025-1.pdf  (1.9 MB)
```

Mirrors this helix path: `$HELIX/user/standards/industry-baselines/security/ibm/`

## Vertex AI Search

- **Project**: `webshell-497114`
- **Data store**: `la-security-baselines`
- **Engine**: `la-search`
- **Status**: pending re-import (PDF saved 2026-06-02; needs .txt copy + reindex per [[2026-06-02-discovery-engine-mime-type]] pattern)

## Documents in this Folder

| File | Authority | Relevance |
|------|-----------|-----------|
| `ibm-adlc-mcp-architecture-2025-10-2026-06-02.md` | HIGH (IBM + Anthropic verification) | Direct architectural alignment with our LASDLC + MCP gateway |

## Citation Anchors (for reuse in helix entries)

- ADLC 6-phase model: Plan → Code & Build → Test & Release → Deploy → Operate → Monitor
- Experimentation Loop (Build↔Test) + Runtime Optimization Loop (Deploy↔Monitor)
- MCP Gateway pattern (p.14): centralized authN/Z, routing, rate limits, policy-as-code, multitenancy, plugins
- 4 agent-specific threat classes (p.10): memory poisoning, tool/API misuse, intent breaking, goal manipulation
- 3 eval types (p.9): offline (CI), online (production), in-the-loop (runtime decision gates)
- "Champion-challenger results have more weight than offline evaluations" (p.12)
- Reproducible signed manifest mandate (p.12)
- 4-category metrics framework: Quality, Safety, Operations, Business
- IBM reference impl: `mcp-context-forge` (Python OSS at github.com/IBM/mcp-context-forge)
