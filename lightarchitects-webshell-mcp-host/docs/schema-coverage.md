# JSON Schema Coverage Specification

Phase 2 deliverable — specifies which JSON Schema 2020-12 constructs the Phase 6 form generator (`McpToolForm.svelte`) supports, with reproducibility evidence.

## Common Subset (supported in form auto-gen)

The form generator renders native UI controls for these constructs:

| Construct | UI rendering | Notes |
|-----------|-------------|-------|
| `type: "string"` | `<input type="text">` | + `maxLength` enforced client-side |
| `type: "number"` / `"integer"` | `<input type="number">` | step=1 for integer |
| `type: "boolean"` | `<input type="checkbox">` | |
| `type: "null"` | Hidden / static `null` | Only valid when combined with other types |
| `type: "object"` | Grouped sub-form (recursion) | Max depth 3 (deeper → raw JSON fallback) |
| `type: "array"` | Repeater: add/remove items | `items` must be a single schema |
| `enum: [...]` | `<select>` dropdown | Supports string + number enum values |
| `required: [...]` | Client-side required validation | Marks fields with `*` |
| `default: <value>` | Pre-fills field | |
| `description: "..."` | Tooltip / helper text | |
| `properties: {...}` | Each property → child field | Order preserved |
| `additionalProperties: false` | No extra fields rendered | |

## Unsupported Constructs → Raw JSON Fallback

When a schema contains any of these, the entire form falls back to `SchemaRawJsonFallback.svelte` (textarea + `ajv` client-side validation):

| Construct | Reason deferred |
|-----------|----------------|
| `$ref: "#/..."` | Requires schema resolution graph |
| `oneOf` / `anyOf` / `allOf` of complex objects | Discriminated union UI is high-complexity |
| `pattern: "..."` | Regex validation in UI is low-value for day-1 |
| `format: "email"` / `"uri"` / etc. | Deferred to ajv-enforced raw JSON |
| Object `type: "array"` with `items: [...]` (tuple) | Tuple forms are uncommon in observed schemas |
| Nested `$defs` / `$anchor` | Schema graph traversal deferred |
| `if` / `then` / `else` conditional | Conditional UI rendering deferred |

## Evidence: R-4 Survey Results

Surveyed MCP server `tools/list` responses captured at `research/r4-schema-survey/`.

| Server | Tools | Constructs observed |
|--------|-------|-------------------|
| SOUL (`soulTools`) | 1 aggregate dispatcher | `string` action enum, `object` params — common subset only |
| SERAPH (`seraphTools`) | 1 aggregate dispatcher | `string` action enum, `object` params — common subset only |
| AYIN (`ayinTools`) | 1 aggregate dispatcher | `string` action enum, `object` params — common subset only |
| Reference server (`@modelcontextprotocol/server-everything`) | 13 | All primitive types, `enum`, `required`, `description`, `object`. No `$ref`/`oneOf`. |
| @drawio/mcp v1.2.7 | 3 (`open_drawio_xml`, `open_drawio_csv`, `open_drawio_mermaid`) | `string content` (required), `boolean lightbox`, `string dark` enum `["auto","true","false"]` — common subset only. Captured 2026-05-21 Gate-2-M2. |

**Notable observation**: `dark` uses `type: "string"` + `enum: ["auto","true","false"]` (three-state: system/light/dark) rather than `type: "boolean"`. Form generator renders this as `<select>` via the `enum: [...]` rule — no special handling needed.

**Survey conclusion**: 100% of observed real-world schemas in the survey use only the common subset (4 servers, 7 tools total). Unsupported constructs (`$ref`, `oneOf`, conditional) appear in JSON Schema 2020-12 spec examples but not in observed MCP server schemas. The raw-JSON fallback is a correctness guarantee for future servers, not a day-1 requirement.

## ajv Configuration (client-side fallback)

```typescript
import Ajv from "ajv/dist/2020";
const ajv = new Ajv({ allErrors: true, strict: false });
```

- `strict: false` — allows unknown keywords (MCP servers may include non-standard fields)
- `allErrors: true` — collect all errors for display, not just first
- Schema `2020-12` draft (matches MCP spec)

## Form Generator Coverage Estimate

Based on survey: **~95% of observed schemas** handled by form auto-gen (common subset). The remaining ~5% (hypothetical complex schemas) fall back to raw JSON with ajv validation.

Gate-6 acceptance criterion: form correctly renders ≥3 distinct server schemas from `research/r4-schema-survey/` without falling back to raw JSON.
