//! Neo4j schema descriptions for LLM-backed Cypher generation.
//!
//! These string constants are injected into the LLM system prompt so the
//! model knows which labels, properties, and relationships are available.
//! Choosing the right schema constant for your graph is critical — wrong
//! property names will cause the generated Cypher to return empty results.

/// Schema description for the **`LongMemEval` ephemeral bench helix**.
///
/// Used in [`LlmCypherGenerator`](lightarchitects::soul::cypher::llm::LlmCypherGenerator)
/// system prompts when generating retrieval queries for the `LongMemEval`
/// benchmark. Each bench question gets its own ephemeral helix — all
/// sessions are stored as `Step` nodes inside it.
///
/// # Key Mapping
///
/// | Logical concept | Neo4j property |
/// |-----------------|----------------|
/// | Session ID      | `s.title`       |
/// | Session text    | `s.content`     |
/// | Session date    | `s.step_date`   |
/// | Role (user/…)   | `s.metadata.role` (JSON) |
///
/// Always scope queries with `MATCH (s:Step {helix_id: $helix_id})` and
/// return session IDs as `RETURN DISTINCT s.title AS session_id`.
pub const BENCH_SCHEMA: &str = "\
Node: Step
  helix_id: String  -- scope all queries: MATCH (s:Step {helix_id: $helix_id})
  title: String     -- SESSION ID (return this): RETURN DISTINCT s.title AS session_id
  content: String   -- full conversation text: WHERE toLower(s.content) CONTAINS 'keyword'
  step_date: Date   -- session date: WHERE s.step_date >= date('YYYY-MM-DD')

Relationship: (Step)-[:LINKS_TO]->(Step)
  link_type: 'temporal_adjacent' | 'semantic_similar'

RULES (mandatory):
  1. Scope every query: MATCH (s:Step {helix_id: $helix_id})
  2. Return session IDs: RETURN DISTINCT s.title AS session_id
  3. Never use CREATE, MERGE, DELETE, SET, REMOVE, DROP, DETACH, CALL
  4. Content search: WHERE toLower(s.content) CONTAINS 'lowercase_keyword'
  5. Output ONLY the Cypher query — no markdown fences, no explanation\
";

/// Schema description for the **production SOUL helix**.
///
/// Use when generating Cypher against the live SOUL knowledge graph.
/// The production helix includes richer metadata than the bench schema:
/// `significance`, community assignment, expiry, and strand membership.
pub const SOUL_HELIX_SCHEMA: &str = "\
Node: Helix
  id: String       -- helix UUID
  owner: String    -- sibling name ('eva', 'corso', etc.)
  name: String     -- human-readable helix name
  level: Integer   -- nesting depth (0 = root)

Node: Step
  id: String       -- unique step UUID
  helix_id: String -- parent helix (scope with $helix_id)
  title: String    -- step title or session identifier
  content: String  -- full text content
  significance: Float  -- 0.0-10.0 importance score
  step_date: Date  -- when this step was recorded

Node: Strand
  id: String
  name: String     -- domain axis ('analytical', 'memory', etc.)
  helix_id: String

Relationships:
  (Helix)-[:CONTAINS]->(Step)
  (Step)-[:LINKS_TO]->(Step)      -- link_type, weight
  (Step)-[:PARTICIPATES_IN]->(SharedExperience)
  (Strand)-[:INDEXES]->(Step)\
";
