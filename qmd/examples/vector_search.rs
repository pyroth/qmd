//! Vector semantic search using embeddings - self-contained example.
//!
//! Run: `cargo run --example vector_search`

mod common;

use anyhow::Result;
use qmd::{EmbeddingEngine, llm::DEFAULT_EMBED_MODEL_URI, pull_model};

fn main() -> Result<()> {
    let store = common::create_sample_store()?;

    // Download embedding model (auto-cached)
    println!("Loading embedding model...");
    let model = pull_model(DEFAULT_EMBED_MODEL_URI, false)?;
    let mut engine = EmbeddingEngine::new(&model.path)?;

    // Generate embeddings for all documents
    println!("Generating embeddings...");
    let now = chrono::Utc::now().to_rfc3339();
    store.ensure_vector_table(768)?;

    for (filename, content) in common::SAMPLE_DOCS {
        let hash = qmd::Store::hash_content(content);
        let emb = engine.embed_document(content, Some(filename))?;
        store.insert_embedding(&hash, 0, 0, &emb.embedding, &emb.model, &now)?;
    }

    // Vector search
    let query = "how to handle errors";
    let query_emb = engine.embed_query(query)?;
    let results = store.search_vec(&query_emb.embedding, 5, None)?;

    println!("\nVector search: '{}'\n", query);
    for r in &results {
        println!("[{:.4}] {}", r.score, r.doc.path);
    }

    // Compare with FTS
    println!("\nFTS comparison:");
    let fts = store.search_fts(query, 5, None)?;
    for r in &fts {
        println!("[{:.2}] {}", r.score, r.doc.path);
    }

    common::cleanup();
    Ok(())
}
