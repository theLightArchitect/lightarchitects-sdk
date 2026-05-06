// Migration 0001: Add scope_tier to existing :Helix nodes
// See plan: helix-of-helices-spec R1 mitigation (atomic Cypher transaction)
// Idempotent: WHERE scope_tier IS NULL guard prevents re-application

MATCH (h:Helix) WHERE h.scope_tier IS NULL
SET h.scope_tier = 'user';

MERGE (m:SchemaMigration {id: '0001_add_helix_tier'})
ON CREATE SET m.applied_at = timestamp();
