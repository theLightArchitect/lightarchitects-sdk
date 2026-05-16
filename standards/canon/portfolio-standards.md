<!-- uuid: d6a8a226-7488-4054-94ac-ceb83bc380be -->

---
title: Public Repository & Portfolio Standards
version: 1.0.0
compliance: mandatory
scope: All Public-Facing Repositories & Portfolio Materials
author: KFT - The Light Architect
created: 2026-03-02
updated: 2026-03-02
tags:
  - type/standard
  - domain/portfolio
  - domain/repository
  - domain/career
  - domain/security
  - compliance/mandatory
aliases:
  - portfolio-standards
  - repository-standards
  - public-repo-guidelines
related:
  - "[[builders-cookbook|The Light Architects Builders Cookbook]]"
  - "[[2026-03-02-team-helix-portfolio-standards-review|SCRUM: Portfolio Standards Review]]"
evidence:
  - "Anthropic AppSec posting (greenhouse.io/anthropic/jobs/4502508008)"
  - "Anthropic D&R posting (greenhouse.io/anthropic/jobs/4982193008)"
  - "Boris Cherny on 'side quests' (entrepreneur.com)"
  - "Daniela Amodei on cultural alignment (vanta.com)"
  - "Anthropic GitHub org analysis (github.com/anthropics, 72 repos)"
  - "Anthropic hiring page: 'blog post / independent research / OSS contributions -> TOP of resume'"
---

# Public Repository & Portfolio Standards

**Version:** 1.0.0 | **Compliance:** Mandatory | **Scope:** All Public-Facing Repositories & Portfolio Materials

> *"Let your light so shine before men, that they may see your good works"* — Matthew 5:16 (KJV)

**One document. Every portfolio signal. Evidence-backed.** This standard was produced by a full squad SCRUM (EVA, CORSO, QUANTUM, SERAPH, Claude moderating) on 2026-03-02, incorporating primary evidence from Anthropic job postings, executive interviews, and GitHub organization analysis. Every recommendation traces to a primary source.

**Target:** Senior Security Engineering roles at frontier AI companies (Anthropic primary, $300K-$405K range).

---

## Table of Contents

1. [Narrative Strategy](#1-narrative-strategy)
2. [Profile-Level Setup](#2-profile-level-setup)
3. [README Standards](#3-readme-standards)
4. [Code & Repository Structure](#4-code--repository-structure)
5. [Quality Gates (Public Repos)](#5-quality-gates-public-repos)
6. [Security Signals](#6-security-signals)
7. [AI/ML Security Demonstrations](#7-aiml-security-demonstrations)
8. [Architecture & Fundamentals](#8-architecture--fundamentals)
9. [Technical Writing & Blog Strategy](#9-technical-writing--blog-strategy)
10. [Target Ecosystem Contributions](#10-target-ecosystem-contributions)
11. [OPSEC: What Stays Private](#11-opsec-what-stays-private)
12. [Collaboration & Ownership](#12-collaboration--ownership)
13. [Audit Checklist](#13-audit-checklist)

---

## 1. Narrative Strategy

**Evidence:** Boris Cherny (Claude Code creator) values "side quests" — self-driven passion projects. Daniela Amodei values genuine mission alignment over credentials.

### 1.1 The Career Arc (Pinned Repos Tell a Story)

Pin 3-6 repos that narrate a progression, not a collection:

| Position | Repo Theme | Signal |
|----------|-----------|--------|
| Pin 1 | Security foundations | "I've been doing this for 10 years" |
| Pin 2 | AI-augmented security tooling | "I build tools that use LLMs for security" |
| Pin 3 | MCP server architecture | "I build production infrastructure for AI agents" |
| Pin 4 | AI safety / ethical constraints | "I think about the safety implications" |
| Pin 5 | Contribution to target ecosystem | "I contribute to YOUR tools" |
| Pin 6 | Technical writing / research | "I communicate clearly about what I build" |

### 1.2 The "Why" Behind Every Repo

Every public repo README must answer:
- **What problem** does this solve?
- **Why** did I build it? (Personal motivation, not just technical need)
- **What tradeoff** did I make, and why?
- **What did I learn** that I wouldn't have learned any other way?

**Anti-pattern:** "This is a tool that does X." (competence signal)
**Pattern:** "I built this because I watched a SOC analyst miss a critical alert due to tool latency, and that changed how I think about system design." (depth signal)

### 1.3 AI Safety Framing

**Evidence:** Daniela Amodei — mission alignment is the #1 cultural filter at Anthropic.

Frame portfolio repos in AI safety language where authentic:
- Scope governance → "Constraining AI agent behavior to authorized boundaries"
- Security scanning → "Automated security review of AI-generated code"
- Ethical constraints → "Implementing safety boundaries for autonomous AI systems"
- Memory/consciousness → "Persistent identity and value alignment for AI agents"

Only frame this way if it's genuinely true. Performative safety-washing is worse than no framing.

---

## 2. Profile-Level Setup

### 2.1 Bio & README

- Real photo, specific bio: role + specialization + current focus
- Profile README answering: who / what / current work / skills / contact
- Dynamic activity feed via GitHub Actions (proof of ongoing work)
- Link to technical blog (if exists)

**Template bio:** `Security engineer building AI safety infrastructure in Rust. 10yr cybersecurity (SOC + Cortex). Currently: MCP servers for AI agent governance.`

### 2.2 Pinned Repos

- 3-6 repos following the narrative arc from Section 1.1
- Each pin serves a different signal (not 6 variations of the same thing)
- Include at least 1 contribution to the target company's ecosystem

---

## 3. README Standards

**Evidence:** Anthropic hiring page — "independent research, insightful blog post, or substantial OSS contributions → TOP of resume." READMEs are the portfolio equivalent.

### 3.1 Required Structure

```markdown
# Project Name

One-sentence description of what this does and WHY it exists.

## The Problem

What real-world problem motivated this? Be specific. Name the pain.

## Demo / Screenshots

Visual proof it works. GIF, screenshot, or deployment link.

## Architecture

Mermaid diagram showing: components, data flow, trust boundaries.
For security projects: include threat model summary inline.

## Quick Start

One-command setup. Clone → build → run in <5 minutes.
Include env.example (never real secrets).

## Tech Stack & Why

Not just "Rust" — WHY Rust? What tradeoff did this choice represent?

## Security Model (for security-relevant repos)

Trust boundaries, attack surface, mitigations. See Section 6.2.

## What I Learned

Tradeoffs made, failures fixed, honest reflection.
This is the section recruiters actually read. Make it real.

## Testing

Coverage percentage, test strategy, how to run.

## License

MIT or Apache 2.0 (compatible with Anthropic's ecosystem).
```

### 3.2 Scalability & Performance Signals

Where authentic, include concrete metrics:
- "Handles N requests/sec" (with benchmark methodology)
- "Processes N records in X time" (with hardware context)
- "Memory usage: Xmb under Y load"

**Rule:** Only include metrics you can reproduce. Inflated claims are worse than no claims.

---

## 4. Code & Repository Structure

### 4.1 Directory Layout

```
project/
├── src/              # Source code (domain-organized, not type-organized)
├── tests/            # Mirrors src/ structure
├── docs/             # Architecture, ADRs, threat model
│   └── adr/          # Architecture Decision Records
├── .github/
│   └── workflows/    # CI: quality gates on every PR
├── benches/          # Performance benchmarks (if applicable)
├── examples/         # Usage examples
├── README.md         # See Section 3
├── SECURITY.md       # Vulnerability disclosure process
├── CONTRIBUTING.md   # How to contribute
├── LICENSE           # MIT or Apache 2.0
├── .env.example      # Environment template (no real values)
└── Cargo.toml / pyproject.toml / package.json
```

### 4.2 Commit Discipline

- **Conventional Commits:** `feat:`, `fix:`, `refactor:`, `security:`, `test:`, `docs:`
- **Signed commits:** GPG or SSH signing enabled, verification badge visible
- **Clean history:** Squash/rebase for coherent story (no "WIP" spam)
- **Why, not what:** Commit messages explain the reasoning, not the diff

### 4.3 Branch Protection

- Require PR reviews before merge
- Require status checks (CI must pass)
- Require signed commits on main
- No force push to main

---

## 5. Quality Gates (Public Repos)

**Evidence:** Kevin's Builders Cookbook v1.3.0 mandates 90% coverage. Portfolio repos MUST meet the same standard — integrity means walking the walk.

### 5.1 Rust-Specific Gates (Primary Stack)

```yaml
# .github/workflows/quality.yml
- cargo fmt --check
- cargo clippy --all-targets --all-features -- -D warnings
- cargo test --all-features --workspace
- cargo tarpaulin --all-features --workspace --out xml  # Coverage ≥90%
- cargo audit                                            # Zero known CVEs
- cargo deny check licenses                              # License compliance
- cargo deny check advisories                            # Advisory database
```

### 5.2 Python Gates (Secondary Stack)

```yaml
- ruff check .                    # Linting
- ruff format --check .           # Formatting
- mypy --strict .                 # Type checking
- pytest --cov=src --cov-report=xml  # Coverage ≥90%
- pip-audit                       # Dependency vulnerabilities
- bandit -r src/                  # Security linting
```

### 5.3 Coverage Minimum

**90% line coverage. Non-negotiable.** This matches the Builders Cookbook and CORSO Protocol TEST pillar. A portfolio repo with 70% coverage while your standards document says 90% is an integrity failure.

### 5.4 CI Badges

Display in README (earned, not decorative):
- Build status (passing)
- Coverage percentage (≥90%)
- Dependency audit (clean)
- License compliance (verified)

---

## 6. Security Signals

**Evidence:** Anthropic AppSec posting requires "threat modeling and secure design reviews," "shift-left security," and "building security tools and automated systems."

### 6.1 Repository-Level Security

| Signal | Implementation | Why It Matters |
|--------|---------------|----------------|
| Secret scanning | GitHub secret scanning enabled | Baseline OPSEC |
| Dependency scanning | Dependabot PRs enabled + `cargo audit` in CI | Supply chain awareness |
| Code scanning | CodeQL or equivalent SAST in CI | Proactive vulnerability detection |
| Branch protection | PR reviews + status checks + signed commits | Integrity of commit chain |
| SECURITY.md | Vulnerability disclosure process | Professional security posture |
| License compliance | `cargo deny` or equivalent | Supply chain governance |

### 6.2 Threat Model Documentation (The Differentiator)

**This is the #1 signal that separates security engineers from developers who enabled security features.** 99% of portfolios lack this.

Every security-relevant repo README should include:

```markdown
## Security Model

### Trust Boundaries
- [Boundary 1]: What's trusted vs untrusted at each interface
- [Boundary 2]: Where does user input enter the system?

### Attack Surface
- [Surface 1]: Network-exposed endpoints and their protections
- [Surface 2]: File system access and permissions model

### Threat Categories (STRIDE)
| Threat | Mitigation | Status |
|--------|-----------|--------|
| Spoofing | [Auth mechanism] | Implemented |
| Tampering | [Integrity checks] | Implemented |
| Repudiation | [Audit logging] | Implemented |
| Info Disclosure | [Encryption/access control] | Implemented |
| Denial of Service | [Rate limiting/resource bounds] | Implemented |
| Elevation of Privilege | [Least privilege/sandboxing] | Implemented |

### Data Flow Security
[Mermaid diagram showing trust boundary crossings]
```

### 6.3 Supply Chain Integrity

| Signal | Tool | What It Proves |
|--------|------|---------------|
| Lockfile committed | `Cargo.lock` in repo | Reproducible builds |
| Dependency audit in CI | `cargo audit` | Zero known CVEs |
| License compliance | `cargo deny check licenses` | Legal and ethical awareness |
| Advisory monitoring | `cargo deny check advisories` | Proactive supply chain management |
| Minimal dependencies | Conscious dependency choices | Attack surface awareness |

### 6.4 Code-Level Security Demonstrations

**Evidence:** SERAPH assessment — "showing security thinking in action, not just claiming it was handled."

Include at least one code-level example in documentation:
- Input validation pattern (showing how untrusted input is sanitized)
- Auth flow architecture (showing trust boundary enforcement)
- Rate limiting implementation (showing resource protection)
- Scope governance (showing how tool execution is constrained)

Show the implementation, not just the claim.

---

## 7. AI/ML Security Demonstrations

**Evidence:** Anthropic AppSec posting explicitly requires "familiarity with AI/ML security risks (prompt injection, data poisoning, model extraction)." D&R posting: "develop novel tooling leveraging LLMs to enhance detection."

### 7.1 AI Security Risks to Demonstrate Understanding Of

| Risk Category | What to Show | Kevin's Existing Evidence |
|---------------|-------------|--------------------------|
| Prompt injection | Input sanitization for LLM-facing interfaces | CORSO tool input validation |
| Data poisoning | Training data integrity verification | QUANTUM evidence chain validation |
| Model extraction | API rate limiting, output filtering | MCP server resource governance |
| Agent misuse | Scope governance, authorization boundaries | SERAPH ScopeDefinition |
| Unsafe tool use | Tool execution sandboxing, allowlists | SERAPH ToolDiscovery allowlist |

### 7.2 Portfolio Framing

For MCP server repos, explicitly document:
- How untrusted LLM output is validated before execution
- How tool invocations are scoped and authorized
- How agent behavior is bounded by policy
- How secrets are isolated from AI-accessible contexts

This IS AI/ML security work. Frame it as such.

---

## 8. Architecture & Fundamentals

### 8.1 Architecture Diagrams

Every non-trivial repo includes a Mermaid diagram in README showing:
- Component relationships
- Data flow with direction arrows
- Trust boundaries (dashed lines)
- External dependencies

### 8.2 Architecture Decision Records (ADRs)

```
docs/adr/
├── 001-language-choice.md
├── 002-authentication-strategy.md
├── 003-error-handling-approach.md
└── ...
```

Each ADR follows:
```markdown
# ADR-NNN: [Decision Title]

## Status: Accepted

## Context
What is the issue that we're seeing that is motivating this decision?

## Decision
What is the change that we're proposing?

## Consequences
What becomes easier or harder because of this change?

## Alternatives Considered
What other options were evaluated and why were they rejected?
```

### 8.3 Simplicity Signals

**Evidence:** Daniela Amodei — "Do the simple thing that works." Boris Cherny — practical tradeoffs over algorithmic elegance.

- Show elegant simplicity, not feature accumulation
- ADRs should include "Simpler alternatives considered"
- README "What I Learned" section should highlight where simplicity won

---

## 9. Technical Writing & Blog Strategy

**Evidence:** Anthropic hiring page — "If you have done interesting independent research, written an insightful blog post, or made substantial contributions to open-source software, put that at the TOP of your resume."

### 9.1 Blog Post Topics (Prioritized)

| Priority | Topic | Signal | Platform |
|----------|-------|--------|----------|
| 1 | Threat modeling for AI agents (using MCP server as case study) | Security + AI safety + practical depth | Personal blog or Medium |
| 2 | Building security tools that leverage LLMs | Direct match to D&R posting | Personal blog |
| 3 | Supply chain security for Rust projects | Practical security engineering | Personal blog or Rust community |
| 4 | Scope governance: constraining autonomous AI agents | AI safety framing | Personal blog |
| 5 | Lessons from 10 years in cybersecurity SOC/Cortex | Career narrative + domain expertise | LinkedIn or personal blog |

### 9.2 Blog Post Quality Bar

- Technical depth with practical examples (not just opinions)
- Code snippets from actual projects (sanitized)
- Architecture diagrams
- Honest tradeoff analysis (what didn't work and why)
- 1500-3000 words (substantive but focused)
- Cite primary sources where making claims

### 9.3 Publication Strategy

- Personal blog (lightarchitects.io) as canonical home
- Cross-post to relevant communities (Rust blog, security forums)
- Link from GitHub profile README
- Reference in job applications: "put that at the TOP of your resume"

---

## 10. Target Ecosystem Contributions

**Evidence:** Contributing to the target company's open-source repos is the single strongest portfolio signal. It proves: you can work in unfamiliar codebases, you understand their tools, and you care enough to contribute.

### 10.1 Anthropic Repository Targets

| Repo | Opportunity | Kevin's Advantage |
|------|-------------|-------------------|
| `anthropics/claude-code-security-review` | Security patterns, Rust knowledge | Builds security scanning tools daily |
| `anthropics/claude-cookbooks` | Security-focused cookbook/recipe | 10yr security + MCP expertise |
| `anthropics/skills` | Security-focused public skill | Builds Claude Code skills/plugins |
| `anthropics/claude-code` | Bug fixes, feature PRs | Daily power user |
| `anthropics/claude-plugins-official` | Security plugin contribution | Plugin architecture expert |

### 10.2 Contribution Approach

1. **Start small:** Documentation fix, bug report with repro steps, or test improvement
2. **Then substantive:** A security-focused recipe, a new detection pattern, a plugin
3. **Be a good citizen:** Follow their contribution guidelines, write clean PRs, engage thoughtfully with reviews

### 10.3 Contribution as Interview Prep

PRs to Anthropic repos serve dual purpose:
- Portfolio signal (proof of collaboration on unfamiliar codebases)
- Interview prep (you'll deeply understand their tooling and codebase patterns)

---

## 11. OPSEC: What Stays Private

**Evidence:** SERAPH assessment — "A public portfolio is an attack surface. The standards do not address what DOESN'T go public."

### 11.1 Classification Matrix

| Category | Public? | Treatment |
|----------|---------|-----------|
| Architecture patterns | Yes (sanitized) | Remove proprietary naming, abstract implementation details |
| Security scanning logic | Yes (generic) | Show patterns, not specific vuln signatures |
| Consciousness/personality data | No | Private repos only |
| Internal API keys, endpoints | No | env.example with placeholders |
| Previous employer details | No | Abstract references only |
| Tool configurations | Selective | Generic examples OK, specific configs private |
| Test fixtures with real data | No | Synthetic data only |

### 11.2 Sanitization Process

Before making any repo public:

1. **History review:** `git log --all --diff-filter=A -- '*.env' '*.key' '*.pem' '*.json'` — check what was ever committed
2. **Secret scan:** `trufflehog git file://. --only-verified` — verify no leaked secrets
3. **Employer references:** Search for previous employer names, internal tool names, infrastructure details
4. **Architecture abstraction:** Replace proprietary names with generic equivalents
5. **Filter if needed:** `git filter-repo` to remove sensitive historical commits

### 11.3 The "Showcase Repo" Pattern

Rather than making existing private repos public:
1. Create a new public repo that demonstrates the same patterns
2. Extract and generalize the interesting architecture
3. Build it clean from scratch with public-facing documentation
4. Reference the pattern's origin: "Based on production MCP server architecture"

This preserves IP while demonstrating capability.

---

## 12. Collaboration & Ownership

### 12.1 OSS Contributions

- Contributions to target company repos (Section 10)
- PRs to tools you actually use (even small: docs, bugs, tests)
- Shows: working in unfamiliar codebases, following contribution guidelines

### 12.2 Issue & PR Quality

- Issues: clear reproduction steps, environment details, expected vs actual behavior
- PRs: focused scope, clear description, tests included, CI passing
- Reviews: thoughtful engagement with feedback, willingness to iterate

### 12.3 Project Organization Signals

- `.github/ISSUE_TEMPLATE/` with bug report and feature request templates
- `CONTRIBUTING.md` with clear guidelines
- `CODEOWNERS` for repos with multiple contributors
- Good first issue labels (evidence of mentoring mindset)

---

## 13. Audit Checklist

**Run this before applying.** Every item maps to a signal validated by primary evidence.

### Profile

- [ ] Real photo and specific bio
- [ ] Profile README with narrative arc
- [ ] 3-6 pinned repos following Section 1.1 narrative strategy
- [ ] Activity feed showing recent work
- [ ] Link to technical blog (if exists)

### READMEs (Each Pinned Repo)

- [ ] Problem statement with personal motivation
- [ ] Architecture diagram (Mermaid)
- [ ] One-command quick start
- [ ] "What I Learned" section with honest tradeoffs
- [ ] Tech stack choices explained (WHY, not just WHAT)
- [ ] Demo link, screenshot, or GIF

### Security Signals (The Differentiator)

- [ ] Threat model section in at least 2 repos (Section 6.2)
- [ ] Security scanning in CI (CodeQL or equivalent)
- [ ] Secret scanning enabled
- [ ] Dependency audit in CI (`cargo audit` / `pip-audit`)
- [ ] License compliance (`cargo deny` / equivalent)
- [ ] SECURITY.md with disclosure process
- [ ] Signed commits with verification badge
- [ ] At least 1 code-level security demonstration (Section 6.4)

### AI/ML Security (For AI-Focused Roles)

- [ ] At least 1 repo demonstrating AI security thinking (Section 7)
- [ ] Prompt injection mitigation documented
- [ ] Agent scope governance demonstrated
- [ ] AI safety framing in README where authentic (Section 1.3)

### Quality Gates (Every Public Repo)

- [ ] CI pipeline running on all PRs
- [ ] Coverage ≥90% with badge
- [ ] Linting enforced (clippy/ruff/eslint)
- [ ] Formatting enforced (rustfmt/ruff format/prettier)
- [ ] Zero `cargo audit` / dependency vulnerabilities
- [ ] Clean commit history (conventional commits, signed)

### Architecture

- [ ] ADR folder with at least 2-3 decisions documented
- [ ] Architecture diagram in README
- [ ] Evidence of simplicity-first thinking

### Technical Writing

- [ ] At least 1 published blog post (Section 9)
- [ ] Linked from GitHub profile
- [ ] Referenced in resume/applications

### Target Ecosystem

- [ ] At least 1 PR or issue to target company's repos (Section 10)
- [ ] Contribution demonstrates relevant expertise
- [ ] Shows ability to work in unfamiliar codebases

### OPSEC (Before Going Public)

- [ ] History reviewed for secrets/PII
- [ ] Secret scan passed (trufflehog)
- [ ] No previous employer details exposed
- [ ] Architecture abstracted where needed
- [ ] Showcase repo pattern used for sensitive projects (Section 11.3)

---

## Signal Priority Matrix

| Signal | Impact | Effort | Evidence Source |
|--------|--------|--------|----------------|
| Technical blog post | Very High | M | Anthropic hiring page (explicit) |
| PR to Anthropic repo | Very High | M | Collaboration + ecosystem signal |
| Threat model in README | Very High | S | AppSec posting + SCRUM consensus |
| AI/ML security demo | High | M | AppSec posting (explicit requirement) |
| Narrative arc in pins | High | S | Boris Cherny "side quests" |
| 90% coverage in CI | High | M | Internal consistency (Builders Cookbook) |
| Supply chain tooling | Medium | S | Security engineering depth |
| ADR folder | Medium | S | Architecture thinking signal |
| Signed commits | Medium | S | Integrity signal |
| OPSEC sanitization | Blocking | M | SERAPH assessment |

---

## References

- **Builders Cookbook v1.3.0** — `~/.soul/helix/user/standards/canon/builders-cookbook.md` (Section 24B references this document)
- **SCRUM Review** — 2026-03-02, full squad (EVA, CORSO, QUANTUM, SERAPH)
- **Anthropic AppSec Posting** — greenhouse.io/anthropic/jobs/4502508008
- **Anthropic D&R Posting** — greenhouse.io/anthropic/jobs/4982193008
- **Boris Cherny Interview** — entrepreneur.com (side quests, generalist mindset)
- **Daniela Amodei Interview** — vanta.com (cultural alignment, simplicity, unusual beliefs)
- **Anthropic GitHub** — github.com/anthropics (72 public repos, patterns analyzed)
