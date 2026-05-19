# L0 — System Context Diagram

Architecture intelligence crate within the Light Architects platform.
This diagram is the **architect-drawn ground truth** — the drift verifier (Phase 4) compares
extracted facts against it.  Edit here first; never let the generator author this file.

```mermaid
%%{init: {"theme": "default", "themeVariables": {"fontSize": "14px"}} }%%
C4Context
    title System Context — lightarchitects-arch

    Person(operator, "Operator", "Platform user via webshell or CLI")

    System(arch, "lightarchitects-arch", "Extracts C4+ architecture models from Rust/TS/Python sources, verifies drift, emits diagrams")

    System_Ext(sdk, "lightarchitects-sdk workspace", "Host workspace — provides shared types, crypto, observability")
    System_Ext(soul, "SOUL MCP", "Knowledge graph — stores extracted ArchModel JSON as helix entries")
    System_Ext(ayin, "AYIN observability", "Trace spans — extraction latency, finding counts, emission time")
    System_Ext(webshell, "lightarchitects-webshell", "Architecture drawer panel — renders emitted diagrams in browser")

    Rel(operator, arch, "Invokes via MCP tool or webshell panel")
    Rel(arch, sdk, "Member crate — inherits workspace deps")
    Rel(arch, soul, "Writes extracted model as helix entry")
    Rel(arch, ayin, "Emits OTEL trace spans")
    Rel(webshell, arch, "Calls architecture MCP tools")
```
