//! Database maintenance and health checks - self-contained example.
//!
//! Run: `cargo run --example maintenance`

mod common;

use anyhow::Result;
use qmd::format_bytes;

fn main() -> Result<()> {
    let store = common::create_sample_store()?;
    let status = store.get_status()?;

    // Database info
    let db_path = store.db_path();
    let size = std::fs::metadata(db_path).map(|m| m.len()).unwrap_or(0);
    println!("Database: {}", db_path.display());
    println!("Size: {}", format_bytes(size as usize));

    // Status
    println!("\nStatus:");
    println!("  Documents: {}", status.total_documents);
    println!("  Needs embedding: {}", status.needs_embedding);
    println!("  Vector index: {}", status.has_vector_index);
    println!("  Collections: {}", status.collections.len());

    // Pending embeddings
    let pending = store.get_hashes_needing_embedding()?;
    println!("\nPending embeddings: {}", pending.len());

    // Cleanup operations demo
    println!("\nCleanup demo:");
    let cleared = store.clear_cache()?;
    println!("  Cleared cache: {} entries", cleared);

    let orphaned = store.cleanup_orphaned_content()?;
    println!("  Orphaned content: {} removed", orphaned);

    store.vacuum()?;
    let new_size = std::fs::metadata(db_path).map(|m| m.len()).unwrap_or(0);
    println!(
        "  Vacuum: {} -> {}",
        format_bytes(size as usize),
        format_bytes(new_size as usize)
    );

    common::cleanup();
    Ok(())
}
