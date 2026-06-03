---
name: mcp-academic-corpus-index
description: Academic security literature specifically targeting Model Context Protocol (MCP) — threat models, vulnerability catalogues, proposed defenses
type: reference
authority_rating: MEDIUM-HIGH
updated: 2026-06-02
---

# MCP Academic Security Literature — Corpus Index

Peer-reviewed and preprint papers on MCP security. Relevant because **we operate an MCP gateway and ship MCP servers (CORSO, EVA, SOUL, QUANTUM, SERAPH, AYIN)**.

## Why This Subfolder

MCP is now a published academic discipline. The first MCP security audit appeared in April 2025; by mid-2026 there are 6+ substantive papers including a 31-attack-method catalog.

## Bibliographic Index

Format: arXiv ID + title + key contribution. Full content available via `hf.co/papers/{id}` or arxiv.org.

### [MCP01] MCP Safety Audit — `2504.03767`
- **Date**: 2 Apr 2025 (first published MCP threat model)
- **Authors**: Radosevich, Halloran
- **Contribution**: Introduces **MCPSafetyScanner** as agentic tool for assessing MCP server security
- **Why canonical**: First in the field; sets terminology
- **Build target**: F13 (`seraph-mcp-vuln-scanner`)

### [MCP02] Systematic Analysis of MCP Security — `2508.12538`
- **Date**: 18 Aug 2025
- **Authors**: Guo, Liu, Ma, Deng, Zhu, Di, Xiao, Wen
- **Contribution**: Catalogues **31 distinct attack methods**; develops attack library + taxonomy + framework
- **Why canonical**: The most comprehensive MCP attack catalogue published to date
- **Build target**: F13 — provides the test corpus

### [MCP03] When MCP Servers Attack: Taxonomy, Feasibility, and Mitigation — `2509.24272`
- **Date**: 29 Sep 2025
- **Authors**: Zhao, Liu, Ruan, Li, Liang
- **Contribution**: Taxonomy from the server-side perspective + feasibility analysis + mitigation strategies
- **Build target**: F13 + threat model for our sibling MCP servers

### [MCP04] MCPGuard — `2510.23673`
- **Date**: 27 Oct 2025
- **Authors**: Wang, Liu, Yu, Yang, Huang, Guo, Cheng, Li, et al.
- **Contribution**: Automated vulnerability detection. Identifies **agent hijacking, web vulnerabilities, supply chain risks**. Proposes proactive server-side scanning + agentic auditing + zero-trust registry + runtime interaction monitoring
- **Why canonical**: Defines the defense architecture
- **Build target**: F13 architecture reference + F1 supply chain integration

### [MCP05] SMCP: Secure Model Context Protocol — `2602.01129`
- **Date**: 1 Feb 2026
- **Authors**: Hou, Wang, Zhang, Xue, Zhao, Fu, Wang
- **Contribution**: Proposed **secure MCP standard** with unified identity management + mutual authentication + security context propagation + policy enforcement + audit logging
- **Why canonical**: Closest existing proposal to a secure MCP profile
- **Build target**: F12 (mcp-gateway-opa-policy) protocol target

### [MCP06] MCP Threat Modeling — Tool Poisoning — `2603.22489`
- **Date**: 23 Mar 2026
- **Authors**: Huang, Huang, Tran, Fard
- **Contribution**: Applies **STRIDE + DREAD** to MCP; focuses on **tool poisoning**; proposes static validation + behavioral anomaly detection + user transparency
- **Why canonical**: Formal threat-modeling methodology applied to MCP
- **Build target**: F13 + CORSO threat modeling skill

### [MCP07] Security Threat Modeling for Emerging AI-Agent Protocols — `2602.11327`
- **Date**: 17 Apr 2026
- **Authors**: Anbiaee, Rabbani, Mirani, Piya, Opushnyev, Ghorbani, Dadkhah
- **Contribution**: **Comparative analysis of MCP, A2A, Agora, ANP** — protocol-level risks, threat modeling, risk assessment, secure deployment guidance
- **Why canonical**: Cross-protocol comparison gives us positioning info
- **Build target**: F12 (informs design choices for our gateway)

## Aggregate Insight

These 7 papers together establish that MCP security is an **active research discipline with a published threat catalogue of 31+ attacks** and several proposed defense architectures (MCPGuard, MCPSafetyScanner, SMCP, DASGuard). F13 (`seraph-mcp-vuln-scanner`) is no longer speculative — there is a citation chain to draw from.

## How to Use This Index

- When planning F13: read MCP02 (31 attacks) + MCP04 (MCPGuard architecture) first
- When designing F12: read MCP05 (SMCP) + MCP07 (cross-protocol comparison)
- When threat-modeling our gateway: read MCP06 (STRIDE/DREAD for MCP)
- For research arguments / canon promotion: cite MCP01 (foundational paper)

## GCS Status

Papers are **NOT** mirrored to GCS (arXiv hosts authoritative versions). Use HuggingFace MCP tools or `arxiv.org/abs/{id}` to fetch on demand.

## Related Folders

- `security/agentic-academic/` — broader agentic AI security literature (parent of MCP topic)
- `security/owasp/` — has OWASP Top 10 for Agentic Applications 2026 + LLM Top 10 v2.0
- `security/mitre/` — likely has MITRE ATLAS for LLM/agent attack matrix
