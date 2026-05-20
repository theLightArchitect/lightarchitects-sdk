<!-- uuid: f8e3b2a1-9c74-4d5e-a6f0-3b8e1c7d2f94 -->
<!-- gate: [S] primary · [C] secondary -->

---
title: "Security Guardrails"
version: "1.2.0"
type: canon
author: Kevin Francis Tan
created: 2026-05-12
ratified: 2026-05-12
ratified_by: kevin
schema_version: "1.0"
canon_uri: "canon://security-guardrails"
gate: "[S]"
gate_owner: seraph
gate_enforcer: corso
gate_auditor: laex
supersedes:
  - "builders-cookbook.md §40 (Pentest Engagement Standards — moved here as Part VIII)"
  - "builders-cookbook.md §12 policy half (Supply Chain policy — moved here as Part VI)"
sources:
  - ".firecrawl/owasp-llm-top10-2025.md"
  - ".firecrawl/owasp-agentic-security.md"
  - ".firecrawl/owasp-genai.md"
  - ".firecrawl/owasp-proactive-controls.md"
  - ".firecrawl/owasp-secure-coding-playbook.md"
  - ".firecrawl/owasp-cheatsheet-index.md"
  - ".firecrawl/nist-ai-rmf.md"
  - ".firecrawl/google-saif.md"
  - ".firecrawl/google-saif-risks.md"
  - ".firecrawl/ptes-main.md"
  - ".firecrawl/ptes-technical.md"
  - ".firecrawl/atomic-red-team.md"
  - "research/protectai-security-mapping.md"
  - "research/deterministic-ai-orchestration.md"
  - "industry-baselines/security/mitre/mitre-atlas-2026-05-04.md"
  - "industry-baselines/security/owasp/owasp-api-security-top-10-2023-2026-05-04.md"
  - "industry-baselines/security/owasp/owasp-asvs-v5.0.0-2026-05-12.md"
  - "industry-baselines/security/nist/nist-sp-800-63-4-2026-05-12.md"
  - "industry-baselines/security/nist/nist-sp-800-63b-4-2026-05-12.md"
  - "industry-baselines/security/nist/nist-csf-v2.0-2026-05-04.md"
  - "industry-baselines/security/openssf/slsa-levels-v1.2-2026-05-12.md"
  - "industry-baselines/security/eu/eu-ai-act-2026-05-04.md"
  - "industry-baselines/security/eu/gdpr-article-25-2026-05-04.md"
  - "industry-baselines/security/linddun/linddun-2026-05-04.md"
  - "industry-baselines/security/owasp/owasp-llm-prompt-injection-cheatsheet-2026-05-05.md"
---

# Security Guardrails

> *"Thou shalt not bow down thyself to them, nor serve them: for I the LORD thy God am a jealous God."* — Exodus 20:5 (KJV)
> No system, no agent, no external dependency is to be trusted unconditionally. Every component earns trust through verification.

**Purpose**: The platform-level security policy for Light Architects. Every domain — code, plans, operations, agents, data, infrastructure, workflows — operates under these guardrails. SERAPH owns the `[S]` gate and scans against this document. Any agent performing a security assessment begins here.

**Who this serves**: Every builder on the platform. These guardrails are not internal-only doctrine — they are the minimum security posture any application built on LA tooling should achieve. Follow them and your security story is defensible.

---

## Canonical Suite

| Document | URI | Purpose |
|---|---|---|
| **Security Guardrails** *(this doc)* | `canon://security-guardrails` | Platform-level security policies · all domains |
| Platform Canon | `canon://platform-canon` | Constitutional principles (Canon I, VII, VIII, XVI) |
| Builders Cookbook | `canon://builders-cookbook` | Code-level security implementation (§10 AppSec, §12 CI detail) |
| Agents Playbook | `canon://agents-playbook` | Agent trust model, post-agent ground truth (§5.4) |
| Architects Blueprint | `canon://architects-blueprint` | [S] gate scoring criteria (Part XIV C1–C8 rubric) |
| Operators Manual | `canon://operators-manual` | Secret-leak procedure (§7.1), deployment gates (§3.3) |
| LASDLC Template | `canon://lasdlc-template` | [S] gate in every phase boundary |

---

## Part I — Security Philosophy & Threat Model

### §1.1 Platform Security Philosophy

Four principles govern every security decision on this platform:

**Least Privilege by Default** — every agent, process, service, and user starts with zero permissions. Access is granted explicitly and scoped to the minimum required for the task. Unused permissions are revoked automatically after TTL expiry. There is no "admin by default."

**Defense in Depth** — no single control is relied upon. Every layer assumes the layer above it has been compromised. Code-level controls exist even when network controls are in place. Sandboxing exists even when auth controls are in place. Every control reduces blast radius independently.

**Verify, Never Trust** — credentials, tokens, agent outputs, tool call results, and external data are all untrusted at the point of receipt. Validation happens at every boundary crossing. Canon VIII: *Validate at the boundary, trust within.*

**Fail Secure** — when a control fails, it fails closed. A crashed auth service means no access, not open access. A timed-out scope check means the operation is denied. An unavailable CVE database means the scan is queued, not skipped.

### §1.2 Platform Attack Surface Inventory

| Surface | Attack Vectors | Owner |
|---|---|---|
| MCP stdio boundary | JSON-RPC injection, oversized payloads, malformed tool calls | CORSO |
| Agent prompts | Prompt injection, jailbreak, system prompt leakage | SERAPH |
| Tool call results | Indirect prompt injection, data exfiltration via tool output | SERAPH |
| Rust dependencies | CVE in upstream crates, supply chain compromise | CORSO + cargo-audit |
| WASM/inline agents | Sandbox escape, resource exhaustion, capability abuse | SERAPH |
| SOUL vault | Path traversal, unauthorized read/write, wikilink injection | CORSO |
| Neo4j graph DB | Cypher injection, authentication bypass, data exfiltration | CORSO |
| ElevenLabs TTS | API key leakage, content injection in audio pipeline | SERAPH |
| Git repositories | Secret commit, history rewrite bypass, malicious hook | CORSO |
| Network egress | Data exfiltration, C2 callback, DNS exfiltration | SERAPH |
| CI/CD pipeline | Build-time injection, artifact tampering, env var leakage | CORSO |
| Keychain / secrets store | Credential theft, ACL bypass, key rotation lag | SERAPH |

### §1.3 Risk Classification

| Tier | CVSS Range | Active Exploit? | SLA | Escalation |
|---|---|---|---|---|
| **CRITICAL** | 9.0–10.0 | Either | **24 hours** | Immediate HITL + rotate + patch |
| **HIGH** | 7.0–8.9 | Either | **7 days** | HITL within 1 day |
| **MEDIUM** | 4.0–6.9 | No | **30 days** | Tracked in build board |
| **LOW** | 0.1–3.9 | No | **90 days** | Batch patch cycle |
| **INFO** | 0.0 | No | Next release | Best effort |

**CISA KEV override**: any vulnerability appearing in the CISA Known Exploited Vulnerabilities catalog is automatically elevated to CRITICAL regardless of CVSS score. CISA lists only vulnerabilities being actively exploited in the wild. CVSS scores lag reality; KEV listing does not.

### §1.4 Security Ownership

| Role | Sibling | Responsibility |
|---|---|---|
| Primary gate owner | SERAPH | Red team, pentest, scan, scope governance |
| Enforcement | CORSO | AppSec code review, supply chain, GUARD pre-commit |
| Canon auditor | LÆX | Guardrail compliance in [S]+[C] gate; HMAC verification key custodian |
| Observability | AYIN | Security signal collection, anomaly detection |
| Research | QUANTUM | CVE triage, threat intelligence, forensics |

### §1.5 Security Exception Process

When a control in this document genuinely cannot be met (legacy dependency, third-party constraint, timeline pressure), an exception may be requested. Exceptions are never silent — every exception is tracked, time-bounded, and carries a compensating control.

**Exception request** (logged to helix under `seraph/exceptions/YYYY-MM-DD-<control-id>.md`):

| Field | Requirement |
|---|---|
| Control ID | Section + brief name (e.g., §4.2 CRITICAL patch SLA) |
| Requester | Name + sibling context |
| Business justification | Why the control cannot currently be met |
| Compensating control | What reduces the risk in lieu of the full control |
| Expiry date | Maximum 90 days; must be re-reviewed before expiry |
| Approval authority | CRITICAL/HIGH controls: SERAPH + Kevin; MEDIUM/LOW: SERAPH alone |

**Rules**:
- No exceptions backdated; all exceptions prospective only
- Compensating control must reduce effective risk tier by at least one level
- All active exceptions reviewed at each LASDLC [S] gate
- Expired exceptions without renewal = control violation (automatic MEDIUM finding)
- **Zero exceptions allowed for**: §2.6 multi-agent trust chain invariant, C3 seccomp `execve` block, §4.2 CRITICAL/HIGH patch SLAs, §10.2 HMAC audit log chain integrity

---

## Part II — Agentic & AI Security

### §2.1 OWASP LLM Top 10 2025 — Platform Stance

| Risk | ID | LA Platform Control |
|---|---|---|
| Prompt Injection | LLM01 | Structured tool calls over free-text where possible; all user input treated as untrusted; output re-validation before action execution |
| Sensitive Information Disclosure | LLM02 | No PII or credentials in prompts; vault access via typed API (not raw text injection); privacy tier gates on SOUL entries |
| Supply Chain Vulnerabilities | LLM03 | Model provenance checked (SHA-256 + source registry); no anonymous model downloads; AIBOM required for fine-tuned models |
| Data and Model Poisoning | LLM04 | Training data integrity checks (§9.3); helix ingestion validates source provenance; no unvetted external data into fine-tuning |
| Improper Output Handling | LLM05 | All LLM output treated as untrusted string; never eval'd, never executed raw; schema validation before downstream use |
| Excessive Agency | LLM06 | ScopeGovernor 5-gate enforced on all agents (TTL · target · tool · concurrent · domain); HITL required for irreversible actions |
| System Prompt Leakage | LLM07 | System prompts versioned as code (not runtime secrets); do not contain credentials; treat as semi-public; output filtering layer detects verbatim or near-verbatim system prompt content in responses and blocks disclosure (SERAPH red team test case in §8.3) |
| Vector and Embedding Weaknesses | LLM08 | Embedding inputs sanitized; adversarial embedding detection in SOUL helix ingestion pipeline; index integrity checks |
| Misinformation | LLM09 | RAG must cite or return Unknown (Cookbook §6.4); no ungrounded assertions in agent outputs used for decisions |
| Unbounded Consumption | LLM10 | Token budgets enforced per request; rate limiting at gateway; cost circuit-breaker at $10 HITL threshold |

### §2.2 OWASP Top 10 for Agentic Applications 2026

| Risk | ID | LA Platform Control |
|---|---|---|
| Prompt Injection (Agentic) | A1 | Tool output re-validation; no chained agent execution without schema gate; indirect injection via tool results blocked |
| Excessive Agency | A2 | Same as LLM06; ScopeGovernor is the mechanical enforcement layer |
| Unsafe Actions | A3 | All destructive/irreversible actions require HITL (AskUserQuestion); no autonomous file deletion, git force-push, or process kill |
| Inadequate Human Oversight | A4 | HITL gates at every phase boundary; agent-reported results are hypotheses until Claude verifies (Playbook §5.4) |
| Trust and Identity Issues | A5 | Agents authenticate via signed tokens; no ambient authority; agent identity declared in system prompt and verified at gateway |
| Memory Poisoning | A6 | SOUL helix writes validated; no agent can overwrite another's memory without explicit cross-sibling authorization |
| Agentic Loop Failures | A7 | Recursion termination invariant (depth ≤7, inode-keyed visited set — Playbook Part XVIII §8.7) |
| Resource Management | A8 | cgroups v2 memory/CPU limits per agent process; fuel metering in WASM; token budget per session |
| Confidentiality & Privacy Violation | A9 | Privacy tier enforced at vault read; no confidential helix entries in agent context without explicit grant |
| Adversarial Robustness | A10 | Adversarial input testing in CI; SERAPH red team exercises cover adversarial prompt variants |

### §2.3 Google SAIF 15 Risks — Platform Controls

| SAIF Risk | LA Control |
|---|---|
| Data Poisoning | Provenance validation on all training inputs; helix entry source tracking |
| Unauthorized Training Data | License gate on all training datasets (§6.3); no scraped data without attribution check |
| Model Source Tampering | SHA-256 verification on all model downloads; no anonymous model sources |
| Excessive Data Handling | Data minimization policy; PII stripped before training (§9.2) |
| Model Exfiltration | Model weights not stored in accessible vault directories; egress filtering |
| Model Deployment Tampering | Codesign on deployed binaries (macOS Gatekeeper + SHA verify); deployment integrity log |
| Denial of ML Service | Rate limiting + circuit breaker; graceful degradation to cached responses |
| Model Reverse Engineering | No model weights in public repos; access-controlled model registry |
| Insecure Integrated Component | sonatype-guide + cargo-audit on all deps; MCP server vetting before install |
| Prompt Injection | As per LLM01/A1 above |
| Model Evasion | Adversarial robustness testing in CI; SERAPH maintains evasion test suite |
| Sensitive Data Disclosure | Privacy tiers (§9.1); no credentials in model context |
| Inferred Sensitive Data | PII inference risk assessment before training; differential privacy where applicable |
| Insecure Model Output | Output schema validation; no raw model output executed |
| Rogue Actions | ScopeGovernor TTL + domain gate; all irreversible actions require HITL |

### §2.4 MCP Server Security

MCP is the primary tool surface for all LA agents. Treat every MCP server as a trust boundary.

**Third-party MCP servers**: review source before install; pin to specific commit hash; run in sandboxed process with network egress filtering; validate all tool schemas against expected types.

**Internal MCP servers**: authenticate via signed tokens on non-local transports; validate JSON-RPC payload size (max 8 KiB default, configurable per tool); reject malformed tool call parameters at the boundary before dispatching to handler.

**Tool result handling**: never pass MCP tool results directly to another tool call without validation; treat tool results as untrusted input from the perspective of the consuming agent (indirect prompt injection vector).

**OWASP API Security Top 10 2023 — MCP/Tool Surface Stance**:

| Risk | ID | LA Tool Surface Control |
|---|---|---|
| Broken Object Level Authorization | API1 | Tool calls validate that caller's active scope authorizes access to the specific target object — not just the tool type |
| Broken Authentication | API2 | §3.5 / §7.1 — per-callee signed tokens; no ambient session auth for tool calls |
| Broken Object Property Level Authorization | API3 | Tool parameter schema validated against caller's declared scope; fields not authorized are stripped at gateway |
| Unrestricted Resource Consumption | API4 | cgroups v2 CPU/memory limits (§5.1); token budget per session; WASM fuel limit (§5.2) |
| Broken Function Level Authorization | API5 | ScopeGovernor tool gate — tools outside declared allowlist are rejected at invocation |
| Unrestricted Sensitive Business Flows | API6 | HITL required for irreversible actions (§2.2 A3); no autonomous destructive operations |
| Server-Side Request Forgery | API7 | §3.1/A10 egress allowlist; private IP ranges blocked; §5.4 deny-by-default egress |
| Security Misconfiguration | API8 | Hardened defaults; no debug endpoints in production (§5); tool schema strictly typed |
| Improper Inventory Management | API9 | MCP server inventory maintained; third-party servers reviewed before install (§6.1 equivalent for tools) |
| Unsafe API Consumption | API10 | All tool results treated as untrusted strings; schema validation before downstream use (§2.4 tool result handling) |

**IPC Bridge Security — ProtectAI Mandatory Finding**: Any bridge between a Rust binary and an external process (sidecar, plugin runner, language bridge) MUST use a **Secure IPC Sidecar** pattern:
- **Prohibited**: passing secrets (`api_key`, tokens, passwords) as command-line arguments — they are visible to any process via `ps aux`, audit logs, and shell history
- **Prohibited**: per-call `std::process::Command` spawning for cryptographic or sensitive operations — performance overhead compounds and each spawn is an `execve` call requiring seccomp exception (§5.1)
- **Required**: persistent sidecar service; Unix Domain Sockets (UDS) on macOS/Linux or Named Pipes on Windows; secrets passed via stdin or shared memory with explicit lifetime, never CLI args
- **mTLS option**: if TCP is required, bind strictly to `127.0.0.1` and enforce Mutual TLS using platform-issued certificates

Source: `research/protectai-security-mapping.md` §"Critical Security Alert: The DEP Bridge Implementation Flaw".

**Source**: OWASP Agentic Security Initiative — *Practical Guide for Secure MCP Server Development* (2026-02). Cached: `.firecrawl/owasp-agentic-security.md`.

### §2.5 Agent Sandboxing — Mandatory Controls

Every agent execution environment must enforce:

| Control | Mechanism | Non-Negotiable? |
|---|---|---|
| Scope TTL | ScopeGovernor TTL gate — engagement expires, no open-ended sessions | Yes |
| Target allowlist | ScopeGovernor target gate — no scanning/accessing unlisted targets | Yes |
| Tool allowlist | ScopeGovernor tool gate — no invoking tools outside declared scope | Yes |
| Concurrency cap | ScopeGovernor concurrent gate — max parallel agent threads per engagement | Yes |
| Domain boundary | ScopeGovernor domain gate — no cross-domain tool calls without explicit grant | Yes |
| Recursion depth | Playbook Part XVIII §8.7 — depth ≤7, inode-keyed visited set | Yes |
| Memory cap | cgroups v2 — max 2 GiB RAM per agent process (configurable, not removable) | Yes |
| CPU cap | cgroups v2 — max 200% CPU (2 cores) sustained per agent | Yes |
| Network egress | Allowlist-only outbound; deny-by-default; logged | Yes |
| Filesystem | Read-only mount except declared write paths; no `/proc`, `/sys`, `/dev` access | Yes |
| Chain trust scope | §2.6 invariant — originating human scope verified at every hop; scope monotonically reduces | Yes |

### §2.6 Multi-Agent Trust Chain Policy

Multi-agent orchestration introduces a class of vulnerability absent in single-agent systems: transitive authority escalation. When Agent A invokes Agent B which invokes Agent C, each hop can inherit, amplify, or launder the calling agent's authority. This section closes that gap.

**Monotonic scope reduction invariant**: as a call propagates through a chain, scope can only decrease, never increase. Agent C cannot be granted permissions that Agent A did not have. ScopeGovernor carries the originating scope claim through the full chain; receiving agents verify against the originating claim, not just the immediate caller's token. This invariant has **zero exceptions** (§1.5).

**Per-hop authentication requirements**:
- Every agent in a chain independently authenticates the immediate caller via signed token
- Tokens carry an `aud` (audience) claim scoped to the specific callee — a token issued for Agent B cannot authenticate to Agent C
- Tokens carry a `chain_depth` claim (incremented per hop) and a `chain_origin` claim (originating human session ID)
- Receiving agent rejects any token where `chain_depth > 7` (aligns with recursion termination invariant)

**Originating scope verification**:
- The originating human session scope is embedded in the root token and signed by the gateway at session start
- Every agent in the chain verifies the root scope claim independently — not just that the immediate caller authorized the call, but that the original human scope permits the specific operation being requested
- If the root scope claim is absent or unverifiable, the call is denied (fail secure)

**Chain logging requirements**:
- AYIN logs every inter-agent call with: caller identity, callee identity, chain depth, originating session ID, scope at time of call
- Any call chain where an agent's effective permissions exceed the originating human session scope is a **CRITICAL** security event

**Implementation**: `ScopeGovernor` 5-gate applies at every hop; `chain_origin` propagates through the full call graph via token claims in the `lightarchitects-sdk` auth layer.

**WASM-hosted agents** additionally enforce:
- Fuel metering: max 10B Wasm instructions per invocation (wasmtime `Config::consume_fuel`)
- Memory cap: 256 MiB linear memory (wasmtime `Config::max_wasm_stack` + `limits.memory_pages`)
- Capability-based I/O: WASI capabilities declared at spawn; no ambient filesystem or network access
- No `wasm_backtrace_details` exposure in production (information disclosure)

### §2.7 MITRE ATLAS — AI/ML Adversary Tactics (v4.5)

MITRE ATLAS (Adversarial Threat Landscape for AI Systems) extends ATT&CK to ML-specific attack vectors. The LA platform's AI/agent surface maps to 16 ATLAS tactics. SERAPH red team exercises reference ATLAS technique IDs.

**High-priority ATLAS techniques for the LA platform**:

| Tactic | Key Techniques | LA Surface | Control |
|---|---|---|---|
| Reconnaissance | AML.T0000 (Search for Victim's Publicly Available AI Resources) | Published model cards, API endpoints | Minimize disclosed model detail; no capability enumeration in public docs |
| Resource Development | AML.T0010 AI Supply Chain Compromise (hardware/software/data/model/registry/tool) | Dependency chain, model downloads | §6 Supply Chain — sonatype-guide + RustSec + SHA-256 model verify |
| Initial Access | AML.T0052 Phishing (incl. Deepfake-Assisted); AML.T0065 LLM Prompt Crafting | Agent input boundary | §3.4 input validation; §2.1 LLM01 prompt injection controls; out-of-band channel policy (§9.1) |
| AI Model Access | AML.T0040 (ML Model Inference API Access) | Anthropic/ElevenLabs/HuggingFace API | Rate limiting; API key scoping; §5.4 egress allowlist |
| Execution | AML.T0065 LLM Prompt Crafting; AML.T0066 Retrieval Content Crafting (RAG poisoning) | Agent prompt + SOUL helix | Output re-validation (§2.1 LLM05); helix ingestion validates source provenance (§2.1 LLM04) |
| Persistence | AML.T0020 Poison Training Data | Fine-tuning pipeline | §9.3 training data security; statistical outlier detection before training run |
| Privilege Escalation | Multi-agent trust chain abuse | Agent orchestration | §2.6 monotonic scope reduction invariant; chain_origin verification |
| Defense Evasion | AML.T0015 Evade ML Model; encoding/obfuscation attacks | Input boundary | Semantic analysis layer; encoding normalization at input (UTF-8 validation §3.4) |
| Credential Access | AML.T0056 LLM Jailbreak; system prompt extraction | Agent system prompts | §2.1 LLM07 system prompt leakage control; output filtering |
| Collection | AML.T0037 Data from Information Repositories | SOUL vault, Neo4j | Privacy tiers (§9.1); RBAC on vault read; §3.6 Neo4j hardening |
| AI Attack Staging | AML.T0066 Retrieval Content Crafting | RAG pipeline (SOUL helix RRF) | Embedding input sanitization (§2.1 LLM08); adversarial embedding detection |
| Command and Control | Exfiltration via LLM output (data encoded in model responses) | Model output | §5.4 egress logging; output schema validation; no raw model output executed |
| Exfiltration | AML.T0024 Exfiltration via ML Inference API | Inference calls | Token budget enforcement; rate limiting; AYIN egress signal |
| Impact | AML.T0020 Poison Training Data; AML.T0029 Denial of ML Service | Training pipeline, API | Circuit-breaker; graceful degradation; §9.3 provenance chain |

**Source**: `industry-baselines/security/mitre/mitre-atlas-2026-05-04.md` (ATLAS v4.5, cached 2026-05-04).

---

## Part III — Code & Application Security

### §3.1 OWASP Top 10 Platform Stance (2021)

| Risk | LA Mandatory Control |
|---|---|
| A01 Broken Access Control | RBAC enforced at every API boundary; no client-side access decisions |
| A02 Cryptographic Failures | §3.5 approved algorithm list; no MD5/SHA1/DES; TLS 1.3 minimum |
| A03 Injection | Parameterized queries only (Neo4j Cypher `$param`); no string-concat SQL/Cypher; input validated at boundary |
| A04 Insecure Design | Threat model (STRIDE) required for every new data flow; LINDDUN privacy threat model (Linking · Identifying · Non-repudiation · Detecting · Data Disclosure · Unawareness · Non-compliance) paired with STRIDE for any flow handling PII — both use the same DFD starting point, run in parallel (§9.2); §3.2 proactive controls applied by design |
| A05 Security Misconfiguration | Hardened defaults; secrets never in config files; no debug endpoints in production |
| A06 Vulnerable Components | §4 Vulnerability Management gates; no unpatched CRITICAL/HIGH deps in production |
| A07 Auth Failures | Session tokens: ≥256 bits entropy; short-lived (1h default); no long-lived tokens without refresh |
| A08 Software & Data Integrity | Artifact signatures verified at deploy; no unsigned binaries in production |
| A09 Logging Failures | §10 Security Monitoring — minimum log set is mandatory; no security events silently dropped |
| A10 SSRF | No user-controlled URLs fetched server-side without allowlist validation; DNS rebinding protection |

### §3.2 OWASP Proactive Controls (C1–C10) — Platform Requirements

| Control | Requirement Level | Implementation |
|---|---|---|
| C1 Define Security Requirements | MANDATORY | Security requirements documented in every LARGE/MEDIUM plan (Architects Runbook Part IV) |
| C2 Leverage Security Frameworks | MANDATORY | Use platform-approved crates only; no rolling own crypto |
| C3 Secure Database Access | MANDATORY | Parameterized queries; least-privilege DB user; no `neo4j` admin in app code |
| C4 Encode and Escape Data | MANDATORY | All untrusted data encoded at output; context-specific (HTML/JSON/Cypher) |
| C5 Validate All Inputs | MANDATORY | Validation at every system boundary; whitelist preferred over blacklist |
| C6 Implement Digital Identity | MANDATORY | Multi-factor for admin operations; PBKDF2/Argon2id for password storage |
| C7 Enforce Access Controls | MANDATORY | Deny-by-default; access checks server-side; no reliance on client-provided roles |
| C8 Protect Data Everywhere | MANDATORY | Encryption at rest (AES-256-GCM) and in transit (TLS 1.3); no plaintext credentials anywhere |
| C9 Implement Security Logging | MANDATORY | §10 security signals; immutable audit log; tamper-evident via HMAC signing |
| C10 Stop Server-Side Request Forgery | MANDATORY | Allowlist for all outbound requests; block private IP ranges (RFC 1918) |

### §3.3 Cryptography Standards

**Approved algorithms** (use only these):

| Purpose | Algorithm | Notes |
|---|---|---|
| Symmetric encryption | AES-256-GCM | No ECB mode; unique nonce per message |
| Asymmetric encryption | RSA-4096 or Ed25519 | Ed25519 preferred for new keys; RSA-4096 for legacy compat only |
| Key exchange | X25519 | No ECDH over NIST P-curves without review |
| Hashing (integrity) | SHA-256 / SHA-512 / BLAKE3 | BLAKE3 preferred for performance-sensitive paths |
| Password hashing | Argon2id | `m=65536, t=3, p=4` minimum params |
| MAC | HMAC-SHA256 | For SOUL audit trail signing (`config/audit_hmac_key`) |
| TLS | TLS 1.3 | TLS 1.2 permitted only for legacy compat with explicit approval; never 1.0/1.1 |
| Random | OS CSPRNG (`getrandom`) | Never `rand::thread_rng` seeded from time |

**Forbidden algorithms** (any use requires SERAPH sign-off and migration plan):
MD5 · SHA-1 · DES/3DES · RC4 · ECB mode · RSA < 2048 · ECDH P-192

**Post-quantum cryptography (PQC) roadmap**: NIST finalized FIPS 203 (ML-KEM) and FIPS 204 (ML-DSA) in August 2024. NVD and major tooling began publishing CVSS 4.0 AI/ML-aware scores in 2024. Platform PQC migration plan required before 2028. Any new key exchange or signing implementation must support algorithm agility (negotiation without code changes) to enable seamless PQC transition.

### §3.4 Input Validation Policy

Every value crossing a system boundary is untrusted until validated:

- **Type** — deserialize to a typed struct; reject unrecognized fields (serde `deny_unknown_fields`)
- **Range** — numeric values checked against domain bounds (not just `u64`)
- **Length** — string inputs have an explicit maximum; default 8 KiB for free-text, 256 bytes for identifiers
- **Character set** — identifiers validated against allowlist pattern; free-text sanitized of null bytes and control characters
- **Encoding** — UTF-8 validation before any string operation; reject overlong sequences
- **Business logic** — semantic validation in the domain layer (dates are valid, references exist, status transitions are legal)

Reject at the first failing check. Log the rejection with the field name but **never log the rejected value** (may contain PII or injection payload).

### §3.5 Authentication & Authorization

**Zero Trust model**: every request authenticated regardless of source. Internal service-to-service calls use signed tokens scoped per caller-callee pair (see §7.1), not implicit trust from network topology.

**Session policy**:
- Access tokens: 1 hour TTL, ≥256 bits entropy, signed with Ed25519
- Refresh tokens: 24 hour TTL, single-use, rotation on use
- API keys: no expiry by default but must be rotatable in <5 minutes; stored in OS keychain (not env vars, not config files)

**RBAC requirements**:
- Roles defined in code, not database (roles as code, not runtime config)
- Permission checks server-side at every handler; never rely on client-provided role claim alone
- Principle of least privilege: every agent/service/user gets the minimum role for the task
- Audit log every privilege escalation

**NIST SP 800-63B Authenticator Assurance Level (AAL) alignment** (superseded by SP 800-63-4, Aug 2025 — principles unchanged):
- **AAL1** — standard platform access (read-only helix, public APIs): single-factor auth with approved cryptography; reauthentication ≤30 days
- **AAL2** — privileged operations (production deploys, secret rotation, pentest engagement start, HITL approval for CRITICAL findings): two distinct authentication factors with approved cryptographic techniques; proof of possession of both factors required
- **AAL3** — not currently required; reserved for future HSM-backed signing operations

All interactions with Personal Identifiable Information (PII) require **minimum AAL2** regardless of operation type (NIST SP 800-63-3 §4 directive).

### §3.5.1 Webshell Two-Auth-Model Invariant

The webshell backend exposes two authentication models that must never be confused:

| Model | Header | Scope |
|-------|--------|-------|
| `X-LA-Notify-Token` | Custom header | Machine-to-machine only — gateway callback → webshell |
| `Authorization: Bearer <token>` via `auth::AuthGuard` | Standard | Operator-facing — browser-callable |

`notify_token` is deliberately excluded from `BuildResponse` (the JSON the browser receives at
session creation). The browser cannot obtain it at runtime. This is intentional:

- **CWE-522** (Insufficiently Protected Credentials) — machine credentials that reach client-side
  code can be extracted from memory, DevTools, or HAR files by any script running in the same
  browsing context.
- **OWASP API2:2023** (Broken Authentication) — using a credential designed for one authentication
  context (machine-to-machine) to gate a different context (human-facing action) is an
  authentication design failure, independent of whether an adversary is present today.

**Gate check ([S] gate — SERAPH or CORSO GUARD):** Before wiring any UI action or Svelte
component to a backend endpoint, verify: *"Can the browser obtain this credential at runtime?"*
If no → wrong auth model. Switch the handler to `auth::AuthGuard`.

This is a **BLOCKING** security finding — a browser-facing endpoint protected by machine-only
credentials is an authentication bypass by design.

### §3.6 Neo4j Hardening

Neo4j is in the platform attack surface inventory (§1.2) with Cypher injection, authentication bypass, and data exfiltration as primary risks. These controls are mandatory for every production Neo4j instance.

**Network binding**: Bolt protocol (`7687`) bound to `127.0.0.1` only — never exposed beyond localhost. HTTP management endpoint (`7474`) disabled in production. Docker Compose `ports` entries use `127.0.0.1:7687:7687` explicitly.

**Authentication**:
- Native auth enabled; `neo4j` admin account password changed from default at first launch
- Application connects as a dedicated non-admin user with minimum required Cypher privileges
- No application code connects as `neo4j` admin — credential verified by CORSO GUARD on every PR

**RBAC within Neo4j**:
- Application role: `READ` on required labels + `WRITE` on labels the application owns; no `MATCH (n)` queries without label filter
- `soul-consolidator` uses a separate role with write access to ingestion labels only
- No role is granted `ALL PRIVILEGES` except the admin account (used only for initial setup)

**Backup encryption**:
- All Neo4j backups encrypted with AES-256-GCM before storage
- Backup files never committed to git repositories
- Backup decryption key stored in OS keychain, not co-located with backup files

**Query safety**: Cookbook §3.1 mandate — all Cypher queries use parameterized `$param` placeholders; string concatenation into Cypher is forbidden; CORSO guard pattern-matches for raw string interpolation into Cypher at code review.

### §3.7 OWASP ASVS Verification Baseline

The OWASP Application Security Verification Standard (ASVS) v5.0.0 provides checkable pass/fail security requirements across 15 chapters. The LA platform targets **ASVS L1 as the minimum baseline** for all production code, with ASVS L2 for security-critical components (auth, cryptography, session management, access control).

| Level | Scope | LA Target |
|---|---|---|
| **L1** (Opportunistic) | Basic security — verifiable via black-box testing; stops opportunistic attacks | Minimum for all production services |
| **L2** (Standard) | Deep verification — requires source code access; stops most targeted attacks | Required for: auth flows, session management, crypto operations, RBAC, secret handling |
| **L3** (Advanced) | Highest assurance — critical systems processing high-value transactions | Not currently targeted; SERAPH pentest coverage provides equivalent assurance |

**Mandatory L2 chapters for the platform**: V1 Encoding/Sanitization · V2 Validation · V3 Web Frontend (Berean only) · V6 Cryptography · V7 Error Handling and Logging · V13 API and Web Service · V14 Config.

ASVS requirements are numbered `<chapter>.<section>.<requirement>` — use `v5.0.0-<req_id>` format when referencing in findings. Source: `industry-baselines/security/owasp/owasp-asvs-v5.0.0-2026-05-12.md`.

---

## Part IV — Vulnerability Management

### §4.1 CVE Database Registry

Agents performing vulnerability assessment MUST query all active databases. Stopping at one database gives false confidence.

| Database | Scope | Query Endpoint | Update Frequency | Priority |
|---|---|---|---|---|
| **CISA KEV** | Actively exploited vulns (any ecosystem) | `https://www.cisa.gov/sites/default/files/feeds/known_exploited_vulnerabilities.json` | Daily | **Check first** |
| **NVD (NIST)** | All CVEs with CVSS scores | `https://services.nvd.nist.gov/rest/json/cves/2.0` | Continuous | High |
| **OSV** | Open source (Rust, npm, Python, Go) | `https://api.osv.dev/v1/query` | Real-time | High |
| **RustSec Advisory DB** | Rust/Cargo specific | `cargo audit` → `https://github.com/RustSec/advisory-db` | Daily pull | High (primary for Rust) |
| **GitHub Advisory DB** | Cross-ecosystem, CWE-mapped | `https://api.github.com/graphql` (`securityAdvisories`) | Real-time | Medium |
| **Sonatype OSS Index** | Maven, npm, Rust, Python | `https://ossindex.sonatype.org/api/v3/component-report` | Daily | Medium |

**Query protocol**:
1. Check CISA KEV first — any match is automatic CRITICAL escalation
2. Run `cargo audit` (RustSec) — blocks CI on CRITICAL/HIGH
3. Run `cargo deny` (OSV + license) — blocks CI on policy violation
4. Query NVD for any dependency not in RustSec/OSV
5. Log all findings to AYIN with timestamp; never silently pass a scan

**Refresh TTL**: CVE database snapshots cached locally for max 24 hours. QUANTUM must re-query before any security assessment; cached results older than 24h are treated as stale.

### §4.2 Patch SLAs

From the moment a vulnerability is confirmed affecting a deployed dependency:

| Severity | Time to Patch | Time to Mitigate (if patch unavailable) | Auto-block deploy? |
|---|---|---|---|
| CRITICAL | 24 hours | Isolate/disable within 1 hour | Yes |
| HIGH | 7 days | Compensating control within 48 hours | Yes (after SLA breach) |
| MEDIUM | 30 days | Accepted risk with documented rationale | No |
| LOW | 90 days | Batch cycle | No |

**Mitigation** means a compensating control that reduces exploitability to MEDIUM or below while the patch is prepared (e.g., disabling the vulnerable endpoint, adding input validation, network ACL).

### §4.3 Zero-Day Protocol

When a zero-day (no CVE, no patch) affecting the platform is disclosed:

1. **Assess blast radius** (SERAPH) — does it affect production paths?
2. **Isolate within 2 hours** if blast radius is CRITICAL
3. **HITL escalation** to Kevin immediately
4. **Document in helix** as an incident entry (Part XI §11.2)
5. **Monitor CISA KEV** — zero-days often appear within 48h of disclosure
6. Do not wait for NVD assignment (NVD lag can be weeks); trust researcher disclosure + PoC

---

## Part V — Sandboxing & Isolation

### §5.1 OS-Level Process Isolation

For any process executing untrusted or semi-trusted code (agent subprocesses, plugin runners, build steps):

**seccomp-bpf** — syscall allowlist (not denylist). Default allow list for Rust/tokio binaries:
`read, write, open, close, stat, fstat, mmap, mprotect, munmap, brk, rt_sigaction, rt_sigprocmask, ioctl, pread64, pwrite64, readv, writev, pipe, select, sched_yield, mremap, msync, futex, clone, wait4, exit_group, getcwd, chdir, fchdir, epoll_*`

**`execve` is explicitly excluded**: permitting `execve` inside a sandboxed agent process is a sandbox escape vector — it allows spawning arbitrary new processes outside every cgroup/namespace control. Rust binaries compiled for async I/O (tokio) do not call `execve` at runtime. Any process that requires `execve` must obtain a named exception with SERAPH sign-off, document the specific binary, and justify why it cannot be refactored to avoid process spawning.

Any syscall outside this list requires explicit justification and SERAPH approval.

**Linux namespaces** — new process gets:
- `pid` — isolated PID namespace; cannot signal host processes
- `net` — isolated network namespace; egress via controlled veth pair only
- `mnt` — isolated mount namespace; read-only root with explicit write mounts
- `user` — non-root inside namespace (uid_map: container uid 0 → host uid 65534)
- `ipc` — isolated IPC namespace

**cgroups v2 limits** (applied to every agent/plugin process):
```
memory.max = 2147483648   # 2 GiB
memory.swap.max = 0       # No swap
cpu.max = 200000 1000000  # 200% CPU (2 cores)
pids.max = 64             # tokio runtime needs ~32 threads max; 512 provides fork-bomb headroom
```

### §5.2 WASM Sandboxing (wasmtime)

For WASM-hosted agents and inline handlers:

```rust
// Mandatory wasmtime configuration
let mut config = Config::new();
config.consume_fuel(true);            // Enable fuel metering
config.max_wasm_stack(1 << 20);       // 1 MiB stack
config.wasm_backtrace_details(WasmBacktraceDetails::Disable); // No info disclosure

let engine = Engine::new(&config)?;
let mut store = Store::new(&engine, ());

// Set fuel limit (10 billion instructions ≈ ~10s at WASM execution speed)
// Compute-intensive agents may request higher limits via §1.5 exception process
store.set_fuel(10_000_000_000)?;

// Fuel exhaustion behavior: traps with OutOfFuel error (fail secure — never silent success)
// OutOfFuel is logged to AYIN with agent_id + task_id; the invocation fails cleanly

// Memory limits via ResourceLimiter
store.limiter(|_| &mut MyLimiter { memory_limit: 256 * 1024 * 1024 });

// WASI capabilities — declare only what's needed
let mut wasi = WasiCtxBuilder::new()
    .args(&args)?
    // Do NOT add: .inherit_env() .inherit_stdio() .preopened_dir(...)
    // unless explicitly required and reviewed
    .build();
```

**No capability by default**. Every capability (filesystem preopen, env var access, stdin/stdout) must be declared in the spawn config and justified.

### §5.3 Container Security

For any containerized workload:

- **Rootless** — run as non-root user (`USER 1000:1000` in Dockerfile); no `--privileged`
- **Read-only filesystem** — `docker run --read-only`; explicit `--tmpfs /tmp`
- **No new privileges** — `--security-opt=no-new-privileges:true`
- **Capability dropping** — `--cap-drop ALL`; add back only what's needed (e.g., `--cap-add NET_BIND_SERVICE` for port <1024)
- **Image scanning** — scan image with `trivy` or `grype` before deployment; block on CRITICAL/HIGH
- **Image provenance** — pin to SHA digest (`image@sha256:...`), not mutable tag
- **Network** — default bridge network; no `--network=host`; explicit port exposure only

### §5.4 Network Egress Policy

Default: **deny all outbound**. Explicit allowlist required.

| Destination | Allowed? | Justification |
|---|---|---|
| Anthropic API (`api.anthropic.com:443`) | Yes | Core platform function |
| HuggingFace API (`huggingface.co:443`) | Yes | Model downloads |
| ElevenLabs API (`api.elevenlabs.io:443`) | Yes | TTS |
| GitHub API (`api.github.com:443`) | Yes | Advisory DB, PR ops |
| CISA KEV feed (`cisa.gov:443`) | Yes | Vulnerability management |
| NVD API (`services.nvd.nist.gov:443`) | Yes | CVE queries |
| OSV API (`api.osv.dev:443`) | Yes | CVE queries |
| RustSec advisory DB (`github.com/RustSec:443`) | Yes | Via cargo audit |
| Neo4j local (`localhost:7687`) | Yes | Graph DB |
| AYIN dashboard (`:3742`) | Yes | Observability |
| Private IP ranges (RFC 1918) | **No** | SSRF prevention |
| `169.254.169.254` (metadata) | **No** | Cloud metadata endpoint — SSRF |
| Any other destination | **Deny** | Requires SERAPH approval + allowlist entry |

All egress logged to AYIN with: timestamp, destination, bytes, agent identity, justification tag.

**DNS Security Requirements**:
- **DNS-over-HTTPS (DoH)**: platform processes making DNS queries use DoH (`1.1.1.1` or `8.8.8.8` with HTTPS) to prevent passive interception and DNS-based exfiltration detection bypass
- **DNS rebinding protection**: HTTP servers validate `Host` header against allowlist; reject requests with private IP `Host` headers; `127.0.0.1` is the only bound address for internal services (§3.6 Neo4j binding policy)
- **DNSSEC validation**: where applicable to operator DNS resolvers; ensures resolution integrity for platform external dependencies
- **DNS exfiltration monitoring**: AYIN flags unusually long DNS labels (>63 chars) or high-frequency subdomain variation as potential exfiltration signal (covert channel detection)

### §5.5 Secrets Isolation

- All secrets stored in OS keychain (macOS Keychain, Linux SecretService)
- Access via `SecretString` type — zeroized on drop (Rust `zeroize` crate)
- No secrets in environment variables in production (env vars are world-readable to processes in same namespace)
- No secrets in config files committed to git (trufflehog pre-commit hook)
- No secrets in log output (structured logging with field redaction)
- Secrets rotation: CRITICAL tier rotated within 1 hour of suspected compromise; scheduled rotation ≤90 days
- Keychain ACL: each secret scoped to specific binary only; no shared keychain entries across different services

### §5.6 Device Security (L1 Physical)

Physical security is the lowest layer of the OSI model and the hardest to monitor remotely. These controls govern every device used to develop, deploy, or operate the LA platform.

| Control | Requirement | Enforcement |
|---|---|---|
| **Full-disk encryption** | FileVault 2 (macOS) enabled on all development machines; recovery key stored in 1Password vault | Checked at developer onboarding; quarterly audit |
| **Screen lock** | Auto-lock ≤5 minutes inactivity; password required to unlock (not just Touch ID alone at initial unlock) | System Preference → Lock Screen |
| **USB/Thunderbolt policy** | Unknown USB devices not connected to machines with production access; Thunderbolt not used for untrusted devices | Manual policy; reported in post-incident reviews |
| **Firmware password** | macOS firmware password set on machines with keychain production secret access | Set at device provisioning |
| **Software updates** | OS and security updates applied within 7 days of release; CRITICAL patches within 24 hours | aligns with §4.2 HIGH patch SLA |
| **Stolen device response** | Remote wipe via Apple Find My within 1 hour of confirmed loss; credential rotation within 4 hours | Incident response SLA (§10.3 HIGH) |

**Rationale**: an unencrypted laptop with a stolen keychain is a CRITICAL platform compromise regardless of all logical controls above. Device security is the physical enforcement layer that makes every cryptographic control in this document meaningful.

---

## Part VI — Supply Chain Security

### §6.1 Dependency Acceptance Policy

Before any new dependency is added to `Cargo.toml` or `package.json`:

1. **Sonatype check** (`mcp__plugin_sonatype-guide__getRecommendedComponentVersions`) — no known CVEs in target version
2. **RustSec check** (`cargo audit`) — no advisories for crate
3. **License check** (`cargo deny`) — license must be in approved list: MIT · Apache-2.0 · BSD-2/3-Clause · ISC · MPL-2.0 · CC0
4. **Maintenance check** — last commit < 18 months; issues actively triaged; not archived
5. **Downloads/trust check** — crates.io downloads > 100K or GitHub stars > 500 (for non-niche crates); OR explicit SERAPH review
6. **Transitive audit** — `cargo tree` to assess transitive dep count; > 20 new transitives requires SERAPH review

No dependency added without passing all six checks. Document the check result in the PR description.

### §6.1.1 Target-Repo Code Execution Surface

**Ratified**: 2026-05-17 (Kevin direct, via Canon XXXIX pipeline; LÆX RATIFY WITH AMENDMENT cleared).

**Classification rule**: Any dependency whose advertised function is to extract, analyze, or document arbitrary user repositories is classified **CRITICAL** — not "MEDIUM stability" — if its extraction path invokes any of:

- `cargo` (with `expand`, `rustdoc-json`, `rustdoc`, or `doc` subcommands)
- `cargo +nightly rustdoc` (executes target repo's `build.rs`)
- Any tool that runs target-repo code as a side-effect of metadata extraction

**Why this classification is needed**: `cargo audit` / `cargo deny` / RustSec are advisory-database-driven. They cannot catch by-design code execution surfaces — a dep that, by its stated function, executes target-repo code is not a CVE; it is the **threat vector itself**.

**Acceptance gate** (in addition to §6.1's six checks):

Before adopting any dep whose function is "extract/analyze/document arbitrary user repos", answer:

> *Does the extraction path invoke `cargo`, `cargo +nightly rustdoc`, `cargo expand`, or `cargo doc`?*

If **yes** → **CRITICAL**. Default verdict: **drop**. Mitigations require explicit operator HITL opt-in (e.g., `--trust-build-rs` flag with confirmation prompt) OR sandboxing (container-isolated extraction).

**Mechanical enforcement**: Cookbook §security includes a static-analysis lint that forbids `Command::new("cargo")` with `expand` / `rustdoc-json` / `doc` args in workspaces handling untrusted repos. See Cookbook §63.P1 (Untrusted-Input Operational Pattern P1 — `build.rs` ACE vector) for the operational counterpart.

**Anchors**: CWE-94 (Code Injection) · AML.T0010 (AI Supply Chain Compromise — referenced in §3 line 294).

**Pressure-tested**: `architecture-intelligence-substrate` SCRUM Round 1 (SERAPH CRITICAL B2, 2026-05-17). rustdoc-JSON was originally listed as "MEDIUM dependency-stability risk" in the plan; SERAPH's adversarial review reclassified it as CRITICAL ACE-on-host. The dep was dropped from MVP rather than mitigated; tree-sitter syntax extraction proved sufficient for L1–L3 facts.

**Cross-reference**: Cookbook §63.P1 holds the operational mitigation pattern (how to harden if a build.rs-touching tool is required). This canon holds the classification policy (when to refuse the dep before it enters Cargo.toml).

### §6.2 Software Bill of Materials (SBOM)

Every production build generates an SBOM:

```bash
# Rust (CycloneDX format)
cargo cyclonedx --format json --output sbom.json

# npm/pnpm
pnpm sbom --format cyclonedx --output-file sbom.json
```

SBOM is:
- Stored in the build artifact alongside the binary
- Scanned against all CVE databases (§4.1) at build time
- Retained for **1 year minimum**, archived for 3 years cold (matches audit log retention §10.2 — 90-day prior value was insufficient for post-incident forensic reconstruction)
- Compared against previous SBOM to detect unexpected additions

### §6.3 Model Supply Chain

For any ML model downloaded or fine-tuned:

- **Source registry**: HuggingFace Hub or internal registry only; no anonymous/unattributed downloads
- **Integrity**: SHA-256 of all model files verified against registry-published hash before load
- **AIBOM**: AI Bill of Materials documenting: model source, training data origin, training procedure, evaluation results, license
- **No unvetted fine-tuned models**: fine-tuned models require the same provenance chain as base models plus the fine-tuning dataset's provenance
- **Model cards**: required for any model hosted or deployed; must document known limitations and failure modes
- **Model change threat model**: any base model swap, fine-tune, or system prompt change ≥20% token diff vs current production requires a STRIDE threat model update and SERAPH [S] gate re-evaluation before deployment — a new model changes the prompt injection, jailbreak, and output handling attack surface even if the API interface is unchanged

### §6.4 CI/CD Pipeline Integrity

- CI environment variables audited quarterly; removed when no longer needed
- Build artifacts signed (SHA-256 + Ed25519 signature) before upload to artifact registry
- No build steps that pull from the internet at build time without pinned hashes
- `cargo update` only allowed in dedicated dependency-update PRs, never in feature PRs
- Pipeline definition files (`.github/workflows/`) reviewed by SERAPH on every change
- **OpenSSF SLSA Build Track target: L2** — signed provenance generated by hosted CI system (GitHub Actions); L2 prevents tampering after build. Target L3 (hardened build isolation) in roadmap for 2027. Current status: L1 (build artifacts signed with SHA-256 + Ed25519 — provenance exists but build is not yet hosted-runner-attested). Source: `industry-baselines/security/openssf/slsa-levels-v1.2-2026-05-12.md`.

---

## Part VII — Cryptography & Key Management

### §7.1 Key Lifecycle

| Phase | Requirement |
|---|---|
| Generation | OS CSPRNG only; minimum key sizes per §3.3 |
| Storage | OS keychain with per-binary ACL; encrypted at rest with AES-256-GCM |
| Rotation | Scheduled ≤90 days; immediate on suspected compromise (24h for CRITICAL) |
| Distribution | Never in config files, env vars, or git; out-of-band distribution only |
| Revocation | Revocation capability required before any key is issued |
| Destruction | Secure zero-fill on decommission; `zeroize` crate for in-memory keys |

**Agent-to-agent signing keys — additional requirements**: a single platform-wide signing key means any compromised agent binary yields tokens that authenticate as every other agent. Keys must be scoped:

- **Per caller-callee pair** (or per-agent with `aud` audience claim listing authorized callees)
- Each key stored under **per-binary keychain ACL** — the callee's binary only, not shared across services
- Tokens include `aud` claim validated by the receiving agent before processing any request
- **90-day rotation** with zero-downtime overlap window ≤1 hour; old key accepted for overlap period then invalidated
- **Key compromise procedure**: rotate within 1 hour; audit all calls made with the compromised key (AYIN chain log provides the audit trail)

### §7.2 TLS Configuration

Minimum required TLS configuration for any HTTPS endpoint:

```
Protocol: TLS 1.3 (TLS 1.2 allowed only with written approval)
Cipher suites (TLS 1.3): TLS_AES_256_GCM_SHA384, TLS_CHACHA20_POLY1305_SHA256
Cipher suites (TLS 1.2 if permitted): TLS_ECDHE_RSA_WITH_AES_256_GCM_SHA384 only
Certificate: RSA-4096 or ECDSA P-256; validity ≤398 days; OCSP stapling enabled
HSTS: max-age=31536000; includeSubDomains; preload
```

---

## Part VIII — Red Team & Assessment Standards

*(Absorbed from Builders Cookbook §40. Cookbook §40 now references `guardrails://Part VIII`.)*

### §8.1 SERAPH Engagement Rules

Every pentest/security assessment engagement is governed by these rules. No exceptions.

**Before any engagement**:
- Written scope definition approved by Kevin
- ScopeGovernor 5-gate configured: TTL (hard deadline), target list (exhaustive), tool allowlist, concurrent cap, domain boundary
- Legal review completed if third-party systems are in scope
- Emergency stop procedure defined (who can halt, how quickly)

**During engagement**:
- Stay strictly within scope; any out-of-scope finding is noted but not exploited
- Destructive tests (DoS, data deletion) require separate written approval per test
- Evidence collected continuously (screenshots, logs, packet captures)
- SERAPH report findings in real time to AYIN; do not batch

**After engagement**:
- Full evidence chain submitted (EvidenceChain schema from SERAPH-SDK)
- Findings triaged against §1.3 risk classification
- HITL for CRITICAL/HIGH findings before disclosure window closes
- Remediation verified by follow-up scan within patch SLA

### §8.2 PTES Methodology Alignment

Light Architects assessments follow PTES (Penetration Testing Execution Standard) phases. Cached source: `.firecrawl/ptes-main.md` + `.firecrawl/ptes-technical.md`.

| Phase | PTES | LA Implementation |
|---|---|---|
| Pre-Engagement | Scope, rules of engagement, legal | ScopeGovernor configuration + Kevin approval |
| Intelligence Gathering | OSINT, service enumeration | SERAPH RECON capability (QUANTUM assists) |
| Threat Modeling | Attack surface mapping | §1.2 attack surface inventory as baseline |
| Vulnerability Analysis | CVE matching, manual review | §4.1 CVE databases + SERAPH code review |
| Exploitation | Controlled PoC development | Strictly in-scope; evidence chain mandatory |
| Post-Exploitation | Impact assessment, lateral movement (controlled) | Blast radius assessment only |
| Reporting | Evidence, CVSS scores, remediation | EvidenceChain → AYIN → helix incident entry |

### §8.3 Atomic Red Team Coverage

Assessments reference MITRE ATT&CK via Atomic Red Team technique library. Cached: `.firecrawl/atomic-red-team.md`.

Priority technique categories for LA platform (by attack surface):

| Category | MITRE Tactic | LA-Relevant Techniques |
|---|---|---|
| Agent prompt manipulation | Initial Access, Execution | T1059 (Command/Script Interpreter via LLM output) |
| Credential theft | Credential Access | T1552 (Unsecured Credentials in env/config) |
| Secrets in git history | Collection | T1213 (Data from Information Repositories) |
| Supply chain | Initial Access | T1195 (Supply Chain Compromise) |
| Lateral movement via tool calls | Lateral Movement | T1021 (Remote Services via MCP) |
| Data exfiltration via LLM output | Exfiltration | T1048 (Exfiltration Over Alternative Protocol) |
| Model/data poisoning | Impact | T1565 (Data Manipulation) |
| Sandbox escape | Privilege Escalation | T1611 (Escape to Host) |

### §8.4 Findings Classification

All findings must include all five fields:

```yaml
finding:
  id: "SERAPH-2026-NNN"
  title: "Short descriptive title"
  cwe: "CWE-XXX"               # Common Weakness Enumeration
  cvss4_vector: "CVSS:4.0/..."  # CVSS 4.0 preferred (FIRST, Nov 2023) — better AI/ML-specific scoring
  cvss3_vector: "CVSS:3.1/..."  # Include for backwards compatibility with NVD tooling
  severity: CRITICAL|HIGH|MEDIUM|LOW|INFO
  evidence:
    - type: screenshot|log|packet_capture|code_snippet
      path: ".evidence/..."
      hash: "sha256:..."
  counter_evidence_sought: "What we looked for that would disprove this finding"
  confidence: "95%"            # Per Canon XXV epistemic rigor mandate
  remediation: "Specific, actionable fix"
  references:
    - "CVE-YYYY-NNNNN"
    - "CWE-XXX"
    - "OWASP A0X:2021"
```

---

## Part IX — Data & Privacy

### §9.1 Data Classification Tiers

| Tier | Examples | Storage | Transmission | Logging |
|---|---|---|---|---|
| **Public** | Published docs, open-source code | Any | Any | Allowed |
| **Internal** | Build plans, helix entries, squad discussions | Encrypted at rest | TLS 1.3 | Allowed (no content) |
| **Confidential** | API keys, personal data, proprietary models | AES-256-GCM + keychain | TLS 1.3 + mTLS | Field names only |
| **Secret** | Signing keys, audit HMAC key, master credentials | HSM or keychain with biometric ACL | Out-of-band only | Never |

**Out-of-band channels (Secret tier)** — approved: (1) macOS Keychain AirDrop to a personally-owned device, (2) physical device handoff, (3) 1Password shared vault with MFA enforced. Explicitly prohibited: email, Slack, Discord, SMS, Signal, any cloud-synced messaging. Social engineering that invokes an "approved" channel name is still a policy violation — the mechanism must be verified, not just named.

### §9.2 PII Handling

- PII is not stored in helix entries without explicit privacy tier annotation
- PII is stripped from training datasets before any fine-tuning operation
- PII in logs is redacted at the structured logging layer (field-level, not full-line scrubbing)
- No PII in agent system prompts (system prompts are observable)
- Right-to-erasure: any PII-tagged helix entry can be hard-deleted within 48 hours on request

### §9.3 Training Data Security

- All training datasets have documented provenance (source, license, collection date)
- Data poisoning detection: statistical outlier analysis before training run
- No training on data containing known PII without explicit anonymization step
- Training data snapshots retained for 1 year (reproducibility + audit)
- Model behavior evaluated after every fine-tuning run for unexpected outputs (alignment drift)

---

## Part X — Security Monitoring & Detection

### §10.1 Mandatory Security Signals

AYIN must collect these signals from every production process. Missing any is a [S] gate FAIL.

| Signal | Source | Alert Threshold |
|---|---|---|
| Auth failure rate | API gateway | > 10 failures / min → MEDIUM; > 50 / min → HIGH |
| Scope violation attempt | ScopeGovernor | Any → HIGH |
| Secrets pattern in logs | trufflehog streaming scan | Any → CRITICAL |
| CVE scan failure | cargo-audit / cargo-deny | Any CRITICAL vuln → CRITICAL |
| Unusual egress destination | Network egress log | Destination not in allowlist → HIGH |
| Sandbox resource limit hit | cgroups v2 events | Sustained (>30s) → MEDIUM |
| Recursion depth > 5 | Agent runtime | → MEDIUM (depth >7 is invariant violation) |
| Token budget exceeded | LLM gateway | > 90% → INFO; > 100% → HIGH |
| Audit log gap | HMAC chain verification | Any gap → CRITICAL |
| Repeated HITL bypass attempt | Agent decision log | Any → HIGH |
| Agent behavioral anomaly | AYIN per-agent-type baseline | Tool call sequence >3σ from historical baseline → MEDIUM; first access to previously-untouched allowlisted resource → LOW |

### §10.2 Audit Log Requirements

Every security-relevant event must be logged with:
- UTC timestamp (monotonic clock)
- Actor identity (agent, user, service)
- Action performed
- Target resource
- Outcome (success/failure)
- Correlation ID (ties related events across services)

Audit log is append-only, HMAC-chained (each entry includes HMAC of previous entry using `config/audit_hmac_key`). Any gap in the HMAC chain triggers CRITICAL alert.

**HMAC chain integrity model** (C4): the HMAC verification key is held by **LÆX** (the canon auditor role), not by AYIN (the log writer). Separation ensures the entity that writes logs cannot forge a consistent chain. A separate verification process independent of AYIN runs chain integrity checks on a 6-hour schedule and reports discrepancies directly to Kevin. The genesis block hash (chain initialization value) is stored in `config/audit_hmac_key.genesis` under keychain protection with biometric ACL.

Log retention: minimum 1 year. Logs older than 1 year archived to cold storage for 3 years.

### §10.3 Incident Response SLAs

| Severity | Detection → Triage | Triage → Containment | Containment → Remediation |
|---|---|---|---|
| CRITICAL | 15 min | 1 hour | 24 hours |
| HIGH | 2 hours | 24 hours | 7 days |
| MEDIUM | 24 hours | 7 days | 30 days |
| LOW | 7 days | 30 days | 90 days |

---

## Part XI — Compliance Mapping

### §11.1 CIS Controls v8 Coverage

| CIS Control | Coverage |
|---|---|
| CG1: Inventory and Control of Enterprise Assets | Partially covered — SBOM (§6.2) covers software; hardware inventory is manual |
| CG2: Inventory and Control of Software Assets | Covered — SBOM + dependency audit (§6.1, §6.2) |
| CG3: Data Protection | Covered — §9 Data & Privacy, §7 Cryptography |
| CG4: Secure Configuration | Covered — hardened defaults, §5 Sandboxing |
| CG5: Account Management | Covered — §3.5 Auth & Authorization |
| CG6: Access Control Management | Covered — RBAC, least privilege (§3.5) |
| CG7: Continuous Vulnerability Management | Covered — §4 Vulnerability Management, 6-database scan |
| CG8: Audit Log Management | Covered — §10.2 Audit Log Requirements |
| CG9: Email and Web Browser Protections | Not applicable (no end-user browser) |
| CG10: Malware Defenses | Partially — SERAPH scanning; no AV agent deployed |
| CG11: Data Recovery | Covered — vault backup, bundle archive policy |
| CG12: Network Infrastructure Management | Covered — §5.4 Egress Policy |
| CG13: Network Monitoring and Defense | Covered — AYIN + §10 Security Monitoring |
| CG14: Security Awareness | Covered — §11.3 Developer Security Training (annual program with frequency, content, and completion tracking) |
| CG15: Service Provider Management | Covered — §2.4 MCP server vetting before install; third-party APIs (Anthropic, ElevenLabs, HuggingFace, Sonatype) reviewed before integration; §6.1 dependency acceptance policy |
| CG16: Application Software Security | Fully covered — §3 Code Security, §4 Vuln Mgmt |
| CG17: Incident Response Management | Covered — §10.3 Incident Response SLAs; §4.3 Zero-Day Protocol; HITL escalation for CRITICAL/HIGH |
| CG18: Penetration Testing | Covered — §8 Red Team & Assessment |

### §11.2 NIST AI RMF Alignment (GOVERN · MAP · MEASURE · MANAGE)

| Function | LA Implementation |
|---|---|
| **GOVERN** | This document + Canon I/VII/VIII/XVI; SERAPH owns [S] gate; LÆX audits compliance |
| **MAP** | §1.2 Attack Surface Inventory; §1.3 Risk Classification; threat model in every LARGE plan |
| **MEASURE** | §4 CVE scanning; §8 Red Team findings; §10 Security Monitoring signals; C2 rubric score |
| **MANAGE** | §4.2 Patch SLAs; §10.3 Incident Response SLAs; §1.4 Security Ownership |

### §11.3 Developer Security Training

CIS Controls v8 CG14 requires a formal security awareness program — document existence alone does not satisfy the requirement.

| Training | Frequency | Content | Tracking |
|---|---|---|---|
| Security foundations | Annual (January) | OWASP Top 10, secure coding, credential hygiene, incident reporting | Completion logged to helix `seraph/training/completions/` |
| AI-specific security | Annual (July) | OWASP LLM Top 10 2025, prompt injection, agent trust chains (§2.6), sandboxing | Completion logged to helix |
| New builder onboarding | Within 30 days of first commit | Both modules above + platform guardrails walkthrough | Required before first production merge |
| Post-incident review | Within 7 days of CRITICAL incident | Incident-specific lessons; all active builders required | Completion tracked in incident record |

**Consequence for non-completion**: new builder training — first production merge blocked until complete. Annual training — MEDIUM compliance finding raised at next [S] gate.

**Delivery**: SERAPH maintains training materials in `helix/seraph/training/`. Annual content reviewed and updated by SERAPH each January.

### §11.4 NIST CSF v2.0 Alignment (Govern · Identify · Protect · Detect · Respond · Recover)

NIST Cybersecurity Framework 2.0 (published February 2024) added a sixth core function — **Govern** — alongside the original five. The LA platform maps to all six.

| CSF 2.0 Function | Description | LA Implementation |
|---|---|---|
| **GV — Govern** | Organizational context, risk strategy, policies, roles, supply chain risk management | This document (canon://security-guardrails); §1.4 Security Ownership; LASDLC [S] gate at every phase |
| **ID — Identify** | Asset management, risk assessment, improvement planning | §1.2 Attack Surface Inventory; §4.1 CVE Registry; §6.2 SBOM; §1.3 Risk Classification |
| **PR — Protect** | Access control, awareness training, data security, platform security, resilience | §3 Code Security; §5 Sandboxing; §7 Cryptography; §9 Data & Privacy; §11.3 Training |
| **DE — Detect** | Anomalies, continuous monitoring, adverse event detection | §10.1 Security Signals; AYIN observability layer; §10.2 Audit Log; §2.7 ATLAS threat detection |
| **RS — Respond** | Incident management, analysis, mitigation, communication | §10.3 Incident Response SLAs; §4.3 Zero-Day Protocol; HITL escalation |
| **RC — Recover** | Recovery planning, improvements, communications | Vault backup + bundle archive; §4.2 Patch SLAs; post-incident review (§11.3 training table) |

Source: `industry-baselines/security/nist/nist-csf-v2.0-2026-05-04.md`.

### §11.5 Regulatory Framework Mapping

| Regulation | Applicability to LA Platform | Key Requirements | Current Coverage |
|---|---|---|---|
| **EU AI Act** (Regulation (EU) 2024/1689; in force Aug 2024; most provisions Aug 2026) | LA's agentic orchestration platform may qualify as a **General-Purpose AI system** if made commercially available; internal use has lighter obligations | Transparency obligations; technical documentation for GPAI models; systemic-risk providers: adversarial testing, incident reporting to AI Office | §6.3 model supply chain; §8 red team assessments serve as adversarial testing; §11.3 training as literacy program; **Gap**: formal conformity assessment process not yet defined |
| **GDPR Art. 25** (Regulation (EU) 2016/679) | Applies to any processing of EU personal data — relevant if LA platform processes user PII in Berean, SOUL vault, or any EU-facing service | **Privacy-by-design**: implement data-protection principles (minimisation, purpose limitation, pseudonymisation) by default; **Privacy-by-default**: only data necessary for each purpose processed by default | §9 Data & Privacy; §3.1/A04 LINDDUN threat modeling (§2.7 parallel to STRIDE for PII flows); §9.2 PII handling; **Gap**: no formal DPIA (Data Protection Impact Assessment) process documented |

**Note**: the LA platform is currently a private internal tool. Regulatory exposure is LOW but grows as commercial deployment scope expands. Treat this mapping as pre-compliance architectural alignment.

---

## Part XII — Industry Baseline Index

All sources are allowlisted for SERAPH scans and LÆX FetchBaseline actions. Scrape dates indicate last update; re-scrape if older than 90 days.

### §12.1 Scraped Baseline Sources

| File | Source | Scrape Date | TTL |
|---|---|---|---|
| `.firecrawl/owasp-llm-top10-2025.md` | `genai.owasp.org/llm-top-10/` | 2026-05-05 | 90d |
| `.firecrawl/owasp-agentic-security.md` | `genai.owasp.org/initiatives/agentic-security-initiative/` | 2026-05-05 | 90d |
| `.firecrawl/owasp-genai.md` | `genai.owasp.org/` | 2026-05-05 | 90d |
| `.firecrawl/owasp-proactive-controls.md` | `owasp.org/www-project-proactive-controls/` | 2026-05-05 | 180d |
| `.firecrawl/owasp-secure-coding-playbook.md` | `owasp.org/www-project-secure-coding-practices-quick-reference-guide/` | 2026-05-05 | 180d |
| `.firecrawl/owasp-cheatsheet-index.md` | `cheatsheetseries.owasp.org/` | 2026-05-05 | 90d |
| `.firecrawl/nist-ai-rmf.md` | `nist.gov/artificial-intelligence/ai-risk-management-framework` | 2026-05-05 | 365d |
| `.firecrawl/google-saif.md` | `saif.google/secure-ai-framework` | 2026-05-05 | 180d |
| `.firecrawl/google-saif-risks.md` | `saif.google/secure-ai-framework/risks` | 2026-05-05 | 180d |
| `.firecrawl/ptes-main.md` | `pentest-standard.org/` | 2026-05-05 | 365d |
| `.firecrawl/ptes-technical.md` | `pentest-standard.org/testing/` | 2026-05-05 | 365d |
| `.firecrawl/atomic-red-team.md` | `atomicredteam.io/` | 2026-05-05 | 90d |
| `research/protectai-security-mapping.md` | Protect AI security mapping | 2026-05-05 | 180d |
| `research/deterministic-ai-orchestration.md` | Internal research | 2026-05-05 | N/A |
| `industry-baselines/security/mitre/mitre-atlas-2026-05-04.md` | MITRE ATLAS v4.5 — AI/ML adversary tactics | 2026-05-04 | 180d |
| `industry-baselines/security/owasp/owasp-api-security-top-10-2023-2026-05-04.md` | OWASP API Security Top 10 2023 | 2026-05-04 | 365d |
| `industry-baselines/security/owasp/owasp-asvs-v5.0.0-2026-05-12.md` | OWASP ASVS v5.0.0 — full specification (May 2025) | 2026-05-12 | 365d |
| `industry-baselines/security/owasp/owasp-llm-prompt-injection-cheatsheet-2026-05-05.md` | OWASP LLM Prompt Injection Prevention Cheat Sheet | 2026-05-05 | 90d |
| `industry-baselines/security/nist/nist-sp-800-63-4-2026-05-12.md` | NIST SP 800-63-4 (final, Aug 2025) — Digital Identity Guidelines | 2026-05-12 | 365d |
| `industry-baselines/security/nist/nist-sp-800-63b-4-2026-05-12.md` | NIST SP 800-63B-4 (final, Aug 2025) — Authentication & Authenticator Management (AAL1-3) | 2026-05-12 | 365d |
| `industry-baselines/security/nist/nist-csf-v2.0-2026-05-04.md` | NIST CSF v2.0 resource center | 2026-05-04 | 180d |
| `industry-baselines/security/openssf/slsa-levels-v1.2-2026-05-12.md` | OpenSSF SLSA v1.2 Build Track L0-L3 + Source Track L1-L4 | 2026-05-12 | 365d |
| `industry-baselines/security/eu/eu-ai-act-2026-05-04.md` | EU AI Act (Regulation (EU) 2024/1689) | 2026-05-04 | 180d |
| `industry-baselines/security/eu/gdpr-article-25-2026-05-04.md` | GDPR Art. 25 — Data Protection by Design | 2026-05-04 | 365d |
| `industry-baselines/security/linddun/linddun-2026-05-04.md` | LINDDUN privacy threat modeling framework | 2026-05-04 | 365d |

### §12.2 CVE Database Registry

Live query endpoints maintained by QUANTUM for vulnerability assessment:

```yaml
cve_databases:
  cisa_kev:
    url: "https://www.cisa.gov/sites/default/files/feeds/known_exploited_vulnerabilities.json"
    format: json
    refresh_ttl: 24h
    priority: 1   # Always check first
  nvd:
    url: "https://services.nvd.nist.gov/rest/json/cves/2.0"
    format: json_api
    auth: api_key  # Free registration; rate limit 50 req/30s with key
    refresh_ttl: 24h
    priority: 2
  osv:
    url: "https://api.osv.dev/v1/query"
    format: json_api
    auth: none
    refresh_ttl: 24h
    priority: 2
  rustsec:
    url: "https://github.com/RustSec/advisory-db"
    format: git_clone
    tool: "cargo audit"
    refresh_ttl: 24h
    priority: 1   # Primary for Rust workspace
  github_advisory:
    url: "https://api.github.com/graphql"
    query: "securityAdvisories"
    auth: github_token
    refresh_ttl: 24h
    priority: 3
  sonatype:
    url: "https://ossindex.sonatype.org/api/v3/component-report"
    format: json_api
    auth: api_key
    refresh_ttl: 48h
    priority: 3
```

### §12.3 Fetch Protocol

SERAPH and QUANTUM fetch from allowlisted sources using `LaexAction::FetchBaseline`. Files cached to `.firecrawl/` (external) or `research/` (internal). Re-fetch trigger: file mtime > TTL or QUANTUM detects source has been updated.

---

## Part XIII — OSI Layer Security Posture

An explicit per-layer control map surfaces gaps that cross-cutting policies can obscure. The LA platform is software-only (no owned physical/data-link infrastructure), so L1 and L2 are narrowly scoped.

| OSI Layer | Controls in Place | Primary Document | Gap / Next Action |
|---|---|---|---|
| **L1 Physical** | FileVault 2 full-disk encryption; screen lock ≤5 min; firmware password; USB policy | §5.6 Device Security (this doc) | No hardware security modules (HSM) currently; add to PQC roadmap |
| **L2 Data Link** | N/A — software-only stack; no owned switches or network devices | — | Not applicable |
| **L3 Network** | RFC 1918 block; SSRF protection; deny-by-default egress allowlist; DNS rebinding protection | §3.1/A10, §5.4 | No BPF/iptables rule documentation; relies on application-layer enforcement |
| **L4 Transport** | TLS 1.3 minimum; TLS 1.2 deprecated; HSTS max-age=31536000 with preload; OCSP stapling | §7.2 | PQC algorithm agility required before 2028 (§3.3 PQC roadmap) |
| **L5 Session** | JWT access tokens (1h TTL, 256-bit entropy, Ed25519); refresh token rotation; ScopeGovernor TTL gate | §3.5, §2.5 | Multi-device session management not yet implemented |
| **L6 Presentation** | UTF-8 validation at input boundary; `serde deny_unknown_fields`; output schema validation; context-specific encoding (HTML/JSON/Cypher) | §3.4, §3.2/C4 | No format-string safety audit for log output; ASVS L2 check pending |
| **L7 Application** | OWASP Top 10 2021; OWASP API Security Top 10 2023 (MCP/tool surface); OWASP Proactive Controls; ASVS L1 baseline; CORS + CSP headers for Berean | §3.1, §2.4, §3.7 | API Top 10 stance formalized in §2.4 (this version); ASVS L2 audit for auth/crypto paths pending |
| **L8 AI/Agent** | OWASP LLM Top 10 2025; OWASP Agentic Top 10 2026; MITRE ATLAS v4.5; ScopeGovernor 5-gate; §2.6 monotonic scope reduction invariant; WASM fuel metering | §2.1–§2.7 | ATLAS adversarial testing in SERAPH red team exercises (formalized §2.7, this version); no automated ATLAS technique coverage tracking yet |

**L8 (AI/Agent layer)** is a platform extension beyond the OSI model — included here because the dominant attack surface for an agentic AI platform operates at this layer, not at L7.

**Residual gap summary** (as of v1.2.0): HSM (L1) · BPF/iptables docs (L3) · PQC transition (L4) · Multi-device sessions (L5) · Format-string log audit (L6) · ASVS L2 auth/crypto paths (L7) · ATLAS coverage tracking (L8).

---

<!-- ──────────────────────────────────────────────────────────────────────────
     IRONCLAW-SPINE CANON AMENDMENT (2026-05-18 iter-7)
     Source plan: ironclaw-spine.md security_compliance + Phase 2A
     Source: ironclaw-architecture.html §13; SCRUM R2 SERAPH adversarial review
     Authority: operator-authorized Canon XV override (2026-05-18)
     ────────────────────────────────────────────────────────────────────────── -->

## §SG-CRYPTO — Artifact Integrity + Key Lifecycle (2026-05-18 ADDITION)

**Scope**: Cryptographic discipline for autonomous-mode build artifacts (program manifest, decision log, supervisor channel, model failover).

### §SG-CRYPTO.1 Program Manifest Integrity (Ed25519, not SHA256-alone)

Autonomous-mode builds lock the plan at `/BUILD` start via **Ed25519-signed manifest**:
- `program.toml` (canonical TOML serialization)
- `program.sig` (detached Ed25519 signature)

Ceremony:
1. Operator approval at `/BUILD --autonomous` triggers keygen
2. Keypair stored in macOS Keychain with `kSecAccessControlBiometryCurrentSet` + `kSecAttrAccessibleWhenUnlockedThisDeviceOnly` (Touch ID-gated; never exported)
3. Detached signature emitted to `.ironclaw/program.sig`
4. `lightarchitects verify-manifest <path>` verifies signature pre-dispatch

**Bare SHA256 IS INSUFFICIENT** (CWE-345). Attacker with local write modifies both `program.toml` AND `program.sha256` in same write — verification passes. Signature with attacker-unreachable private key closes this.

### §SG-CRYPTO.2 Supervisor Channel HMAC (HKDF Per-Wave Subkeys)

Long-running supervisor channels MUST rotate HMAC keys per wave via **HKDF-SHA256**:
- Master key derived from operator-approval ceremony (Keychain-stored; never logged)
- Per-wave subkey: `HKDF(master, salt=build_id || wave_id, info="supervisor-channel-v1")`
- Subkey-id stamped in every channel message + every `.ironclaw/decisions.md` entry
- Revocation: supervisor restart → fresh master → all prior subkeys invalidated

Single per-build HMAC is insufficient (CWE-320). If compromised at task 3 of 72-task program, attacker forges 69 remaining decisions. Per-wave subkeys + restart-revocation bound the blast radius.

### §SG-CRYPTO.3 decisions.md Hash-Chain (Append-Only Tamper Detection)

`.ironclaw/decisions.md` MUST be hash-chained newline-delimited JSON:
```jsonl
{"line":1,"prev_hash":"00…","ts":...,"task_id":...,"layer":1,"verdict":...,"subkey_id":"w1-sk-3"}
{"line":2,"prev_hash":"<sha256(line 1)>",...}
```

Write protocol: `write to .tmp + fsync + rename` (atomic per CWE-662). Never truncate. On supervisor reload, verify entire chain — any mismatch = tamper detected, HALT.

### §SG-CRYPTO.4 cargo-vet TTL ≤ 30d for Cross-Repo Dependencies

For autonomous-mode builds with git-dep across repos (e.g., `lightarchitects-gateway` → `lightarchitects-cli`):
- SHA-pinned `rev =` (NEVER mutable branch ref) — MITRE ATLAS AML.T0010 mitigation
- `cargo-vet` attestation present
- `deny.toml` source allowlist enforced
- **Attestation TTL ≤ 30 days** — stale attestations rubber-stamp post-compromise upstream
- CI re-verifies attestations every build; expired → BLOCK merge

### §SG-CRYPTO.5 Failover Rate-Limit Circuit Breaker

When primary model lane fails over to backup (e.g., Ollama 429 → Anthropic Haiku failover per ironclaw §12 Model Routing), a **circuit breaker** MUST cap failover-lane spend per program:
- Counter `model.failover_total{from,to,cause}` instrumented (AYIN span per observability-canon)
- HITL prompt at 50% of program cost ceiling
- Auto-HALT at 100% of ceiling

Failover-as-resilience inverts into failover-as-cost-amplification under adversarial induction (OWASP-LLM10 Unbounded Consumption; MITRE ATLAS AML.T0034 Cost Harvesting).

### §SG-CRYPTO.6 PermissionMatrix Denylist→Allowlist (Bash verbs)

For autonomous-mode workers, the AgentRunner PermissionMatrix MUST use an **allowlist of permitted Bash verbs**, NOT a denylist of denied patterns:
- Denylist enumerated patterns (`ln -s outside cwd`, `chmod +x on host paths`, `curl|sh`, `> /etc/`...) are bypassable via splits (`cp /bin/sh /tmp/x; chmod +x`) or alternate interpreters (`python -c "import os; os.symlink(...)"`)
- Allowlist of permitted verbs is enforceable and auditable

CWE-184 (Incomplete List of Disallowed Inputs). The allowlist composes with `safe_cwd` canonicalize-after-..-rejection pattern from `git_routes.rs:165-174`.

### §SG-CRYPTO.8 Canon-File SHA256 Integrity at Session Start

When a supervisor session initializes, it MUST verify the SHA256 digest of each of the 8 canonical canon docs before allowing any L1 decision to fire. A mismatch between the on-disk digest and the expected digest (stored in Keychain as `lightarchitects-canon-sha256-<doc-name>`) halts the session with a CANON_INTEGRITY_FAIL event.

**Mechanism**: on first run after any canon update, the supervisor writes new expected digests to Keychain. On subsequent runs, digests are compared before canon docs are loaded as cached system prompt (§11.3a).

**Covered docs**: the 8 canonical files enumerated in agents-playbook §11.3a.

**Rationale**: §11.3a's composition note cited §SG-CRYPTO.3 for this check, but §SG-CRYPTO.3 covers decisions.md tamper detection (a different chain). Canon-file integrity at session start is a distinct mechanism — this section defines it.

**Status**: stub — implementation target Phase 7+ (ironclaw-spine post-ship). Canon XV operator-authorized pre-declaration per agents-playbook §11.3a annotation.

---

### §SG-CRYPTO.7 Cross-References

- Cookbook §64 (serialized git-ops mutex)
- Cookbook §65 (Builder Completeness Invariant — fail-closed permission matrix)
- LASDLC v2.5.2 `program_manifest_integrity` block
- Architects Blueprint §24.3 (manifest integrity discipline)
- webshell-api-surface §1.6 (cross-reference)
- observability-canon §AYIN span schema (escalation.notify, model.failover_total)
- Source: ironclaw-spine SCRUM R2 SERAPH + R3 follow-up threats (2026-05-18)

---

## Amendment history

Detailed amendment narrative — sections added, source audit findings, CVE/CWE references, cross-canon ties, LÆX candidate IDs — lives in the companion file:

  **`standards/canon/security-guardrails.CHANGELOG.md`**

Mechanical history: `git log -- standards/canon/security-guardrails.md`

**Rule** (per separation-of-concerns refactor, 2026-05-18): no tail-changelog tables or orphan changelog rows in this file. Section content lives here; amendment narrative lives in the CHANGELOG companion.

**Current version**: see CHANGELOG for latest. As of 2026-05-19: **v1.3.0** (§SG-CRYPTO Artifact Integrity + Key Lifecycle; ratified by LÆX + Kevin Francis Tan — 2026-05-19, ironclaw-spine Phase 7).

---

*Part of the Canonical Suite · `canon://security-guardrails`. Gate: [S] primary (SERAPH) · [C] secondary (LÆX). Supersedes: builders-cookbook.md §40 (pentest) · builders-cookbook.md §12 policy half.*
