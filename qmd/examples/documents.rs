//! Document retrieval and utility functions - self-contained example.
//!
//! Run: `cargo run --example documents`

mod common;

use anyhow::Result;
use qmd::{Store, format_bytes, is_docid};

fn main() -> Result<()> {
    let store = common::create_sample_store()?;
    let status = store.get_status()?;

    // Get document details
    if let Some(coll) = status.collections.first() {
        let files = store.list_files(&coll.name, None)?;
        if let Some((path, title, _, size)) = files.first() {
            println!("Document: {}/{}", coll.name, path);
            println!("  Title: {}", title);
            println!("  Size: {}", format_bytes(*size));

            if let Some(doc) = store.get_document(&coll.name, path)? {
                println!("  Hash: {}...", &doc.hash[..16]);
                println!("  DocID: {}", doc.docid);

                // Preview
                if let Some(body) = &doc.body {
                    let preview: String = body.chars().take(80).collect();
                    println!("  Preview: {}...", preview.replace('\n', " "));
                }

                // Find by docid
                if let Some((c, p)) = store.find_document_by_docid(&doc.docid)? {
                    println!("  Lookup by docid: {}/{}", c, p);
                }
            }
        }
    }

    // Hash utilities
    println!("\nHash content:");
    let hash = Store::hash_content("Hello, world!");
    println!("  'Hello, world!' -> {}...", &hash[..16]);

    // DocID validation
    println!("\nDocID check:");
    for id in ["#abc123", "abc123", "#ab", "#ABCDEF"] {
        println!("  '{}' -> {}", id, is_docid(id));
    }

    common::cleanup();
    Ok(())
}
