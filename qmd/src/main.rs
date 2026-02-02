//! QMD - Query Markdown Documents
//!
//! A full-text search CLI for markdown files.

use anyhow::Result;
use clap::Parser;
use colored::Colorize;
use qmd::cli::{
    Cli, CollectionCommands, Commands, ContextCommands, DbCommands, ModelCommands, OutputFormat,
};
use qmd::collections::{
    add_collection as yaml_add_collection, add_context, get_collection, list_all_contexts,
    list_collections as yaml_list_collections, remove_collection as yaml_remove_collection,
    remove_context, rename_collection as yaml_rename_collection, set_global_context,
};
use qmd::formatter::{
    add_line_numbers, format_bytes, format_documents, format_ls_time, format_search_results,
    format_time_ago,
};
use qmd::store::{
    Store, is_docid, is_virtual_path, match_files_by_glob, parse_virtual_path, should_exclude,
};
use std::collections::HashSet;
use std::fs;
use std::path::Path;
use walkdir::WalkDir;

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Collection(cmd) => handle_collection(cmd),
        Commands::Context(cmd) => handle_context(cmd),
        Commands::Ls { path } => handle_ls(path),
        Commands::Get {
            file,
            from_line,
            max_lines,
            line_numbers,
        } => handle_get(&file, from_line, max_lines, line_numbers),
        Commands::MultiGet {
            pattern,
            max_lines,
            max_bytes,
            format,
        } => handle_multi_get(&pattern, max_lines, max_bytes, &format),
        Commands::Status => handle_status(),
        Commands::Update { pull } => handle_update(pull),
        Commands::Search {
            query,
            collection,
            limit,
            min_score,
            full,
            line_numbers: _,
            format,
        } => handle_search(
            &query,
            collection.as_deref(),
            limit,
            min_score,
            full,
            &format,
        ),
        Commands::Vsearch {
            query,
            collection,
            limit,
            min_score,
            full,
            line_numbers: _,
            format,
            model,
        } => handle_vsearch(
            &query,
            collection.as_deref(),
            limit,
            min_score,
            full,
            &format,
            model.as_deref(),
        ),
        Commands::Embed { force, model } => handle_embed(force, model.as_deref()),
        Commands::Models(cmd) => handle_models(cmd),
        Commands::Db(cmd) => handle_db(cmd),
        Commands::Qsearch {
            query,
            collection,
            limit,
            full,
            no_expand,
            no_rerank,
            format,
        } => handle_qsearch(
            &query,
            collection.as_deref(),
            limit,
            full,
            no_expand,
            no_rerank,
            &format,
        ),
        Commands::Expand { query, lexical } => handle_expand(&query, lexical),
        Commands::Rerank {
            query,
            files,
            limit,
            format,
        } => handle_rerank(&query, &files, limit, &format),
        Commands::Ask {
            question,
            collection,
            limit,
            max_tokens,
        } => handle_ask(&question, collection.as_deref(), limit, max_tokens),
        Commands::Index { name } => handle_index(name.as_deref()),
        Commands::Cleanup => handle_cleanup(),
    }
}

/// Handle cleanup command (combines db cleanup + vacuum).
fn handle_cleanup() -> Result<()> {
    let store = Store::new()?;

    println!("{}\n", "Database Cleanup".bold());

    // Clear LLM cache
    let cache_cleared = store.clear_cache()?;
    println!("{} Cleared {} cached entries", "✓".green(), cache_cleared);

    // Delete inactive documents
    let inactive = store.delete_inactive_documents()?;
    if inactive > 0 {
        println!("{} Removed {} inactive documents", "✓".green(), inactive);
    }

    // Cleanup orphaned content
    let orphaned_content = store.cleanup_orphaned_content()?;
    if orphaned_content > 0 {
        println!(
            "{} Removed {} orphaned content entries",
            "✓".green(),
            orphaned_content
        );
    }

    // Cleanup orphaned vectors
    let orphaned_vectors = store.cleanup_orphaned_vectors()?;
    if orphaned_vectors > 0 {
        println!(
            "{} Removed {} orphaned vector entries",
            "✓".green(),
            orphaned_vectors
        );
    }

    // Vacuum database
    store.vacuum()?;
    println!("{} Database vacuumed", "✓".green());

    println!("\n{} Cleanup complete", "✓".green());
    Ok(())
}

/// Handle collection subcommands.
fn handle_collection(cmd: CollectionCommands) -> Result<()> {
    match cmd {
        CollectionCommands::Add { path, name, mask } => {
            let abs_path = fs::canonicalize(&path)?;
            let abs_path_str = abs_path.to_string_lossy().to_string();

            // Generate name from path if not provided.
            let coll_name = name.unwrap_or_else(|| {
                abs_path
                    .file_name()
                    .map_or_else(|| "root".to_string(), |s| s.to_string_lossy().to_string())
            });

            // Check if collection exists.
            if get_collection(&coll_name)?.is_some() {
                eprintln!(
                    "{} Collection '{}' already exists.",
                    "Error:".red(),
                    coll_name
                );
                eprintln!("Use a different name with --name <name>");
                std::process::exit(1);
            }

            // Add to YAML config.
            yaml_add_collection(&coll_name, &abs_path_str, &mask)?;

            // Index files.
            println!("Creating collection '{coll_name}'...");
            index_files(&abs_path_str, &mask, &coll_name)?;
            println!(
                "{} Collection '{}' created successfully",
                "✓".green(),
                coll_name
            );
        }
        CollectionCommands::List => {
            let store = Store::new()?;
            let collections = store.list_collections()?;

            if collections.is_empty() {
                println!("No collections found. Run 'qmd collection add .' to create one.");
                return Ok(());
            }

            println!("{}\n", "Collections:".bold());
            for coll in &collections {
                let time_ago = coll
                    .last_modified
                    .as_ref()
                    .map_or_else(|| "never".to_string(), |t| format_time_ago(t));

                println!(
                    "{} {}",
                    coll.name.cyan(),
                    format!("(qmd://{}/)", coll.name).dimmed()
                );
                println!("  {} {}", "Pattern:".dimmed(), coll.glob_pattern);
                println!("  {} {}", "Files:".dimmed(), coll.active_count);
                println!("  {} {}", "Updated:".dimmed(), time_ago);
                println!();
            }
        }
        CollectionCommands::Remove { name } => {
            // Check if collection exists.
            if get_collection(&name)?.is_none() {
                eprintln!("{} Collection not found: {}", "Error:".red(), name);
                std::process::exit(1);
            }

            let store = Store::new()?;
            let (deleted_docs, cleaned) = store.remove_collection_documents(&name)?;
            yaml_remove_collection(&name)?;

            println!("{} Removed collection '{}'", "✓".green(), name);
            println!("  Deleted {deleted_docs} documents");
            if cleaned > 0 {
                println!("  Cleaned up {cleaned} orphaned content hashes");
            }
        }
        CollectionCommands::Rename { old_name, new_name } => {
            // Check if old collection exists.
            if get_collection(&old_name)?.is_none() {
                eprintln!("{} Collection not found: {}", "Error:".red(), old_name);
                std::process::exit(1);
            }

            // Check if new name already exists.
            if get_collection(&new_name)?.is_some() {
                eprintln!(
                    "{} Collection name already exists: {}",
                    "Error:".red(),
                    new_name
                );
                std::process::exit(1);
            }

            let store = Store::new()?;
            store.rename_collection_documents(&old_name, &new_name)?;
            yaml_rename_collection(&old_name, &new_name)?;

            println!(
                "{} Renamed collection '{}' to '{}'",
                "✓".green(),
                old_name,
                new_name
            );
        }
    }
    Ok(())
}

/// Handle context subcommands.
fn handle_context(cmd: ContextCommands) -> Result<()> {
    match cmd {
        ContextCommands::Add { path, text } => {
            let path_arg = path.as_deref().unwrap_or(".");

            // Handle global context.
            if path_arg == "/" {
                set_global_context(Some(&text))?;
                println!("{} Set global context", "✓".green());
                println!("{}", format!("Context: {text}").dimmed());
                return Ok(());
            }

            // Handle virtual paths.
            if is_virtual_path(path_arg) {
                let Some((coll_name, file_path)) = parse_virtual_path(path_arg) else {
                    eprintln!("{} Invalid virtual path: {}", "Error:".red(), path_arg);
                    std::process::exit(1);
                };

                if get_collection(&coll_name)?.is_none() {
                    eprintln!("{} Collection not found: {}", "Error:".red(), coll_name);
                    std::process::exit(1);
                }

                add_context(&coll_name, &file_path, &text)?;
                let display = if file_path.is_empty() {
                    format!("qmd://{coll_name}/ (collection root)")
                } else {
                    format!("qmd://{coll_name}/{file_path}")
                };
                println!("{} Added context for: {}", "✓".green(), display);
                println!("{}", format!("Context: {text}").dimmed());
                return Ok(());
            }

            // Filesystem path - detect collection.
            let abs_path = fs::canonicalize(path_arg)?;
            let abs_path_str = abs_path.to_string_lossy().to_string();

            // Find matching collection.
            let collections = yaml_list_collections()?;
            let mut best_match: Option<(&str, String)> = None;

            for coll in &collections {
                if abs_path_str.starts_with(&format!("{}/", coll.path)) || abs_path_str == coll.path
                {
                    let rel_path = if abs_path_str.starts_with(&format!("{}/", coll.path)) {
                        abs_path_str[coll.path.len() + 1..].to_string()
                    } else {
                        String::new()
                    };

                    if best_match.is_none()
                        || coll.path.len() > best_match.as_ref().unwrap().0.len()
                    {
                        best_match = Some((&coll.name, rel_path));
                    }
                }
            }

            let Some((coll_name, rel_path)) = best_match else {
                eprintln!(
                    "{} Path is not in any indexed collection: {}",
                    "Error:".red(),
                    abs_path_str
                );
                std::process::exit(1);
            };

            add_context(coll_name, &rel_path, &text)?;
            let display = if rel_path.is_empty() {
                format!("qmd://{coll_name}/")
            } else {
                format!("qmd://{coll_name}/{rel_path}")
            };
            println!("{} Added context for: {}", "✓".green(), display);
            println!("{}", format!("Context: {text}").dimmed());
        }
        ContextCommands::List => {
            let contexts = list_all_contexts()?;

            if contexts.is_empty() {
                println!(
                    "{}",
                    "No contexts configured. Use 'qmd context add' to add one.".dimmed()
                );
                return Ok(());
            }

            println!("\n{}\n", "Configured Contexts".bold());
            let mut last_collection = String::new();

            for ctx in &contexts {
                if ctx.collection != last_collection {
                    println!("{}", ctx.collection.cyan());
                    last_collection.clone_from(&ctx.collection);
                }

                let path_display = if ctx.path.is_empty() || ctx.path == "/" {
                    "  / (root)".to_string()
                } else {
                    format!("  {}", ctx.path)
                };
                println!("{path_display}");
                println!("    {}", ctx.context.dimmed());
            }
        }
        ContextCommands::Check => {
            let store = Store::new()?;
            let collections = store.list_collections()?;
            let contexts = list_all_contexts()?;

            // Find collections without any context.
            let collections_with_context: HashSet<_> =
                contexts.iter().map(|c| c.collection.as_str()).collect();

            let mut missing = Vec::new();
            for coll in &collections {
                if !collections_with_context.contains(coll.name.as_str()) && coll.name != "*" {
                    missing.push(&coll.name);
                }
            }

            if missing.is_empty() {
                println!(
                    "\n{} {}\n",
                    "✓".green(),
                    "All collections have context configured".bold()
                );
            } else {
                println!("\n{}\n", "Collections without any context:".yellow());
                for name in missing {
                    println!("{}", name.cyan());
                    println!(
                        "  {}",
                        format!(
                            "Suggestion: qmd context add qmd://{name}/ \"Description of {name}\""
                        )
                        .dimmed()
                    );
                }
            }
        }
        ContextCommands::Rm { path } => {
            if path == "/" {
                set_global_context(None)?;
                println!("{} Removed global context", "✓".green());
                return Ok(());
            }

            if is_virtual_path(&path) {
                let Some((coll_name, file_path)) = parse_virtual_path(&path) else {
                    eprintln!("{} Invalid virtual path: {}", "Error:".red(), path);
                    std::process::exit(1);
                };

                if !remove_context(&coll_name, &file_path)? {
                    eprintln!("{} No context found for: {}", "Error:".red(), path);
                    std::process::exit(1);
                }

                println!("{} Removed context for: {}", "✓".green(), path);
            } else {
                eprintln!(
                    "{} Use virtual path format (qmd://collection/path)",
                    "Error:".red()
                );
                std::process::exit(1);
            }
        }
    }
    Ok(())
}

/// Handle ls command.
fn handle_ls(path: Option<String>) -> Result<()> {
    let store = Store::new()?;

    let Some(path_arg) = path else {
        // List all collections.
        let collections = yaml_list_collections()?;

        if collections.is_empty() {
            println!("No collections found. Run 'qmd collection add .' to index files.");
            return Ok(());
        }

        println!("{}\n", "Collections:".bold());
        for coll in collections {
            // Get file count from database.
            let files = store.list_files(&coll.name, None)?;
            println!(
                "  {}{}{}  {}",
                "qmd://".dimmed(),
                coll.name.cyan(),
                "/".dimmed(),
                format!("({} files)", files.len()).dimmed()
            );
        }
        return Ok(());
    };

    // Parse path argument.
    let (coll_name, path_prefix) = if is_virtual_path(&path_arg) {
        parse_virtual_path(&path_arg).unwrap_or_else(|| {
            eprintln!("{} Invalid virtual path: {}", "Error:".red(), path_arg);
            std::process::exit(1);
        })
    } else {
        // Assume collection name or collection/path format.
        let parts: Vec<&str> = path_arg.splitn(2, '/').collect();
        (
            parts[0].to_string(),
            parts.get(1).map(ToString::to_string).unwrap_or_default(),
        )
    };

    // Check collection exists.
    if get_collection(&coll_name)?.is_none() {
        eprintln!("{} Collection not found: {}", "Error:".red(), coll_name);
        eprintln!("Run 'qmd ls' to see available collections.");
        std::process::exit(1);
    }

    let prefix = if path_prefix.is_empty() {
        None
    } else {
        Some(path_prefix.as_str())
    };

    let files = store.list_files(&coll_name, prefix)?;

    if files.is_empty() {
        if prefix.is_some() {
            println!("No files found under qmd://{coll_name}/{path_prefix}");
        } else {
            println!("No files found in collection: {coll_name}");
        }
        return Ok(());
    }

    // Calculate max width for size alignment.
    let max_size = files
        .iter()
        .map(|(_, _, _, size)| format_bytes(*size).len())
        .max()
        .unwrap_or(0);

    for (file_path, _title, modified_at, size) in files {
        let size_str = format!("{:>width$}", format_bytes(size), width = max_size);
        let time_str = format_ls_time(&modified_at);
        println!(
            "{}  {}  {}{}",
            size_str,
            time_str,
            format!("qmd://{coll_name}/").dimmed(),
            file_path.cyan()
        );
    }

    Ok(())
}

/// Handle get command.
fn handle_get(
    file: &str,
    from_line: Option<usize>,
    max_lines: Option<usize>,
    line_numbers: bool,
) -> Result<()> {
    let store = Store::new()?;

    // Parse :linenum suffix.
    let (input_path, parsed_from_line) = if let Some(pos) = file.rfind(':') {
        let suffix = &file[pos + 1..];
        if let Ok(line) = suffix.parse::<usize>() {
            (&file[..pos], Some(line))
        } else {
            (file, None)
        }
    } else {
        (file, None)
    };

    let from_line = from_line.or(parsed_from_line);

    // Resolve document.
    let (collection, path) = if is_docid(input_path) {
        store
            .find_document_by_docid(input_path)?
            .ok_or_else(|| anyhow::anyhow!("Document not found: {input_path}"))?
    } else if is_virtual_path(input_path) {
        parse_virtual_path(input_path)
            .ok_or_else(|| anyhow::anyhow!("Invalid virtual path: {input_path}"))?
    } else {
        // Try as collection/path format.
        let parts: Vec<&str> = input_path.splitn(2, '/').collect();
        if parts.len() == 2 {
            (parts[0].to_string(), parts[1].to_string())
        } else {
            return Err(anyhow::anyhow!(
                "Could not resolve path: {input_path}. Use qmd://collection/path format."
            ));
        }
    };

    let doc = store
        .get_document(&collection, &path)?
        .ok_or_else(|| anyhow::anyhow!("Document not found: qmd://{collection}/{path}"))?;

    let mut body = doc.body.unwrap_or_default();
    let start_line = from_line.unwrap_or(1);

    // Apply line filtering.
    if from_line.is_some() || max_lines.is_some() {
        let lines: Vec<&str> = body.lines().collect();
        let start = start_line.saturating_sub(1);
        let end = max_lines.map_or(lines.len(), |n| (start + n).min(lines.len()));
        body = lines[start..end].join("\n");
    }

    // Add line numbers.
    if line_numbers {
        body = add_line_numbers(&body, start_line);
    }

    // Output context if exists.
    if let Some(ref ctx) = doc.context {
        println!("Folder Context: {ctx}\n---\n");
    }

    println!("{body}");
    Ok(())
}

/// Handle multi-get command.
fn handle_multi_get(
    pattern: &str,
    max_lines: Option<usize>,
    max_bytes: usize,
    format: &OutputFormat,
) -> Result<()> {
    let store = Store::new()?;

    // Parse pattern - comma-separated list or glob.
    let is_comma_list = pattern.contains(',') && !pattern.contains('*') && !pattern.contains('?');

    let mut results: Vec<(qmd::store::DocumentResult, bool, Option<String>)> = Vec::new();

    if is_comma_list {
        // Handle comma-separated list of files
        for name in pattern.split(',').map(str::trim).filter(|s| !s.is_empty()) {
            let (collection, path) = if is_virtual_path(name) {
                if let Some(p) = parse_virtual_path(name) {
                    p
                } else {
                    eprintln!("Invalid path: {name}");
                    continue;
                }
            } else {
                let parts: Vec<&str> = name.splitn(2, '/').collect();
                if parts.len() == 2 {
                    (parts[0].to_string(), parts[1].to_string())
                } else {
                    eprintln!("Invalid path format: {name}");
                    continue;
                }
            };

            match store.get_document(&collection, &path)? {
                Some(mut doc) => {
                    if doc.body_length > max_bytes {
                        let reason = format!(
                            "File too large ({}KB > {}KB)",
                            doc.body_length / 1024,
                            max_bytes / 1024
                        );
                        doc.body = None;
                        results.push((doc, true, Some(reason)));
                    } else {
                        // Apply line limit.
                        if let Some(limit) = max_lines
                            && let Some(ref mut body) = doc.body
                        {
                            let lines: Vec<&str> = body.lines().take(limit).collect();
                            *body = lines.join("\n");
                        }
                        results.push((doc, false, None));
                    }
                }
                None => {
                    eprintln!("File not found: {name}");
                }
            }
        }
    } else {
        // Glob pattern matching
        let matched_docs = match_files_by_glob(&store, pattern)?;

        if matched_docs.is_empty() {
            eprintln!("No files matched pattern: {pattern}");
            std::process::exit(1);
        }

        for mut doc in matched_docs {
            if doc.body_length > max_bytes {
                let reason = format!(
                    "File too large ({}KB > {}KB). Use 'qmd get {}' to retrieve.",
                    doc.body_length / 1024,
                    max_bytes / 1024,
                    doc.display_path
                );
                doc.body = None;
                results.push((doc, true, Some(reason)));
            } else {
                // Fetch full document body
                if let Ok(Some(mut full_doc)) = store.get_document(&doc.collection_name, &doc.path)
                {
                    // Apply line limit if specified
                    if let Some(limit) = max_lines {
                        if let Some(ref body) = full_doc.body {
                            let lines: Vec<&str> = body.lines().take(limit).collect();
                            full_doc.body = Some(lines.join("\n"));
                        }
                    }
                    results.push((full_doc, false, None));
                }
            }
        }
    }

    format_documents(&results, format);
    Ok(())
}

/// Handle status command.
fn handle_status() -> Result<()> {
    let store = Store::new()?;
    let db_path = store.db_path().to_string_lossy().to_string();

    // Get database size.
    let index_size = fs::metadata(store.db_path()).map_or(0, |m| m.len() as usize);

    let status = store.get_status()?;
    let contexts = list_all_contexts()?;

    println!("{}\n", "QMD Status".bold());
    println!("Index: {db_path}");
    println!("Size:  {}\n", format_bytes(index_size));

    println!("{}", "Documents".bold());
    println!("  Total:    {} files indexed", status.total_documents);
    if status.needs_embedding > 0 {
        println!(
            "  {} {} (run 'qmd embed')",
            "Pending:".yellow(),
            format!("{} need embedding", status.needs_embedding)
        );
    }

    if status.collections.is_empty() {
        println!(
            "\n{}",
            "No collections. Run 'qmd collection add .' to index markdown files.".dimmed()
        );
    } else {
        println!("\n{}", "Collections".bold());

        for coll in &status.collections {
            let time_ago = coll
                .last_modified
                .as_ref()
                .map_or_else(|| "never".to_string(), |t| format_time_ago(t));

            // Get contexts for this collection.
            let coll_contexts: Vec<_> = contexts
                .iter()
                .filter(|c| c.collection == coll.name)
                .collect();

            println!(
                "  {} {}",
                coll.name.cyan(),
                format!("(qmd://{}/)", coll.name).dimmed()
            );
            println!("    {} {}", "Pattern:".dimmed(), coll.glob_pattern);
            println!(
                "    {} {} (updated {})",
                "Files:".dimmed(),
                coll.active_count,
                time_ago
            );

            if !coll_contexts.is_empty() {
                println!("    {} {}", "Contexts:".dimmed(), coll_contexts.len());
                for ctx in coll_contexts {
                    let path_display = if ctx.path.is_empty() || ctx.path == "/" {
                        "/".to_string()
                    } else {
                        format!("/{}", ctx.path)
                    };
                    let preview = if ctx.context.len() > 60 {
                        format!("{}...", &ctx.context[..57])
                    } else {
                        ctx.context.clone()
                    };
                    println!("      {} {}", format!("{path_display}:").dimmed(), preview);
                }
            }
        }
    }

    Ok(())
}

/// Handle update command.
fn handle_update(pull: bool) -> Result<()> {
    let store = Store::new()?;
    store.clear_cache()?;

    let collections = store.list_collections()?;

    if collections.is_empty() {
        println!(
            "{}",
            "No collections found. Run 'qmd collection add .' to index markdown files.".dimmed()
        );
        return Ok(());
    }

    // Load YAML config to get update commands
    let yaml_collections = yaml_list_collections().unwrap_or_default();

    println!(
        "{}\n",
        format!("Updating {} collection(s)...", collections.len()).bold()
    );

    for (i, coll) in collections.iter().enumerate() {
        println!(
            "{} {} {}",
            format!("[{}/{}]", i + 1, collections.len()).cyan(),
            coll.name.bold(),
            format!("({})", coll.glob_pattern).dimmed()
        );

        // Check for custom update command in YAML config
        if let Some(yaml_coll) = yaml_collections.iter().find(|c| c.name == coll.name) {
            if let Some(ref update_cmd) = yaml_coll.update {
                println!("    Running update command: {}", update_cmd.dimmed());
                let output = if cfg!(target_os = "windows") {
                    std::process::Command::new("cmd")
                        .args(["/C", update_cmd])
                        .current_dir(&coll.pwd)
                        .output()
                } else {
                    std::process::Command::new("sh")
                        .args(["-c", update_cmd])
                        .current_dir(&coll.pwd)
                        .output()
                };

                match output {
                    Ok(o) if o.status.success() => {
                        let stdout = String::from_utf8_lossy(&o.stdout);
                        if !stdout.trim().is_empty() {
                            for line in stdout.lines().take(10) {
                                println!("    {line}");
                            }
                        }
                    }
                    Ok(o) => {
                        eprintln!("    {} update command failed", "Warning:".yellow());
                        let stderr = String::from_utf8_lossy(&o.stderr);
                        for line in stderr.lines().take(5) {
                            eprintln!("    {line}");
                        }
                    }
                    Err(e) => {
                        eprintln!(
                            "    {} Could not run update command: {}",
                            "Warning:".yellow(),
                            e
                        );
                    }
                }
            }
        }

        // Git pull if requested.
        if pull {
            let git_dir = Path::new(&coll.pwd).join(".git");
            if git_dir.exists() {
                println!("    Running git pull...");
                let output = std::process::Command::new("git")
                    .arg("pull")
                    .current_dir(&coll.pwd)
                    .output();

                match output {
                    Ok(o) if o.status.success() => {
                        let stdout = String::from_utf8_lossy(&o.stdout);
                        if !stdout.trim().is_empty() {
                            for line in stdout.lines() {
                                println!("    {line}");
                            }
                        }
                    }
                    Ok(o) => {
                        eprintln!("    {} git pull failed", "Warning:".yellow());
                        eprintln!("    {}", String::from_utf8_lossy(&o.stderr));
                    }
                    Err(e) => {
                        eprintln!("    {} Could not run git pull: {}", "Warning:".yellow(), e);
                    }
                }
            }
        }

        index_files(&coll.pwd, &coll.glob_pattern, &coll.name)?;
        println!();
    }

    println!("{} All collections updated.", "✓".green());
    Ok(())
}

/// Handle search command.
fn handle_search(
    query: &str,
    collection: Option<&str>,
    limit: usize,
    min_score: Option<f64>,
    full: bool,
    format: &OutputFormat,
) -> Result<()> {
    let store = Store::new()?;

    let mut results = store.search_fts(query, limit, collection)?;

    // Apply minimum score filter.
    if let Some(min) = min_score {
        results.retain(|r| r.score >= min);
    }

    // Load full body if requested.
    if full {
        for result in &mut results {
            if result.doc.body.is_none()
                && let Ok(Some(doc)) =
                    store.get_document(&result.doc.collection_name, &result.doc.path)
            {
                result.doc.body = doc.body;
            }
        }
    }

    format_search_results(&results, format, full);
    Ok(())
}

/// Handle vector search command.
fn handle_vsearch(
    query: &str,
    collection: Option<&str>,
    limit: usize,
    min_score: Option<f64>,
    full: bool,
    format: &OutputFormat,
    model_path: Option<&str>,
) -> Result<()> {
    use qmd::llm::EmbeddingEngine;
    use std::path::PathBuf;

    let store = Store::new()?;

    // Check index health and warn if needed
    store.check_and_warn_health();

    // Load embedding model
    let mut engine = if let Some(path) = model_path {
        EmbeddingEngine::new(&PathBuf::from(path))?
    } else if let Ok(e) = EmbeddingEngine::load_default() {
        e
    } else {
        eprintln!(
            "{} Embedding model not found. Please specify --model or download a model.",
            "Error:".red()
        );
        eprintln!(
            "Place a GGUF embedding model in: {}",
            qmd::config::get_model_cache_dir().display()
        );
        std::process::exit(1);
    };

    // Generate query embedding
    println!("Generating query embedding...");
    let query_result = engine.embed_query(query)?;

    // Search
    let mut results = store.search_vec(&query_result.embedding, limit, collection)?;

    // Apply minimum score filter
    if let Some(min) = min_score {
        results.retain(|r| r.score >= min);
    }

    if results.is_empty() {
        println!("No results found. Run 'qmd embed' to generate embeddings first.");
        return Ok(());
    }

    // Load full body if requested
    if full {
        for result in &mut results {
            if result.doc.body.is_none()
                && let Ok(Some(doc)) =
                    store.get_document(&result.doc.collection_name, &result.doc.path)
            {
                result.doc.body = doc.body;
            }
        }
    }

    format_search_results(&results, format, full);
    Ok(())
}

/// Handle embed command with improved progress display.
fn handle_embed(force: bool, model_path: Option<&str>) -> Result<()> {
    use qmd::llm::{
        CHUNK_OVERLAP_TOKENS, CHUNK_SIZE_TOKENS, Cursor, EmbeddingEngine, Progress,
        chunk_document_by_tokens, format_doc_for_embedding, format_eta, render_progress_bar,
    };
    use std::io::Write;
    use std::path::PathBuf;
    use std::time::Instant;

    let store = Store::new()?;

    // Clear existing embeddings if force
    if force {
        let cleared = store.clear_embeddings()?;
        println!("Cleared {cleared} existing embeddings");
    }

    // Get documents needing embedding
    let pending = store.get_hashes_needing_embedding()?;

    if pending.is_empty() {
        println!("{} All documents already have embeddings.", "✓".green());
        return Ok(());
    }

    // Load embedding model
    let mut engine = if let Some(path) = model_path {
        EmbeddingEngine::new(&PathBuf::from(path))?
    } else if let Ok(e) = EmbeddingEngine::load_default() {
        e
    } else {
        eprintln!(
            "{} Embedding model not found. Please specify --model or download a model.",
            "Error:".red()
        );
        eprintln!(
            "Place a GGUF embedding model in: {}",
            qmd::config::get_model_cache_dir().display()
        );
        std::process::exit(1);
    };

    // Prepare chunks using token-based chunking
    eprintln!("Chunking {} documents by token count...", pending.len());

    #[allow(dead_code)]
    struct ChunkItem {
        hash: String,
        title: String,
        text: String,
        seq: usize,
        pos: usize,
        tokens: usize, // Kept for future logging/debugging
        bytes: usize,
        display_name: String,
    }

    let mut all_chunks: Vec<ChunkItem> = Vec::new();
    let mut multi_chunk_docs = 0usize;

    for (hash, path, content) in &pending {
        if content.is_empty() {
            continue;
        }

        let title = Store::extract_title(content);

        // Use token-based chunking for accuracy
        match chunk_document_by_tokens(&engine, content, CHUNK_SIZE_TOKENS, CHUNK_OVERLAP_TOKENS) {
            Ok(chunks) => {
                if chunks.len() > 1 {
                    multi_chunk_docs += 1;
                }
                for (seq, chunk) in chunks.into_iter().enumerate() {
                    all_chunks.push(ChunkItem {
                        hash: hash.clone(),
                        title: title.clone(),
                        text: chunk.text,
                        seq,
                        pos: chunk.pos,
                        tokens: chunk.tokens,
                        bytes: chunk.bytes,
                        display_name: path.clone(),
                    });
                }
            }
            Err(_) => {
                // Fallback: treat entire document as single chunk
                all_chunks.push(ChunkItem {
                    hash: hash.clone(),
                    title: title.clone(),
                    text: content.clone(),
                    seq: 0,
                    pos: 0,
                    tokens: content.len() / 4, // Estimate
                    bytes: content.len(),
                    display_name: path.clone(),
                });
            }
        }
    }

    if all_chunks.is_empty() {
        println!("{} No non-empty documents to embed.", "✓".green());
        return Ok(());
    }

    let total_bytes: usize = all_chunks.iter().map(|c| c.bytes).sum();
    let total_chunks = all_chunks.len();
    let total_docs = pending.len();

    println!(
        "{} {} {}",
        "Embedding".bold(),
        format!("{total_docs} documents").bold(),
        format!("({total_chunks} chunks, {})", format_bytes(total_bytes)).dimmed()
    );
    if multi_chunk_docs > 0 {
        println!(
            "{}",
            format!("{multi_chunk_docs} documents split into multiple chunks").dimmed()
        );
    }

    // Ensure vector table exists with first embedding
    let progress = Progress::new();
    progress.indeterminate();

    let first_chunk = &all_chunks[0];
    let first_text = format_doc_for_embedding(&first_chunk.text, Some(&first_chunk.title));
    let first_result = engine.embed(&first_text)?;
    let dims = first_result.embedding.len();
    store.ensure_vector_table(dims)?;

    // Hide cursor during embedding
    Cursor::hide();

    let now = chrono::Utc::now().to_rfc3339();
    let start_time = Instant::now();
    let mut chunks_embedded = 0usize;
    let mut errors = 0usize;
    let mut bytes_processed = 0usize;

    // Insert first chunk result
    store.insert_embedding(
        &first_chunk.hash,
        first_chunk.seq,
        first_chunk.pos,
        &first_result.embedding,
        &first_result.model,
        &now,
    )?;
    chunks_embedded += 1;
    bytes_processed += first_chunk.bytes;

    // Process remaining chunks
    for chunk in all_chunks.iter().skip(1) {
        let formatted = format_doc_for_embedding(&chunk.text, Some(&chunk.title));

        match engine.embed(&formatted) {
            Ok(result) => {
                store.insert_embedding(
                    &chunk.hash,
                    chunk.seq,
                    chunk.pos,
                    &result.embedding,
                    &result.model,
                    &now,
                )?;
                chunks_embedded += 1;
            }
            Err(e) => {
                errors += 1;
                eprintln!(
                    "\n{} Error embedding \"{}\" chunk {}: {}",
                    "⚠".yellow(),
                    chunk.display_name,
                    chunk.seq,
                    e
                );
            }
        }
        bytes_processed += chunk.bytes;

        // Update progress
        let percent = (bytes_processed as f64 / total_bytes as f64) * 100.0;
        progress.set(percent);

        let elapsed = start_time.elapsed().as_secs_f64();
        let bytes_per_sec = bytes_processed as f64 / elapsed;
        let remaining_bytes = total_bytes.saturating_sub(bytes_processed);
        let eta_sec = remaining_bytes as f64 / bytes_per_sec;

        let bar = render_progress_bar(percent, 20);
        let percent_str = format!("{:3.0}%", percent);
        let throughput = format!("{}/s", format_bytes(bytes_per_sec as usize));
        let eta = if elapsed > 2.0 {
            format_eta(eta_sec)
        } else {
            "...".to_string()
        };
        let err_str = if errors > 0 {
            format!(" {} err", errors).yellow().to_string()
        } else {
            String::new()
        };

        eprint!(
            "\r{} {} {}/{}{} {} ETA {}   ",
            bar.cyan(),
            percent_str.bold(),
            chunks_embedded,
            total_chunks,
            err_str,
            throughput.dimmed(),
            eta.dimmed()
        );
        std::io::stderr().flush().ok();
    }

    progress.clear();
    Cursor::show();

    let total_time_sec = start_time.elapsed().as_secs_f64();
    let avg_throughput = format_bytes((total_bytes as f64 / total_time_sec) as usize);

    println!(
        "\r{} {}                                    ",
        render_progress_bar(100.0, 20).green(),
        "100%".bold()
    );
    println!(
        "\n{} Embedded {} chunks from {} documents in {} ({})",
        "✓".green(),
        chunks_embedded.to_string().bold(),
        total_docs.to_string().bold(),
        format_eta(total_time_sec).bold(),
        format!("{avg_throughput}/s").dimmed()
    );
    if errors > 0 {
        println!("{} {} chunks failed", "⚠".yellow(), errors);
    }

    Ok(())
}

/// Handle models subcommand.
fn handle_models(cmd: ModelCommands) -> Result<()> {
    use qmd::llm::{DEFAULT_EMBED_MODEL, list_cached_models};

    match cmd {
        ModelCommands::List => {
            let models = list_cached_models();
            let cache_dir = qmd::config::get_model_cache_dir();

            println!("{}\n", "Available Models".bold());
            println!("Cache directory: {}\n", cache_dir.display());

            if models.is_empty() {
                println!("No models found in cache.");
                println!(
                    "\n{}",
                    "To use vector search, download a GGUF embedding model:".dimmed()
                );
                println!("  1. Download a model (e.g., embeddinggemma-300M-Q8_0.gguf)");
                println!("  2. Place it in: {}", cache_dir.display());
            } else {
                println!("{}", "Cached models:".cyan());
                for model in &models {
                    let is_default = model == DEFAULT_EMBED_MODEL;
                    if is_default {
                        println!("  {} {}", model, "(default)".green());
                    } else {
                        println!("  {model}");
                    }
                }
            }
        }
        ModelCommands::Info { name } => {
            let model_name = name.as_deref().unwrap_or(DEFAULT_EMBED_MODEL);
            let cache_dir = qmd::config::get_model_cache_dir();
            let model_path = cache_dir.join(model_name);

            println!("{}\n", "Model Info".bold());
            println!("Name: {model_name}");
            println!("Path: {}", model_path.display());

            if model_path.exists() {
                let size = fs::metadata(&model_path).map_or_else(
                    |_| "unknown".to_string(),
                    |m| format_bytes(m.len() as usize),
                );
                println!("Status: {} ({})", "Downloaded".green(), size);
            } else {
                println!("Status: {}", "Not downloaded".red());
            }
        }
        ModelCommands::Pull { model, refresh } => {
            use qmd::llm::{
                DEFAULT_EMBED_MODEL_URI, DEFAULT_RERANK_MODEL_URI, pull_model, pull_models,
            };

            println!("{}\n", "Pulling Models".bold());

            let results = if model == "all" {
                // Pull default models
                let default_models = [DEFAULT_EMBED_MODEL_URI, DEFAULT_RERANK_MODEL_URI];
                println!("Downloading {} default models...\n", default_models.len());
                pull_models(&default_models, refresh)?
            } else {
                // Pull single model
                vec![pull_model(&model, refresh)?]
            };

            println!();
            for result in &results {
                let status = if result.refreshed {
                    "Downloaded".green()
                } else {
                    "Cached".cyan()
                };
                println!(
                    "{} {} ({})",
                    status,
                    result
                        .path
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy(),
                    format_bytes(result.size_bytes as usize)
                );
            }

            println!("\n{} {} model(s) ready", "✓".green(), results.len());
        }
    }

    Ok(())
}

/// Handle database maintenance commands.
fn handle_db(cmd: DbCommands) -> Result<()> {
    let store = Store::new()?;

    match cmd {
        DbCommands::Cleanup => {
            let inactive = store.delete_inactive_documents()?;
            let orphaned_content = store.cleanup_orphaned_content()?;
            let orphaned_vectors = store.cleanup_orphaned_vectors()?;

            println!("{} Database cleanup complete", "✓".green());
            println!("  Removed {inactive} inactive documents");
            println!("  Removed {orphaned_content} orphaned content entries");
            println!("  Removed {orphaned_vectors} orphaned vector entries");
        }
        DbCommands::Vacuum => {
            println!("Vacuuming database...");
            store.vacuum()?;
            println!("{} Database vacuumed", "✓".green());
        }
        DbCommands::ClearCache => {
            let cleared = store.clear_cache()?;
            println!("{} Cleared {} cached entries", "✓".green(), cleared);
        }
    }

    Ok(())
}

/// Handle qsearch (hybrid search with query expansion and reranking).
fn handle_qsearch(
    query: &str,
    collection: Option<&str>,
    limit: usize,
    full: bool,
    no_expand: bool,
    no_rerank: bool,
    format: &OutputFormat,
) -> Result<()> {
    use qmd::llm::{EmbeddingEngine, GenerationEngine, RerankDocument, RerankEngine};

    let store = Store::new()?;

    // Check index health and warn if needed
    store.check_and_warn_health();

    // Step 1: Query expansion (optional)
    let queries = if no_expand || !GenerationEngine::is_available() {
        vec![qmd::Queryable::lex(query), qmd::Queryable::vec(query)]
    } else {
        println!("Expanding query...");
        match GenerationEngine::load_default() {
            Ok(engine) => match engine.expand_query(query, true) {
                Ok(q) => q,
                Err(_) => qmd::expand_query_simple(query),
            },
            Err(_) => qmd::expand_query_simple(query),
        }
    };

    // Step 2: Run searches based on query types
    let mut fts_results: Vec<(String, String, String, String)> = Vec::new();
    let mut vec_results: Vec<(String, String, String, String)> = Vec::new();

    for q in &queries {
        match q.query_type {
            qmd::QueryType::Lex => {
                if let Ok(results) = store.search_fts(&q.text, limit * 2, collection) {
                    for r in results {
                        let body = r.doc.body.clone().unwrap_or_default();
                        fts_results.push((
                            r.doc.filepath.clone(),
                            r.doc.display_path.clone(),
                            r.doc.title.clone(),
                            body,
                        ));
                    }
                }
            }
            qmd::QueryType::Vec | qmd::QueryType::Hyde => {
                if let Ok(mut engine) = EmbeddingEngine::load_default() {
                    if let Ok(query_result) = engine.embed_query(&q.text) {
                        if let Ok(results) =
                            store.search_vec(&query_result.embedding, limit * 2, collection)
                        {
                            for r in results {
                                let body = store
                                    .get_document(&r.doc.collection_name, &r.doc.path)
                                    .ok()
                                    .flatten()
                                    .and_then(|d| d.body)
                                    .unwrap_or_default();
                                vec_results.push((
                                    r.doc.filepath.clone(),
                                    r.doc.display_path.clone(),
                                    r.doc.title.clone(),
                                    body,
                                ));
                            }
                        }
                    }
                }
            }
        }
    }

    // Step 3: RRF fusion
    let mut rrf_results = qmd::hybrid_search_rrf(fts_results, vec_results, 60);

    // Step 4: Rerank (optional)
    if !no_rerank && RerankEngine::is_available() && !rrf_results.is_empty() {
        println!("Reranking {} results...", rrf_results.len().min(limit * 2));
        if let Ok(mut reranker) = RerankEngine::load_default() {
            let docs: Vec<RerankDocument> = rrf_results
                .iter()
                .take(limit * 2)
                .map(|r| RerankDocument {
                    file: r.file.clone(),
                    text: r.body.clone(),
                    title: Some(r.title.clone()),
                })
                .collect();

            if let Ok(reranked) = reranker.rerank(query, &docs) {
                // Reorder based on rerank scores
                let mut reordered = Vec::new();
                for rr in reranked.results {
                    if let Some(orig) = rrf_results.iter().find(|r| r.file == rr.file) {
                        reordered.push(orig.clone());
                    }
                }
                rrf_results = reordered;
            }
        }
    }

    // Step 5: Format and output
    rrf_results.truncate(limit);

    if rrf_results.is_empty() {
        println!("{}", "No results found.".dimmed());
        return Ok(());
    }

    // Convert to SearchResult for formatting
    let search_results: Vec<qmd::store::SearchResult> = rrf_results
        .iter()
        .map(|r| {
            let parts: Vec<&str> = r
                .file
                .strip_prefix("qmd://")
                .unwrap_or(&r.file)
                .splitn(2, '/')
                .collect();
            let (collection_name, path) = if parts.len() == 2 {
                (parts[0].to_string(), parts[1].to_string())
            } else {
                (String::new(), r.file.clone())
            };

            qmd::store::SearchResult {
                doc: qmd::store::DocumentResult {
                    filepath: r.file.clone(),
                    display_path: r.display_path.clone(),
                    title: r.title.clone(),
                    context: None,
                    hash: String::new(),
                    docid: String::new(),
                    collection_name,
                    path,
                    modified_at: String::new(),
                    body_length: r.body.len(),
                    body: if full { Some(r.body.clone()) } else { None },
                },
                score: r.score,
                source: qmd::store::SearchSource::Fts,
                chunk_pos: None,
            }
        })
        .collect();

    format_search_results(&search_results, format, full);
    Ok(())
}

/// Handle expand command.
fn handle_expand(query: &str, include_lexical: bool) -> Result<()> {
    use qmd::llm::GenerationEngine;

    println!("{}\n", "Query Expansion".bold());
    println!("Original: {query}\n");

    let queries = if GenerationEngine::is_available() {
        match GenerationEngine::load_default() {
            Ok(engine) => match engine.expand_query(query, include_lexical) {
                Ok(q) => q,
                Err(e) => {
                    eprintln!("{} LLM expansion failed: {}", "Warning:".yellow(), e);
                    qmd::expand_query_simple(query)
                }
            },
            Err(e) => {
                eprintln!("{} Could not load model: {}", "Warning:".yellow(), e);
                qmd::expand_query_simple(query)
            }
        }
    } else {
        println!(
            "{}",
            "Generation model not available, using simple expansion.".dimmed()
        );
        qmd::expand_query_simple(query)
    };

    println!("{}", "Expanded queries:".cyan());
    for q in &queries {
        let type_str = match q.query_type {
            qmd::QueryType::Lex => "lex".green(),
            qmd::QueryType::Vec => "vec".blue(),
            qmd::QueryType::Hyde => "hyde".magenta(),
        };
        println!("  {}: {}", type_str, q.text);
    }

    Ok(())
}

/// Handle rerank command.
fn handle_rerank(query: &str, files: &str, limit: usize, format: &OutputFormat) -> Result<()> {
    use qmd::llm::{RerankDocument, RerankEngine};

    let store = Store::new()?;

    // Parse file list
    let file_list: Vec<&str> = files
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();

    if file_list.is_empty() {
        eprintln!("{} No files specified", "Error:".red());
        std::process::exit(1);
    }

    // Load documents
    let mut docs: Vec<RerankDocument> = Vec::new();
    for file in &file_list {
        let (collection, path) = if is_virtual_path(file) {
            parse_virtual_path(file).unwrap_or_else(|| {
                eprintln!("{} Invalid path: {}", "Warning:".yellow(), file);
                (String::new(), file.to_string())
            })
        } else {
            let parts: Vec<&str> = file.splitn(2, '/').collect();
            if parts.len() == 2 {
                (parts[0].to_string(), parts[1].to_string())
            } else {
                continue;
            }
        };

        if let Ok(Some(doc)) = store.get_document(&collection, &path) {
            docs.push(RerankDocument {
                file: doc.filepath.clone(),
                text: doc.body.unwrap_or_default(),
                title: Some(doc.title),
            });
        }
    }

    if docs.is_empty() {
        eprintln!("{} No valid documents found", "Error:".red());
        std::process::exit(1);
    }

    println!("Reranking {} documents...", docs.len());

    let mut engine = RerankEngine::load_default().map_err(|e| {
        eprintln!("{} Could not load rerank model: {}", "Error:".red(), e);
        eprintln!("Run 'qmd models pull' to download required models.");
        std::process::exit(1);
    })?;

    let result = engine.rerank(query, &docs)?;

    // Output results
    match format {
        OutputFormat::Json => {
            let output: Vec<serde_json::Value> = result
                .results
                .iter()
                .take(limit)
                .map(|r| {
                    serde_json::json!({
                        "file": r.file,
                        "score": r.score,
                        "rank": r.index + 1,
                    })
                })
                .collect();
            println!("{}", serde_json::to_string_pretty(&output)?);
        }
        _ => {
            println!("\n{}", "Reranked Results:".bold());
            for (i, r) in result.results.iter().take(limit).enumerate() {
                println!(
                    "{}. {} {}",
                    (i + 1).to_string().cyan(),
                    format!("{:.4}", r.score).dimmed(),
                    r.file.bold()
                );
            }
        }
    }

    Ok(())
}

/// Handle ask command (question answering).
fn handle_ask(
    question: &str,
    collection: Option<&str>,
    limit: usize,
    max_tokens: usize,
) -> Result<()> {
    use qmd::llm::{EmbeddingEngine, GenerationEngine};

    let store = Store::new()?;

    println!("{}", "Searching for relevant documents...".dimmed());

    // Search for relevant documents using vector search
    let context_docs = if let Ok(mut engine) = EmbeddingEngine::load_default() {
        if let Ok(query_result) = engine.embed_query(question) {
            store
                .search_vec(&query_result.embedding, limit, collection)
                .unwrap_or_default()
        } else {
            // Fallback to FTS
            store
                .search_fts(question, limit, collection)
                .unwrap_or_default()
        }
    } else {
        store
            .search_fts(question, limit, collection)
            .unwrap_or_default()
    };

    if context_docs.is_empty() {
        println!("{}", "No relevant documents found.".yellow());
        return Ok(());
    }

    // Build context from documents
    let mut context = String::new();
    for (i, result) in context_docs.iter().enumerate() {
        let body = store
            .get_document(&result.doc.collection_name, &result.doc.path)
            .ok()
            .flatten()
            .and_then(|d| d.body)
            .unwrap_or_default();

        // Truncate each document to ~1000 chars
        let truncated: String = body.chars().take(1000).collect();
        context.push_str(&format!(
            "\n--- Document {} ({}): ---\n{}\n",
            i + 1,
            result.doc.display_path,
            truncated
        ));
    }

    println!(
        "Found {} relevant documents. Generating answer...\n",
        context_docs.len()
    );

    // Generate answer
    let gen_engine = GenerationEngine::load_default().map_err(|e| {
        eprintln!("{} Could not load generation model: {}", "Error:".red(), e);
        eprintln!("Run 'qmd models pull all' to download required models.");
        std::process::exit(1);
    })?;

    let prompt = format!(
        r#"Based on the following documents, answer the question concisely and accurately.

Documents:
{context}

Question: {question}

Answer:"#
    );

    let result = gen_engine.generate(&prompt, max_tokens)?;

    println!("{}\n", "Answer:".green().bold());
    println!("{}", result.text);

    println!("\n{}", "Sources:".dimmed());
    for result in &context_docs {
        println!("  - {}", result.doc.display_path);
    }

    Ok(())
}

/// Handle index command (switch index).
fn handle_index(name: Option<&str>) -> Result<()> {
    use qmd::collections::set_config_index_name;

    match name {
        Some(index_name) => {
            set_config_index_name(index_name);
            let db_path = qmd::config::get_default_db_path(index_name)
                .unwrap_or_else(|| std::path::PathBuf::from("unknown"));
            println!("{} Switched to index: {}", "✓".green(), index_name.cyan());
            println!("  Database: {}", db_path.display());
        }
        None => {
            // Show current index
            let default_path = qmd::config::get_default_db_path("index")
                .unwrap_or_else(|| std::path::PathBuf::from("unknown"));
            println!("{}", "Current Index".bold());
            println!("  Name: {}", "index".cyan());
            println!("  Path: {}", default_path.display());
            println!("\n{}", "Usage:".dimmed());
            println!("  qmd index <name>  - Switch to a different index");
        }
    }

    Ok(())
}

/// Index files in a directory.
fn index_files(pwd: &str, glob_pattern: &str, collection_name: &str) -> Result<()> {
    let store = Store::new()?;
    let now = chrono::Utc::now().to_rfc3339();

    // Collect matching files.
    let glob_matcher = glob::Pattern::new(glob_pattern)?;
    let mut files = Vec::new();

    for entry in WalkDir::new(pwd)
        .follow_links(true)
        .into_iter()
        .filter_map(std::result::Result::ok)
    {
        let path = entry.path();

        // Skip directories.
        if !path.is_file() {
            continue;
        }

        // Skip excluded paths.
        if should_exclude(path) {
            continue;
        }

        // Check glob match.
        let rel_path = path.strip_prefix(pwd).unwrap_or(path);
        let rel_path_str = rel_path.to_string_lossy();

        if glob_matcher.matches(&rel_path_str) {
            files.push((path.to_path_buf(), rel_path_str.to_string()));
        }
    }

    if files.is_empty() {
        println!("  No files found matching pattern.");
        return Ok(());
    }

    let mut indexed = 0;
    let mut updated = 0;
    let mut unchanged = 0;
    let mut seen_paths = HashSet::new();

    for (abs_path, rel_path) in &files {
        let normalized_path = Store::handelize(rel_path);
        seen_paths.insert(normalized_path.clone());

        // Read file content.
        let content = match fs::read_to_string(abs_path) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("  Warning: Could not read {rel_path}: {e}");
                continue;
            }
        };

        let hash = Store::hash_content(&content);
        let title = Store::extract_title(&content);

        // Check if document exists.
        if let Some((doc_id, existing_hash, existing_title)) =
            store.find_active_document(collection_name, &normalized_path)?
        {
            if existing_hash == hash {
                // Check if title changed.
                if existing_title != title {
                    store.update_document_title(doc_id, &title, &now)?;
                }
                unchanged += 1;
            } else {
                // Content changed - update.
                store.insert_content(&hash, &content, &now)?;
                store.update_document(doc_id, &title, &hash, &now)?;
                updated += 1;
            }
        } else {
            // New document.
            store.insert_content(&hash, &content, &now)?;
            store.insert_document(collection_name, &normalized_path, &title, &hash, &now, &now)?;
            indexed += 1;
        }
    }

    // Deactivate removed files.
    let existing_paths = store.get_active_document_paths(collection_name)?;
    let mut deactivated = 0;

    for path in existing_paths {
        if !seen_paths.contains(&path) {
            store.deactivate_document(collection_name, &path)?;
            deactivated += 1;
        }
    }

    println!(
        "  {indexed} indexed, {updated} updated, {unchanged} unchanged, {deactivated} removed"
    );

    Ok(())
}
