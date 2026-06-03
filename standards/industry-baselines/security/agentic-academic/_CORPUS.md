---
name: agentic-academic-corpus-index
description: Academic security literature on agentic AI systems — memory poisoning, prompt injection, supply chain, red team, evaluation, defense
type: reference
authority_rating: MEDIUM-HIGH
updated: 2026-06-02
---

# Agentic AI Security Academic Literature — Corpus Index

Peer-reviewed and preprint papers on agentic AI (LLM-driven autonomous agents) security. Parent topic of `security/mcp-academic/`.

## Why This Subfolder

The agentic-AI security field has matured rapidly (2024-2026). Several SoK papers, benchmarks, and proposed defense architectures are now stable enough to cite as canonical.

## Bibliographic Index

Curated subset of ~30 papers surveyed 2026-06-02. Selection criteria: published in last 24 months + high citation potential + direct platform implication.

### Surveys / Systematizations of Knowledge

#### [AAS01] Agentic AI Security: Threats, Defenses, Evaluation, Open Challenges — `2510.23883`
- **Date**: Oct 2025
- **Authors**: Chhabra, Datta, Nahin, Mohapatra
- **Contribution**: Comprehensive SoK with threat taxonomy, benchmarks, evaluation methodologies, defense strategies, secure-by-design principles
- **Why canonical**: **The reference SoK for the field**

#### [AAS02] TRiSM for Agentic AI — `2506.04133`
- **Date**: Jun 2025
- **Authors**: Raza, Sapkota, Karkee, Emmanouilidis
- **Contribution**: Trust/Risk/Security Management review covering governance, explainability, ModelOps, privacy/security, adversarial defense, compliance
- **Why canonical**: Frames the regulatory/compliance lens

#### [AAS03] From Prompt Injections to Protocol Exploits — `2506.23260`
- **Date**: Jun 2025
- **Authors**: Ferrag, Tihanyi, Hamouda, Maglaras, Debbah
- **Contribution**: **Catalogues 30+ attack techniques across 4 domains** (input manipulation, model compromise, system/privacy attacks, protocol vulnerabilities). Covers MCP, A2A, Agent Network Protocol
- **Why canonical**: Cross-protocol attack catalog

#### [AAS04] A Safety and Security Framework for Real-World Agentic Systems — `2511.21990`
- **Date**: Nov 2025
- **Authors**: Ghosh, Simkin, Shiarlis et al. (NVIDIA et al., 12+ authors)
- **Contribution**: Dynamic framework with **contextual risk management + auxiliary AI safety models + human oversight + AI-driven red teaming**
- **Why canonical**: Enterprise-grade reference architecture from industry consortium

### Memory Poisoning & Persistent Compromise

#### [AAS05] SuperLocalMemory — `2603.02240`
- **Date**: Feb 2026
- **Authors**: Bhardwaj
- **Contribution**: Privacy-preserving multi-agent memory; **explicitly maps to OWASP ASI06**; Bayesian trust scoring + architectural isolation + adaptive learning-to-rank; SQLite FTS5 + Leiden clustering; GDPR Article 17 support
- **Build target**: F16 (`eva-asi06-bayesian-trust`) — direct architectural reference

#### [AAS06] Agent Security Bench (ASB) — `2410.02644`
- **Date**: Oct 2024
- **Authors**: Zhang, Huang, Mei, Yao, Wang, Zhan, Wang, Zhang
- **Contribution**: Formalized attack taxonomy + benchmark across prompt injection, memory poisoning, Plan-of-Thought backdoor, mixed attacks
- **Build target**: F7 + F13 test corpus

#### [AAS07] AgentPoison — `2407.12784`
- **Date**: Jul 2024
- **Authors**: Chen, Xiang, Xiao, Song, Li (Berkeley + UChicago + UIUC)
- **Contribution**: Red-team via memory or knowledge base poisoning; **constrained-optimization backdoor in RAG retrieval embedding space**
- **Build target**: F7 — defines the attack pattern to defend against

#### [AAS08] Memory Poisoning Attack and Defense (MINJA) — `2601.05504`
- **Date**: Jan 2026
- **Authors**: Devarangadi Sunil et al.
- **Contribution**: **MINJA** memory-injection attack; defense via composite trust scoring + memory sanitization + temporal decay + pattern-based filtering
- **Build target**: F7 + F16 — defense recipe

#### [AAS09] MemoryGraft — `2512.16962`
- **Date**: Dec 2025
- **Authors**: Srivastava, He
- **Contribution**: Persistent compromise via **poisoned experience retrieval**; semantic imitation heuristic + union retrieval; behavioral drift detection
- **Build target**: F7 + F16 — advanced attack class

#### [AAS10] AgentLAB — `2602.16901`
- **Date**: Feb 2026
- **Authors**: Jiang, Wang, Liang, Wang
- **Contribution**: Benchmark for **long-horizon attacks**: intent hijacking, tool chaining, task injection, objective drifting, memory poisoning
- **Build target**: F7 + F14 — evaluation harness

### Prompt Injection (Detection + Defense)

#### [AAS11] DataSentinel — `2504.11358`
- **Date**: Nov 2025
- **Authors**: Liu, Jia, Jia, Song, Gong
- **Contribution**: **Game-theoretic detection via minimax optimization**; fine-tuned LLM judges adversarial inputs
- **Build target**: F7 + F10 — defense architecture

#### [AAS12] PromptShield — `2501.15145`
- **Date**: Jan 2025
- **Authors**: Jacob, Alzahrani, Hu, Alomair, Wagner (UC Berkeley)
- **Contribution**: Deployable detector + **PromptShield benchmark**
- **Build target**: F7 + F10 — benchmark target

#### [AAS13] AgentWatcher — `2604.01194`
- **Date**: Apr 2026
- **Authors**: Wang, Zou, Geng, Jia
- **Contribution**: **Rule-based monitor with causal attribution** for explainability — bridges detection and audit
- **Build target**: F7 + AYIN trace integration

#### [AAS14] ToolHijacker — `2504.19793`
- **Date**: Aug 2025
- **Authors**: Shi, Yuan, Tie, Zhou, Gong, Sun
- **Contribution**: **Tool selection injection attack** in no-box scenarios; defeats StruQ, SecAlign, known-answer detection, DataSentinel, perplexity detection
- **Why canonical**: Defines a new attack class our gateway must defend against

#### [AAS15] Indirect Prompt Injection via Tool Result Parsing — `2601.04795`
- **Date**: Jan 2026
- **Authors**: Yu, Cheng, Liu
- **Contribution**: LLM agents in physical-control systems; defense by **parsing tool results to filter malicious code** while maintaining utility
- **Build target**: F7 — defense pattern

#### [AAS16] DASGuard — `2605.31042`
- **Date**: May 2026
- **Authors**: Tan, Dou, Yang, Hu, Cheng, Li, Wen
- **Contribution**: Multi-step trojan defense for agentic harnesses; **runtime attack blocking + sanitized commits**
- **Build target**: F7 — runtime defense

### Supply Chain & SBOM

#### [AAS17] Supply-Chain Poisoning Attacks Against LLM Coding Agent Skill Ecosystems — `2604.03081`
- **Date**: Apr 2026
- **Authors**: Qu, Liu, Geng, Deng, Li, Zhang, Zhang, Ma
- **Contribution**: **Document-Driven Implicit Payload Execution** — malicious skills with embedded payloads in documentation examples; MITRE ATT&CK categorization; static analysis defense
- **Build target**: F17 (`corso-skill-payload-detection`) — **direct threat to our plugin marketplace**
- **Why canonical**: First paper on skill ecosystem supply chain attacks

#### [AAS18] Agentic AI as a Cybersecurity Attack Surface — `2602.19555`
- **Date**: Feb 2026
- **Authors**: Jiang, Yang, Yang, Liu, Ji
- **Contribution**: Runtime security risks in LLM-agents from dynamic dependencies; proposes **zero-trust runtime architecture with cryptographic provenance** for tool execution
- **Build target**: F1 + F18 + Canon L candidate

#### [AAS19] Understanding Security Risks of AI Agents' Dependency Updates — `2601.00205`
- **Date**: Jan 2026
- **Authors**: Singla, Çakar, Amusuo, Davis (Purdue)
- **Contribution**: Dependency-update security analysis specific to AI agents
- **Build target**: F1 + champion-challenger

#### [AAS20] Formal Analysis and Supply Chain Security for Agentic AI Skills — `2603.00195`
- **Date**: Feb 2026
- **Authors**: Bhardwaj
- **Contribution**: Formal methods applied to skill supply chain
- **Build target**: F17 — formal verification reference

### Red Team / Autonomous Cybersecurity Agents

#### [AAS21] CAI: Cybersecurity AI — `2504.06017`
- **Date**: Apr 2025
- **Authors**: Mayoral-Vilches et al. (12+ authors)
- **Contribution**: **Open-source bug-bounty-ready autonomous cybersecurity AI framework**; CTF benchmarks + real-world bug bounty; HITL oversight; modular agent design + tool integration
- **Build target**: F14 (`seraph-cai-mode`) — **direct reference implementation**

#### [AAS22] Co-RedTeam — `2602.02164`
- **Date**: Feb 2026
- **Authors**: He, Fox, Miculicich, Friedli, Fabian, Gokturk, Tang, Lee, et al.
- **Contribution**: **Security-aware multi-agent framework** for vulnerability discovery + exploitation; integrated security knowledge + code analysis + execution feedback + memory
- **Build target**: F14 architecture reference

### Evaluation / Benchmarks

#### [AAS23] FinVault — `2601.07853`
- **Date**: Jan 2026
- **Authors**: Yang, Li, Qiang, Wang, Lou, Li, Cheng, Xu et al. (18+ authors)
- **Contribution**: **First execution-grounded security benchmark for financial agents**; state-writable databases + compliance constraints
- **Build target**: F15 — evaluation target (alongside FinBot CTF)

#### [AAS24] Arena-Hard / BenchBuilder — `2406.11939`
- **Date**: Jun 2024
- **Authors**: Li, Chiang, Frick, Dunlap, Wu, Zhu, Gonzalez, Stoica
- **Contribution**: Living benchmark with automated prompt selection + LLM judges + confidence intervals; aligns with Chatbot Arena human preferences
- **Build target**: F8 (champion-challenger) closest analog

## Aggregate Insight

The agentic-AI security field has approximately 4-tier maturity:
1. **Threat models established** — multiple comprehensive SoKs available
2. **Attack catalogues comprehensive** — 30+ attacks indexed in [AAS03], 31 in MCP02
3. **Defense architectures proposed** — DataSentinel, AgentWatcher, DASGuard, MCPGuard, SMCP
4. **Benchmarks emerging** — ASB, AgentLAB, FinVault, PromptShield benchmark

Our SERAPH/CORSO/EVA roadmap items (F7, F10, F13-F18) directly target this stack.

## How to Use This Index

- **Implementing F7** (EVA agent threat detection): start with AAS06 (ASB benchmark) + AAS04 (NVIDIA framework) + AAS11-16 (detection methods)
- **Implementing F14** (SERAPH CAI mode): start with AAS21 (CAI reference impl) + AAS22 (Co-RedTeam architecture)
- **Implementing F17** (skill payload detection): start with AAS17 (Document-Driven Implicit Payload Execution) — first reference of this attack class
- **Implementing F1/F18** (SBOM + provenance): cite AAS18 (zero-trust runtime + cryptographic provenance) + AAS19 + AAS20

## GCS Status

Papers are **NOT** mirrored to GCS (arXiv hosts authoritative versions). Use HuggingFace MCP `paper_search` or `arxiv.org/abs/{id}` to fetch on demand. Local search via `hf.co/papers/{id}` for AI summaries.

## Related Folders

- `security/mcp-academic/` — MCP-specific subset of this literature
- `security/ibm/` — IBM industry-verified guide (complementary to academic literature)
- `security/owasp/` — OWASP standards organization
- `security/portswigger/` — practical web pentest reference (WAHH)
