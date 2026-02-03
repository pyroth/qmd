//! Embedding generation and similarity computation - self-contained example.
//!
//! Run: `cargo run --example embedding`

mod common;

use anyhow::Result;
use qmd::{
    CHUNK_OVERLAP_TOKENS, CHUNK_SIZE_TOKENS, EmbeddingEngine, chunk_document_by_tokens,
    cosine_similarity, llm::DEFAULT_EMBED_MODEL_URI, pull_model,
};

fn main() -> Result<()> {
    println!("Loading embedding model...");
    let model = pull_model(DEFAULT_EMBED_MODEL_URI, false)?;
    let mut engine = EmbeddingEngine::new(&model.path)?;

    // Embed sample documents
    let doc1 = common::SAMPLE_DOCS[1].1; // error-handling.md
    let doc2 = common::SAMPLE_DOCS[2].1; // async-await.md

    let emb1 = engine.embed_document(doc1, Some("Error Handling"))?;
    let emb2 = engine.embed_document(doc2, Some("Async Await"))?;

    println!("Embedding dimensions: {}", emb1.embedding.len());
    println!(
        "Doc similarity: {:.4}\n",
        cosine_similarity(&emb1.embedding, &emb2.embedding)
    );

    // Query similarity
    println!("Query similarities to 'Error Handling' doc:");
    for q in [
        "Result and Option types",
        "async runtime",
        "python exceptions",
    ] {
        let q_emb = engine.embed_query(q)?;
        let sim = cosine_similarity(&emb1.embedding, &q_emb.embedding);
        println!("  '{}' -> {:.4}", q, sim);
    }

    // Document chunking
    println!("\nChunking:");
    let chunks = chunk_document_by_tokens(&engine, doc1, CHUNK_SIZE_TOKENS, CHUNK_OVERLAP_TOKENS)?;
    println!(
        "  {} chunks (size={}, overlap={})",
        chunks.len(),
        CHUNK_SIZE_TOKENS,
        CHUNK_OVERLAP_TOKENS
    );

    Ok(())
}
