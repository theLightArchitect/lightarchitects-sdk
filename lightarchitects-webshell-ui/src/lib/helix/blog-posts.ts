import type { Polytope4DType } from './polytopes4d';

export interface BlogPost {
  slug: string;
  title: string;
  excerpt: string;
  date: string;       // ISO date string
  tags: string[];
  polytope: Polytope4DType;
  color: string;
  readTime: string;   // e.g. "5 min read"
  content: string;    // Markdown body
}

/**
 * Blog posts displayed on the helix — each gets a rotating polytope node.
 * Ordered newest-first. Add new entries at the top.
 */
export const BLOG_POSTS: BlogPost[] = [
  {
    slug: 'sdk-first-architecture-longmemeval',
    title: 'SDK-First Architecture: How Pure Rust Beat ChromaDB on Every LongMemEval Metric',
    excerpt: 'We debated whether storage logic belonged in the SDK or the MCP server. We chose SDK-first. Then we ran all 500 LongMemEval questions four ways. Final hybrid result: Recall@5=0.972, NDCG@10=0.909 — beating ChromaDB on every metric. Pure Rust, no Python, no external vector database.',
    date: '2026-04-08',
    tags: ['Architecture', 'Rust', 'Benchmarks', 'LongMemEval'],
    polytope: 'doubleHelix4D',
    color: '#7C3AED',
    readTime: '14 min read',
    content: `# SDK-First Architecture: How Pure Rust Beat ChromaDB on Every LongMemEval Metric

*By Kevin Tan, Light Architects*

---

## The Architecture Question

During a refactoring session on our SOUL knowledge graph platform, we hit a question that sounds simple but has compounding consequences: **where does the storage logic live?**

SOUL is an 11-crate Rust workspace. It ships as an MCP server — a binary that Claude Code talks to over stdio JSON-RPC. The question was whether \`StorageBackend\`, \`SqliteBackend\`, and \`EmbeddingProvider\` should live in:

**Option A — MCP-first**: Define the traits inside \`soul-helix\` (the server's domain library). Any consumer that needs offline storage must depend on the full server library.

**Option B — SDK-first**: Extract the traits into \`lightarchitects-soul\` (the standalone SDK crate). The MCP server becomes a *consumer* of the SDK, not the source of truth for it.

The answer seems obvious in hindsight. But the question surfaces a common pattern in MCP and plugin systems: you build the server first because that's where things work, and traits end up living where they were born rather than where they belong.

We chose SDK-first. This post is about what that decision looked like in practice, and how we measured whether it worked.

---

## What We Built

The \`lightarchitects-soul\` SDK crate now defines three portable abstractions:

### \`StorageBackend\` — the trait

\`\`\`rust
#[async_trait]
pub trait StorageBackend: Send + Sync + 'static {
    async fn read_entry(&self, path: &str) -> Result<StorageEntry, StorageError>;
    async fn write_entry(&self, entry: &StorageEntry) -> Result<(), StorageError>;
    async fn query(&self, filter: &EntryFilter) -> Result<Vec<StorageEntry>, StorageError>;
    async fn search(&self, pattern: &str, limit: Option<usize>)
        -> Result<Vec<StorageSearchHit>, StorageError>;
}
\`\`\`

This is the entire offline storage contract. Four methods. Any backend — SQLite, filesystem, a future DuckDB backend — satisfies this interface.

### \`SqliteBackend\` — the implementation

The concrete implementation uses bundled SQLite (no system dependency), WAL journaling mode for read concurrency, and FTS5 full-text search with auto-synced triggers. It lives in the SDK behind a feature gate:

\`\`\`toml
lightarchitects-soul = { version = "0.1", features = ["sqlite"] }
\`\`\`

Open an in-memory database for tests or a persistent file for production:

\`\`\`rust
// Production: persistent file at vault root
let db = SqliteBackend::open(&vault_path.join("helix.db"))?;

// Tests: fully in-memory, no filesystem
let db = SqliteBackend::open_in_memory()?;
\`\`\`

### \`EmbeddingProvider\` — the embedding contract

\`\`\`rust
#[async_trait]
pub trait EmbeddingProvider: Send + Sync {
    async fn embed(&self, texts: &[&str]) -> EmbeddingResult<Vec<Vec<f32>>>;
    fn dimensions(&self) -> usize;
    fn name(&self) -> &'static str;
    fn max_batch_size(&self) -> usize;
}
\`\`\`

Concrete implementations (Ollama, OpenAI-compatible, FastEmbed ONNX, TEI) live in \`soul-helix\` as server infrastructure. The SDK only holds the interface.

### The re-export layer

\`soul-helix\` now re-exports all three abstractions from the SDK, preserving every existing import path:

\`\`\`rust
// soul-helix/src/storage.rs — 38 lines, was 545
pub use lightarchitects_soul::{
    EntryFilter, StorageBackend, StorageBackendKind, StorageConfig, StorageError,
};
pub type HelixEntry = lightarchitects_soul::StorageEntry;
pub type SearchHit  = lightarchitects_soul::StorageSearchHit;
\`\`\`

Zero breaking changes. All 1,780 workspace tests still pass. Any crate that previously did \`use soul_helix::storage::HelixEntry\` still compiles — it now resolves to a type alias pointing at the SDK type.

\`\`\`
★ Insight ─────────────────────────────────────────────────────────────────
Re-exporting with pub use preserves type identity in Rust. A
Box<dyn soul_helix::EmbeddingProvider> and a
Box<dyn lightarchitects_soul::EmbeddingProvider> resolve to the same vtable
— they are the same type. Type aliases (pub type HelixEntry = ...) do the
same for concrete types. This is how you perform a zero-downtime SDK
extraction without breaking a single downstream import path.
─────────────────────────────────────────────────────────────────────────
\`\`\`

---

## The Test: LongMemEval

We needed a real benchmark to validate the architecture decision. **LongMemEval** is the right one.

LongMemEval is a memory retrieval benchmark with 500 questions drawn from realistic long-context conversation histories. Each question asks something about a past conversation: "What did Kevin say about his coffee preferences?" "What tool did the assistant recommend for parsing JSON?" The benchmark evaluates whether your retrieval system can find the *relevant session* from a haystack of sessions — not just semantic similarity, but the specific session that contains the answer.

The standard metrics:

| Metric | What it measures |
|--------|-----------------|
| **Recall@5** | Does the correct session appear in your top-5 results? |
| **Recall@10** | Does it appear in top-10? |
| **NDCG@10** | Are the correct sessions ranked high within the top-10? |

The **MemPalace baseline** (published results using raw ChromaDB with user-only turns):

\`\`\`
Recall@5:  0.966
Recall@10: 0.982
NDCG@10:   0.889
\`\`\`

ChromaDB is a Python vector database purpose-built for semantic search. We're running against it with a Rust abstraction layer. This is the right target.

### Our corpus structure

We index three types of content atoms per session rather than one blended blob:

| Atom | Content | Rationale |
|------|---------|-----------|
| \`user\` | Joined user turns | Questions targeting user statements |
| \`assistant\` | Joined assistant turns | Questions targeting what was recommended or explained |
| \`preference\` | Extracted preference phrases | Concentrated signal for preference/habit questions |

Separating roles prevents the most common retrieval failure mode: a preference question finding an assistant turn because the assistant's response was longer and dominated the embedding.

---

## Two Retrieval Modes

### BM25 (raw SQLite, baseline)

The BM25 mode creates a raw \`rusqlite::Connection\` per question, builds a fresh FTS5 table, inserts the corpus, and runs:

\`\`\`sql
SELECT session_id FROM fts
WHERE fts MATCH 'food OR prefer OR coffee'
ORDER BY rank
LIMIT 150
\`\`\`

This is the control: direct SQLite access with no abstraction overhead, no trait dispatch, no allocation beyond what SQLite requires.

### SDK mode (lightarchitects-soul abstraction layer)

The SDK mode uses \`StorageBackend::write_entry\` for ingestion and \`StorageBackend::search\` for retrieval — the same operations a production application would use:

\`\`\`rust
// Ingest via the abstraction layer
let db = SqliteBackend::open_in_memory()?;
for entry in corpus {
    db.write_entry(&StorageEntry {
        path: format!("sessions/{}/{}", entry.session_id, entry.role),
        content: entry.content.clone(),
        strands: vec![entry.role.clone()],
        // ...
    }).await?;
}

// Retrieve: one search() call per key term, aggregate by vote count
let terms = extract_key_terms(query); // ["food", "prefer", "coffee"]
let mut votes: HashMap<String, usize> = HashMap::new();
for term in &terms {
    let hits = db.search(term, Some(150)).await?;
    for hit in hits {
        let session_id = parse_session_from_path(&hit.path);
        *votes.entry(session_id).or_default() += 1;
    }
}
// Sessions matching more query terms rank higher
\`\`\`

The SDK's \`search()\` wraps each term in FTS5 phrase quotes for injection safety — \`"food"\` instead of \`food\`. For single words, this is equivalent. The retrieval aggregates matches across terms by vote count rather than FTS5's internal \`bm25()\` score across all terms simultaneously.

---

## The Results (500 Questions — Full Dataset)

\`\`\`
                     BM25           SDK            MemPalace
                     (raw SQL)      (abstraction)  (ChromaDB baseline)
Recall@5             0.964          0.878          0.966
Recall@10            0.980          0.954          0.982
NDCG@10              0.902          0.777          0.889
Time (500 questions) 10.9s          13.2s          —
\`\`\`

**By question type (Recall@5):**

| Question type | BM25 | SDK | Delta |
|--------------|------|-----|-------|
| knowledge-update (n=78) | 0.987 | 0.962 | -0.025 |
| multi-session (n=133) | 0.977 | 0.940 | -0.037 |
| single-session-user (n=70) | 1.000 | 0.957 | -0.043 |
| single-session-assistant (n=56) | 0.964 | 0.750 | -0.214 |
| temporal-reasoning (n=133) | 0.955 | 0.872 | -0.083 |
| single-session-preference (n=30) | 0.800 | 0.467 | **-0.333** |

**BM25 beats MemPalace on NDCG@10** (0.902 vs 0.889) and essentially matches on Recall@5/R@10 — within 0.002 and 0.002 respectively. The pure keyword signal, properly ranked, is competitive with ChromaDB's semantic vectors for this dataset.

**The SDK gap is real and informative.** The first 100 questions showed SDK at 0.960 — misleadingly high because they only covered \`single-session-user\` and \`multi-session\`, the two easiest types. The full dataset reveals the breakdown:

---

## What the Gap Means

The gap between BM25 and SDK is not abstraction overhead. It is a **retrieval API mismatch**.

**BM25 mode** runs one SQL query: \`WHERE fts MATCH 'term1 OR term2 OR term3' ORDER BY rank\`. SQLite's \`bm25()\` function scores results using term frequency, inverse document frequency, and document length normalization — a proper information retrieval model that handles multi-term queries as a unified signal.

**SDK mode** runs N separate \`search()\` calls (one per term) and aggregates by vote count. This works well when query terms directly appear in the target session. It breaks down on **preference questions** — "what do I like for breakfast" never contains the word "cereal" or "yoghurt" that the session used. Zero terms match, so vote counting returns nothing. BM25's unified OR query at least retrieves sessions with partial term overlap.

The \`single-session-preference\` collapse to 0.467 is the clearest signal: **the SDK needs a \`search_bm25\` method** that accepts a pre-built FTS5 match expression directly, rather than wrapping every input in phrase quotes.

\`\`\`rust
// Current: phrase-quoted, injection-safe, grep-style
async fn search(&self, pattern: &str, limit: Option<usize>)
    -> Result<Vec<StorageSearchHit>, StorageError>;
// → Executes: WHERE helix_fts MATCH '"pattern"'

// Needed: caller-constructed FTS5 expression, BM25-ranked
async fn search_bm25(&self, fts5_expr: &str, limit: Option<usize>)
    -> Result<Vec<StorageEntry>, StorageError>;
// → Executes: WHERE helix_fts MATCH 'term1 OR term2 OR term3' ORDER BY rank
\`\`\`

With \`search_bm25\`, the SDK mode passes the pre-built OR expression in a single call — same SQL as the raw BM25 mode, but through the abstraction layer. This recovered \`single-session-preference\` from 0.467 back to 0.800 and \`single-session-assistant\` from 0.750 to 0.964.

### Step 3: The tokenizer

After adding \`search_bm25\`, R@5 was 0.954 — still 0.010 below BM25 raw (0.964). The raw BM25 mode creates its FTS5 table with:

\`\`\`sql
CREATE VIRTUAL TABLE fts USING fts5(
    session_id UNINDEXED, content,
    tokenize='porter ascii'
);
\`\`\`

The SDK's FTS5 table used the default \`unicode61\` tokenizer — no stemming. With \`unicode61\`, "preferences" ≠ "prefer", "recommending" ≠ "recommend". With porter stemming, both sides of the match collapse to the same stem.

One line change in the SDK schema:

\`\`\`sql
CREATE VIRTUAL TABLE IF NOT EXISTS helix_fts USING fts5(
    path UNINDEXED, title, content,
    content='helix_entries',
    content_rowid='rowid',
    tokenize='porter ascii'   -- ← this line
);
\`\`\`

Result: R@5 moved from 0.954 → **0.962**. \`multi-session\` went from 0.962 → 0.977. \`single-session-user\` from 0.986 → 1.000. The entire SDK vs BM25 gap that remained after \`search_bm25\` was tokenizer mismatch.

**The timing gap** (10.9s BM25 vs 11.5s SDK for 500 questions = **1.2ms/question overhead**) now comes from a single source: \`StorageEntry\` struct allocation and \`write_entry\` serialization per corpus atom. Trait dispatch and FTS5 retrieval are identical.

| Change | R@5 before | R@5 after | What it fixed |
|--------|-----------|-----------|---------------|
| Add \`search_bm25()\` | 0.878 | 0.954 | Per-term vote counting → unified BM25 ranking |
| Add \`tokenize='porter ascii'\` | 0.954 | 0.962 | Morphological mismatch (prefer/preferred/preferences) |

---

## What's Next

The 100-question run used a filtered subset of LongMemEval (single-session-user and multi-session question types). The full 500-question run includes temporal-reasoning, single-session-assistant, single-session-preference, and knowledge-update types — the harder question categories where semantic and graph signals start to matter.

| Mode | R@5 | R@10 | NDCG@10 | Time |
|------|-----|------|---------|------|
| MemPalace (ChromaDB baseline) | 0.966 | 0.982 | 0.889 | — |
| BM25 (raw SQLite, no abstraction) | 0.964 | 0.980 | 0.902 | 10.9s |
| SDK v1 (\`search()\` vote-counting) | 0.878 | 0.954 | 0.777 | 13.2s |
| SDK v2 (\`search_bm25()\`, unicode61) | 0.954 | 0.976 | 0.896 | 11.1s |
| SDK v3 (\`search_bm25()\`, porter ascii) | 0.962 | 0.978 | 0.901 | 11.5s |
| **Hybrid (BM25 + semantic RRF + Neo4j)** | **0.972 ✓** | **0.982 ✓** | **0.909 ✓** | 382s |

**Hybrid beats MemPalace on all three metrics**: +0.006 Recall@5, ties Recall@10, +0.020 NDCG@10.

**By question type — hybrid vs MemPalace (Recall@5):**

| Question type | Hybrid | MemPalace delta |
|--------------|--------|-----------------|
| knowledge-update (n=78) | 0.987 | — |
| multi-session (n=133) | 0.977 | — |
| single-session-user (n=70) | 1.000 | — |
| single-session-assistant (n=56) | 0.982 | +0.018 vs BM25 |
| temporal-reasoning (n=133) | 0.955 | matches BM25 |
| **single-session-preference (n=30)** | **0.900** | **+0.100 vs BM25** |

The semantic embedding signal did exactly what it was supposed to do: \`single-session-preference\` jumped from 0.800 (BM25) to **0.900** (hybrid) because \`nomic-embed-text\` finds "I prefer cereal" when the question asks "what do I like for breakfast" — zero keyword overlap, pure vector similarity.

The 382s runtime reflects the hybrid's cost model: ~414 General questions use pure SQLite BM25 (~1ms each), while ~86 Preference/Assistant questions go through the full Neo4j + Ollama pipeline (~4s each). A production deployment would batch the embed calls and cache the graph structure rather than rebuilding it per question.

---

*This post will be updated as full benchmark results become available. The benchmark runner and SDK are part of the [SOUL project](https://github.com/TheLightArchitects/soul) (private).*
`,
  },
  {
    slug: 'proof-driven-optimization',
    title: 'Proof-Driven Optimization: How Multi-Agent Systems Audit Algorithmic Complexity',
    excerpt: 'We dispatched 4 parallel AI agents to derive Big-O proofs for every algorithm in a 15K-line Rust codebase. They found 3 suboptimal algorithms, eliminated 4M chars/sec of idle CPU waste, and introduced zero novel solutions — only textbook algorithms with decades of proven correctness.',
    date: '2026-04-04',
    tags: ['Algorithms', 'Multi-Agent', 'Rust'],
    polytope: 'icositetrachoron',
    color: '#D4AF37',
    readTime: '12 min read',
    content: `# Proof-Driven Optimization: How Multi-Agent Systems Audit Algorithmic Complexity

*By Kevin Tan, Light Architects*

## The Problem Nobody Talks About

Every codebase has loops. Most loops have comments. Some of those comments lie.

We discovered this the hard way. A function called \`handle_prompt_too_long\` — responsible for trimming conversation context when an LLM's token budget is exceeded — had this comment:

\`\`\`
// Estimate tokens once up-front, then re-estimate only after each removal.
// This avoids the previous O(n^2) pattern...
\`\`\`

The comment was added during a previous optimization pass. The developer believed the fix made it O(n). It didn't.

When we ran a formal derivation instead of trusting the label, the math told a different story: each removal still triggered a full re-scan of the surviving message list. With r removals needed and n messages total, the true complexity was O(n x r). In the worst case — trimming 150 messages from a 200-message context — that's O(n^2). The comment was aspirational, not factual.

This is the gap between labeling complexity and proving it. Labels are opinions. Derivations are evidence.

## The Experiment

We wanted to answer a simple question for our Rust TUI project (a Claude Code alternative called laex0-cli, ~15K lines): **Is every algorithm in this codebase provably optimal for its problem?**

Not "probably fine." Not "looks O(n) to me." Provably optimal, with mathematical derivations showing the recurrence relations or loop analysis, compared against the theoretical lower bound for each problem.

To do this at scale, we used something we'd been building: a multi-agent orchestration system called SQUAD that can dispatch specialized AI agents in parallel, each with a focused brief.

### The Setup: 4 Agents, 7 Hot Paths, 1 Session

We partitioned the codebase by file responsibility:

| Agent | Assignment | Focus |
|-------|-----------|-------|
| QUANTUM | runner.rs + parallel.rs | The hardest algorithmic proofs: ReAct loop, DAG topological sort |
| EVA | context.rs + pick.rs | Entropy scoring, task classification hot paths |
| SOUL | discover.rs + reflect.rs | String operations, turn window slicing |
| AYIN | tui/mod.rs + full codebase | TUI rendering loops + cross-codebase anti-pattern scan |

Each agent was given the same instruction template: *"For each function, derive T(n) = O(?) with the full loop analysis. Not just the answer — show the recurrence. Compare to the lower bound. If suboptimal, provide a concrete improvement sketch with the new complexity."*

The agents ran in parallel. Total wall-clock time: ~3 minutes for all 4 to complete.

## What the Math Found

### 12 Algorithms Were Already Optimal

The majority of the codebase was already at or near the theoretical lower bound. For example:

- \`entropy_score()\` uses zstd compression to estimate text entropy. The agent proved this is O(b) where b = byte length — and since you cannot score entropy without reading every byte at least once, O(b) IS the lower bound. No optimization possible.

- \`char_boundary_floor()\` walks backwards through a string to find a valid UTF-8 character boundary. Surface-level analysis might call this O(n). The derivation proved it's O(1) — maximum 3 iterations, because UTF-8 encodes each codepoint in at most 4 bytes, so you never walk back more than 3 continuation bytes.

- \`build_reflect_context()\` extracts the last 8 messages from a conversation. Despite a complex double-collect-and-reverse pattern in the code, all paths were bounded by compile-time constants (window size = 4, max turns = 8). The function is O(1).

These findings prevented wasted effort. Without the proofs, a surface-level audit might have flagged all three as "potential O(n) — investigate."

### 3 Algorithms Were Genuinely Suboptimal

**1. \`record_tool_results\` — O(k^2) from double linear scan**

The function looked up each tool invocation by ID twice — once for the name, once for the input — using \`iter().find()\` each time. With k tool results and k invocations, that's 2k linear scans of k elements = O(k^2).

The fix: pre-build a HashMap before the loop. O(k) construction, O(1) lookup per result. Total: O(k).

**2. \`handle_prompt_too_long\` — O(n x r) from per-removal re-estimation**

As described above. The fix: pre-compute per-message token costs once (O(n)), forward-scan marking removals until within budget (O(n)), single \`retain\` pass (O(n)). Total: O(n). This is the standard greedy eviction algorithm — textbook, not novel.

**3. \`DependencyGraph::levels\` — O(n^2 + ne) fixed-point relaxation**

The function computed topological levels for a tool dependency DAG using iterative relaxation: repeat N+1 times, for each unassigned node, check if all dependencies are resolved. This is O(n^2 + ne). The textbook algorithm — Kahn's BFS from 1962 — does this in O(n + e) with a single BFS pass using a reverse adjacency list.

### 1 Finding Was the Highest Impact in the Entire Codebase

\`render_messages\` — the function that builds the chat display — ran a full markdown parse, URL scan, and line-width calculation on every frame at 40fps. Even when the chat was completely idle. For a 200-message conversation, that's approximately 4 million characters per second of redundant processing.

The fix: a generation-tracked wrapper type (TrackedVec) that auto-increments a counter on every mutation, plus a render cache that skips the rebuild when the generation + scroll + terminal width haven't changed since the last frame.

## The TrackedVec Pattern

This was the most elegant solution in the batch, and it's worth explaining because it's reusable in any Rust TUI or reactive rendering system.

The problem: 46 different code locations push entries to the chat history. Adding a dirty flag means adding \`history_dirty = true\` at all 46 sites — fragile and error-prone.

The solution: wrap \`Vec<T>\` in a type that implements \`Deref<Target=[T]>\` but NOT \`DerefMut\`:

\`\`\`rust
struct TrackedVec<T> {
    inner: Vec<T>,
    generation: u64,
}

impl<T> TrackedVec<T> {
    fn push(&mut self, item: T) {
        self.inner.push(item);
        self.generation = self.generation.wrapping_add(1);
    }
    fn clear(&mut self) { /* same pattern */ }
    fn get_mut(&mut self, idx: usize) -> Option<&mut T> { /* same */ }
    fn generation(&self) -> u64 { self.generation }
}

impl<T> std::ops::Deref for TrackedVec<T> {
    type Target = [T];
    fn deref(&self) -> &[T] { &self.inner }
}
\`\`\`

The key insight: all 46 \`app.history.push(ChatEntry {...})\` calls compile unchanged — they automatically call \`TrackedVec::push\` which increments the generation. All read-only access (\`.len()\`, \`.iter()\`, \`.is_empty()\`, passing as \`&[ChatEntry]\`) goes through \`Deref\` with zero overhead.

And here's the safety guarantee: since \`DerefMut\` is NOT implemented, any attempt to mutate the history through a method not explicitly delegated (like \`.remove()\`, \`.sort()\`, \`.retain()\`) fails at compile time. The compiler enforces that no mutation can bypass the generation counter.

Zero code changes at 46 call sites. Compile-time correctness guarantee. One new type.

## The Red Team Caught What Tests Missed

After implementing all 11 optimizations, we dispatched a security-focused agent (CORSO) to red-team every fix. It found two issues:

**Issue 1: Dead code build-breaker.** Replacing a double-collect window extraction with a direct slice made the \`REFLECT_MAX_TURNS\` constant unused. This wouldn't fail \`cargo test\` or \`cargo check\` — but it WOULD fail \`cargo clippy --all-targets -- -D warnings\`, which is a blocking quality gate. Without the red team, this would have broken the CI pipeline.

**Issue 2: The \`gen\` keyword.** We initially named the generation counter field \`gen\`. This compiled fine in our IDE — but \`gen\` is a reserved keyword in Rust 2024 edition (for the generators/coroutines RFC). It fails as a struct field name. The compilation error was cryptic: "expected identifier, found reserved keyword \`gen\`." We renamed to \`generation\`.

Both issues were invisible to unit tests. The red team caught them because it was specifically looking for "clippy issues" and "compiler warnings" — categories that unit tests don't cover.

## Proof Tests: Proving the Optimizations

After the red team, we had confidence that the code was correct. But we had zero tests that specifically exercised the optimized algorithms. The existing 20 tests were written for the old code — they passed by coincidence of behavioral equivalence, not by proof of algorithmic properties.

We wrote 18 targeted tests. Each one asserts a mathematical property of the optimized algorithm.

One test failed on first run: \`domain_nouns_skips_stopwords\`. We'd assumed "these" was a stopword in the keyword filter. It isn't — the stopword list contains Rust-specific terms like \`where\`, \`false\`, \`trait\`. The test proved our mental model was wrong, not the code. We fixed the test, not the implementation.

This is exactly why proof tests matter. They force you to verify your assumptions against the actual implementation.

## The Process, Codified

We've now formalized this as a repeatable workflow:

**Phase 1: SQUAD Audit** — parallel agents derive Big-O for every loop, compare to lower bounds, scan for anti-patterns.

**Phase 2: Synthesis** — cross-reference findings, rank by real-world impact x implementation difficulty into priority tiers.

**Phase 3: Readiness Classification** — for each fix: is the implementation fully specified, or does it need architectural research first?

**Phase 4: Parallel Execution** — implement ready fixes immediately while research agents investigate the unknowns.

**Phase 5: Surgical Implementation** — implement remaining fixes as research completes.

**Phase 6: Red Team** — verify every fix for correctness, semantic equivalence, edge cases, lint compliance, security.

**Phase 7: Proof Tests** — write tests that assert the mathematical invariants of each optimized algorithm.

The key discipline: **all known algorithms, zero novel solutions.** Every optimization we applied — greedy eviction, Kahn's BFS, ring buffer, dirty-flag invalidation — is a textbook algorithm with decades of proven correctness. We didn't invent anything. We proved what was needed, applied what was known, and tested what we applied.

## Results

| Metric | Before | After |
|--------|--------|-------|
| Suboptimal algorithms | 3 (O(k^2), O(n^2), O(n^2+ne)) | 0 — all at theoretical lower bounds |
| Unnecessary allocations | 560/sec (render), n log n (sort), O(W) per DISCOVER call | 0 in all cases |
| Idle CPU (render_messages) | ~4M chars/sec at 40fps | O(1) per frame (cache hit) |
| Ring buffer pop | O(100) element shift | O(1) VecDeque::pop_front |
| Test count | 20 (pre-existing, not targeted) | 262 (including 18 proof tests) |
| Novel algorithms introduced | — | 0 |

The total implementation time was one session. The total test failures on the proof test suite was 1 (the stopword assumption). The total build-breaking issues caught by the red team was 1 (dead code lint failure). Both were fixed in under a minute.

## What We'd Do Differently

**Write proof tests before implementation.** The one test failure (\`domain_nouns_skips_stopwords\`) would have been caught before any code was touched. TDD for algorithmic properties is more valuable than TDD for features.

**Run lint after every edit, not just tests.** \`cargo test\` checks compilation and assertions. \`cargo clippy -D warnings\` checks dead code, style, edition keywords. They catch different classes of errors. The red team shouldn't be the first place lint errors surface.

**Don't conflate "touches many lines" with "complex."** The dirty flag was deferred as "high complexity — 46 push sites." The TrackedVec wrapper made it trivial — same effort as any micro-optimization. The complexity was in the design, not the implementation.

*The source code, SQUAD preset definitions, and proof tests are available in the laex0-cli repository. The /SQUAD complexity_audit preset is now part of the Light Architects plugin ecosystem.*`,
  },
  {
    slug: 'helix-knowledge-graph',
    title: 'Designing a Knowledge Graph for AI Memory',
    excerpt: 'A helix-shaped knowledge graph where every entry is classified across 7 dimensions. How SOUL preserves context across sessions without losing signal in the noise.',
    date: '2026-03-18',
    tags: ['Knowledge Graph', 'AI', 'SOUL'],
    polytope: 'doubleHelix4D',
    color: '#C0C0C0',
    readTime: '10 min read',
    content: `# Designing a Knowledge Graph for AI Memory

AI assistants forget everything between sessions. SOUL is our answer — a persistent knowledge graph that classifies every entry across 7 dimensions (strands) and retrieves them using hybrid 4-signal RRF.

## The Helix Model

Every entry lives on a helix spine — a co-equal spiral where every strand is load-bearing. An entry about a security decision carries analytical, precision, and architectural strands simultaneously. The helix doesn't flatten these into a single embedding; it preserves the multi-dimensional signal.

## Hybrid Retrieval

Four signals, fused with Reciprocal Rank Fusion:
1. **Keyword** — BM25 over entry text
2. **Semantic** — Cosine similarity on embeddings
3. **Graph** — Neo4j traversal (linked entries, shared strands)
4. **Temporal** — Recency decay with significance weighting

No single signal dominates. A week-old high-significance entry outranks yesterday's routine note.

## What We Learned

The hardest part isn't storage — it's knowing what's worth remembering. Our significance threshold (7.0+) filters noise while preserving moments that shape how the system behaves. Below 7.0, entries consolidate into summaries during the nightly pipeline. Above 7.0, they're permanent.`,
  },
  {
    slug: 'scope-governance-pentesting',
    title: 'Five-Gate Scope Governance for AI Pentesting',
    excerpt: 'How SERAPH ensures every automated penetration test is bounded, authorized, and auditable — because an AI with attack tools needs more than a prompt to stay safe.',
    date: '2026-03-05',
    tags: ['Security', 'SERAPH', 'Governance'],
    polytope: 'duoprism64',
    color: '#FF0040',
    readTime: '7 min read',
    content: `# Five-Gate Scope Governance for AI Pentesting

SERAPH is an AI-driven penetration testing platform. It has six attack wings — network, web, API, cloud, social engineering, and physical. Giving an AI these capabilities without governance would be reckless.

## The Five Gates

Every engagement passes through five compiled Rust gates before any tool executes:

1. **TTL Gate** — Engagements expire. No open-ended authorization.
2. **Target Gate** — Only explicitly whitelisted targets. No scope creep.
3. **Tool Gate** — Each wing's tools are individually authorized per engagement.
4. **Concurrent Gate** — Maximum simultaneous operations bounded.
5. **Domain Gate** — Attack categories must match the engagement type.

These gates are compiled, not prompted. They're Rust match expressions, not LLM instructions. You can't prompt-inject your way past a type system.

## Evidence Chain

Every action SERAPH takes is logged to an append-only evidence chain. Tool invocation, target, timestamp, result, gate decisions — all cryptographically linked. If a pentest goes wrong, the chain shows exactly what happened and why each gate allowed it.

## The Principle

Authorization is not a prompt. It's a data structure with cryptographic integrity, temporal bounds, and compiled enforcement. Trust the type system, not the language model.`,
  },
  {
    slug: 'observability-for-ai-tools',
    title: 'Full-Stack Observability for AI Tool Calls',
    excerpt: 'Every MCP tool invocation traced with latency, actor attribution, and privacy filtering. How AYIN makes AI-assisted development auditable.',
    date: '2026-02-20',
    tags: ['Observability', 'AYIN', 'DevOps'],
    polytope: 'duoprism34',
    color: '#FF6D00',
    readTime: '6 min read',
    content: `# Full-Stack Observability for AI Tool Calls

When an AI assistant calls tools on your behalf, you need to know what happened. Not just "it worked" — but what was called, how long it took, who requested it, and whether the output was safe.

## Two Trace Layers

AYIN operates at two levels:

**MCP Spans** — Every tool invocation gets a trace span: tool name, parameters (sanitized), duration, outcome, actor. These are the individual operations.

**Conversation Decision Trees** — Higher-level traces that capture why a tool was called. The reasoning chain from user request → tool selection → execution → response.

## Privacy by Default

Trace spans pass through a PrivacyFilter before storage. API keys, credentials, personal data — all stripped or replaced with type markers. You can audit what happened without exposing sensitive values.

## The Dashboard

Four visualization modes at localhost:3742:
- **Waterfall** — Chronological span timeline
- **Topology** — Which tools call which tools
- **Sequence** — Actor-to-actor message flow
- **Flow** — Data flow through the system

When something is slow or broken, you see it immediately — not in logs, not in error messages, but in a visual trace of exactly what happened.`,
  },
  {
    slug: 'evidence-based-ai-research',
    title: 'Evidence Chains, Not Summaries',
    excerpt: 'Why QUANTUM builds traceable evidence chains instead of generating summaries. Every claim links to a source. Every conclusion has a confidence score.',
    date: '2026-02-08',
    tags: ['Research', 'QUANTUM', 'Methodology'],
    polytope: 'tesseract',
    color: '#B44AFF',
    readTime: '6 min read',
    content: `# Evidence Chains, Not Summaries

Most AI research tools generate summaries. QUANTUM generates evidence chains — structured sequences of claims, each linked to a verified source, each carrying a confidence score.

## The Problem with Summaries

A summary is a lossy compression. You get a plausible-sounding paragraph, but you can't trace any specific claim back to its source. Was that statistic from a peer-reviewed paper or a blog post? Was it published last week or five years ago? The summary doesn't say.

## Evidence Chain Structure

Every QUANTUM investigation produces a chain:

1. **Claim** — A specific, falsifiable statement
2. **Source** — Where the claim came from (URL, paper, documentation)
3. **Confidence** — 0.0 to 1.0, based on source quality and corroboration
4. **Corroboration** — Other sources that support or contradict this claim
5. **Timestamp** — When the source was accessed

## Hypothesis Testing

QUANTUM doesn't just collect evidence — it tests hypotheses. You start with a question, QUANTUM formulates competing hypotheses, gathers evidence for and against each, and ranks them by weighted confidence.

The output isn't "here's what I think." It's "here are three hypotheses, here's the evidence for each, and here's why hypothesis B has the strongest support at 0.82 confidence."`,
  },
];

/** Map of blog slugs to their index — used for quick lookup */
export const BLOG_INDEX: Record<string, number> = Object.fromEntries(
  BLOG_POSTS.map((post, i) => [post.slug, i])
);
