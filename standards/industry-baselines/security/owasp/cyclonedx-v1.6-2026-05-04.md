<!-- uuid: d22d21d5-aa20-4255-9d8c-6f9a7b93b15e -->
<!-- source: https://cyclonedx.org/specification/overview/ | version: v1.6 | scraped: 2026-05-04 | tool: firecrawl v1.10.0 | re-pull: per REGISTRY.md policy -->
<!-- gate: [S] -->

[![CycloneDX](https://cyclonedx.org/images/logo-all-white.svg)](https://cyclonedx.org/)

Open main menu

- [Getting Started](https://cyclonedx.org/specification/overview/#)
  - [Capabilities](https://cyclonedx.org/capabilities)
  - [Use Cases and Examples](https://cyclonedx.org/use-cases)
  - [Tool Center](https://cyclonedx.org/tool-center)
  - [Guides](https://cyclonedx.org/guides)
- [Specification](https://cyclonedx.org/specification/overview/#)
  - [Overview](https://cyclonedx.org/specification/overview)
  - [Documentation](https://cyclonedx.org/docs/latest)
  - [Cryptography Registry](https://cyclonedx.org/registry/cryptography)
  - [ECMA-424](https://ecma-international.org/publications-and-standards/standards/ecma-424/)
- [Participate](https://cyclonedx.org/specification/overview/#)
  - [Contribute](https://cyclonedx.org/participate/contribute)
  - [Working Groups](https://cyclonedx.org/participate/working-groups)
  - [TC54](https://cyclonedx.org/participate/tc54)
  - [Standardization Process](https://cyclonedx.org/participate/standardization-process)
- [Newsroom](https://cyclonedx.org/news)
- [About](https://cyclonedx.org/specification/overview/#)
  - [Guiding Principles](https://cyclonedx.org/about/guiding-principles)
  - [Governance](https://cyclonedx.org/about/governance)
  - [Supporters](https://cyclonedx.org/about/supporters)
  - [History](https://cyclonedx.org/about/history)
  - [Branding](https://cyclonedx.org/about/branding)

- Getting Started
  - [Capabilities](https://cyclonedx.org/capabilities)
  - [Use Cases and Examples](https://cyclonedx.org/use-cases)
  - [Tool Center](https://cyclonedx.org/tool-center)
  - [Guides](https://cyclonedx.org/guides)
- Specification
  - [Overview](https://cyclonedx.org/specification/overview)
  - [Documentation](https://cyclonedx.org/docs/latest)
  - [Cryptography Registry](https://cyclonedx.org/registry/cryptography)
  - [ECMA-424](https://ecma-international.org/publications-and-standards/standards/ecma-424/)
- Participate
  - [Contribute](https://cyclonedx.org/participate/contribute)
  - [Working Groups](https://cyclonedx.org/participate/working-groups)
  - [TC54](https://cyclonedx.org/participate/tc54)
  - [Standardization Process](https://cyclonedx.org/participate/standardization-process)
- [Newsroom](https://cyclonedx.org/news)
- About
  - [Guiding Principles](https://cyclonedx.org/about/guiding-principles)
  - [Governance](https://cyclonedx.org/about/governance)
  - [Supporters](https://cyclonedx.org/about/supporters)
  - [History](https://cyclonedx.org/about/history)
  - [Branding](https://cyclonedx.org/about/branding)

# Specification Overview

## Explore how CycloneDX elevates supply chain transparency. Discover how its modular, extensible design delivers actionable insights.

[Explore Tools](https://cyclonedx.org/tool-center) [Read Guides](https://cyclonedx.org/guides)

The CycloneDX specification is a highly modular and extensible framework designed to represent a broad range of supply chain information with precision and flexibility. At its core, CycloneDX employs a robust object model capable of capturing components, services, dependencies, and relationships across various inventory types, including software, hardware, cryptographic assets, and operational configurations. This object model is structured to support detailed metadata, lifecycle stages, and extensible attributes, enabling organizations to adapt the specification to their unique needs without sacrificing interoperability.

## Specification Details

| Title | CycloneDX |
| Current Version | 1.7 |
| Documentation | [JSON](https://cyclonedx.org/docs/latest/json/) [XML](https://cyclonedx.org/docs/latest/xml/) [Protobuf](https://cyclonedx.org/docs/latest/proto/) |
| Release Date | 2025-10-21 |
| Media Types | `vnd.cyclonedx+json`<br>`vnd.cyclonedx+xml`<br>`x.vnd.cyclonedx+protobuf` |
| Developed By | OWASP Foundation<br>Ecma International |
| Standards | [ECMA-424](https://ecma-international.org/publications-and-standards/standards/ecma-424/) |
| Published On | 2025-12-10 |
| Technical Committee | [TC54](https://tc54.org/) |

## The CycloneDX Object Model

The CycloneDX object model is a structured framework for representing information relevant for software and system transparency. Designed for clarity, precision, and extensibility, it organizes complex supply chain data into a well-defined schema that is both machine-readable and human-friendly. This model forms the backbone of CycloneDX’s ability to support diverse use cases ranging from vulnerability management and license compliance to cryptographic transparency and operational assurance.

![Object Model Overview](https://cyclonedx.org/images/object-model/CycloneDX-Object-Type-Overview.svg)

### BOM Metadata

BOM metadata includes the supplier, manufacturer, and target component for which the BOM describes. It also includes the tools used to create the BOM, and license information for the BOM document itself.

![BOM Metadata](https://cyclonedx.org/images/object-model/Metadata.svg)

### Components

Components describe the complete inventory of first-party and third-party components. The specification can represent software, hardware devices, machine learning models, source code, and configurations, along with the manufacturer information, license and copyright details, and complete pedigree and provenance for every component.

![Components](https://cyclonedx.org/images/object-model/Components.svg)

### Services

Services represent external APIs that the software may call. They describe endpoint URIs, authentication requirements, and trust boundary traversals. The data flow between software and services can also be described, including the data classifications and the flow direction of each type.

![Services](https://cyclonedx.org/images/object-model/Services.svg)

### Dependencies

CycloneDX provides the ability to describe components and their dependency on other components. The dependency graph is capable of representing both direct and transitive relationships. Components that depend on services can be represented in the dependency graph, and services that depend on other services can be represented as well.

![Dependencies](https://cyclonedx.org/images/object-model/Dependencies.svg)

### Compositions

Compositions describe constituent parts (including components, services, and dependency relationships) and their completeness. The aggregate of each composition can be described as complete, incomplete, incomplete first-party only, incomplete third-party only, or unknown.

![Compositions](https://cyclonedx.org/images/object-model/Compositions.svg)

### Vulnerabilities

Known vulnerabilities inherited from the use of third-party and open-source software and the exploitability of the vulnerabilities can be communicated with CycloneDX. Previously unknown vulnerabilities affecting both components and services may also be disclosed using CycloneDX, making it ideal for both vulnerability disclosure and VEX use cases.

![Vulnerabilities](https://cyclonedx.org/images/object-model/Vulnerabilities.svg)

### Formulation

Formulation describes how something was manufactured or deployed. CycloneDX achieves this through the support of multiple formulas, workflows, tasks, and steps, which represent the declared formulation for reproduction along with the observed formula describing the actions which transpired in the manufacturing process.

![Formulation](https://cyclonedx.org/images/object-model/Formulation.svg)

### Annotations

Annotations contain comments, notes, explanations, or similar textual content which provide additional context to the object(s) being annotated. They are often automatically added to a BOM via a tool or as a result of manual review by individuals or organizations. Annotations can be independently signed and verified using digital signatures.

![Annotations](https://cyclonedx.org/images/object-model/Annotations.svg)

### Definitions

Standards, requirements, levels, and all supporting documentation are defined here. CycloneDX provides a general-purpose, machine-readable way to define virtually any type of standard. Security standards such as OWASP ASVS, MASVS, SCVS, and SAMM are available in CycloneDX format. Standards from other bodies are available as well. Additionally, organizations can create internal standards and represent them in CycloneDX.

![Definitions](https://cyclonedx.org/images/object-model/Definitions.svg)

### Declarations

Declarations describe the conformance to standards. Each declaration may include attestations, claims, counter-claims, evidence, counter-evidence, along with conformance and confidence. Signatories can also be declared and supports both digital and analog signatures. Declarations provide the basis for "compliance-as-code".

![Declarations](https://cyclonedx.org/images/object-model/Declarations.svg)

### Citations

Citations identify who contributed specific pieces of information to a CycloneDX BOM and when that contribution was made. They connect data in the BOM to its source, whether that’s a tool, person, organization, or process. This traceability adds transparency, helping others assess the reliability and origin of the data. Citations are essential when multiple sources contribute to the same BOM.

![Citations](https://cyclonedx.org/images/object-model/Citations.svg)

### Extensions

Multiple extension points exist throughout the CycloneDX object model, allowing fast prototyping of new capabilities and support for specialized and future use cases. The CycloneDX project maintains extensions that are beneficial to the larger community. The project encourages community participation and the development of extensions that target specialized or industry-specific use cases.

![Extensions](https://cyclonedx.org/images/object-model/Extensions.svg)

## Media Types

The following media types are officially registered with IANA:

| Media Type | Format | Assignment |
| --- | --- | --- |
| `application/vnd.cyclonedx+xml` | XML | [IANA](https://www.iana.org/assignments/media-types/application/vnd.cyclonedx+xml) |
| `application/vnd.cyclonedx+json` | JSON | [IANA](https://www.iana.org/assignments/media-types/application/vnd.cyclonedx+json) |
| `application/x.vnd.cyclonedx+protobuf` | Protocol Buffers |  |

Specific versions of CycloneDX can be specified by using the version parameter, such as:

`application/vnd.cyclonedx+xml; version=1.7;`

## Recognized file patterns

The following file names are conventionally used for storing CycloneDX BOM files:

- `bom.json` for JSON encoded CycloneDX BOM files.
- `bom.xml` for XML encoded CycloneDX BOM files.

Alternatively, files that match the glob pattern below are also recognized:

- `*.cdx.json` for JSON encoded CycloneDX BOM files.
- `*.cdx.xml` for XML encoded CycloneDX BOM files.

## Recognized predicate type

Many tools in the software supply chain capture attestations at the time of execution. A predicate contains metadata about the attestation. Tools such as in-toto use predicate types to provide context about the subject of the predicate. OWASP recognizes `https://cyclonedx.org/bom` as the official predicate type for all CycloneDX bill of material varieties including SBOM, SaaSBOM, and HBOM.

[![CycloneDX Logo](https://cyclonedx.org/images/logo-all-white.svg)](https://cyclonedx.org/)[![OWASP Logo](https://cyclonedx.org/images/owasp-white.svg)](https://owasp.org/)

##### Getting Started

- [Capabilities](https://cyclonedx.org/capabilities)
- [Use Cases and Examples](https://cyclonedx.org/use-cases)
- [Tool Center](https://cyclonedx.org/tool-center)
- [Guides](https://cyclonedx.org/guides)

##### Specification

- [Overview](https://cyclonedx.org/specification/overview)
- [Documentation](https://cyclonedx.org/docs/latest)
- [Cryptography Registry](https://cyclonedx.org/registry/cryptography)
- [ECMA-424](https://ecma-international.org/publications-and-standards/standards/ecma-424/)

##### Participate

- [Contribute](https://cyclonedx.org/participate/contribute)
- [Working Groups](https://cyclonedx.org/participate/working-groups)
- [TC54](https://cyclonedx.org/participate/tc54)
- [Standardization Process](https://cyclonedx.org/participate/standardization-process)

##### Newsroom

[Newsroom](https://cyclonedx.org/news)

##### About

- [Guiding Principles](https://cyclonedx.org/about/guiding-principles)
- [Governance](https://cyclonedx.org/about/governance)
- [Supporters](https://cyclonedx.org/about/supporters)
- [History](https://cyclonedx.org/about/history)
- [Branding](https://cyclonedx.org/about/branding)

© 2026 OWASP Foundation. All Rights Reserved.

[LinkedIn](https://www.linkedin.com/company/owasp-cyclonedx/)[Slack](https://cyclonedx.org/slack/invite)[GitHub](https://github.com/CycloneDX)[YouTube](https://www.youtube.com/@CycloneDX)[Twitter](https://x.com/CycloneDX_Spec)[Bluesky](https://bsky.app/profile/cyclonedx.bsky.social)

![](https://static.scarf.sh/a.png?x-pxid=ce3b481f-33a5-4c88-aaf1-00c8805f24d9)