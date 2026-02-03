//! Hybrid search with RRF (Reciprocal Rank Fusion) - self-contained example.
//!
//! Run: `cargo run --example hybrid_search`

mod common;

use anyhow::Result;
use qmd::{EmbeddingEngine, hybrid_search_rrf, llm::DEFAULT_EMBED_MODEL_URI, pull_model};

fn main() -> Result<()> {
    let store = common::create_sample_store()?;

    // Setup embeddings
    println!("Preparing embeddings...");
    let model = pull_model(DEFAULT_EMBED_MODEL_URI, false)?;
    let mut engine = EmbeddingEngine::new(&model.path)?;
    let now = chrono::Utc::now().to_rfc3339();
    store.ensure_vector_table(768)?;

    for (filename, content) in common::SAMPLE_DOCS {
        let hash = qmd::Store::hash_content(content);
        let emb = engine.embed_document(content, Some(filename))?;
        store.insert_embedding(&hash, 0, 0, &emb.embedding, &emb.model, &now)?;
    }

    // Hybrid search
    let query = "error handling best practices";
    println!("\nHybrid search: '{}'\n", query);

    // FTS results
    let fts = store.search_fts(query, 10, None)?;
    let fts_tuples: Vec<_> = fts
        .iter()
        .map(|r| {
            (
                r.doc.filepath.clone(),
                r.doc.display_path.clone(),
                r.doc.title.clone(),
                r.doc.body.clone().unwrap_or_default(),
            )
        })
        .collect();

    // Vector results
    let query_emb = engine.embed_query(query)?;
    let vec = store.search_vec(&query_emb.embedding, 10, None)?;
    let vec_tuples: Vec<_> = vec
        .iter()
        .map(|r| {
            (
                r.doc.filepath.clone(),
                r.doc.display_path.clone(),
                r.doc.title.clone(),
                String::new(),
            )
        })
        .collect();

    // RRF fusion
    let results = hybrid_search_rrf(fts_tuples, vec_tuples, 60);

    println!(
        "FTS: {} | Vec: {} | Hybrid: {}",
        fts.len(),
        vec.len(),
        results.len()
    );
    for (i, r) in results.iter().take(5).enumerate() {
        println!("{}. [{:.4}] {}", i + 1, r.score, r.display_path);
    }

    common::cleanup();
    Ok(())
}
