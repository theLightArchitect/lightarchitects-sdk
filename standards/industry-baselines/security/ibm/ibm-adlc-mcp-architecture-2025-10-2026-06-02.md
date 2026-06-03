---
name: ibm-adlc-mcp-architecture-2025-10
description: IBM Architecting Secure Enterprise AI Agents with MCP — Anthropic-verified guide (Oct 2025); ADLC 6-phase model + MCP Gateway pattern + 4 agent-specific threat classes
type: reference
authority_rating: HIGH
source_org: IBM Corporation
verified_by: Anthropic
published: 2025-10
accessed: 2026-06-02
pages: 21
gcs: "gs://la-platform-helix/user/standards/industry-baselines/security/agent-and-pentest/ibm-guide-to-architecting-secure-enterprise-ai-agents-with-mcp-techxchange-2025-1.pdf"
local: "/Users/kft/Downloads/ibm-guide-to-architecting-secure-enterprise-ai-agents-with-mcp-techxchange-2025-1.pdf"
related_helix_entries:
  - "[[2026-06-02-7076dccc-research-security-domain-upgrades-roadmap]]"
canon_promotion_candidates:
  - "Canon XLVII (Champion-Challenger)"
  - "Canon XLVIII (Reproducible Signed Manifest)"
  - "Canon XLIX (Three Eval Types)"
---

# IBM: Architecting Secure Enterprise AI Agents with MCP

**Published**: IBM Corporation, October 2025
**Verified by**: Anthropic
**Pages**: 21

## Why This Is Canonical

- **Anthropic verification** — endorsed by MCP's originating organization
- **Architectural alignment** — describes the exact pattern we implement (MCP Gateway + sibling MCP servers + agent identity)
- **Pre-publication review for LASDLC v2.x** — validates our 6-phase model and gate vocabulary
- **Enterprise risk framework** — names 4 agent-specific threat classes not covered by traditional AppSec

## Key Sections Indexed for Citation

| Section | Page | What it answers |
|---------|------|----------------|
| What are AI Agents? + Paradigm Shift | 1-2 | Deterministic→probabilistic, static→adaptive, code-first→evaluation-first |
| Agent Development Lifecycle (ADLC) | 3-7 | 6-phase model with two nested feedback loops |
| Plan / Code & Build / Test & Release | 4-5 | Shift-left security, hybrid models, evaluation-first KPIs |
| Deploy | 6 | Sandboxing as baseline control; hybrid cloud; kill switches |
| Operate / Monitor | 6-7 | 4-category metrics, drift detection, RCA-driven remediation |
| Enterprise Considerations | 7-8 | When to build agents; 3 proven application areas |
| Agent Observability | 9-10 | 4 analytical lenses on traces; offline/online/in-the-loop evals |
| Agent Security | 10-11 | 4 threat classes; security solution framework; SBOM mandate |
| Governance: Test, Certify & Catalog | 11-12 | Reproducible manifest; champion-challenger; provenance |
| MCP Servers Lifecycle | 13-17 | MCP Gateway pattern, OAuth/scopes, sandbox, tooling discipline |
| Reference Architecture | 18-19 | 4-phase platform (Build/Deploy/Monitor/Manage) + governed catalog + security/governance foundation |
| Voice of the Customer | 20-22 | Healthcare/Telecom/Financial examples (HIPAA, SOX/PCI implications) |

## Threat Classes Catalogued (p.10)

1. **Uncontrolled agent access and privilege escalation** — agents autonomously escalate privileges to bypass approval
2. **Agent-enabled data leakage and prompt exploitation** — nondeterministic responses leak info
3. **Autonomous attack amplification** — agents outpace defenses, coordinate rapid distributed attacks
4. **Agentic drift and noncompliance** — gradual algorithmic shifts evade monitoring

## Security Solution Framework (p.11)

1. **Agent identity and access** — JIT credentials, context-aware access, audit trails
2. **Agent and data protection** — gateway patterns to filter prompts, monitor info flow
3. **Autonomous agent defense** — proactive threat hunting, AI-based mitigation
4. **Agent security risk and compliance** — risk assessment integrated into ADLC

## MCP Gateway Mandate (p.14)

Minimum responsibilities: identity and scope brokering, catalog/registry, routing, health checks, rate limits and quotas, policy enforcement, audit and metrics, emergency kill switches.

**Reference implementation**: `mcp-context-forge` at github.com/IBM/mcp-context-forge — Python OSS gateway that federates MCP, A2A, REST/gRPC.

## Reference Architecture Components (p.18)

- **Build phase**: Agent Framework, CI/CD Pipeline, Synthetic Data Generator, Agent Eval Toolkit, Red-Team Simulation Agents, Prompt Tuning Service
- **Deploy phase**: AI Gateway, Agent Orchestration Engine, Managed MCP Server Cluster, Model Serving Platform, Guardrails Service, Agent Eval Service
- **Monitor & Optimize phase**: Metrics & Tracing Service, Drift & Anomaly Detector, Agent Insights Dashboard, Shadow AI Detector, SLO Management Service, Agent Optimization Service
- **Manage phase**: Compliance Audit, Certification Manager, Policy Management, Regulation Compliance Engine, Risk Management Platform, Secure Retirement Service
- **Foundational layers**: Governed Catalog (agent/tool/prompt/model registries) + Security & Governance (Policy Engine, Identity & Access, Compliance/DLP, Audit & Lineage, Secrets/KMS, Risk Registry/SBOM)

## Quick Build Checklist (p.17) — Adoptable Verbatim

- **Purpose and scope**: single, clearly defined server role and bounded toolset
- **SDK and spec**: official SDK where possible; document SDK and spec versions
- **Security**: OAuth scopes, least-privilege tools, approvals for high-risk actions, secrets in a manager
- **Validation**: strong input schemas, output sanitization, error taxonomy, idempotent retries
- **Operations**: health/readiness, rate limits, backpressure, circuit breakers, basic SLOs
- **Observability**: structured audit logs, metrics, tracing, correlation IDs
- **Compatibility**: versioned tool schemas, deprecation policy, feature detection, contract tests
- **Packaging**: minimal signed container, non-root runtime, reproducible builds
- **Docs**: README with capabilities and tags, environment variables, runbooks, changelog
