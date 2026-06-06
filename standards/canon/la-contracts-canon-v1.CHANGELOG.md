# Light Architects Contract Canon — Amendment History

Companion changelog for `la-contracts-canon-v1.md`. The canon doc holds **current state only**; this file holds the **amendment narrative** — section added, source build, canon reference, rationale — that `git log` doesn't capture in narrative form.

**Authoritative latest version**: see the header inline summary in `la-contracts-canon-v1.md`.
**Mechanical history**: `git log -- standards/canon/la-contracts-canon-v1.md standards/canon/la-contracts.schema.json`
**Constitutional basis**: Canon XLII — Schema-Changelog Separation. See `canon://platform-canon` §"Canon XLII".

---

## v1.0.1 — §2.1 count audit + §5.6 `query_params` field + `wire_http_ext` schema patch (2026-06-05, Path A cleanup)

**Source**: Path A landing of `gateway.get.v1-platform-builds-codename-progress.yaml`. The new contract's `validate.sh` failure surfaced two real schema gaps; cleanup pass also reconciled long-stale §2.1 counts.

### §5.6 — `query_params` field for `wire.http` (NEW subsection)

**Background**: Several existing `wire.http` contracts (and the new build-progress contract) accept URL query parameters that weren't representable in the schema. Documentation lived in prose only — invisible to the validator. The schema had `additionalProperties: false` on `wire_http`, so any attempt to declare query parameters as a structured field failed validation.

**Rule**: `wire.http` endpoints accepting URL query parameters declare them in a top-level `query_params` field under `wire_http`. Each parameter declares `type`, optional `default`, optional `enum`, and `required`. Distinct from `request_schema` (which is for the request body on POST/PUT/PATCH). The field is **optional** — endpoints with no query parameters omit it entirely.

**Schema delta** (`la-contracts.schema.json`):

```diff
 "wire_http_ext": {
   "properties": {
     "wire_http": {
       "properties": {
+        "query_params": {
+          "type": "object",
+          "patternProperties": {
+            "^[a-z][a-z0-9_]*$": {
+              "type": "object",
+              "required": ["type"],
+              "properties": {
+                "type": { "enum": ["string", "integer", "boolean", "number"] },
+                "default": {},
+                "description": { "type": "string" },
+                "enum": { "type": "array" },
+                "required": { "type": "boolean", "default": false }
+              },
+              "additionalProperties": false
+            }
+          }
+        },
```

**Backward compatibility**: Purely additive optional field. `schema_version` remains `la-contracts/v1` (no bump). Existing 239 contracts continue to validate unchanged. First exemplar: `gateway.get.v1-platform-builds-codename-progress.yaml` declares two query params (`fleet_required`, `include_pr_state`).

**Validation evidence**: `bash validate.sh` → `240/240 contracts validate (100%)` post-landing.

### §2.1 — Stale count reconciliation (audit)

**Background**: §2.1 listed 18 kinds shipped + counts that had drifted from reality across multiple builds. Some entries were as far off as 7× the documented count.

**Reconciliation table** (delta is doc → actual at 2026-06-05):

| Kind | Doc said | Actual | Action |
|---|---|---|---|
| `wire.http` | 180 + 1 (159+7+~14) | 182 (157+1+24) | Updated count + breakdown; logged latest gateway addition |
| `wire.mcp` | schema branch; 0 stubs | 2 | Moved from schema-only to shipped |
| `agent.skill` | schema branch; 0 stubs | 23 | Moved from schema-only to shipped (BUILD/PLAN/SCRUM landed) |
| `code.trait` | 1 | 2 | Bumped count |
| `event.bus` | schema branch; 0 stubs | 1 | Moved from schema-only to shipped |
| `provider.llm` | 6 | 8 | Bumped count |
| `operator.surface` | 1 | 7 | Bumped count |
| `ui.component` | 1 | 3 | Bumped count |
| `mcp.capability` | (in PLANNED Phase A) | 1 | Promoted from PLANNED to SHIPPED |
| `strand.activation` | (in PLANNED Phase B) | 1 | Promoted from PLANNED to SHIPPED |
| `hmac_chain.audit_trail` | (in PLANNED Phase B) | 1 | Promoted from PLANNED to SHIPPED |
| `replay.deterministic_seed` | (in PLANNED Phase B) | 2 | Promoted from PLANNED to SHIPPED |

**Header bump**: `18 kinds shipped` → `19 kinds shipped with contracts, 4 schema-only kinds, 24 more in kind_enum (47 total)`.

**Net delta**: 240 contracts across 19 kinds at audit time; up from the 180+1 / 18-kinds posture the doc claimed.

**Operator stamp**: pending — these are corrections of stale documentation against present reality, not new canon. Recommend operator acknowledge the audit; future drift caught by adding a per-PR check (`grep -c "[0-9]\+ contracts" la-contracts-canon-v1.md` vs actual `ls contracts/*/ | wc -l`).

---

## v1.0.0 — Initial canon (2026-06-03, schema-version: la-contracts/v1)

Initial publication of the contract canon paired with `la-contracts.schema.json`. 47 kinds defined in `kind_enum`; 18 shipped with contracts at publication time.

**Constitutional basis**: Canon XLIII (the contract-gate doctrine).
