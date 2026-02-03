//! Query expansion for improved search coverage - self-contained example.
//!
//! Run: `cargo run --example query_expansion`

mod common;

use anyhow::Result;
use qmd::{GenerationEngine, QueryType, Queryable, expand_query_simple};

fn main() -> Result<()> {
    let store = common::create_sample_store()?;
    let query = "rust error handling";

    // Simple expansion (no LLM needed)
    println!("Query: '{}'\n", query);
    println!("Simple expansion:");
    for q in expand_query_simple(query) {
        let t = match q.query_type {
            QueryType::Lex => "LEX",
            QueryType::Vec => "VEC",
            QueryType::Hyde => "HYD",
        };
        println!("  [{}] {}", t, q.text);
    }

    // Use expanded queries for search
    println!("\nSearch with expanded queries:");
    for q in expand_query_simple(query) {
        if q.query_type == QueryType::Lex {
            let results = store.search_fts(&q.text, 3, None)?;
            println!("  '{}': {} results", q.text, results.len());
        }
    }

    // LLM expansion (if available)
    println!("\nLLM expansion:");
    if GenerationEngine::is_available() {
        let engine = GenerationEngine::load_default()?;
        for q in engine.expand_query(query, true)? {
            println!("  [{:?}] {}", q.query_type, q.text);
        }
    } else {
        println!("  (GenerationEngine not available - run `qmd model pull all`)");
    }

    // Manual query construction
    println!("\nManual Queryable:");
    let queries = [
        Queryable::lex("rust error"),
        Queryable::vec("exception handling patterns"),
        Queryable::hyde("Rust uses Result type for error handling..."),
    ];
    for q in &queries {
        println!("  [{:?}] {}", q.query_type, q.text);
    }

    common::cleanup();
    Ok(())
}
