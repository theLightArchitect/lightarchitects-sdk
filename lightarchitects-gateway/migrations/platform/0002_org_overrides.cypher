// Migration 0002: Org override layer (Model B).
//
// OrgOverride stores per-org JSON-patch overrides for platform content.
// Composite unique constraint on (org_id, target_path) prevents duplicate
// overrides for the same org+path. Resolution is lazy: fetched per-request
// via OPTIONAL MATCH and merged at query time (not pre-materialized).
//
// Fields: org_id, target_path, override_value (JSON string), created_at, updated_by.

CREATE CONSTRAINT org_override_composite_unique IF NOT EXISTS
  FOR (o:OrgOverride) REQUIRE (o.org_id, o.target_path) IS UNIQUE;

CREATE INDEX org_override_org IF NOT EXISTS
  FOR (o:OrgOverride) ON (o.org_id);

CREATE INDEX org_override_path IF NOT EXISTS
  FOR (o:OrgOverride) ON (o.target_path);
