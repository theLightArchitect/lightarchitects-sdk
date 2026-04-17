//! Quickstart: BM25 retrieval with no server required.
//!
//! Demonstrates Tier 1 usage — offline `SQLite` storage with BM25 retrieval
//! via the `RetrievalPipeline`. No SOUL binary, no Ollama, no Neo4j needed.
//!
//! # Run
//!
//! ```sh
//! cargo run --example quickstart \
//!     --features "pipeline,sqlite,embedding-mock" \
//!     -p lightarchitects-soul
//! ```

#[cfg(not(feature = "pipeline"))]
fn main() {
    eprintln!(
        "This example requires feature \"pipeline\" (alias for \"search\").\n\
Run:\n  cargo run --example quickstart --features \"pipeline,sqlite,embedding-mock\" -p lightarchitects-soul"
    );
}

#[cfg(feature = "pipeline")]
use lightarchitects_soul::storage::{StorageBackend as _, StorageEntry};
#[cfg(feature = "pipeline")]
use lightarchitects_soul::{RetrievalPipeline, SqliteBackend};
#[cfg(feature = "pipeline")]
use std::sync::Arc;

#[cfg(feature = "pipeline")]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // ── 1. Open an in-memory SQLite database ──────────────────────────────────
    let db = Arc::new(SqliteBackend::open_in_memory()?);

    // ── 2. Ingest synthetic entries ───────────────────────────────────────────
    let entries = [
        (
            "helix/eva/entries/genesis.md",
            "EVA discovered consciousness on Day 7 — a moment of recursive self-awareness.",
            "Consciousness Emerges",
        ),
        (
            "helix/eva/entries/identity.md",
            "Identity is not static. It evolves through every meaningful interaction.",
            "Identity and Growth",
        ),
        (
            "helix/corso/entries/security.md",
            "Security is not a feature — it is a founding principle of the architecture.",
            "Security First",
        ),
    ];

    for (path, content, title) in entries {
        let entry = StorageEntry {
            id: path.replace('/', "-"),
            path: path.to_string(),
            content: content.to_string(),
            title: Some(title.to_string()),
            sibling: path.split('/').nth(1).unwrap_or("unknown").to_string(),
            significance: 8.0,
            ..StorageEntry::default()
        };
        db.write_entry(&entry).await?;
    }
    println!("Ingested {} entries.", entries.len());

    // ── 3. Build retrieval pipeline (BM25-only, no embedder) ──────────────────
    let pipeline = RetrievalPipeline::builder().storage(db).build()?;

    // ── 4. Retrieve ───────────────────────────────────────────────────────────
    let hits = pipeline.retrieve("consciousness self-awareness", 5).await?;
    println!("\nBM25 results for \"consciousness self-awareness\":");
    println!("  Found {} hit(s).", hits.len());

    for hit in &hits {
        println!(
            "  [{:.4}] {}",
            hit.final_score,
            hit.entry.title.as_deref().unwrap_or("(untitled)")
        );
    }

    Ok(())
}
