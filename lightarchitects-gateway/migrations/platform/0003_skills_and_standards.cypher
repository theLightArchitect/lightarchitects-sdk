// Migration 0003: Skill + Standard nodes.
//
// Skill: served via /v1/platform/skills and /v1/platform/skills/:name.
// Fields: name (unique), description, trigger_patterns (list), version,
//         published (bool), content_hash, updated_at.
//
// Standard: served via /v1/platform/standards/:name.
// Fields: name (unique), title, content_hash, content_text, updated_at.
// Examples: builders-cookbook, lasdlc-spec, canon, agents-md.

CREATE CONSTRAINT skill_name_unique IF NOT EXISTS
  FOR (s:Skill) REQUIRE s.name IS UNIQUE;

CREATE INDEX skill_published IF NOT EXISTS
  FOR (s:Skill) ON (s.published);

CREATE CONSTRAINT standard_name_unique IF NOT EXISTS
  FOR (s:Standard) REQUIRE s.name IS UNIQUE;

// Migration tracker — records which migrations have been applied.
// Applied by apply_migrations() in http/neo4j.rs before running each file.
CREATE CONSTRAINT migration_name_unique IF NOT EXISTS
  FOR (m:Migration) REQUIRE m.name IS UNIQUE;
