<!-- uuid: catalogue-root — this file is self-describing, not assigned a document UUID -->

# Standards — Master UUID Catalogue

**Single canonical lookup** for every document under `helix/user/standards/`.
UUID is permanently assigned to a document instance. Re-pulls and renamed files get new UUIDs; old entries are marked `[archived]`.

**Canonical path**: `helix/user/standards/UUID-CATALOGUE.md`
**Total documents**: 97 (53 industry baselines + 44 canonical standards)
**Generated**: 2026-05-04
**Expanded**: 2026-05-04 (added 22 new industry-baselines entries — 17 free live-pulled + 2 paywalled stubs + 1 academic + 2 from parallel session)
**Reorganized**: 2026-05-04 (canon/ + runbooks/ + licenses/ consolidation; industry-baselines/ regrouped by [ASQPTDO] gate)
**W2 additions**: 2026-05-05 (squad-synthesizer-protocol.md + synthesize-squad-review.py)

Reference scheme: `std://<uuid>` or full relative path from `helix/user/standards/`.

## Standards directory layout

```
standards/
├── canon/              # 22 LA-normative documents
├── industry-baselines/ # 53 external standards (organized by [ASQPTDO] gate)
├── cookbooks/          # 8 CORSO cookbooks
├── licenses/           # 5 LICENSE templates + 5 licensing CI artifacts
├── runbooks/           # 3 operational runbooks
├── research/           # 9 research notes
├── archive/            # 1 archived doc
├── scripts/            # tooling scripts
├── _index-standards.md
├── UUID-CATALOGUE.md
└── lessons-learned.md
```

---

## Canonical Standards Documents

### `canon/` — LightArchitects normative canon

The LA-specific body of doctrine. Moved from root 2026-05-04 to clearly distinguish LA's own normative documents from external industry baselines.

| UUID | File | Description |
|------|------|-------------|
| `121ca105-7e10-42e8-8229-449d4f93031f` | `canon/platform-canon.md` | Canon (master) |
| `c1976ecb-11ef-4a88-b33a-0b54b32ec2b7` | `canon/canon-xxx-strand-mosaic.md` | Canon XXX strand mosaic |
| `25080cf0-42a7-4ac3-aabd-89d70d70ff0d` | `canon/builders-cookbook.md` | Builders Cookbook (coding canon) |
| `30b55cb7-e5d9-4f58-9e48-cf5d2c218029` | `canon/coding-guidelines.md` | Coding guidelines (companion to Builders Cookbook) |
| `f0165963-e558-4aa5-9748-ea8069ef23fb` | `canon/lasdlc-spec.md` | LASDLC specification |
| `0620a6c1-21a9-48da-beaf-b3c90b2e6870` | `canon/lasdlc-effectiveness-rubric.md` | LASDLC Effectiveness Rubric |
| `2bc243b7-6196-4d03-ace5-e1e3f65b8f98` | `canon/mvt-protocol.md` | MVT protocol (token efficiency) |
| `70f30043-678f-4a43-91f5-78dc605650e4` | `canon/soul-cycle.md` | SOUL cycle |
| `4dd9a5b6-8830-4d2b-8386-b499413d1e5a` | `canon/verification-protocol.md` | Verification protocol |
| `7f160cf2-8d81-4471-ae86-c0a811cfc0c0` | `canon/parallel-execution-policy.md` | Parallel execution policy |
| `b8bb57bd-dfde-41fd-8789-29af91e84293` | `canon/parallel-dispatch-principles.md` | Parallel dispatch principles |
| `81ee1444-29f5-45dc-85dd-8fd748f1902f` | `canon/recursion-termination-invariant.md` | Recursion termination invariant |
| `66f7e51a-2cd8-4423-b9c8-8294af412200` | `canon/agent-architecture.md` | Agent (squad) architecture |
| `f9d12d4b-6349-4ef4-b016-2abfdf46de86` | `canon/agent-dispatch-templates.md` | Agent dispatch templates |
| `3c59984b-ea51-4ec9-bdfc-80b4ed798d5d` | `canon/lens-driven-squad-selection.md` | Lens-driven squad selection |
| `5152f864-eaa2-42bc-b3aa-3b9124036235` | `canon/bond-007-identity-template.md` | Bond-007 identity template |
| `59547bf3-c1d3-4a09-b84f-07de435083e2` | `canon/platform-architecture-v2.md` | Platform architecture v2 |
| `d2460c90-ea43-457b-823d-5287be2399b0` | `canon/five-star-engineering-targets.md` | Five-star engineering targets |
| `2bd60433-a85a-4811-a1d5-d35a2c466f30` | `canon/architects-blueprint.md` | Architects Blueprint (renamed 2026-05-13 from gold-standard-planning-framework) |
| `d6a8a226-7488-4054-94ac-ceb83bc380be` | `canon/portfolio-standards.md` | Portfolio standards |
| `381b62ed-cb1b-4346-bfad-e82d8540cc1f` | `canon/training-standard.md` | Training standard |
| `27063030-a375-43da-b73b-0fff91b45b15` | `canon/research-output-standard.md` | Research output standard |
| `3f345852-c5ed-4cd9-8a4a-d8272787bb9f` | `canon/gatekeeper-registry.yaml` | Gatekeeper registry — agent-to-gate authority map (LASDLC v2.5.0) |
| `7c4e9b2a-1f83-4d5e-8a61-c0e3d5f2b784` | `canon/squad-synthesizer-protocol.md` | Squad Synthesizer — fan-in algorithm: 7 gate_evaluations → squad_review verdict |
| `scripts-synth-no-uuid` | `scripts/synthesize-squad-review.py` | Reference Python implementation of Squad Synthesizer (not a standards doc; no UUID) |

### Root — index + journal

Files that remain at the standards/ root.

| UUID | File | Description |
|------|------|-------------|
| `272cba00-3a9c-4bd8-9f65-7714b9e3e854` | `_index-standards.md` | Standards index (Obsidian wiki-link hub) |
| `d0703354-4f9c-4b93-94e6-a97ef5fb2a2d` | `lessons-learned.md` | Operational lessons-learned journal |

(`UUID-CATALOGUE.md` is this file — no UUID assigned to itself; it's the catalogue.)

### `licenses/` — license templates + licensing CI infrastructure

LICENSE-* template files for each supported license type, plus the CI guards that detect license-line drift, the migration playbook, and the deny.toml template. Consolidated 2026-05-04 from the standards/ root.

| UUID | File | Description |
|------|------|-------------|
| _(no UUID — license text)_ | `licenses/LICENSE-AGPL-3.0-only` | AGPL-3.0-only license text (template) |
| _(no UUID — license text)_ | `licenses/LICENSE-Apache-2.0` | Apache-2.0 license text (template) |
| _(no UUID — license text)_ | `licenses/LICENSE-LA-Proprietary` | LA-Proprietary license text (template) |
| _(no UUID — license text)_ | `licenses/LICENSE-MIT` | MIT license text (template) |
| _(no UUID — license text)_ | `licenses/LICENSE-MPL-2.0` | MPL-2.0 license text (template) |
| `e02604c3-0edd-4ba0-8cb5-19fb4c953769` | `licenses/license-migration-playbook.md` | License migration runbook |
| `c005461f-0360-4d85-b618-9530aa21c4c9` | `licenses/notice-template.md` | NOTICE template (header + license declaration block) |
| `94c715fc-a241-42f7-adf1-b14c5d29f7c8` | `licenses/deny-toml-template.toml` | cargo-deny config template per license type |
| `f8979dc6-296f-4442-ae6e-65b07bbf2f58` | `licenses/license-line-ci.yml` | License-line CI guard (detects license drift in non-license PRs) |
| `4a6bcdf8-326b-4d99-9edf-0b82eb04cb5f` | `licenses/workspace-integrity-ci.yml` | Workspace integrity CI gate |

### `runbooks/` — operational procedures

Operational checklists and runbooks. Consolidated 2026-05-04 from the standards/ root. Distinguished from `canon/` because these are procedural (how-to) rather than normative (what-is).

| UUID | File | Description |
|------|------|-------------|
| `c85f4041-cfe3-4542-85f0-f2df15e84fac` | `runbooks/ai-detection-checklist.md` | AI detection checklist |
| `067dc873-875c-4b5f-90c5-589f79ec27bd` | `runbooks/secret-leak-runbook.md` | Secret leak remediation runbook |
| `9d4c4b5a-cbbe-469f-88c6-abdf581040f9` | `runbooks/tts-voice-production.md` | TTS voice production runbook |

### `cookbooks/`

| UUID | File | Description |
|------|------|-------------|
| `139d1d0b-9673-43c2-b58c-99ae57da79be` | `cookbooks/00-getting-started.md` | Getting started |
| `b501151a-b97a-4e79-96f9-e45c7573032f` | `cookbooks/01-foundations.md` | Foundations |
| `ed17a31d-14dd-43a5-9886-556ce227b316` | `cookbooks/02-orchestrator.md` | Orchestrator |
| `cfaed4d6-0ba3-429f-9fac-3d2139614a41` | `cookbooks/03-security.md` | Security |
| `8115682c-f317-4d57-b22e-aad174d0d8b8` | `cookbooks/04-provider.md` | Provider |
| `5c76aea5-5e2e-4ad5-9dab-89405406d54f` | `cookbooks/05-mcp.md` | MCP |
| `6af53838-1dc8-4074-bbab-403c9dded7bc` | `cookbooks/06-workflow.md` | Workflow |
| `38ad3ef7-fe3e-4a31-bb1e-307501757e74` | `cookbooks/07-reference.md` | Reference |

### `archive/`

| UUID | File | Description |
|------|------|-------------|
| `0a085a01-eade-4a35-ab9d-95a57dcb7a4d` | `archive/platform-architecture-v1.md` | Platform architecture v1 (archived) |

---

## Industry Baselines (`industry-baselines/`)

Organized by LASDLC [ASQPTDO] gate. Issuing body is preserved as subfolder within each gate. Full source URLs and re-pull policy: `industry-baselines/REGISTRY.md`.

### [A] Architecture

| UUID | File | Standard | Version |
|------|------|----------|---------|
| `b6671805-4865-44a4-88aa-a6dc381acd9b` | `industry-baselines/architecture/ieee/ieee-42010-2022-stub.md` | IEEE/ISO/IEC 42010 Architecture Description | 2022 |

### [S] Security

| UUID | File | Standard | Version |
|------|------|----------|---------|
| `9d413b9c-bf64-4752-be3e-630b9b203f0c` | `industry-baselines/security/nist/nist-ssdf-v1.1-2026-05-04.md` | NIST SP 800-218 SSDF | v1.1 |
| `aa65a42a-a5e0-4047-838f-52bbc7775ad3` | `industry-baselines/security/nist/nist-ssdf-practices-2026-05-04.md` | NIST SP 800-218 SSDF Practices | v1.1 |
| `691e1048-913d-4ca8-adfa-c3cba1d7e2e9` | `industry-baselines/security/nist/nist-csf-v2.0-2026-05-04.md` | NIST Cybersecurity Framework | v2.0 (Feb 2024) |
| `6167e411-d0d0-4179-9d46-86bcabdcce0d` | `industry-baselines/security/nist/nist-sp-800-53-rev5-2026-05-04.md` | NIST SP 800-53 Security Controls | Rev 5 + upd1 |
| `833d4db5-968b-4eb2-987c-9e6c3479c584` | `industry-baselines/security/nist/nist-sp-800-63b-2026-05-04.md` | NIST SP 800-63B Authentication | SP 800-63B (final) |
| `96fe69e5-637b-4651-a06d-d913c7290cf6` | `industry-baselines/security/owasp/owasp-asvs-2026-05-04.md` | OWASP ASVS | v4.0 |
| `6f395a3c-af6a-41dd-b483-f03542ec1888` | `industry-baselines/security/owasp/owasp-top-10-project-2026-05-04.md` | OWASP Top 10 (project page) | 2021 |
| `fd958397-8c56-41a3-bbd0-38d06cbacd28` | `industry-baselines/security/owasp/owasp-top-10-2021-2026-05-04.md` | OWASP Top 10 (full list) | 2021 |
| `6d40d2e9-625f-4847-addd-8d887d79a775` | `industry-baselines/security/owasp/owasp-llm-top-10-v1.1-2026-05-04.md` | OWASP LLM Top 10 | v1.1 |
| `a4c1708d-4583-49c4-b4c9-f696ea444801` | `industry-baselines/security/owasp/owasp-api-security-top-10-2023-2026-05-04.md` | OWASP API Security Top 10 | 2023 |
| `281c3946-8fac-4f97-bf67-9b011dec7e5a` | `industry-baselines/security/owasp/owasp-samm-v2.0-2026-05-04.md` | OWASP SAMM | v2.0 |
| `d22d21d5-aa20-4255-9d8c-6f9a7b93b15e` | `industry-baselines/security/owasp/cyclonedx-v1.6-2026-05-04.md` | CycloneDX SBOM | v1.6 |
| `a993bb15-b3ec-4803-815a-5771046435ae` | `industry-baselines/security/mitre/mitre-attack-enterprise-2026-05-04.md` | MITRE ATT&CK Enterprise | v15 (2024) |
| `7a014e8c-7c35-42ee-b3f4-0c61339fdde0` | `industry-baselines/security/mitre/mitre-atlas-2026-05-04.md` | MITRE ATLAS | v4.5 |
| `a9aebc9f-fa75-4c20-9372-84df8f204276` | `industry-baselines/security/mitre/mitre-cwe-top-25-2024-2026-05-04.md` | CWE Top 25 | 2024 |
| `573f2fa8-8ebf-4a36-976b-d8140aead9c2` | `industry-baselines/security/mitre/mitre-cwe-top-25-2024-list-2026-05-04.md` | CWE Top 25 (list only) | 2024 |
| `0d59db0f-e8d3-40cd-a7f4-9c21530ff195` | `industry-baselines/security/iso/iso-27001-2022-stub.md` | ISO/IEC 27001 Information Security | 2022 |
| `0da768f1-056a-4150-8b13-9de7292a35b7` | `industry-baselines/security/iso/iso-27034-stub.md` | ISO/IEC 27034 Application Security | multi-part |
| `cb89e7a8-77d3-4d06-b550-ce631a037918` | `industry-baselines/security/cis/cis-controls-v8-2026-05-04.md` | CIS Controls | v8 |
| `0024b111-e186-4055-bbc6-f67e6982a0d1` | `industry-baselines/security/openssf/slsa-spec-v1.0-2026-05-04.md` | SLSA Specification | v1.0 |
| `74c62141-0443-415b-b573-c87b197fba07` | `industry-baselines/security/openssf/slsa-levels-v1.0-2026-05-04.md` | SLSA Security Levels | v1.0 |
| `d518d2fb-268c-4097-9d4d-6b122abe9016` | `industry-baselines/security/openssf/slsa-threats-v1.0-2026-05-04.md` | SLSA Threats & Mitigations | v1.0 |
| `bb146bec-368c-4402-be62-ddd3c70b638f` | `industry-baselines/security/linux-foundation/spdx-v2.3-2026-05-04.md` | SPDX SBOM specification | v2.3 (ISO/IEC 5962:2021) |
| `3660c050-c307-476e-9169-eeecbf9897fe` | `industry-baselines/security/microsoft/microsoft-stride-2026-05-04.md` | Microsoft STRIDE threat model | SDL canonical |
| `74c76212-2429-4a0c-a6d2-f32e9d09daf4` | `industry-baselines/security/linddun/linddun-2026-05-04.md` | LINDDUN privacy threat modelling | current |
| `17588fec-570e-456c-9a58-0dbb9ac6287f` | `industry-baselines/security/eu/gdpr-article-25-2026-05-04.md` | GDPR Article 25 (Privacy by Design) | Reg. (EU) 2016/679 |
| `8086dbcc-37c7-450a-800c-4ccff8ae75b4` | `industry-baselines/security/eu/eu-ai-act-2026-05-04.md` | EU AI Act | Reg. (EU) 2024/1689 |
| `edd27f25-aeb5-4b23-98c3-28f186895352` | `industry-baselines/security/aicpa/aicpa-soc-2-type-ii-2026-05-04.md` | AICPA SOC 2 Type II | TSC 2017 (rev 2022) |

### [Q] Quality

| UUID | File | Standard | Version |
|------|------|----------|---------|
| `ab9aad9a-786d-4077-8ede-3ec2c2c86456` | `industry-baselines/quality/iso/iso-25010-2023-stub.md` | ISO/IEC 25010 Software Quality | 2023 |
| `82e414cd-8b4d-49bc-8305-b685acdca942` | `industry-baselines/quality/iso/iso-25023-2016-stub.md` | ISO/IEC 25023 Quality Measurement | 2016 |
| `e1019be4-c297-4772-bd3c-e5f209abedd7` | `industry-baselines/quality/iso/iso-5055-2021-stub.md` | ISO/IEC 5055 CISQ ASCQM | 2021 |
| `bbe7864a-1a2c-4687-87e9-46c9b6c9287b` | `industry-baselines/quality/cisq/cisq-cost-poor-quality-2022-2026-05-04.md` | CISQ Cost of Poor Software Quality | 2022 |
| `c9041ba3-335d-4f3d-baca-cac463c15249` | `industry-baselines/quality/w3c/w3c-wcag-2.2-2026-05-04.md` | W3C WCAG (Web Content Accessibility Guidelines) | 2.2 |

### [P] Performance

| UUID | File | Standard | Publication |
|------|------|----------|------------|
| `20aac364-b451-48b2-a11c-193c02d2574a` | `industry-baselines/performance/academic/amdahl/amdahl-1967.md` | Amdahl's Law | AFIPS 1967 |
| `ef54e129-4fb5-44a3-b3db-fc4c6b909769` | `industry-baselines/performance/academic/gustafson/gustafson-1988.md` | Gustafson's Law | CACM 1988 |
| `8396cb33-2c83-441c-b9e0-6cce2232033f` | `industry-baselines/performance/academic/karp-flatt/karp-flatt-1990.md` | Karp–Flatt Metric | CACM 1990 |
| `335d2e82-58f0-4d4a-83d5-eadbdd33e8f6` | `industry-baselines/performance/academic/little/little-1961.md` | Little's Law | Operations Research 1961 |
| `a8fab7b0-e408-4cf9-897d-2cb076d044e7` | `industry-baselines/performance/academic/critical-path/kelley-walker-1959.md` | Critical Path Method | Kelley & Walker, EJCC 1959 |

### [T] Testing

| UUID | File | Standard | Version / Publication |
|------|------|----------|----------------------|
| `bf8fd8a9-aba2-4d0e-a591-e070ad90d92b` | `industry-baselines/testing/owasp/owasp-wstg-v4.2-2026-05-04.md` | OWASP WSTG (Web Security Testing Guide) | v4.2 |
| `f1a8ae5b-e3e3-4807-9db2-db32d3c730ab` | `industry-baselines/testing/academic/ml-test-score/breck-2017.md` | ML Test Score | Breck et al., IEEE Big Data 2017 |

### [D] Documentation

_Placeholder — no external standards anchored yet. See `industry-baselines/documentation/README.md` for candidates._

### [O] Operations

| UUID | File | Standard | Version |
|------|------|----------|---------|
| `09cd905b-8908-4aaf-875f-d8f53353711a` | `industry-baselines/operations/google-cloud/dora-metrics-landing-2026-05-04.md` | DORA Metrics | 2024 |
| `6785e805-747c-4ef3-a576-ebf55f440edc` | `industry-baselines/operations/google-cloud/dora-research-index-2026-05-04.md` | DORA Research Index | 2024 |
| `01ba28a7-3fd7-491e-acd6-9849669074cd` | `industry-baselines/operations/google-cloud/state-of-devops-2024-2026-05-04.md` | DORA State of DevOps Report | 2024 |
| `835daeee-7b29-4b37-af01-29c59b158b07` | `industry-baselines/operations/cncf/opentelemetry-semconv-2026-05-04.md` | OpenTelemetry Semantic Conventions | 1.29.0 |
| `d2be8492-bd64-42dd-950f-7defa0832dfd` | `industry-baselines/operations/cncf/opentelemetry-trace-api-2026-05-04.md` | OpenTelemetry Trace API | 1.29.0 |
| `a492c5c4-af1f-4f4f-b1d1-ce648c0c66af` | `industry-baselines/operations/cncf/opentelemetry-metrics-api-2026-05-04.md` | OpenTelemetry Metrics API | 1.29.0 |
| `2289fdfd-315d-4495-8915-10b9f5c94e0e` | `industry-baselines/operations/cncf/apdex-spec-v1.0-2026-05-04.md` | Apdex Specification | v1.0 |
| `9f8ccc37-6b77-494e-9cbc-9d0c998e409b` | `industry-baselines/operations/cncf/apdex-methodology-2026-05-04.md` | Apdex Methodology | — |
| `572d95ae-45c9-4187-a0a2-89100c774858` | `industry-baselines/operations/google/google-sre-golden-signals-2026-05-04.md` | Google SRE Golden Signals | SRE Book Ch.6 canonical |
| `81e8a3cf-e86c-42bd-acdb-ad1701d6e5cc` | `industry-baselines/operations/w3c/w3c-trace-context-v1-2026-05-04.md` | W3C Trace Context | v1 (Recommendation) |
| `5ec6b420-83dd-49f2-9d59-b996b1c35721` | `industry-baselines/operations/academic/space/forsgren-2021.md` | SPACE Framework | Forsgren et al., ACM Queue 2021 |
| `13b48bd7-9523-4ab8-b040-c1c41e2c84a9` | `industry-baselines/operations/academic/flow/kersten-2018.md` | Flow Framework | Kersten, IT Revolution 2018 |

## UUID policy

- UUIDs are **permanent**. A re-pulled document (new date) gets a new UUID; old entry is marked `[archived]`.
- UUID is embedded as `<!-- uuid: ... -->` (Markdown) or `# uuid: ...` (YAML/TOML) on line 1 of every document.
- **This file is the single lookup** — do not maintain UUID lists elsewhere.
- Reference scheme: `std://<uuid>` or full path from `helix/user/standards/`.
- When adding new documents: generate UUID with `uuidgen | tr '[:upper:]' '[:lower:]'`, prepend to file, add row here.
