// Migration 0001: Platform tier — PlatformEntry + SiblingIdentity nodes.
//
// PlatformEntry is the root content node for all canonical content served
// via /v1/platform/*. Fields: path (unique), kind, content_hash, content_json,
// content_text, version, updated_at.
//
// SiblingIdentity holds agent identity data (strands, voice, role) served
// via /v1/platform/agents/:sibling.

CREATE CONSTRAINT platform_entry_path_unique IF NOT EXISTS
  FOR (p:PlatformEntry) REQUIRE p.path IS UNIQUE;

CREATE INDEX platform_entry_kind IF NOT EXISTS
  FOR (p:PlatformEntry) ON (p.kind);

CREATE INDEX platform_entry_updated IF NOT EXISTS
  FOR (p:PlatformEntry) ON (p.updated_at);

CREATE CONSTRAINT sibling_identity_name_unique IF NOT EXISTS
  FOR (s:SiblingIdentity) REQUIRE s.sibling IS UNIQUE;
