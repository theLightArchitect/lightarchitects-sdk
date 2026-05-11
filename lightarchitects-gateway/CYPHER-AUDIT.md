# Cypher Injection Audit — platform-api-v1

**Date**: 2026-05-04  
**Scope**: `lightarchitects-gateway` — all Neo4j query call sites for Phase 4  
**Verdict**: ✅ No injection vectors. All 16 call sites are fully parameterized or contain no user input.

---

## Methodology

Every `neo4rs::query(...)` call site was reviewed for:

1. **String concatenation** — any use of `format!`, `+`, or interpolation inside the query string literal.
2. **Unparameterized user input** — any value from a request path, query string, header, or body passed to the query without `.param(...)`.
3. **Insufficient pre-validation** — cases where a parameterized value could still cause logic abuse (e.g. unbounded `LIMIT`).

neo4rs sends queries over the Bolt protocol. Parameters are transmitted as typed values in a separate message frame — the Bolt server never interpolates them into the query string. Parameterization is therefore structural, not cosmetic.

---

## Query Inventory

### `src/http/routes/platform.rs`

| # | Handler | Query template | User-controlled params | Pre-validation | Verdict |
|---|---------|---------------|------------------------|---------------|---------|
| 1 | `fetch_canon_body` | `MATCH (p:PlatformEntry { path: $path }) OPTIONAL MATCH (o:OrgOverride { org_id: $org_id, target_path: $path })` | `path` (`:name` path param), `org_id` (`X-Org-Id` header) | `validate_path_param` rejects `..`, `/`, `\`, non-printable bytes; `org_id` is read-only (no write side-effect) | ✅ Safe |
| 2 | `fetch_agent_body` | `MATCH (s:SiblingIdentity { sibling: $sibling }) OPTIONAL MATCH (o:OrgOverride { org_id: $org_id, target_path: $sibling })` | `sibling` (`:sibling` path param), `org_id` | `validate_path_param`; `org_id` read-only | ✅ Safe |
| 3 | `skills_list` (cursor branch) | `MATCH (s:Skill { published: true }) WHERE s.name > $after … LIMIT $limit` | `after` (`after_id` query param), `limit` (query param) | `limit` capped server-side at 100 (`q.limit.min(100)`); `after` is a parameterized string comparison, not concatenated | ✅ Safe |
| 4 | `skills_list` (no-cursor branch) | `MATCH (s:Skill { published: true }) … LIMIT $limit` | `limit` | Capped at 100 | ✅ Safe |
| 5 | `skills_get` | `MATCH (s:Skill { name: $name })` | `name` (`:name` path param) | `validate_path_param` | ✅ Safe |
| 6 | `standards_get` | `MATCH (s:Standard { name: $name })` | `name` (`:name` path param) | `validate_path_param` | ✅ Safe |
| 7 | `build_helix_query` — kind+tier | `MATCH (h:Helix) WHERE h.kind = $kind AND h.tier = $tier … LIMIT $limit` | `kind`, `tier` (query params), `limit` | `limit` capped at 100; `kind`/`tier` are parameterized string filters (no structural role in query) | ✅ Safe |
| 8 | `build_helix_query` — kind only | `MATCH (h:Helix) WHERE h.kind = $kind … LIMIT $limit` | `kind`, `limit` | Same as #7 | ✅ Safe |
| 9 | `build_helix_query` — tier only | `MATCH (h:Helix) WHERE h.tier = $tier … LIMIT $limit` | `tier`, `limit` | Same as #7 | ✅ Safe |
| 10 | `build_helix_query` — no filter | `MATCH (h:Helix) … LIMIT $limit` | `limit` | Capped at 100 | ✅ Safe |
| 11 | `vault_info` | `MATCH (n) RETURN labels(n)[0] AS label, count(n) AS cnt ORDER BY cnt DESC LIMIT 20` | **None** — hardcoded `LIMIT 20` | N/A | ✅ Safe |

### `src/http/routes/admin.rs`

| # | Handler | Query template | User-controlled params | Pre-validation | Verdict |
|---|---------|---------------|------------------------|---------------|---------|
| 12 | `upload_canon` | `MERGE (p:PlatformEntry { path: $path }) SET p.kind = $kind, p.content_text = $content_text, p.content_json = $content_json, p.version = $version, p.content_hash = $content_hash, p.updated_at = $updated_at` | `path` (body), `kind` (body), `content_text` (body), `content_json` (body), `version` (body) | Path: non-empty guard. Kind: `ALLOWED_KINDS` allowlist `["canon","standard","template","skill"]`. Version: `is_valid_semver` (3-part numeric). Content: size-capped at 512 KB. `content_hash` and `updated_at` are server-computed — not user input. Token checked before body is read. | ✅ Safe |

### `src/http/neo4j.rs`

| # | Function | Query template | User-controlled params | Source | Verdict |
|---|---------|---------------|------------------------|--------|---------|
| 13 | `apply_migrations` — check | `MATCH (m:Migration { name: $name }) RETURN count(m) AS n` | `name` | Filesystem filename — not HTTP input | ✅ Safe |
| 14 | `apply_migrations` — execute | DDL statements from `.cypher` migration files | **None** | Trusted files bundled in the binary's working directory; content is `CREATE CONSTRAINT` / `CREATE INDEX` DDL with no variable substitution | ✅ Safe |
| 15 | `apply_migrations` — record | `CREATE (:Migration { name: $name, applied_at: $ts })` | `name` (filename), `ts` (server clock via `chrono::Utc::now()`) | Both values are server-internal, not user-supplied | ✅ Safe |
| 16 | `verify_schema` | `SHOW CONSTRAINTS YIELD name` | **None** | Introspection query — no user input | ✅ Safe |

---

## String Concatenation Scan

```
grep -n 'format!\|String::from\|push_str\|+.*neo4rs' \
  src/http/routes/platform.rs \
  src/http/routes/admin.rs \
  src/http/neo4j.rs
```

Result: **zero matches** in query-adjacent code. All string operations on user input occur before query construction and feed into `.param()` calls, not into the query string literal.

---

## Defense-in-Depth Summary

Beyond parameterization, three additional layers prevent abuse:

| Layer | Mechanism | Covers |
|-------|-----------|--------|
| Path validation | `validate_path_param` — rejects `..`, `/`, `\`, non-printable bytes | Queries #1, #2, #5, #6 |
| Kind allowlist | `ALLOWED_KINDS` static slice — enum over 4 strings | Query #12 |
| Payload cap | `MAX_CONTENT_BYTES = 512 KB` — checked before Neo4j | Query #12 |
| Limit cap | `.min(100)` applied to all `LIMIT` params | Queries #3, #4, #7–#10 |
| Admin auth | `subtle::ConstantTimeEq` token check before any body read | Query #12 |

---

## Conclusion

All 16 query call sites across `platform.rs`, `admin.rs`, and `neo4j.rs` use neo4rs parameterized queries exclusively. No string concatenation, format-string interpolation, or unsanitized user input reaches any query template. The Bolt protocol's structural separation of query text and parameter values makes injection mechanically impossible at the driver level regardless of input content.

**Status: CLOSED — no findings.**
