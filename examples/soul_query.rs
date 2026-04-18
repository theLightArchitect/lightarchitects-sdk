//! Minimal example: query the SOUL knowledge graph via `SoulClient`.
//!
//! Set your API key in the `LA_API_KEY` environment variable, then run:
//! ```bash
//! LA_API_KEY=la_your_key cargo run --example soul_query
//! ```

use lightarchitects::soul::SoulClient;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let api_key = std::env::var("LA_API_KEY")
        .unwrap_or_else(|_| "la_your_key_here".to_owned());

    let client = SoulClient::builder().api_key(api_key).build()?;

    // Full-text search across the helix vault
    let hits = client.search("architecture decision").await?;
    println!("Search results ({} hits):", hits.len());
    for hit in &hits {
        println!("  {}:{} — {}", hit.path, hit.line_number, hit.line);
    }

    // Retrieve high-significance entries from a specific sibling
    let entries = client
        .helix()
        .sibling("eva")
        .significance_min(7.0)
        .call()
        .await?;
    println!("\nEVA helix entries (sig ≥ 7.0): {}", entries.len());
    for entry in &entries {
        println!("  [{:.1}] {}", entry.significance, entry.title);
    }

    Ok(())
}
