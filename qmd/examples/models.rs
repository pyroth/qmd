//! Model download and management - self-contained example.
//!
//! Run: `cargo run --example models`

use anyhow::Result;
use qmd::{
    EmbeddingEngine, GenerationEngine, RerankEngine,
    config::get_model_cache_dir,
    llm::{DEFAULT_EMBED_MODEL, DEFAULT_EMBED_MODEL_URI, list_cached_models, model_exists},
    pull_model, resolve_model,
};

fn main() -> Result<()> {
    // Cache directory
    println!("Cache: {}\n", get_model_cache_dir().display());

    // Cached models
    println!("Cached models:");
    let models = list_cached_models();
    if models.is_empty() {
        println!("  (none - will download)");
    } else {
        for m in &models {
            println!("  {}", m);
        }
    }

    // Model availability check
    println!("\nAvailability:");
    let check = |name: &str, ok: bool| println!("  {}: {}", name, if ok { "yes" } else { "no" });
    check("embed model", model_exists(DEFAULT_EMBED_MODEL));
    check("EmbeddingEngine", EmbeddingEngine::load_default().is_ok());
    check("GenerationEngine", GenerationEngine::is_available());
    check("RerankEngine", RerankEngine::is_available());

    // Download model (demo)
    println!("\nDownloading embedding model...");
    let result = pull_model(DEFAULT_EMBED_MODEL_URI, false)?;
    println!("  Path: {}", result.path.display());
    println!("  Size: {} bytes", result.size_bytes);
    println!("  Refreshed: {}", result.refreshed);

    // URI resolution
    println!("\nURI resolution:");
    let uri = "hf:ggml-org/embeddinggemma-300M-GGUF/embeddinggemma-300M-Q8_0.gguf";
    if let Ok(path) = resolve_model(uri) {
        println!("  {} ->\n  {}", uri, path.display());
    }

    // Load and test
    let mut engine = EmbeddingEngine::new(&result.path)?;
    let emb = engine.embed("test embedding")?;
    println!("\nModel info:");
    println!("  Name: {}", emb.model);
    println!("  Dimensions: {}", emb.embedding.len());

    Ok(())
}
