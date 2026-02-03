//! Common utilities and sample data for examples.
//!
//! Provides self-contained sample documents and helper functions
//! so examples can run independently without CLI setup.

use anyhow::Result;
use qmd::Store;
use std::path::PathBuf;

/// Sample markdown documents covering various Rust topics.
pub const SAMPLE_DOCS: &[(&str, &str)] = &[
    ("rust-basics.md", include_str!("data/rust-basics.md")),
    ("error-handling.md", include_str!("data/error-handling.md")),
    ("async-await.md", include_str!("data/async-await.md")),
    ("ownership.md", include_str!("data/ownership.md")),
    ("traits.md", include_str!("data/traits.md")),
    ("collections.md", include_str!("data/collections.md")),
    ("testing.md", include_str!("data/testing.md")),
    ("modules.md", include_str!("data/modules.md")),
];

/// Create a temporary store with sample documents indexed.
#[allow(dead_code)]
pub fn create_sample_store() -> Result<Store> {
    // Clean up any existing database first
    cleanup();

    let db_path = temp_db_path();
    let store = Store::open(&db_path)?;

    let now = chrono::Utc::now().to_rfc3339();
    let collection = "samples";

    for (filename, content) in SAMPLE_DOCS {
        let hash = Store::hash_content(content);
        let title = Store::extract_title(content);

        store.insert_content(&hash, content, &now)?;
        store.insert_document(collection, filename, &title, &hash, &now, &now)?;
    }

    Ok(store)
}

/// Get a temporary database path for examples.
#[allow(dead_code)]
pub fn temp_db_path() -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push("qmd_example.db");
    path
}

/// Clean up temporary database.
#[allow(dead_code)]
pub fn cleanup() {
    let path = temp_db_path();
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(path.with_extension("db-wal"));
    let _ = std::fs::remove_file(path.with_extension("db-shm"));
}
