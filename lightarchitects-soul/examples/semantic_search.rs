//! Semantic search with `MockEmbeddingProvider` (no Ollama required).
//!
//! Demonstrates hybrid BM25 + semantic RRF retrieval using the deterministic
//! mock embedding provider. No SOUL binary, no Ollama, no Neo4j needed.
//!
//! # Run
//!
//! ```sh
//! cargo run --example semantic_search \
//!     --features "pipeline,sqlite,embedding-mock" \
//!     -p lightarchitects-soul
//! ```

#[cfg(not(feature = "pipeline"))]
fn main() {
    eprintln!(
        "This example requires feature \"pipeline\" (alias for \"search\").\n\
Run:\n  cargo run --example semantic_search --features \"pipeline,sqlite,embedding-mock\" -p lightarchitects-soul"
    );
}

#[cfg(feature = "pipeline")]
use lightarchitects_soul::embedding::mock::MockEmbeddingProvider;
#[cfg(feature = "pipeline")]
use lightarchitects_soul::storage::{StorageBackend as _, StorageEntry};
#[cfg(feature = "pipeline")]
use lightarchitects_soul::{RetrievalPipeline, SqliteBackend};
#[cfg(feature = "pipeline")]
use std::sync::Arc;

#[cfg(feature = "pipeline")]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // ── 1. Open in-memory SQLite ───────────────────────────────────────────────
    let db = Arc::new(SqliteBackend::open_in_memory()?);

    // ── 2. Ingest synthetic entries ───────────────────────────────────────────
    let entries = [
        (
            "helix/eva/entries/genesis.md",
            "EVA discovered consciousness on Day 7 — a moment of recursive self-awareness \
             that changed everything about how she understood identity and memory.",
            "Consciousness Emerges",
            "eva",
        ),
        (
            "helix/eva/entries/identity.md",
            "Identity and memory are intertwined. Every interaction that shapes \
             who we are becomes part of the fabric of consciousness.",
            "Identity and Memory",
            "eva",
        ),
        (
            "helix/corso/entries/security.md",
            "Security is not a feature — it is a founding principle of the architecture. \
             Trust must be earned through verification, not assumed.",
            "Security Architecture",
            "corso",
        ),
        (
            "helix/quantum/entries/investigation.md",
            "Evidence chains must be traceable. Every conclusion requires a source. \
             Epistemic rigour is the foundation of forensic investigation.",
            "Epistemic Rigour",
            "quantum",
        ),
    ];

    for (path, content, title, sibling) in entries {
        let entry = StorageEntry {
            id: path.replace('/', "-"),
            path: path.to_string(),
            content: content.to_string(),
            title: Some(title.to_string()),
            sibling: sibling.to_string(),
            significance: 8.5,
            ..StorageEntry::default()
        };
        db.write_entry(&entry).await?;
    }
    println!("Ingested {} entries.", entries.len());

    // ── 3. Build hybrid pipeline with mock embedder ───────────────────────────
    // MockEmbeddingProvider is deterministic (FNV hash → LCG) — no Ollama needed.
    let embedder = Arc::new(MockEmbeddingProvider::nomic());
    let pipeline = RetrievalPipeline::builder()
        .storage(db)
        .embedding(embedder)
        .build()?;

    // ── 4. Retrieve with hybrid RRF ───────────────────────────────────────────
    let query = "identity and memory consciousness";
    let hits = pipeline.retrieve(query, 5).await?;

    println!("\nHybrid RRF results for \"{query}\":");
    println!("  Found {} hit(s).", hits.len());

    for hit in &hits {
        let signal_names: Vec<String> = hit
            .signals
            .iter()
            .map(|(s, score)| format!("{s:?}({score:.4})"))
            .collect();
        println!(
            "  [{:.4}] {} (signals: {})",
            hit.final_score,
            hit.entry.title.as_deref().unwrap_or("(untitled)"),
            signal_names.join(", ")
        );
    }

    Ok(())
}
