# Industry Baselines Registry

**Canonical index** of all industry-standard references cached in this directory. Used by LASDLC §7.7 LDB v1.0 for D1-D8 component anchoring per Canon XXXV [3] verbatim primary-source citation discipline.

**Canonical path**: `helix/user/standards/industry-baselines/`
**UUID catalogue**: `helix/user/standards/UUID-CATALOGUE.md` (master index for all 94 standards documents)
**Generated**: 2026-05-04
**Relocated**: 2026-05-04 (moved from `.calibration-sample/industry-baselines/` → `standards/industry-baselines/`)
**Expanded**: 2026-05-04 (added 22 standards across D2/D4/D5/D6/D8/cross-cutting domains per gap analysis)
**Reorganized**: 2026-05-04 (parent folders by LASDLC gate; issuing body retained as subfolder); **Expanded**: 2026-05-05 (gate vocabulary updated to [A+S+Q+C+O+P+K+D+T+R]; research/ folder added for [R] gate)
**Refreshed**: 2026-05-12 (14 baseline files updated to latest official versions: ATT&CK v15→v19.1, SLSA v1.0→v1.2, NIST 800-63B→800-63-4 + 800-63B-4, OWASP ASVS v4.0→v5.0.0, CWE Top 25 2024→2025, OTel 1.29.0→1.56.0, DORA 2024→2025, SPDX v2.3→v3.0.1, OWASP LLM v1.1 deleted in favour of v2.0)
**Total entries**: 61 (47 live-pulled + 8 academic + 6 paid-stubs) — +7 added 2026-05-22 (Khadas Edge2 hardware/NPU/add-ons, Khadas Captain carrier board, Rockchip RK3588 RKLLM benchmarks, Ollama cloud routing)

---

## Organization

Top-level folders correspond to LASDLC [A+S+Q+C+O+P+K+D+T+R] gates:

| Folder | Gate | Purpose |
|--------|------|---------|
| `architecture/` | [A] | Architecture description standards |
| `security/` | [S] | All security, threat-modeling, SBOM, compliance-as-security standards |
| `quality/` | [Q] | Software quality, accessibility, ASCQM measurement |
| `performance/` | [P] | Parallel/serial-fraction theory, throughput academic foundations |
| `testing/` | [T] | Test methodologies, ML readiness rubrics |
| `documentation/` | [D] | Doc framework standards (placeholder — no anchors yet) |
| `operations/` | [O] | DORA, observability, SRE, productivity, audit |

Issuing body is preserved as a subfolder within each gate (e.g. `security/owasp/`, `operations/cncf/`). Multi-gate standards live at their **primary-purpose gate** and are reverse-indexed from secondary gates below.

---

## Population status legend

| Symbol | Meaning |
|--------|---------|
| ✓ POPULATED | Full content scraped/cited; ready for D1-D8 reference |
| ⚠ PARTIAL | Landing page scraped; deeper sub-pages may need follow-up pull |
| 📚 ACADEMIC | Cited from canonical publication; no re-pull needed |
| 🔒 SCAFFOLDED | Paid standard; institutional access required to fully populate |

---

## [A] Architecture

| Source | Status | File | Original URL | Re-pull |
|--------|--------|------|--------------|---------|
| **IEEE/ISO/IEC 42010:2022** (Architecture Description) | 🔒 SCAFFOLDED | `architecture/ieee/ieee-42010-2022-stub.md` | https://standards.ieee.org/ieee/42010/6846/ | per IEEE/ISO revision |

### Khadas (Edge SBC carrier-board reference)
| Source | Status | File | Original URL | Re-pull |
|--------|--------|------|--------------|---------|
| **Khadas Captain Carrier Board for Edge** | ✓ POPULATED | `architecture/khadas/khadas-captain-carrier-board-2026-05-22.md` | https://www.khadas.com/captain | per Khadas product revision |

## [S] Security

### NIST
| Source | Status | File | Original URL | Re-pull |
|--------|--------|------|--------------|---------|
| **NIST SP 800-218 SSDF** v1.1 | ✓ POPULATED | `security/nist/nist-ssdf-v1.1-2026-05-04.md` | https://csrc.nist.gov/pubs/sp/800/218/final | per NIST revision |
| **NIST SSDF Practices** | ✓ POPULATED | `security/nist/nist-ssdf-practices-2026-05-04.md` | https://csrc.nist.gov/Projects/ssdf | per NIST revision |
| **NIST CSF 2.0** | ✓ POPULATED | `security/nist/nist-csf-v2.0-2026-05-04.md` | https://www.nist.gov/cyberframework | per NIST revision |
| **NIST SP 800-53 Rev 5** | ✓ POPULATED | `security/nist/nist-sp-800-53-rev5-2026-05-04.md` | https://csrc.nist.gov/pubs/sp/800/53/r5/upd1/final | per NIST revision |
| **NIST SP 800-63-4** (Digital Identity Guidelines, final Aug 2025) | ✓ POPULATED | `security/nist/nist-sp-800-63-4-2026-05-12.md` | https://pages.nist.gov/800-63-4/sp800-63.html | per NIST revision |
| **NIST SP 800-63B-4** (Authentication & Authenticator Mgmt, final Aug 2025) | ✓ POPULATED | `security/nist/nist-sp-800-63b-4-2026-05-12.md` | https://pages.nist.gov/800-63-4/sp800-63b.html | per NIST revision |

### OWASP
| Source | Status | File | Original URL | Re-pull |
|--------|--------|------|--------------|---------|
| **OWASP ASVS** v5.0.0 (final May 2025) | ✓ POPULATED | `security/owasp/owasp-asvs-v5.0.0-2026-05-12.md` | https://owasp.org/www-project-application-security-verification-standard/ | per ASVS GitHub releases |
| **OWASP Top 10** (project page) | ⚠ PARTIAL | `security/owasp/owasp-top-10-project-2026-05-04.md` | https://owasp.org/www-project-top-ten/ | annual / per OWASP release |
| **OWASP Top 10 2021** (full list) | ✓ POPULATED | `security/owasp/owasp-top-10-2021-2026-05-04.md` | https://owasp.org/Top10/ | per OWASP release |
| **OWASP LLM Top 10** v2.0 (2025 edition) | ✓ POPULATED | `security/owasp/owasp-llm-top-10-v2.0-2026-05-05.md` | https://owasp.org/www-project-top-10-for-large-language-model-applications/ | per OWASP release (~6mo) |
| **OWASP API Security Top 10 (2023)** | ✓ POPULATED | `security/owasp/owasp-api-security-top-10-2023-2026-05-04.md` | https://owasp.org/API-Security/editions/2023/en/0x00-header/ | per OWASP release |
| **OWASP SAMM v2.0** | ✓ POPULATED | `security/owasp/owasp-samm-v2.0-2026-05-04.md` | https://owaspsamm.org/model/ | per OWASP release |
| **CycloneDX 1.6** (SBOM) | ✓ POPULATED | `security/owasp/cyclonedx-v1.6-2026-05-04.md` | https://cyclonedx.org/specification/overview/ | per OWASP release |

### MITRE
| Source | Status | File | Original URL | Re-pull |
|--------|--------|------|--------------|---------|
| **MITRE ATT&CK Enterprise** v19.1 (Apr 2026) | ✓ POPULATED | `security/mitre/mitre-attack-enterprise-2026-05-12.md` | https://attack.mitre.org/matrices/enterprise/ | per MITRE matrix update |
| **MITRE ATLAS** v4.5 | ✓ POPULATED | `security/mitre/mitre-atlas-2026-05-04.md` | https://atlas.mitre.org/matrices/ATLAS | per MITRE update |
| **CWE Top 25** 2025 (Dec 2025) | ✓ POPULATED | `security/mitre/mitre-cwe-top-25-2025-2026-05-12.md` | https://cwe.mitre.org/top25/archive/2025/2025_cwe_top25.html | annual (MITRE refresh) |
| **CWE Top 25 2025** (list) | ✓ POPULATED | `security/mitre/mitre-cwe-top-25-2025-list-2026-05-12.md` | https://cwe.mitre.org/top25/archive/2025/2025_cwe_top25.html | annual |

### ISO (security)
| Source | Status | File | Original URL | Re-pull |
|--------|--------|------|--------------|---------|
| **ISO/IEC 27001:2022** | 🔒 SCAFFOLDED | `security/iso/iso-27001-2022-stub.md` | https://www.iso.org/standard/27001 | per ISO revision |
| **ISO/IEC 27034** (multi-part) | 🔒 SCAFFOLDED | `security/iso/iso-27034-stub.md` | https://www.iso.org/standard/44378.html | per ISO revision |

### CIS / OpenSSF / Linux Foundation
| Source | Status | File | Original URL | Re-pull |
|--------|--------|------|--------------|---------|
| **CIS Controls v8** | ✓ POPULATED | `security/cis/cis-controls-v8-2026-05-04.md` | https://www.cisecurity.org/controls/v8 | annual revision |
| **SLSA spec** v1.2 (final Nov 2025) | ✓ POPULATED | `security/openssf/slsa-spec-v1.2-2026-05-12.md` | https://slsa.dev/spec/v1.2/about | per OpenSSF release |
| **SLSA Build Track + Requirements** v1.2 | ✓ POPULATED | `security/openssf/slsa-levels-v1.2-2026-05-12.md` | https://slsa.dev/spec/v1.2/build-track-basics | per OpenSSF release |
| **SLSA Threats & Mitigations** v1.2 | ✓ POPULATED | `security/openssf/slsa-threats-v1.2-2026-05-12.md` | https://slsa.dev/spec/v1.2/threats | per OpenSSF release |
| **SPDX 3.0.1** (System Package Data Exchange) | ✓ POPULATED | `security/linux-foundation/spdx-v3.0.1-2026-05-12.md` | https://spdx.github.io/spdx-spec/v3.0.1/ | per SPDX revision |

### Threat modeling
| Source | Status | File | Original URL | Re-pull |
|--------|--------|------|--------------|---------|
| **Microsoft STRIDE** | ✓ POPULATED | `security/microsoft/microsoft-stride-2026-05-04.md` | https://learn.microsoft.com/en-us/azure/security/develop/threat-modeling-tool-threats | per Microsoft SDL update |
| **LINDDUN** (privacy threat modelling) | ✓ POPULATED | `security/linddun/linddun-2026-05-04.md` | https://linddun.org/ | per LINDDUN release |

### Regulatory / compliance (security-mandate variant)
| Source | Status | File | Original URL | Re-pull |
|--------|--------|------|--------------|---------|
| **GDPR Article 25** (Privacy by Design) | ✓ POPULATED | `security/eu/gdpr-article-25-2026-05-04.md` | https://eur-lex.europa.eu/eli/reg/2016/679/oj | per EU regulation revision |
| **EU AI Act** (Reg. 2024/1689) | ✓ POPULATED | `security/eu/eu-ai-act-2026-05-04.md` | https://artificialintelligenceact.eu/the-act/ | per EU regulation revision |
| **AICPA SOC 2 Type II** (TSC) | ✓ POPULATED | `security/aicpa/aicpa-soc-2-type-ii-2026-05-04.md` | https://www.aicpa-cima.com/topic/audit-assurance/audit-and-assurance-greater-than-soc-2 | per AICPA revision |

## [Q] Quality

### ISO (quality)
| Source | Status | File | Original URL | Re-pull |
|--------|--------|------|--------------|---------|
| **ISO/IEC 25010:2023** | 🔒 SCAFFOLDED | `quality/iso/iso-25010-2023-stub.md` | https://www.iso.org/standard/35733.html | per ISO revision |
| **ISO/IEC 25023:2016** (measurement) | 🔒 SCAFFOLDED | `quality/iso/iso-25023-2016-stub.md` | https://www.iso.org/standard/35747.html | per ISO revision |
| **ISO/IEC 5055:2021** (CISQ ASCQM) | 🔒 SCAFFOLDED | `quality/iso/iso-5055-2021-stub.md` | https://www.iso.org/standard/80623.html | per ISO revision |

### CISQ
| Source | Status | File | Original URL | Re-pull |
|--------|--------|------|--------------|---------|
| **CISQ Cost of Poor Software Quality 2022** | ⚠ PARTIAL | `quality/cisq/cisq-cost-poor-quality-2022-2026-05-04.md` | https://www.it-cisq.org/ | annual after CISQ publishes |

### W3C (accessibility)
| Source | Status | File | Original URL | Re-pull |
|--------|--------|------|--------------|---------|
| **W3C WCAG 2.2** | ✓ POPULATED | `quality/w3c/w3c-wcag-2.2-2026-05-04.md` | https://www.w3.org/TR/WCAG22/ | per W3C revision |

## [P] Performance

### Academic foundations
| Source | Status | File | Citation | Re-pull |
|--------|--------|------|----------|---------|
| **Amdahl 1967** — parallel speedup ceiling | 📚 ACADEMIC | `performance/academic/amdahl/amdahl-1967.md` | Amdahl, AFIPS 1967 | NEVER |
| **Gustafson 1988** — scaled speedup | 📚 ACADEMIC | `performance/academic/gustafson/gustafson-1988.md` | Gustafson, CACM 1988 | NEVER |
| **Karp–Flatt 1990** — empirical serial fraction | 📚 ACADEMIC | `performance/academic/karp-flatt/karp-flatt-1990.md` | Karp & Flatt, CACM 1990 | NEVER |
| **Little 1961** — L = λW queuing | 📚 ACADEMIC | `performance/academic/little/little-1961.md` | Little, Operations Research 1961 | NEVER |
| **Critical Path Method 1959** | 📚 ACADEMIC | `performance/academic/critical-path/kelley-walker-1959.md` | Kelley & Walker, EJCC 1959 | NEVER |

### Rockchip RK3588 NPU inference
| Source | Status | File | Original URL | Re-pull |
|--------|--------|------|--------------|---------|
| **DeepSeek-R1-Distill-Qwen-1.5B on RK3588 NPU (RKLLM benchmarks)** | ✓ POPULATED | `performance/rockchip/rk3588-npu-rkllm-benchmarks-2026-05-22.md` | https://www.electronics-lab.com/deepseek-r1-distill-qwen-1-5b-ai-model-deployed-on-rockchip-rk3588-soc-using-rkllm-toolkit/ | per RKLLM toolkit revision |

## [T] Testing

| Source | Status | File | Original URL / Citation | Re-pull |
|--------|--------|------|-------------------------|---------|
| **OWASP WSTG v4.2** | ✓ POPULATED | `testing/owasp/owasp-wstg-v4.2-2026-05-04.md` | https://owasp.org/www-project-web-security-testing-guide/v42/ | per OWASP release |
| **ML Test Score 2017** | 📚 ACADEMIC | `testing/academic/ml-test-score/breck-2017.md` | Breck et al., IEEE Big Data 2017 | NEVER |

## [D] Documentation

_Placeholder — no external standards anchored yet. See `documentation/README.md` for candidates (Diátaxis, Write the Docs, OpenAPI, JSON Schema, AsyncAPI, C4 Model)._

## [O] Operations

### Google Cloud / DORA
| Source | Status | File | Original URL | Re-pull |
|--------|--------|------|--------------|---------|
| **DORA Metrics** (landing) | ⚠ PARTIAL | `operations/google-cloud/dora-metrics-landing-2026-05-04.md` | https://dora.dev/ | annual |
| **DORA Research** (index) | ⚠ PARTIAL | `operations/google-cloud/dora-research-index-2026-05-04.md` | https://dora.dev/research/ | annual |
| **DORA State of AI-assisted Software Development 2025** | ✓ POPULATED | `operations/google-cloud/state-of-devops-2025-2026-05-12.md` | https://dora.dev/research/2025/ | annual |

### CNCF / OpenTelemetry / Apdex
| Source | Status | File | Original URL | Re-pull |
|--------|--------|------|--------------|---------|
| **OpenTelemetry Semantic Conventions** 1.56.0 | ✓ POPULATED | `operations/cncf/opentelemetry-semconv-2026-05-12.md` | https://opentelemetry.io/docs/specs/semconv/ | per CNCF release |
| **OpenTelemetry Trace API** 1.56.0 | ✓ POPULATED | `operations/cncf/opentelemetry-trace-api-2026-05-12.md` | https://opentelemetry.io/docs/specs/otel/trace/api/ | per CNCF release |
| **OpenTelemetry Metrics API** 1.56.0 | ✓ POPULATED | `operations/cncf/opentelemetry-metrics-api-2026-05-12.md` | https://opentelemetry.io/docs/specs/otel/metrics/api/ | per CNCF release |
| **Apdex spec** v1.0 | ✓ POPULATED | `operations/cncf/apdex-spec-v1.0-2026-05-04.md` | https://www.apdex.org/specs.html | per Apdex Alliance update |
| **Apdex methodology** | ✓ POPULATED | `operations/cncf/apdex-methodology-2026-05-04.md` | https://www.apdex.org/index.php/history/ | rare |

### Google SRE
| Source | Status | File | Original URL | Re-pull |
|--------|--------|------|--------------|---------|
| **Google SRE Golden Signals** | ✓ POPULATED | `operations/google/google-sre-golden-signals-2026-05-04.md` | https://sre.google/sre-book/monitoring-distributed-systems/ | rare (canonical) |

### W3C (operations)
| Source | Status | File | Original URL | Re-pull |
|--------|--------|------|--------------|---------|
| **W3C Trace Context** v1 | ✓ POPULATED | `operations/w3c/w3c-trace-context-v1-2026-05-04.md` | https://www.w3.org/TR/trace-context/ | per W3C update |

### Academic (operations)
| Source | Status | File | Citation | Re-pull |
|--------|--------|------|----------|---------|
| **SPACE Framework 2021** | 📚 ACADEMIC | `operations/academic/space/forsgren-2021.md` | Forsgren et al., ACM Queue 2021 | NEVER |
| **Flow Framework 2018** | 📚 ACADEMIC | `operations/academic/flow/kersten-2018.md` | Kersten, IT Revolution 2018 | NEVER |

### Khadas (Edge2 SBC operations)
| Source | Status | File | Original URL | Re-pull |
|--------|--------|------|--------------|---------|
| **Khadas Edge2 Overview** (RK3588S spec) | ✓ POPULATED | `operations/khadas/khadas-edge2-overview-2026-05-22.md` | https://docs.khadas.com/products/sbc/edge2/start | per Khadas docs revision |
| **Khadas Edge2 Hardware Interfaces** | ✓ POPULATED | `operations/khadas/khadas-edge2-hardware-interfaces-2026-05-22.md` | https://docs.khadas.com/products/sbc/edge2/hardware/interfaces | per Khadas docs revision |
| **Khadas Edge2 NPU LLM Guide** (`khadas_llm.sh`) | ✓ POPULATED | `operations/khadas/khadas-edge2-npu-llm-guide-2026-05-22.md` | https://docs.khadas.com/products/sbc/edge2/npu/llm-on-edge2 | per Khadas docs revision |
| **Khadas Edge2 Add-ons Catalogue** | ✓ POPULATED | `operations/khadas/khadas-edge2-addons-2026-05-22.md` | https://docs.khadas.com/products/sbc/edge2/add-ons/start | per Khadas docs revision |

### Ollama (LLM runtime)
| Source | Status | File | Original URL | Re-pull |
|--------|--------|------|--------------|---------|
| **Ollama Cloud Models — `:cloud` routing behaviour** | ✓ POPULATED | `operations/ollama/ollama-cloud-models-2026-05-22.md` | https://docs.ollama.com/cloud | per Ollama docs revision |

---

## Multi-gate standards (reverse-index)

Standards living at their primary gate but also relevant to other gates. Source of truth is the file at primary gate; this index gives operational lookup for "all standards at gate X".

| Standard | Primary | Also applies to |
|----------|---------|-----------------|
| OWASP WSTG | [T] | [S] (vulnerability test procedures) |
| OWASP SAMM | [S] | [O] (security org maturity → operations) |
| NIST CSF 2.0 | [S] | [O] (Govern function spans operations) |
| NIST SP 800-53 | [S] | [O] (operational controls) |
| AICPA SOC 2 | [S] | [O] (operational audit), [Q] (confidentiality / processing integrity quality char per ISO 25010) |
| Apdex spec / methodology | [O] | [P] (performance metric variant) |
| Google SRE Golden Signals | [O] | [P] (latency / saturation = perf signals) |
| OpenTelemetry Metrics API | [O] | [P] (perf measurement emission) |
| W3C Trace Context | [O] | [S] (auth boundary tracing context) |
| ML Test Score (Breck 2017) | [T] | [Q] (ML production-readiness quality) |
| Little 1961 | [P] | [O] (queuing applies to operational throughput) |
| Flow Framework 2018 | [O] | [P] (delivery cycle time) |
| GDPR Art. 25 | [S] | [Q] (privacy-by-design quality char) |
| EU AI Act | [S] | [O] (AI deployment governance) |
| WCAG 2.2 | [Q] | [T] (accessibility test criteria) |

## Used by LASDLC components (LDB reverse index)

| LDB Component | Cited baselines |
|---------------|-----------------|
| **D1** (Request fidelity) | Per-build acceptance criteria + Northstar predicate (no external anchors) |
| **D2** (ISO 25010 conformance) | `quality/iso/iso-25010-2023-stub.md` + `quality/iso/iso-25023-2016-stub.md` |
| **D3** (CISQ automated) | `quality/iso/iso-5055-2021-stub.md` + `quality/cisq/cisq-cost-poor-quality-2022-*` |
| **D4** (DORA operational) | `operations/google-cloud/dora-metrics-landing-*` + `dora-research-index-*` + `state-of-devops-2025-*` |
| **D5** (Domain conditional) | `testing/academic/ml-test-score/breck-2017.md` (ai_ml); `quality/w3c/w3c-wcag-2.2-*` (accessibility); `security/eu/gdpr-article-25-*` (privacy); `security/eu/eu-ai-act-*` (AI regulation); `security/nist/nist-sp-800-63-4-*` + `security/nist/nist-sp-800-63b-4-*` (auth); `security/aicpa/aicpa-soc-2-type-ii-*` (compliance) |
| **D6a** (OWASP ASVS) | `security/owasp/owasp-asvs-v5.0.0-2026-05-12.md` |
| **D6b** (ISO 27001/27034) | `security/iso/iso-27001-2022-stub.md` + `security/iso/iso-27034-stub.md` + `security/cis/cis-controls-v8-*` |
| **D6c** (OWASP LLM Top 10 + MITRE ATLAS) | `security/owasp/owasp-llm-top-10-v2.0-*` + `security/mitre/mitre-atlas-*` |
| **D6d** (MITRE ATT&CK) | `security/mitre/mitre-attack-enterprise-2026-05-12.md` |
| **D6e** (CWE Top 25 + OWASP Top 10) | `security/mitre/mitre-cwe-top-25-2025-*` + `security/owasp/owasp-top-10-2021-*` + `security/owasp/owasp-api-security-top-10-2023-*` |
| **D6f** (NIST SSDF) | `security/nist/nist-ssdf-v1.1-*` + `security/nist/nist-ssdf-practices-*` + `security/nist/nist-csf-v2.0-*` + `security/nist/nist-sp-800-53-rev5-*` |
| **D6g** (SLSA + SBOM) | `security/openssf/slsa-levels-v1.2-*` + `security/openssf/slsa-threats-v1.2-*` + `security/openssf/slsa-spec-v1.2-*` + `security/linux-foundation/spdx-v3.0.1-*` + `security/owasp/cyclonedx-v1.6-*` |
| **D6h** (STRIDE + LINDDUN) | `security/microsoft/microsoft-stride-*` + `security/linddun/linddun-*` |
| **D6i** (Live pen-test) | SERAPH-runable; no static anchor |
| **D6j** (Compliance attestation) | `security/aicpa/aicpa-soc-2-type-ii-*` + framework selection per §6.5 |
| **D7** (Comparative baseline) | suppressed at N<3; activates with sample growth |
| **D8a** (Delivery time) | `operations/academic/flow/kersten-2018.md` + `operations/google-cloud/*` |
| **D8b** (Parallel speedup) | `performance/academic/amdahl/amdahl-1967.md` + `performance/academic/gustafson/gustafson-1988.md` |
| **D8c** (Empirical serial fraction) | `performance/academic/karp-flatt/karp-flatt-1990.md` |
| **D8d** (Agent utilization) | `operations/academic/flow/kersten-2018.md` + `performance/academic/little/little-1961.md` |
| **D8e** (Hand-off latency) | `operations/w3c/w3c-trace-context-v1-*` + `operations/cncf/opentelemetry-semconv-*` + `opentelemetry-trace-api-*` |
| **D8f** (Cache hit rate) | §0.6 inline_citation_protocol (internal substrate) |
| **D8g** (DAG critical path) | `performance/academic/critical-path/kelley-walker-1959.md` |
| **D8h** (Wave throughput) | `performance/academic/little/little-1961.md` |
| **D8i** (Operator satisfaction) | `operations/cncf/apdex-spec-v1.0-*` + `operations/cncf/apdex-methodology-*` + `operations/google/google-sre-golden-signals-*` |
| **D8j** (SPACE composite) | `operations/academic/space/forsgren-2021.md` |
| **C1e** (architectural thesis) | `architecture/ieee/ieee-42010-2022-stub.md` |

---

## Re-scrape policy summary (per §60.9)

| Source class | Re-scrape interval | Rationale |
|--------------|--------------------|-----------|
| OWASP / MITRE / CIS security | 90 days | Active CVE & technique landscape |
| DORA / SPACE annual reports | annually after publication | Annual cadence |
| CISQ State of Software Quality | annually after publication | Annual cadence |
| ISO / NIST / IEEE standards | per official revision | Standards revisions are rare |
| EU regulations | per regulation amendment | Multi-year cadence |
| SLSA / OpenTelemetry / Apdex / W3C | 180 days | Slower spec churn |
| Academic foundations | NEVER | Papers don't change |
| Paid stubs | per ISO/IEEE revision (5+ years typical) | Operator-driven re-pull |

---

## Composition with §60.9 cache convention

This registry is the **operational index** for the `helix/user/standards/industry-baselines/` cache substrate. Per cookbook §60.9, individual `.meta.json` sidecars per file are the strict cache convention; this REGISTRY consolidates the metadata into a single readable index. Future tooling MAY generate per-file `.meta.json` from this registry.

For each Firecrawl-pulled file, the canonical metadata is:
- `original_url`: embedded in file header comment (line 2) + listed in this registry
- `accessed_iso8601`: 2026-05-04T00:00:00Z (bootstrap date; encoded in filename)
- `scrape_tool`: firecrawl v1.10.0
- `scrape_options`: default markdown format
- `content_sha256`: computed lazily on first staleness check; not pre-computed at bootstrap

For academic foundations:
- `cited_from_training_corpus`: true
- `original_publication`: as listed in the citation field
- `verbatim_quote`: present in the .md file under "Verbatim quote (load-bearing)" header

For paid stubs:
- `access_method_required`: institutional | purchase
- `populated`: false (full text); true (bibliographic + scope summary)
- `next_action`: operator obtains authorized text + replaces stub

---

## What's deferred

1. **Per-file `.meta.json` sidecars** — strict §60.9 compliance requires them; deferred to follow-up tooling. REGISTRY.md + inline file headers serve as consolidated alternative.
2. **content_sha256 pre-computation** — required for change-detection on re-pulls; deferred to first staleness check or batch tooling.
3. **DORA 2025 report full text** — `operations/google-cloud/state-of-devops-2025-*` captures the research overview and year-in-review. Full quantitative report (performer tiers, metric benchmarks) is at `https://dora.dev/research/2025/dora-report/`. The 2025 report renamed to "State of AI-assisted Software Development." Operator action: download PDF and extract verbatim statistics for Canon XXXV D4 citations.
4. **CISQ 2022 report PDF** — `quality/cisq/cisq-cost-poor-quality-2022-*` captures the main CISQ site (original report URL returned 404). Operator action: navigate to https://www.it-cisq.org/technical-reports/, download the 2022 Cost of Poor Software Quality PDF, and add verbatim extracts.
5. **Paid-stub institutional pulls** — 6 stubs await operator authorized access (4 ISO + 1 IEEE 42010 + 1 ISO 25023).

---

## Status

- **Bootstrap completed**: 2026-05-04
- **Relocated to canonical standards path**: 2026-05-04
- **Expanded with 22 new entries**: 2026-05-04
- **Reorganized by [ASQPTDO] gate**: 2026-05-04
- **Gate vocabulary expanded**: 2026-05-05 ([ASQPTDO] → [ASQPTDOK] → [A+S+Q+C+O+P+K+D+T+R]); research/ folder scaffolded for [R] gate
- **Baseline refresh**: 2026-05-12 — 14 files updated to latest official versions (14 old files deleted); total 54 entries (40 live + 8 academic + 6 paid-stubs)
- **File headers**: source URL + version + scrape date embedded in all live-scraped files
- **D7 activation**: at sample N≥3 with baselines now populated; first activation expected on N=3 build
- **Cache substrate**: ready for cross-build citation reuse per Canon XXXVI Phase 3 promise
- **Per-sibling quick reference**: 2026-05-13 — 7 domain specialist files created (`helix/{corso,eva,soul,quantum,seraph,ayin,laex0}/industry-baselines.md`)

---

## Per-Sibling Quick Reference Files

For `/SCRUM` sessions, each domain specialist has a curated quick reference:

| Sibling | File | Gates |
|---------|------|-------|
| **CORSO** | `helix/corso/industry-baselines.md` | [A][Q][T] |
| **EVA** | `helix/eva/industry-baselines.md` | [O][P] |
| **SOUL** | `helix/soul/industry-baselines.md` | [K][D] |
| **QUANTUM** | `helix/quantum/industry-baselines.md` | [R] |
| **SERAPH** | `helix/seraph/industry-baselines.md` | [S] |
| **AYIN** | `helix/ayin/industry-baselines.md` | [O][P] |
| **LÆX** | `helix/laex0/industry-baselines.md` | [C] (all gates) |

These files provide gate-filtered baseline lists for quick lookup during squad reviews. The canonical source remains this REGISTRY.md.
