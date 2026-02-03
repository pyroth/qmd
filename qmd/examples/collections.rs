//! Collection management operations - self-contained example.
//!
//! Run: `cargo run --example collections`

mod common;

use anyhow::Result;
use qmd::{is_virtual_path, parse_virtual_path};

fn main() -> Result<()> {
    let store = common::create_sample_store()?;
    let status = store.get_status()?;

    // Collection stats
    println!("Collections:");
    for c in &status.collections {
        println!("  {}: {} docs ({})", c.name, c.active_count, c.glob_pattern);
    }

    // List files in collection
    if let Some(coll) = status.collections.first() {
        println!("\nFiles in '{}':", coll.name);
        let files = store.list_files(&coll.name, None)?;
        for (path, title, _, size) in files.iter().take(5) {
            println!("  {} - {} ({} bytes)", path, title, size);
        }
    }

    // Virtual path utilities
    println!("\nVirtual path parsing:");
    let paths = [
        "qmd://samples/rust-basics.md",
        "/local/file.md",
        "relative.md",
    ];
    for path in paths {
        if is_virtual_path(path) {
            if let Some((coll, file)) = parse_virtual_path(path) {
                println!("  {} -> [{}] {}", path, coll, file);
            }
        } else {
            println!("  {} -> local", path);
        }
    }

    common::cleanup();
    Ok(())
}
