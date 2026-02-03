//! Cross-encoder reranking for improved relevance - self-contained example.
//!
//! Run: `cargo run --example rerank`

mod common;

use anyhow::Result;
use qmd::{RerankDocument, RerankEngine, llm::DEFAULT_RERANK_MODEL_URI, pull_model};

fn main() -> Result<()> {
    let store = common::create_sample_store()?;

    println!("Loading rerank model...");
    let model = pull_model(DEFAULT_RERANK_MODEL_URI, false)?;
    let mut reranker = RerankEngine::new(&model.path)?;

    // Get initial FTS results
    let query = "error handling";
    let initial = store.search_fts(query, 10, None)?;

    // Convert to rerank format
    let docs: Vec<RerankDocument> = initial
        .iter()
        .filter_map(|r| {
            store
                .get_document(&r.doc.collection_name, &r.doc.path)
                .ok()
                .flatten()
                .map(|d| RerankDocument {
                    file: r.doc.path.clone(),
                    text: d.body.unwrap_or_default(),
                    title: Some(d.title),
                })
        })
        .collect();

    let result = reranker.rerank(query, &docs)?;

    println!("\nQuery: '{}'\n", query);
    println!("Before (BM25):");
    for (i, d) in docs.iter().take(5).enumerate() {
        println!("  {}. {}", i + 1, d.file);
    }

    println!("\nAfter (Reranked):");
    for (i, r) in result.results.iter().take(5).enumerate() {
        println!("  {}. [{:.4}] {}", i + 1, r.score, r.file);
    }

    common::cleanup();
    Ok(())
}
