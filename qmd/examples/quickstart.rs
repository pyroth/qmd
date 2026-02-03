//! Minimal quickstart example - runs independently with embedded sample data.
//!
//! Run: `cargo run --example quickstart`

mod common;

use anyhow::Result;

fn main() -> Result<()> {
    // Create store with sample documents (no CLI needed)
    let store = common::create_sample_store()?;
    let status = store.get_status()?;

    println!("Documents: {}", status.total_documents);
    println!("Collections: {}", status.collections.len());

    // Full-text search
    let results = store.search_fts("rust ownership", 5, None)?;
    println!("\nSearch 'rust ownership':");
    for r in &results {
        println!("  [{:.2}] {}", r.score, r.doc.display_path);
    }

    common::cleanup();
    Ok(())
}
