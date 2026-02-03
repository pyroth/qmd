//! Full-text search with BM25 ranking - self-contained example.
//!
//! Run: `cargo run --example fts_search`

mod common;

use anyhow::Result;
use qmd::{OutputFormat, format_search_results};

fn main() -> Result<()> {
    let store = common::create_sample_store()?;

    // Search for error handling topics
    let query = "error handling Result";
    let results = store.search_fts(query, 5, None)?;

    println!("Query: '{}'\n", query);
    for r in &results {
        println!("[{:.2}] {} - {}", r.score, r.doc.path, r.doc.title);
    }

    // Different queries
    println!("\nMore searches:");
    for q in ["async await", "ownership borrowing", "trait bounds"] {
        let n = store.search_fts(q, 10, None)?.len();
        println!("  '{}': {} results", q, n);
    }

    // Output formats
    println!("\nJSON format:");
    format_search_results(&results, &OutputFormat::Json, false);

    common::cleanup();
    Ok(())
}
