<!-- uuid: 81e8a3cf-e86c-42bd-acdb-ad1701d6e5cc -->
<!-- source: https://www.w3.org/TR/trace-context/ | version: v1 (W3C Recommendation) | scraped: 2026-05-04 | tool: firecrawl v1.10.0 | re-pull: per REGISTRY.md policy -->
<!-- gate: [O], [S] -->

[↑Jump to Table of Contents](https://www.w3.org/TR/trace-context/#toc) [←Collapse Sidebar](https://www.w3.org/TR/trace-context/#toc)

[![W3C](https://www.w3.org/StyleSheets/TR/2021/logos/W3C)](https://www.w3.org/)

# Trace Context

[W3C Recommendation](https://www.w3.org/standards/types#REC) 23 November 2021

More details about this documentThis version:[https://www.w3.org/TR/2021/REC-trace-context-1-20211123/](https://www.w3.org/TR/2021/REC-trace-context-1-20211123/)Latest published version:[https://www.w3.org/TR/trace-context-1/](https://www.w3.org/TR/trace-context-1/)Latest editor's draft:[https://w3c.github.io/trace-context/](https://w3c.github.io/trace-context/)History:[https://www.w3.org/standards/history/trace-context-1](https://www.w3.org/standards/history/trace-context-1)[Commit history](https://github.com/w3c/trace-context/commits/level-1)Implementation report:[https://github.com/w3c/trace-context/#reference-implementations](https://github.com/w3c/trace-context/#reference-implementations)Editors:Sergey Kanzhelev ( [Microsoft](https://microsoft.com/))
Morgan McLean ( [Google](https://google.com/))
Alois Reitbauer ( [Dynatrace](https://dynatrace.com/))
Bogdan Drutu ( [Google](https://google.com/))
Nik Molnar ( [Microsoft](https://microsoft.com/))
Yuri Shkuro (Invited Expert)
Feedback:[GitHub w3c/trace-context](https://github.com/w3c/trace-context/)
( [pull requests](https://github.com/w3c/trace-context/pulls/),
[new issue](https://github.com/w3c/trace-context/issues/new/choose),
[open issues](https://github.com/w3c/trace-context/issues/))
[public-trace-context@w3.org](mailto:public-trace-context@w3.org?subject=trace-context) with subject line `trace-context` ( [archives](https://lists.w3.org/Archives/Public/public-trace-context))Errata:[Errata exists](https://w3c.github.io/trace-context/errata.html).Discussions[We are on Gitter.](https://gitter.im/TraceContext/Lobby)

See also
[**translations**](https://www.w3.org/Translations/?technology=trace-context).


[Copyright](https://www.w3.org/Consortium/Legal/ipr-notice#Copyright)
©
2021

[W3C](https://www.w3.org/) ® ( [MIT](https://www.csail.mit.edu/),
[ERCIM](https://www.ercim.eu/), [Keio](https://www.keio.ac.jp/),
[Beihang](https://ev.buaa.edu.cn/)). W3C
[liability](https://www.w3.org/Consortium/Legal/ipr-notice#Legal_Disclaimer),
[trademark](https://www.w3.org/Consortium/Legal/ipr-notice#W3C_Trademarks) and
[permissive document license](https://www.w3.org/Consortium/Legal/2015/copyright-software-and-document "W3C Software and Document Notice and License") rules apply.


* * *

## Abstract

This specification defines standard HTTP headers and a value format to propagate context information that enables distributed tracing scenarios. The specification standardizes how context information is sent and modified between services. Context information uniquely identifies individual requests in a distributed system and also defines a means to add and propagate provider-specific context information.

## Status of This Document

_This section describes the status of this_
_document at the time of its publication. A list of current W3C_
_publications and the latest revision of this technical report can be found_
_in the [W3C technical reports index](https://www.w3.org/TR/) at_
_https://www.w3.org/TR/._

This specification includes editorial updates since the [6 February 2020](https://www.w3.org/TR/2020/REC-trace-context-1-20200206/) W3C Recommendation.

This document was published by the [Distributed Tracing Working Group](https://www.w3.org/groups/wg/distributed-tracing) as
a Recommendation using the
[Recommendation track](https://www.w3.org/2021/Process-20211102/#recs-and-notes).


W3C recommends the wide deployment of this specification as a standard for
the Web.


A W3C Recommendation is a specification that, after extensive consensus-building, is endorsed by W3C and its Members, and has commitments from Working Group members to [royalty-free licensing](https://www.w3.org/Consortium/Patent-Policy/#sec-Requirements) for implementations.

This document was produced by a group
operating under the
[1 August 2017 W3C Patent\\
Policy](https://www.w3.org/Consortium/Patent-Policy-20170801/).

W3C maintains a
[public list of any patent disclosures](https://www.w3.org/groups/wg/distributed-tracing/ipr)
made in connection with the deliverables of
the group; that page also includes
instructions for disclosing a patent. An individual who has actual
knowledge of a patent which the individual believes contains
[Essential Claim(s)](https://www.w3.org/Consortium/Patent-Policy-20170801/#def-essential)
must disclose the information in accordance with
[section 6 of the W3C Patent Policy](https://www.w3.org/Consortium/Patent-Policy-20170801/#sec-Disclosure).



This document is governed by the
[2 November 2021 W3C Process Document](https://www.w3.org/2021/Process-20211102/).


## Table of Contents

01. [Abstract](https://www.w3.org/TR/trace-context/#abstract)
02. [Status of This Document](https://www.w3.org/TR/trace-context/#sotd)
03. [1\. Conformance](https://www.w3.org/TR/trace-context/#conformance)
04. [2\. Overview](https://www.w3.org/TR/trace-context/#overview)
    1. [2.1 Problem Statement](https://www.w3.org/TR/trace-context/#problem-statement)
    2. [2.2 Solution](https://www.w3.org/TR/trace-context/#solution)
    3. [2.3 Design Overview](https://www.w3.org/TR/trace-context/#design-overview)
05. [3\. Trace Context HTTP Headers Format](https://www.w3.org/TR/trace-context/#trace-context-http-headers-format)
    1. [3.1 Relationship Between the Headers](https://www.w3.org/TR/trace-context/#relationship-between-the-headers)
    2. [3.2 Traceparent Header](https://www.w3.org/TR/trace-context/#traceparent-header)
       1. [3.2.1 Header Name](https://www.w3.org/TR/trace-context/#header-name)
       2. [3.2.2 traceparent Header Field Values](https://www.w3.org/TR/trace-context/#traceparent-header-field-values)
          1. [3.2.2.1 version](https://www.w3.org/TR/trace-context/#version)
          2. [3.2.2.2 version-format](https://www.w3.org/TR/trace-context/#version-format)
          3. [3.2.2.3 trace-id](https://www.w3.org/TR/trace-context/#trace-id)
          4. [3.2.2.4 parent-id](https://www.w3.org/TR/trace-context/#parent-id)
          5. [3.2.2.5 trace-flags](https://www.w3.org/TR/trace-context/#trace-flags)
             1. [3.2.2.5.1 Sampled flag](https://www.w3.org/TR/trace-context/#sampled-flag)
             2. [3.2.2.5.2 Other Flags](https://www.w3.org/TR/trace-context/#other-flags)
       3. [3.2.3 Examples of HTTP traceparent Headers](https://www.w3.org/TR/trace-context/#examples-of-http-traceparent-headers)
       4. [3.2.4 Versioning of traceparent](https://www.w3.org/TR/trace-context/#versioning-of-traceparent)
    3. [3.3 Tracestate Header](https://www.w3.org/TR/trace-context/#tracestate-header)
       1. [3.3.1 Header Name](https://www.w3.org/TR/trace-context/#header-name-0)
          1. [3.3.1.1 tracestate Header Field Values](https://www.w3.org/TR/trace-context/#tracestate-header-field-values)
          2. [3.3.1.2 list](https://www.w3.org/TR/trace-context/#list)
          3. [3.3.1.3 list-members](https://www.w3.org/TR/trace-context/#list-members)
             1. [3.3.1.3.1 Key](https://www.w3.org/TR/trace-context/#key)
             2. [3.3.1.3.2 Value](https://www.w3.org/TR/trace-context/#value)
          4. [3.3.1.4 Combined Header Value](https://www.w3.org/TR/trace-context/#combined-header-value)
          5. [3.3.1.5 tracestate Limits:](https://www.w3.org/TR/trace-context/#tracestate-limits)
       2. [3.3.2 Examples of tracestate HTTP Headers](https://www.w3.org/TR/trace-context/#examples-of-tracestate-http-headers)
       3. [3.3.3 Versioning of tracestate](https://www.w3.org/TR/trace-context/#versioning-of-tracestate)
    4. [3.4 Mutating the traceparent Field](https://www.w3.org/TR/trace-context/#mutating-the-traceparent-field)
    5. [3.5 Mutating the tracestate Field](https://www.w3.org/TR/trace-context/#mutating-the-tracestate-field)
06. [4\. Processing Model](https://www.w3.org/TR/trace-context/#processing-model)
    1. [4.1 Processing Model for Working with Trace Context](https://www.w3.org/TR/trace-context/#processing-model-for-working-with-trace-context)
    2. [4.2 No traceparent Received](https://www.w3.org/TR/trace-context/#no-traceparent-received)
    3. [4.3 A traceparent is Received](https://www.w3.org/TR/trace-context/#a-traceparent-is-received)
    4. [4.4 Alternative Processing](https://www.w3.org/TR/trace-context/#alternative-processing)
07. [5\. Other Communication Protocols](https://www.w3.org/TR/trace-context/#other-communication-protocols)
08. [6\. Privacy Considerations](https://www.w3.org/TR/trace-context/#privacy-considerations)
    1. [6.1 Privacy of traceparent field](https://www.w3.org/TR/trace-context/#privacy-of-traceparent-field)
    2. [6.2 Privacy of tracestate field](https://www.w3.org/TR/trace-context/#privacy-of-tracestate-field)
    3. [6.3 Other risks](https://www.w3.org/TR/trace-context/#other-risks)
09. [7\. Security Considerations](https://www.w3.org/TR/trace-context/#security-considerations)
    1. [7.1 Information Exposure](https://www.w3.org/TR/trace-context/#information-exposure)
    2. [7.2 Denial of Service](https://www.w3.org/TR/trace-context/#denial-of-service)
    3. [7.3 Other Risks](https://www.w3.org/TR/trace-context/#other-risks-0)
10. [8\. Considerations for trace-id field generation](https://www.w3.org/TR/trace-context/#considerations-for-trace-id-field-generation)
    1. [8.1 Uniqueness of `trace-id`](https://www.w3.org/TR/trace-context/#uniqueness-of-trace-id)
    2. [8.2 Randomness of `trace-id`](https://www.w3.org/TR/trace-context/#randomness-of-trace-id)
    3. [8.3 Handling `trace-id` for compliant platforms with shorter internal identifiers](https://www.w3.org/TR/trace-context/#handling-trace-id-for-compliant-platforms-with-shorter-internal-identifiers)
    4. [8.4 Interoperating with existing systems which use shorter identifiers](https://www.w3.org/TR/trace-context/#interoperating-with-existing-systems-which-use-shorter-identifiers)
11. [A. Acknowledgments](https://www.w3.org/TR/trace-context/#acknowledgments)
12. [B. Glossary](https://www.w3.org/TR/trace-context/#glossary)
13. [C. References](https://www.w3.org/TR/trace-context/#references)
    1. [C.1 Normative references](https://www.w3.org/TR/trace-context/#normative-references)

## 1\. Conformance [§](https://www.w3.org/TR/trace-context/\#conformance)

As well as sections marked as non-normative, all authoring guidelines, diagrams, examples, and notes in this specification are non-normative. Everything else in this specification is normative.

The key words _MAY_, _MUST_, _MUST NOT_, _SHOULD_, and _SHOULD NOT_ in this document
are to be interpreted as described in
[BCP 14](https://datatracker.ietf.org/doc/html/bcp14)
\[[RFC2119](https://www.w3.org/TR/trace-context/#bib-rfc2119 "Key words for use in RFCs to Indicate Requirement Levels")\] \[[RFC8174](https://www.w3.org/TR/trace-context/#bib-rfc8174 "Ambiguity of Uppercase vs Lowercase in RFC 2119 Key Words")\]
when, and only when, they appear in all capitals, as shown here.


## 2\. Overview [§](https://www.w3.org/TR/trace-context/\#overview)

### 2.1 Problem Statement [§](https://www.w3.org/TR/trace-context/\#problem-statement)

Distributed tracing is a methodology implemented by tracing tools to follow, analyze and debug a transaction across multiple software components. Typically, a [distributed trace](https://www.w3.org/TR/trace-context/#dfn-distributed-traces) traverses more than one component which requires it to be uniquely identifiable across all participating systems. Trace context propagation passes along this unique identification. Today, trace context propagation is implemented individually by each tracing vendor. In multi-vendor environments, this causes interoperability problems, like:

- Traces that are collected by different tracing vendors cannot be correlated as there is no shared unique identifier.
- Traces that cross boundaries between different tracing vendors can not be propagated as there is no uniformly agreed set of identification that is forwarded.
- Vendor specific metadata might be dropped by intermediaries.
- Cloud platform vendors, intermediaries and service providers, cannot guarantee to support trace context propagation as there is no standard to follow.

In the past, these problems did not have a significant impact as most applications were monitored by a single tracing vendor and stayed within the boundaries of a single platform provider. Today, an increasing number of applications are highly distributed and leverage multiple middleware services and cloud platforms.

This transformation of modern applications calls for a distributed tracing context propagation standard.

### 2.2 Solution [§](https://www.w3.org/TR/trace-context/\#solution)

The trace context specification defines a universally agreed-upon format for the exchange of trace context propagation data - referred to as _trace context_. Trace context solves the problems described above by

- providing an unique identifier for individual traces and requests, allowing trace data of multiple providers to be linked together.
- providing an agreed-upon mechanism to forward vendor-specific trace data and avoid broken traces when multiple tracing tools participate in a single transaction.
- providing an industry standard that intermediaries, platforms, and hardware providers can support.

A unified approach for propagating trace data improves visibility into the behavior of distributed applications, facilitating problem and performance analysis. The interoperability provided by trace context is a prerequisite to manage modern micro-service based applications.

### 2.3 Design Overview [§](https://www.w3.org/TR/trace-context/\#design-overview)

Trace context is split into two individual propagation fields supporting interoperability and vendor-specific extensibility:

- `traceparent` describes the position of the incoming request in its trace graph in a portable, fixed-length format. Its design focuses on fast parsing. Every tracing tool _MUST_ properly set `traceparent` even when it only relies on vendor-specific information in `tracestate`
- `tracestate` extends `traceparent` with vendor-specific data represented by a set of name/value pairs. Storing information in `tracestate` is optional.

Tracing tools can provide two levels of compliant behavior interacting with trace context:

- At a minimum they _MUST_ propagate the `traceparent` and `tracestate` headers and guarantee traces are not broken. This behavior is also referred to as forwarding a trace.
- In addition they CAN also choose to participate in a trace by modifying the `traceparent` header and relevant parts of the `tracestate` header containing their proprietary information. This is also referred to as participating in a trace.

A tracing tool can choose to change this behavior for each individual request to a component it is monitoring.

## 3\. Trace Context HTTP Headers Format [§](https://www.w3.org/TR/trace-context/\#trace-context-http-headers-format)

This section describes the binding of the distributed trace context to `traceparent` and `tracestate` HTTP headers.

### 3.1 Relationship Between the Headers [§](https://www.w3.org/TR/trace-context/\#relationship-between-the-headers)

The `traceparent` header represents the incoming request in a tracing system in a common format, understood by all vendors. Here’s an example of a `traceparent` header.

```
traceparent: 00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01
```

The `tracestate` header includes the parent in a potentially vendor-specific format:

```
tracestate: congo=t61rcWkgMzE
```

For example, say a client and server in a system use different tracing vendors: Congo and Rojo. A client traced in the Congo system adds the following headers to an outbound HTTP request.

```
traceparent: 00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01
tracestate: congo=t61rcWkgMzE
```

**Note**: In this case, the `tracestate` value `t61rcWkgMzE` is the result of Base64 encoding the parent ID (`b7ad6b7169203331`), though such manipulations are not required.

The receiving server, traced in the Rojo tracing system, carries over the `tracestate` it received and adds a new entry to the left.

```
traceparent: 00-0af7651916cd43dd8448eb211c80319c-00f067aa0ba902b7-01
tracestate: rojo=00f067aa0ba902b7,congo=t61rcWkgMzE
```

You'll notice that the Rojo system reuses the value of its `traceparent` for its entry in `tracestate`. This means it is a generic tracing system (no proprietary information is being passed). Otherwise, `tracestate` entries are opaque and can be vendor-specific.

If the next receiving server uses Congo, it carries over the `tracestate` from Rojo and adds a new entry for the parent to the left of the previous entry.

```
traceparent: 00-0af7651916cd43dd8448eb211c80319c-b9c7c989f97918e1-01
tracestate: congo=ucfJifl5GOE,rojo=00f067aa0ba902b7
```

**Note:**`ucfJifl5GOE` is the Base64 encoded parent ID `b9c7c989f97918e1`.

Notice when Congo wrote its `traceparent` entry, it is not encoded, which helps in consistency for those doing correlation. However, the value of its entry `tracestate` is encoded and different from `traceparent`. This is ok.

Finally, you'll see `tracestate` retains an entry for Rojo exactly as it was, except pushed to the right. The left-most position lets the next server know which tracing system corresponds with `traceparent`. In this case, since Congo wrote `traceparent`, its `tracestate` entry should be left-most.

### 3.2 Traceparent Header [§](https://www.w3.org/TR/trace-context/\#traceparent-header)

The `traceparent` HTTP header field identifies the incoming request in a tracing system. It has four fields:

- `version`
- `trace-id`
- `parent-id`
- `trace-flags`

#### 3.2.1 Header Name [§](https://www.w3.org/TR/trace-context/\#header-name)

Header name: `traceparent`

In order to increase interoperability across multiple protocols and encourage successful integration, by default vendors _SHOULD_ keep the header name lowercase. The header name is a single word without any delimiters, for example, a hyphen (`-`).

Vendors _MUST_ expect the header name in any case (upper, lower, mixed), and _SHOULD_ send the header name in lowercase.

#### 3.2.2 traceparent Header Field Values [§](https://www.w3.org/TR/trace-context/\#traceparent-header-field-values)

This section uses the Augmented Backus-Naur Form (ABNF) notation of \[[RFC5234](https://www.w3.org/TR/trace-context/#bib-rfc5234 "Augmented BNF for Syntax Specifications: ABNF")\], including the DIGIT rule from that document. The `DIGIT` rule defines a single number character `0`-`9`.

```
HEXDIGLC = DIGIT / "a" / "b" / "c" / "d" / "e" / "f" ; lowercase hex character
value           = version "-" version-format
```

The dash (`-`) character is used as a delimiter between fields.

##### 3.2.2.1 version [§](https://www.w3.org/TR/trace-context/\#version)

```
version         = 2HEXDIGLC   ; this document assumes version 00. Version ff is forbidden
```

The value is US-ASCII encoded (which is UTF-8 compliant).

Version (`version`) is 1 byte representing an 8-bit unsigned integer. Version `ff` is invalid. The current specification assumes the `version` is set to `00`.

##### 3.2.2.2 version-format [§](https://www.w3.org/TR/trace-context/\#version-format)

The following `version-format` definition is used for version `00`.

```
version-format   = trace-id "-" parent-id "-" trace-flags
trace-id         = 32HEXDIGLC  ; 16 bytes array identifier. All zeroes forbidden
parent-id        = 16HEXDIGLC  ; 8 bytes array identifier. All zeroes forbidden
trace-flags      = 2HEXDIGLC   ; 8 bit flags. Currently, only one bit is used. See below for details
```

##### 3.2.2.3 trace-id [§](https://www.w3.org/TR/trace-context/\#trace-id)

This is the ID of the whole trace forest and is used to uniquely identify a [distributed trace](https://www.w3.org/TR/trace-context/#dfn-distributed-traces) through a system. It is represented as a 16-byte array, for example, `4bf92f3577b34da6a3ce929d0e0e4736`. All bytes as zero (`00000000000000000000000000000000`) is considered an invalid value.

If the `trace-id` value is invalid (for example if it contains non-allowed characters or all zeros), vendors _MUST_ ignore the `traceparent`.

See [considerations for trace-id field\\
generation](https://www.w3.org/TR/trace-context/#considerations-for-trace-id-field-generation) for recommendations
on how to operate with `trace-id`.

##### 3.2.2.4 parent-id [§](https://www.w3.org/TR/trace-context/\#parent-id)

This is the ID of this request as known by the caller (in some tracing systems, this is known as the `span-id`, where a `span` is the execution of a client request). It is represented as an 8-byte array, for example, `00f067aa0ba902b7`. All bytes as zero (`0000000000000000`) is considered an invalid value.

Vendors _MUST_ ignore the `traceparent` when the `parent-id` is invalid (for example, if it contains non-lowercase hex characters).

##### 3.2.2.5 trace-flags [§](https://www.w3.org/TR/trace-context/\#trace-flags)

An [8-bit field](https://en.wikipedia.org/wiki/Bit_field#firstHeading) that controls tracing flags such as sampling, trace level, etc. These flags are recommendations given by the caller rather than strict rules to follow for three reasons:

1. Trust and abuse
2. Bug in the caller
3. Different load between caller service and callee service might force callee to downsample.

You can find more in the section [Security considerations](https://www.w3.org/TR/trace-context/#security-considerations) of this specification.

Like other fields, `trace-flags` is hex-encoded. For example, all `8` flags set would be `ff` and no flags set would be `00`.

As this is a bit field, you cannot interpret flags by decoding the hex value and looking at the resulting number. For example, a flag `00000001` could be encoded as `01` in hex, or `09` in hex if present with the flag `00001000`. A common mistake in bit fields is forgetting to mask when interpreting flags.

Here is an example of properly handling trace flags:

```
static final byte FLAG_SAMPLED = 1; // 00000001
...
boolean sampled = (traceFlags & FLAG_SAMPLED) == FLAG_SAMPLED;
```

###### 3.2.2.5.1 Sampled flag [§](https://www.w3.org/TR/trace-context/\#sampled-flag)

The current version of this specification (`00`) only supports a single flag called `sampled`.

When set, the least significant bit (right-most), denotes that the caller may have recorded trace data. When unset, the caller did not record trace data out-of-band.

There are a number of recording scenarios that may break distributed tracing:

- Only recording a subset of requests results in broken traces.
- Recording information about all incoming and outgoing requests becomes prohibitively expensive, at load.
- Making random or component-specific data collection decisions leads to fragmented data in all traces.

Because of these issues, tracing vendors make their own recording decisions, and there is no consensus on what is the best algorithm for this job.

Various techniques include:

- Probability sampling (sample 1 out of 100 [distributed traces](https://www.w3.org/TR/trace-context/#dfn-distributed-traces) by flipping a coin)
- Delayed decision (make collection decision based on duration or a result of a request)
- Deferred sampling (let the callee decide whether information about this request needs to be collected)

How these techniques are implemented can be tracing vendor-specific or application-defined.

The `tracestate` field is designed to handle the variety of techniques for making recording decisions (or other specific information) specific for a given vendor. The `sampled` flag provides better interoperability between vendors. It allows vendors to communicate recording decisions and enable a better experience for the customer.

For example, when a SaaS service participates in a [distributed trace](https://www.w3.org/TR/trace-context/#dfn-distributed-traces), this service has no knowledge of the tracing vendor used by its caller. This service may produce records of incoming requests for monitoring or troubleshooting purposes. The `sampled` flag can be used to ensure that information about requests that were marked for recording by the caller will also be recorded by SaaS service downstream so that the caller can troubleshoot the behavior of every recorded request.

The `sampled` flag has no restriction on its mutations except that it can only be mutated when [parent-id is updated](https://www.w3.org/TR/trace-context/#parent-id).

The following are a set of suggestions that vendors _SHOULD_ use to increase vendor interoperability.

- If a component made definitive recording decision - this decision _SHOULD_ be reflected in the `sampled` flag.
- If a component needs to make a recording decision - it _SHOULD_ respect the `sampled` flag value.
  [Security considerations](https://www.w3.org/TR/trace-context/#security-considerations) _SHOULD_ be applied to protect from abusive or malicious use of this flag.
- If a component deferred or delayed the decision and only a subset of telemetry will be recorded, the `sampled` flag should be propagated unchanged. It should be set to `0` as the default option when the trace is initiated by this component.

There are two additional options that vendors _MAY_ follow:

- A component that makes a deferred or delayed recording decision may communicate the priority of a recording by setting `sampled` flag to `1` for a subset of requests.
- A component may also fall back to probability sampling and set the `sampled` flag to `1` for the subset of requests.

###### 3.2.2.5.2 Other Flags [§](https://www.w3.org/TR/trace-context/\#other-flags)

The behavior of other flags, such as (`00000100`) is not defined and is reserved for future use. Vendors _MUST_ set those to zero.

#### 3.2.3 Examples of HTTP traceparent Headers [§](https://www.w3.org/TR/trace-context/\#examples-of-http-traceparent-headers)

_Valid traceparent when caller sampled this request:_

```
Value = 00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01
base16(version) = 00
base16(trace-id) = 4bf92f3577b34da6a3ce929d0e0e4736
base16(parent-id) = 00f067aa0ba902b7
base16(trace-flags) = 01  // sampled
```

_Valid traceparent when caller didn’t sample this request:_

```
Value = 00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-00
base16(version) = 00
base16(trace-id) = 4bf92f3577b34da6a3ce929d0e0e4736
base16(parent-id) = 00f067aa0ba902b7
base16(trace-flags) = 00  // not sampled
```

#### 3.2.4 Versioning of traceparent [§](https://www.w3.org/TR/trace-context/\#versioning-of-traceparent)

This specification is opinionated about future versions of trace context. The current version of this specification assumes that future versions of the `traceparent` header will be additive to the current one.

Vendors _MUST_ follow these rules when parsing headers with an unexpected format:

- Pass-through services should not analyze the version. They should expect that headers may have larger size limits in the future and only disallow prohibitively large headers.

- When the version prefix cannot be parsed (it's not 2 hex characters followed by a dash (`-`)), the implementation should restart the trace.

- If a higher version is detected, the implementation _SHOULD_ try to parse it by trying the following:


  - If the size of the header is shorter than 55 characters, the vendor should not parse the header and should restart the trace.
  - Parse `trace-id` (from the first dash through the next 32 characters). Vendors _MUST_ check that the 32 characters are hex, and that they are followed by a dash (`-`).
  - Parse `parent-id` (from the second dash at the 35th position through the next 16 characters). Vendors _MUST_ check that the 16 characters are hex and followed by a dash.
  - Parse the `sampled` bit of `flags` (2 characters from the third dash). Vendors _MUST_ check that the 2 characters are either the end of the string or a dash.

 If all three values were parsed successfully, the vendor should use them.

Vendors _MUST NOT_ parse or assume anything about unknown fields for this version. Vendors _MUST_ use these fields to construct the new `traceparent` field according to the highest version of the specification known to the implementation (in this specification it is `00`).

### 3.3 Tracestate Header [§](https://www.w3.org/TR/trace-context/\#tracestate-header)

The main purpose of the `tracestate` HTTP header is to provide additional vendor-specific trace identification information across different distributed tracing systems and is a companion header for the `traceparent` field. It also conveys information about the request’s position in multiple distributed tracing graphs.

If the vendor failed to parse `traceparent`, it _MUST NOT_ attempt to parse `tracestate`. Note that the opposite is not true: failure to parse `tracestate` _MUST NOT_ affect the parsing of `traceparent`.

#### 3.3.1 Header Name [§](https://www.w3.org/TR/trace-context/\#header-name-0)

Header name: `tracestate`

In order to increase interoperability across multiple protocols and encourage successful integration, by default you _SHOULD_ keep the header name lowercase. The header name is a single word without any delimiters, for example, a hyphen (`-`).

Vendors _MUST_ expect the header name in any case (upper, lower, mixed), and _SHOULD_ send the header name in lowercase.

##### 3.3.1.1 tracestate Header Field Values [§](https://www.w3.org/TR/trace-context/\#tracestate-header-field-values)

The `tracestate` field may contain any opaque value in any of the keys. Tracestate _MAY_ be sent or received as multiple header fields. Multiple tracestate header fields _MUST_ be handled as specified by [RFC7230 Section 3.2.2 Field Order](https://httpwg.org/specs/rfc7230.html#field.order). The `tracestate` header _SHOULD_ be sent as a single field when possible, but _MAY_ be split into multiple header fields. When sending `tracestate` as multiple header fields, it _MUST_ be split according to [RFC7230](https://httpwg.org/specs/rfc7230.html#field.order). When receiving multiple `tracestate` header fields, they _MUST_ be combined into a single header according to [RFC7230](https://httpwg.org/specs/rfc7230.html#field.order).

This section uses the Augmented Backus-Naur Form (ABNF) notation of \[[RFC5234](https://www.w3.org/TR/trace-context/#bib-rfc5234 "Augmented BNF for Syntax Specifications: ABNF")\], including the DIGIT rule in [appendix B.1 for RFC5234](https://www.rfc-editor.org/rfc/rfc5234#appendix-B.1). It also includes the `OWS` rule from [RFC7230 section 3.2.3](https://httpwg.org/specs/rfc7230.html#whitespace).

The `DIGIT` rule defines numbers `0`-`9`.

The `OWS` rule defines an optional whitespace character. To improve readability, it is used where zero or more whitespace characters might appear.

The caller _SHOULD_ generate the optional whitespace as a single space; otherwise, a caller _SHOULD NOT_ generate optional whitespace. See details in the [corresponding RFC](https://httpwg.org/specs/rfc7230.html#whitespace).

The `tracestate` field value is a `list` of `list-members` separated by commas (`,`). A `list-member` is a key/value pair separated by an equals sign (`=`). Spaces and horizontal tabs surrounding `list-member`s are ignored. There can be a maximum of 32 `list-member`s in a `list`.

Empty and whitespace-only list members are allowed. Vendors _MUST_ accept empty `tracestate` headers but _SHOULD_ avoid sending them. Empty list members are allowed in `tracestate` because it is difficult for a vendor to recognize the empty value when multiple `tracestate` headers are sent. Whitespace characters are allowed for a similar reason, as some vendors automatically inject whitespace after a comma separator, even in the case of an empty header.

##### 3.3.1.2 list [§](https://www.w3.org/TR/trace-context/\#list)

A simple example of a `list` with two `list-member`s might look like:
`vendorname1=opaqueValue1,vendorname2=opaqueValue2`.

```
list  = list-member 0*31( OWS "," OWS list-member )
list-member = (key "=" value) / OWS
```

Identifiers for a `list` are short (up to 256 characters) textual identifiers.

##### 3.3.1.3 list-members [§](https://www.w3.org/TR/trace-context/\#list-members)

A `list-member` contains a key/value pair.

###### 3.3.1.3.1 Key [§](https://www.w3.org/TR/trace-context/\#key)

The key identifies the tracestate entry.

```
key = simple-key / multi-tenant-key
simple-key = lcalpha 0*255( lcalpha / DIGIT / "_" / "-"/ "*" / "/" )
multi-tenant-key = tenant-id "@" system-id
tenant-id = ( lcalpha / DIGIT ) 0*240( lcalpha / DIGIT / "_" / "-"/ "*" / "/" )
system-id = lcalpha 0*13( lcalpha / DIGIT / "_" / "-"/ "*" / "/" )
lcalpha    = %x61-7A ; a-z
```

**Note**: Identifiers _MUST_ begin with a lowercase letter or a digit, and can only contain lowercase letters (`a`-`z`), digits (`0`-`9`), underscores (`_`), dashes (`-`), asterisks (`*`), and forward slashes (`/`).

There are two different types of tracestate keys. The first type of key is a simple key used by tracing systems which do not have multiple tenants. Simple keys contain only lowercase alphanumeric characters, underscores, dashes, asterisks, and forward slashes. For example: `my-tracing-system`.

The second type of key is used by multi-tenant tracing systems where each tenant requires a unique tracestate entry. Multi-tenant keys consist of a tenant ID followed by the `@` character followed by a system ID. This allows for fast and robust parsing. For example, tracing system `xyz` can easily find all of its tracestate entries by searching for all instances of `@xyz=`.

###### 3.3.1.3.2 Value [§](https://www.w3.org/TR/trace-context/\#value)

The value is an opaque string containing up to 256 printable ASCII \[[RFC0020](https://www.w3.org/TR/trace-context/#bib-rfc0020 "ASCII format for network interchange")\] characters (i.e., the range 0x20 to 0x7E) except comma (`,`) and (`=`). Note that this also excludes tabs, newlines, carriage returns, etc.

```
value    = 0*255(chr) nblk-chr
nblk-chr = %x21-2B / %x2D-3C / %x3E-7E
chr      = %x20 / nblk-chr
```

##### 3.3.1.4 Combined Header Value [§](https://www.w3.org/TR/trace-context/\#combined-header-value)

The `tracestate` value is the concatenation of trace graph key/value pairs

Example: `vendorname1=opaqueValue1,vendorname2=opaqueValue2`

Only one entry per key is allowed because the entry represents that last position in the trace. Hence vendors must overwrite their entry upon reentry to their tracing system.

For example, if a vendor name is Congo and a trace started in their system and then went through a system named Rojo and later returned to Congo, the `tracestate` value would not be:

`congo=congosFirstPosition,rojo=rojosFirstPosition,congo=congosSecondPosition`

Instead, the entry would be rewritten to only include the most recent position:
`congo=congosSecondPosition,rojo=rojosFirstPosition`

##### 3.3.1.5 tracestate Limits: [§](https://www.w3.org/TR/trace-context/\#tracestate-limits)

Vendors _SHOULD_ propagate at least 512 characters of a combined header. This length includes commas required to separate list items and optional white space (`OWS`) characters.

There are systems where propagating of 512 characters of `tracestate` may be expensive. In this case, the maximum size of the propagated `tracestate` header _SHOULD_ be documented and explained. The cost of propagating `tracestate` _SHOULD_ be weighted against the value of monitoring scenarios enabled for the end users.

In a situation where `tracestate` needs to be truncated due to size limitations, the vendor _MUST_ truncate whole entries. Entries larger than `128` characters long _SHOULD_ be removed first. Then entries _SHOULD_ be removed starting from the end of `tracestate`. Note that other truncation strategies like safe list entries, blocked list entries, or size-based truncation _MAY_ be used, but are highly discouraged. Those strategies decrease the interoperability of various tracing vendors.

#### 3.3.2 Examples of tracestate HTTP Headers [§](https://www.w3.org/TR/trace-context/\#examples-of-tracestate-http-headers)

Single tracing system (generic format):

```
tracestate: rojo=00f067aa0ba902b7
```

Multiple tracing systems (with different formatting):

```
tracestate: rojo=00f067aa0ba902b7,congo=t61rcWkgMzE
```

#### 3.3.3 Versioning of tracestate [§](https://www.w3.org/TR/trace-context/\#versioning-of-tracestate)

The version of `tracestate` is defined by the version prefix of `traceparent` header. Vendors need to attempt to parse `tracestate` if a higher version is detected, to the best of its ability. It is the vendor’s decision whether to use partially-parsed `tracestate` key/value pairs or not.

### 3.4 Mutating the traceparent Field [§](https://www.w3.org/TR/trace-context/\#mutating-the-traceparent-field)

A vendor receiving a `traceparent` request header _MUST_ send it to outgoing requests. It _MAY_ mutate the value of this header before passing it to outgoing requests.

If the value of the `traceparent` field wasn't changed before propagation, `tracestate` _MUST NOT_ be modified as well. Unmodified header propagation is typically implemented in pass-through services like proxies. This behavior may also be implemented in a service which currently does not collect distributed tracing information.

Following is the list of allowed mutations:

- **Update `parent-id`**: The value of [the parent-id field](https://www.w3.org/TR/trace-context/#parent-id) can be set to the new value representing the ID of the current operation. This is the most typical mutation and should be considered a default.
- **Update `sampled`**: The value of [the sampled field](https://www.w3.org/TR/trace-context/#trace-flags) reflects the caller's recording behavior: either trace data was dropped or may have been recorded out-of-band. This can be indicated by toggling the flag in both directions. This mutation gives the downstream vendor information about the likelihood that its parent's information was recorded. The `parent-id` field _MUST_ be set to a new value with the `sampled` flag update.
- **Restart trace**: All properties (`trace-id`, `parent-id`, `trace-flags`) are regenerated. This mutation is used in services that are defined as a front gate into secure networks and eliminates a potential denial-of-service attack surface. Vendors _SHOULD_ clean up `tracestate` collection on `traceparent` restart. There are rare cases when the original `tracestate` entries must be preserved after a restart. This typically happens when the `trace-id` is reverted back at some point of the trace flow, for instance, when it leaves the secure network. However, it _SHOULD_ be an explicit decision, and not the default behavior.
- **Downgrade the version**: This version of the specification (`00`) [defines the behavior](https://www.w3.org/TR/trace-context/#version) for a vendor that receives a `traceparent` header of a higher version. In this case, the first mutation is to downgrade the version of the header. Other mutations are allowed in combination with this one.

Vendors _MUST NOT_ make any other mutations to the `traceparent` header.

### 3.5 Mutating the tracestate Field [§](https://www.w3.org/TR/trace-context/\#mutating-the-tracestate-field)

Vendors receiving a `tracestate` request header _MUST_ send it to outgoing requests. It _MAY_ mutate the value of this header before passing to outgoing requests. When mutating `tracestate`, the order of unmodified key/value pairs _MUST_ be preserved. Modified keys _SHOULD_ be moved to the beginning (left) of the list.

Following are allowed mutations:

- **Add a new key/value pair**. The new key/value pair _SHOULD_ be added to the beginning of the list.
- **Update an existing value**. The value for any given key can be updated. Modified keys _SHOULD_ be moved to the beginning (left) of the list.
- **Delete a key/value pair**. Any key/value pair _MAY_ be deleted. Vendors _SHOULD NOT_ delete keys that were not generated by them. The deletion of an unknown key/value pair will break correlation in other systems. This mutation enables two scenarios. The first is that proxies can block certain `tracestate` keys for privacy and security concerns. The second scenario is a truncation of long `tracestate`s.

## 4\. Processing Model [§](https://www.w3.org/TR/trace-context/\#processing-model)

_This section is non-normative._

This section provides a step-by-step example of a tracing vendor receiving a request with trace context headers, processing the request and then potentially forwarding it. This description can be used as a reference when implementing a trace context-compliant tracing system, middleware (like a proxy or messaging bus), or a cloud service.

### 4.1 Processing Model for Working with Trace Context [§](https://www.w3.org/TR/trace-context/\#processing-model-for-working-with-trace-context)

This processing model describes the behavior of a vendor that modifies and forwards trace context headers. How the model works depends on whether or not a `traceparent` header is received.

### 4.2 No traceparent Received [§](https://www.w3.org/TR/trace-context/\#no-traceparent-received)

If no traceparent header is received:

1. The vendor checks an incoming request for a `traceparent` and a `tracestate` header.
2. Because the `traceparent` header is not received, the vendor creates a new `trace-id` and `parent-id` that represents the current request.
3. If a `tracestate` header is received without an accompanying `traceparent` header, it is invalid and _MUST_ be discarded.
4. The vendor _SHOULD_ create a new `tracestate` header and add a new key/value pair.
5. The vendor sets the `traceparent` and `tracestate` header for the outgoing request.

### 4.3 A traceparent is Received [§](https://www.w3.org/TR/trace-context/\#a-traceparent-is-received)

If a `traceparent` header is received:

1. The vendor checks an incoming request for a `traceparent` and a `tracestate` header.
2. Because the `traceparent` header _is present_, the vendor tries to parse the version of the `traceparent` header.
   1. If the _version cannot be parsed_, the vendor creates a new `traceparent` header and deletes `tracestate`.
   2. If the _version number is higher_ than supported by the tracer, the vendor uses the format defined in this specification (`00`) to parse `trace-id` and `parent-id`.
      The vendor will only parse the `trace-flags` values supported by this version of this specification and ignore all other values. If parsing fails, the vendor creates a new `traceparent` header and deletes the `tracestate`. Vendors will set all unparsed / unknown `trace-flags` to 0 on outgoing requests.
   3. If the vendor _supports the version number_, it validates `trace-id` and `parent-id`. If either `trace-id`, `parent-id` or `trace-flags` are invalid, the vendor creates a new `traceparent` header and deletes `tracestate`.
3. The vendor _MAY_ validate the `tracestate` header. If the `tracestate` header cannot be parsed the vendor _MAY_ discard the entire header. Invalid `tracestate` entries _MAY_ also be discarded.
4. For each outgoing request the vendor performs the following steps:
   1. The vendor _MUST_ modify the `traceparent` header:

      - **Update `parent-id`:** The value of property `parent-id` _MUST_ be set to a value representing the ID of the current operation.
      - **Update `sampled`:** The value of `sampled` reflects the caller's recording behavior. The value of the `sampled` flag of `trace-flags` _MAY_ be set to `1` if the trace data is likely to be recorded or to `0` otherwise. Setting the flag is no guarantee that the trace will be recorded but increases the likeliness of end-to-end recorded traces.
   2. The vendor _MAY_ modify the `tracestate` header:

      - **Update a key value:** The value of any key can be updated. Modified keys _MUST_ be moved to the beginning (left) of the list.
      - **Add a new key/value pair:** The new key-value pair _MUST_ be added to the beginning (left) of the list.
      - **Delete a key/value pair:** Any key/value pair _MAY_ be deleted. Vendors _SHOULD NOT_ delete keys that weren't generated by themselves. Deletion of any key/value pair _MAY_ break correlation in other systems.
   3. The vendor sets the `traceparent` and `tracestate` header for the outgoing request.

### 4.4 Alternative Processing [§](https://www.w3.org/TR/trace-context/\#alternative-processing)

The processing model above describes the complete set of steps for processing trace context headers. There are, however, situations when a vendor might only support a subset of the steps described above. Proxies or messaging middleware _MAY_ decide not to modify the `traceparent` headers but remove invalid headers or add additional information to `tracestate`.

## 5\. Other Communication Protocols [§](https://www.w3.org/TR/trace-context/\#other-communication-protocols)

While trace context is defined for HTTP, the authors acknowledge it is also relevant for other communication protocols. Extensions of this specification, as well as specifications produced by external organizations, define the format of trace context serialization and deserialization for other protocols. Note that these extensions may be at a different maturity level than this specification.

Please refer to the \[[trace-context-protocols-registry](https://www.w3.org/TR/trace-context/#bib-trace-context-protocols-registry "Trace Context Protocols Registry")\] for the details of trace context implementation for other protocols.

## 6\. Privacy Considerations [§](https://www.w3.org/TR/trace-context/\#privacy-considerations)

Requirements to propagate headers to downstream services, as well as storing values of these headers, open up potential privacy concerns. Tracing vendors _MUST NOT_ use `traceparent` and `tracestate` fields for any personally identifiable or otherwise sensitive information. The only purpose of these fields is to enable trace correlation.

Vendors _MUST_ assess the risk of header abuse. This section provides some considerations and initial assessment of the risk associated with storing and propagating these headers. Tracing vendors may choose to inspect and remove sensitive information from the fields before allowing the tracing system to execute code that can potentially propagate or store these fields. All mutations should, however, conform to the list of mutations defined in this specification.

### 6.1 Privacy of traceparent field [§](https://www.w3.org/TR/trace-context/\#privacy-of-traceparent-field)

The `traceparent` field is comprised of randomly-generated numbers. If a random number generator leverages any user identifiable information like IP address as seed state, this information may be exposed. Random number generators _MUST NOT_ rely on any information that can potentially be user-identifiable.

Another privacy risk of the `traceparent` field is the ability to correlate requests made as part of a single transaction. A downstream service may track and correlate two or more requests made in a single transaction and may make assumptions about the identity of the caller of a request based on information from another request.

Note that these privacy concerns of the `traceparent` field are theoretical rather than practical. Some services initiating or receiving a request _MAY_ choose to restart a `traceparent` field to eliminate those risks completely. Vendors _SHOULD_ find a way to minimize the number of [distributed trace](https://www.w3.org/TR/trace-context/#dfn-distributed-traces) restarts to promote interoperability of tracing vendors. Instead of restarts, different techniques may be used. For example, services may define trust boundaries of upstream and downstream connections and the level of exposure that any requests may bring. For instance, a vendor might only restart `traceparent` for authentication requests from or to external services.

Services may also define an algorithm and audit mechanism to validate the randomness of incoming or outgoing random numbers in the `traceparent` field. Note that this algorithm is services-specific and not a part of this specification. One example might be a temporal algorithm where a reversible hash function is applied to the current clock time. The receiver can validate that the time is within agreed upon boundaries, meaning the random number was generated with the required algorithm and in fact doesn't contain any personally identifiable information.

### 6.2 Privacy of tracestate field [§](https://www.w3.org/TR/trace-context/\#privacy-of-tracestate-field)

The `tracestate` field may contain any opaque value in any of the keys. The main purpose of this header is to provide additional vendor-specific trace-identification information across different distributed tracing systems.

Vendors _MUST NOT_ include any personally identifiable information in the `tracestate` header.

Vendors extremely sensitive to personal information exposure _MAY_ implement selective removal of values corresponding to the unknown keys. Vendors _SHOULD NOT_ mutate the `tracestate` field, as it defeats the purpose of allowing multiple tracing systems to collaborate.

### 6.3 Other risks [§](https://www.w3.org/TR/trace-context/\#other-risks)

When vendors include `traceparent` and `tracestate` headers in responses, these values may inadvertently be passed to cross-origin callers. Vendors should ensure that they include only these response headers when responding to systems that participated in the trace.

## 7\. Security Considerations [§](https://www.w3.org/TR/trace-context/\#security-considerations)

There are two types of potential security risks associated with this specification: information exposure and denial-of-service attacks against the vendor.

Vendors relying on `traceparent` and `tracestate` headers should also follow all best practices for parsing potentially malicious headers, including checking for header length and content of header values. These practices help to avoid buffer overflow and HTML injection attacks.

### 7.1 Information Exposure [§](https://www.w3.org/TR/trace-context/\#information-exposure)

As mentioned in the privacy section, information in the `traceparent` and `tracestate` headers may carry information that can be considered sensitive. For example, `traceparent` may allow one request to be correlated to the data sent with another requeest, or the `tracestate` header may imply the version of monitoring software used by the caller. This information could potentially be used to create a larger attack.

Application owners should either ensure that no proprietary or confidential information is stored in `tracestate`, or they should ensure that `tracestate` isn't present in requests to external systems.

### 7.2 Denial of Service [§](https://www.w3.org/TR/trace-context/\#denial-of-service)

When distributed tracing is enabled on a service with a public API and naively continues any trace with the `sampled` flag set, a malicious attacker could overwhelm an application with tracing overhead, forge `trace-id` collisions that make monitoring data unusable, or run up your tracing bill with your SaaS tracing vendor.

Tracing vendors and platforms should account for these situations and make sure that checks and balances are in place to protect denial of monitoring by malicious or badly authored callers.

One example of such protection may be different tracing behavior for authenticated and unauthenticated requests. Various rate limiters for data recording can also be implemented.

### 7.3 Other Risks [§](https://www.w3.org/TR/trace-context/\#other-risks-0)

Application owners need to make sure to test all code paths leading to the sending of `traceparent` and `tracestate` headers. For example, in single page browser applications, it is typical to make cross-origin requests. If one of these code paths leads to `traceparent` and `tracestate` headers being sent by cross-origin calls that are restricted using [`Access-Control-Allow-Headers`](https://fetch.spec.whatwg.org/#http-access-control-request-headers) \[[FETCH](https://www.w3.org/TR/trace-context/#bib-fetch "Fetch Standard")\], it may fail.

## 8\. Considerations for trace-id field generation [§](https://www.w3.org/TR/trace-context/\#considerations-for-trace-id-field-generation)

_This section is non-normative._

This section suggests some best practices to consider when platform or tracing
vendor implement `trace-id` generation and propagation algorithms. These
practices will ensure better interoperability of different systems.

### 8.1 Uniqueness of `trace-id` [§](https://www.w3.org/TR/trace-context/\#uniqueness-of-trace-id)

The value of `trace-id` _SHOULD_ be globally unique. This field is typically used
for unique identification of a [distributed trace](https://www.w3.org/TR/trace-context/#dfn-distributed-traces). It is common for
[distributed traces](https://www.w3.org/TR/trace-context/#dfn-distributed-traces) to span various components, including, for example,
cloud services. Cloud services tend to serve variety of clients and have a very
high throughput of requests. So global uniqueness of `trace-id` is important,
even when local uniqueness might seem like a good solution.

### 8.2 Randomness of `trace-id` [§](https://www.w3.org/TR/trace-context/\#randomness-of-trace-id)

Randomly generated value of `trace-id` _SHOULD_ be preferred over other
algorithms of generating a globally unique identifiers. Randomness of `trace-id`
addresses some [security](https://www.w3.org/TR/trace-context/#security-considerations) and [privacy\\
concerns](https://www.w3.org/TR/trace-context/#privacy-considerations) of exposing unwanted information. Randomness
also allows tracing vendors to base sampling decisions on `trace-id` field value
and avoid propagating an additional sampling context.

As shown in the next section, it is important for `trace-id` to carry
"uniqueness" and "randomness" in the right part of the `trace-id`, for better
inter-operability with some existing systems.

### 8.3 Handling `trace-id` for compliant platforms with shorter internal identifiers [§](https://www.w3.org/TR/trace-context/\#handling-trace-id-for-compliant-platforms-with-shorter-internal-identifiers)

There are tracing systems which use a `trace-id` that is shorter than 16 bytes,
which are still willing to adopt this specification.

If such a system is capable of propagating a fully compliant `trace-id`, even
while still requiring a shorter, non-compliant identifier for internal purposes,
the system is encouraged to utilize the `tracestate` header to propagate the
additional internal identifier. However, if a system would instead prefer to use
the internal identifier as the basis for a fully compliant `trace-id`, it _SHOULD_
be incorporated at the as rightmost part of a `trace-id`. For example, tracing
system may receive `234a5bcd543ef3fa53ce929d0e0e4736` as a `trace-id`, hovewer
internally it will use `53ce929d0e0e4736` as an identifier.

### 8.4 Interoperating with existing systems which use shorter identifiers [§](https://www.w3.org/TR/trace-context/\#interoperating-with-existing-systems-which-use-shorter-identifiers)

There are tracing systems which are not capable of propagating the entire 16
bytes of a `trace-id`. For better interoperability between a fully compliant
systems with these existing systems, the following practices are recommended:

1. When a system creates an outbound message and needs to generate a fully
   compliant 16 bytes `trace-id` from a shorter identifier, it _SHOULD_ left pad
   the original identifier with zeroes. For example, the identifier
   `53ce929d0e0e4736`, _SHOULD_ be converted to `trace-id` value
   `000000000000000053ce929d0e0e4736`.
2. When a system receives an inbound message and needs to convert the 16 bytes
   `trace-id` to a shorter identifier, the rightmost part of `trace-id` _SHOULD_
   be used as this identifier. For instance, if the value of `trace-id` was
   `234a5bcd543ef3fa53ce929d0e0e4736` on an incoming request, tracing system
   _SHOULD_ use identifier with the value of `53ce929d0e0e4736`.

Similar transformations are expected when tracing system converts other
distributed trace context propagation formats to W3C Trace Context. Shorter
identifiers _SHOULD_ be left padded with zeros when converted to 16 bytes
`trace-id` and rightmost part of `trace-id` _SHOULD_ be used as a shorter
identifier.

Note, many existing systems that are not capable of propagating the whole
`trace-id` will not propagate `tracestate` header either. However, such system
can still use `tracestate` header to propagate additional data that is known by
this system. For example, some systems use two flags indicating whether
distributed trace needs to be recorded or not. In this case one flag can be send
as `sampled` flag of `traceparent` header and `tracestate` can be used to send
and receive an additional flag. Compliant systems will propagate this flag along
all other key/value pairs. Existing systems which are not capable of
`tracestate` propagation will truncate all additional values from `tracestate`
and only pass along that flag.

## A. Acknowledgments [§](https://www.w3.org/TR/trace-context/\#acknowledgments)

Thanks to Adrian Cole, Christoph Neumüller, Daniel Khan, Erika Arnold, Fabian Lange, Matthew Wear, Reiley Yang, Ted Young, Tyler Benson, Victor Soares for their contributions to this work.

## B. Glossary [§](https://www.w3.org/TR/trace-context/\#glossary)

_This section is non-normative._

Distributed trace
 A distributed trace is a set of events, triggered as a result
 of a single logical operation, consolidated across various
 components of an application. A distributed trace contains
 events that cross process, network and security boundaries.
 A distributed trace may be initiated when someone presses a
 button to start an action on a website - in this example, the
 trace will represent calls made between the downstream services
 that handled the chain of requests initiated by this button
 being pressed.


## C. References [§](https://www.w3.org/TR/trace-context/\#references)

### C.1 Normative references [§](https://www.w3.org/TR/trace-context/\#normative-references)

\[BIT-FIELD\][8-bit field](https://en.wikipedia.org/wiki/Bit_field). Wikipedia. URL: [https://en.wikipedia.org/wiki/Bit\_field](https://en.wikipedia.org/wiki/Bit_field)\[FETCH\][Fetch Standard](https://fetch.spec.whatwg.org/). Anne van Kesteren. WHATWG. Living Standard. URL: [https://fetch.spec.whatwg.org/](https://fetch.spec.whatwg.org/)\[RFC0020\][ASCII format for network interchange](https://www.rfc-editor.org/rfc/rfc20). V.G. Cerf. IETF. October 1969. Internet Standard. URL: [https://www.rfc-editor.org/rfc/rfc20](https://www.rfc-editor.org/rfc/rfc20)\[RFC2119\][Key words for use in RFCs to Indicate Requirement Levels](https://www.rfc-editor.org/rfc/rfc2119). S. Bradner. IETF. March 1997. Best Current Practice. URL: [https://www.rfc-editor.org/rfc/rfc2119](https://www.rfc-editor.org/rfc/rfc2119)\[RFC5234\][Augmented BNF for Syntax Specifications: ABNF](https://www.rfc-editor.org/rfc/rfc5234). D. Crocker, Ed.; P. Overell. IETF. January 2008. Internet Standard. URL: [https://www.rfc-editor.org/rfc/rfc5234](https://www.rfc-editor.org/rfc/rfc5234)\[RFC7230\][Hypertext Transfer Protocol (HTTP/1.1): Message Syntax and Routing](https://httpwg.org/specs/rfc7230.html). R. Fielding, Ed.; J. Reschke, Ed.. IETF. June 2014. Proposed Standard. URL: [https://httpwg.org/specs/rfc7230.html](https://httpwg.org/specs/rfc7230.html)\[RFC8174\][Ambiguity of Uppercase vs Lowercase in RFC 2119 Key Words](https://www.rfc-editor.org/rfc/rfc8174). B. Leiba. IETF. May 2017. Best Current Practice. URL: [https://www.rfc-editor.org/rfc/rfc8174](https://www.rfc-editor.org/rfc/rfc8174)\[trace-context-protocols-registry\][Trace Context Protocols Registry](https://www.w3.org/TR/trace-context-protocols-registry/). Sergey Kanzhelev; Philippe Le Hégaret. W3C. 19 November 2019. W3C Working Group Note. URL: [https://www.w3.org/TR/trace-context-protocols-registry/](https://www.w3.org/TR/trace-context-protocols-registry/)

[↑](https://www.w3.org/TR/trace-context/#title)

[Permalink](https://www.w3.org/TR/trace-context/#dfn-distributed-traces)

**Referenced in:**

- [§ 2.1 Problem Statement](https://www.w3.org/TR/trace-context/#ref-for-dfn-distributed-traces-1 "§ 2.1 Problem Statement")
- [§ 3.2.2.3 trace-id](https://www.w3.org/TR/trace-context/#ref-for-dfn-distributed-traces-2 "§ 3.2.2.3 trace-id")
- [§ 3.2.2.5.1 Sampled flag](https://www.w3.org/TR/trace-context/#ref-for-dfn-distributed-traces-3 "§ 3.2.2.5.1 Sampled flag") [(2)](https://www.w3.org/TR/trace-context/#ref-for-dfn-distributed-traces-4 "Reference 2")
- [§ 6.1 Privacy of traceparent field](https://www.w3.org/TR/trace-context/#ref-for-dfn-distributed-traces-5 "§ 6.1 Privacy of traceparent field")
- [§ 8.1 Uniqueness of trace-id](https://www.w3.org/TR/trace-context/#ref-for-dfn-distributed-traces-6 "§ 8.1 Uniqueness of trace-id") [(2)](https://www.w3.org/TR/trace-context/#ref-for-dfn-distributed-traces-7 "Reference 2")