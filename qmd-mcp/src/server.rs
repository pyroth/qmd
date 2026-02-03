//! MCP Server implementation for QMD.
//!
//! Uses `spawn_blocking` to run synchronous rusqlite operations in a
//! dedicated thread pool, following the Rust community best practice.

use rmcp::{
    ServerHandler,
    handler::server::{tool::ToolRouter, wrapper::Parameters},
    model::{
        CallToolResult, Content, Implementation, InitializeResult, ProtocolVersion,
        ServerCapabilities,
    },
    schemars::JsonSchema,
    tool, tool_handler, tool_router,
};
use serde::{Deserialize, Serialize};

/// Type alias for ServerInfo (same as InitializeResult).
type ServerInfo = InitializeResult;

/// QMD MCP Server that provides search and document retrieval tools.
#[derive(Clone, Default, Debug)]
pub struct QmdMcpServer {
    /// Tool router for handling tool calls.
    tool_router: ToolRouter<Self>,
}

impl QmdMcpServer {
    /// Create a new QMD MCP server instance.
    #[must_use]
    pub fn new() -> Self {
        Self {
            tool_router: Self::tool_router(),
        }
    }
}

/// Parameters for search tool.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct SearchParams {
    /// Search query - keywords or phrases to find.
    pub query: String,
    /// Maximum number of results (default: 10).
    #[serde(default = "default_limit")]
    pub limit: usize,
    /// Minimum relevance score 0-1 (default: 0).
    #[serde(default)]
    pub min_score: f64,
    /// Filter to a specific collection by name.
    pub collection: Option<String>,
}

/// Parameters for get tool.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct GetParams {
    /// File path or docid from search results (e.g., 'notes/meeting.md', '#abc123').
    pub file: String,
    /// Start from this line number (1-indexed).
    pub from_line: Option<usize>,
    /// Maximum number of lines to return.
    pub max_lines: Option<usize>,
    /// Add line numbers to output (default: true).
    #[serde(default = "default_true")]
    pub line_numbers: bool,
}

fn default_limit() -> usize {
    10
}
fn default_true() -> bool {
    true
}

/// Search result item for JSON output.
#[derive(Debug, Serialize)]
struct SearchResultItem {
    docid: String,
    file: String,
    title: String,
    score: f64,
    context: Option<String>,
}

/// Status result for JSON output.
#[derive(Debug, Serialize)]
struct StatusResult {
    total_documents: usize,
    needs_embedding: usize,
    has_vector_index: bool,
    collections: Vec<CollectionStatus>,
}

/// Collection status for JSON output.
#[derive(Debug, Serialize)]
struct CollectionStatus {
    name: String,
    path: String,
    documents: usize,
}

/// Convert qmd error to MCP error.
fn to_mcp_error(e: impl std::fmt::Display) -> rmcp::ErrorData {
    rmcp::ErrorData::internal_error(e.to_string(), None)
}

/// Add line numbers to text.
fn add_line_numbers(text: &str, start: usize) -> String {
    text.lines()
        .enumerate()
        .map(|(i, line)| format!("{}: {}", start + i, line))
        .collect::<Vec<_>>()
        .join("\n")
}

#[tool_router]
impl QmdMcpServer {
    /// Fast keyword-based full-text search using BM25.
    /// Best for finding documents with specific words or phrases.
    #[tool(name = "search")]
    async fn search(
        &self,
        params: Parameters<SearchParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let p = params.0;

        // Run synchronous database operation in blocking thread pool
        let result =
            tokio::task::spawn_blocking(move || -> Result<Vec<SearchResultItem>, qmd::QmdError> {
                let store = qmd::Store::new()?;
                let results = store.search_fts(&p.query, p.limit, p.collection.as_deref())?;

                Ok(results
                    .into_iter()
                    .filter(|r| r.score >= p.min_score)
                    .map(|r| SearchResultItem {
                        docid: format!("#{}", r.doc.docid),
                        file: r.doc.display_path,
                        title: r.doc.title,
                        score: (r.score * 100.0).round() / 100.0,
                        context: r.doc.context,
                    })
                    .collect())
            })
            .await
            .map_err(|e| to_mcp_error(e))?
            .map_err(to_mcp_error)?;

        let summary = if result.is_empty() {
            "No results found".to_string()
        } else {
            result
                .iter()
                .map(|r| {
                    format!(
                        "{} {}% {} - {}",
                        r.docid,
                        (r.score * 100.0) as i32,
                        r.file,
                        r.title
                    )
                })
                .collect::<Vec<_>>()
                .join("\n")
        };

        Ok(CallToolResult::success(vec![Content::text(summary)]))
    }

    /// Retrieve the full content of a document by its file path or docid (#abc123).
    #[tool(name = "get")]
    async fn get(&self, params: Parameters<GetParams>) -> Result<CallToolResult, rmcp::ErrorData> {
        let p = params.0;
        let file_for_err = p.file.clone();

        let result = tokio::task::spawn_blocking(
            move || -> Result<Option<(String, String, Option<String>)>, qmd::QmdError> {
                let store = qmd::Store::new()?;

                // Check if it's a docid
                let (collection, path) = if p.file.starts_with('#') {
                    match store.find_document_by_docid(&p.file)? {
                        Some(cp) => cp,
                        None => return Ok(None),
                    }
                } else {
                    // Parse collection/path format
                    let parts: Vec<&str> = p.file.splitn(2, '/').collect();
                    if parts.len() == 2 {
                        (parts[0].to_string(), parts[1].to_string())
                    } else {
                        return Ok(None);
                    }
                };

                match store.get_document(&collection, &path)? {
                    Some(doc) => {
                        let mut body = doc.body.unwrap_or_default();

                        // Apply line range
                        if let Some(from) = p.from_line {
                            let lines: Vec<&str> = body.lines().collect();
                            let start = from.saturating_sub(1);
                            let end = p.max_lines.map(|m| start + m).unwrap_or(lines.len());
                            body = lines
                                .get(start..end.min(lines.len()))
                                .map(|s| s.join("\n"))
                                .unwrap_or_default();
                        }

                        // Add line numbers
                        if p.line_numbers {
                            body = add_line_numbers(&body, p.from_line.unwrap_or(1));
                        }

                        Ok(Some((doc.title, body, doc.context)))
                    }
                    None => Ok(None),
                }
            },
        )
        .await
        .map_err(|e| to_mcp_error(e))?
        .map_err(to_mcp_error)?;

        match result {
            Some((title, body, context)) => {
                let mut text = format!("# {}\n\n", title);
                if let Some(ctx) = context {
                    text.push_str(&format!("<!-- Context: {} -->\n\n", ctx));
                }
                text.push_str(&body);
                Ok(CallToolResult::success(vec![Content::text(text)]))
            }
            None => Ok(CallToolResult::success(vec![Content::text(format!(
                "Document not found: {}",
                file_for_err
            ))])),
        }
    }

    /// Show the status of the QMD index: collections, document counts, and health information.
    #[tool(name = "status")]
    async fn status(&self) -> Result<CallToolResult, rmcp::ErrorData> {
        let result = tokio::task::spawn_blocking(|| -> Result<StatusResult, qmd::QmdError> {
            let store = qmd::Store::new()?;
            let status = store.get_status()?;

            Ok(StatusResult {
                total_documents: status.total_documents,
                needs_embedding: status.needs_embedding,
                has_vector_index: status.has_vector_index,
                collections: status
                    .collections
                    .into_iter()
                    .map(|c| CollectionStatus {
                        name: c.name,
                        path: c.pwd,
                        documents: c.active_count,
                    })
                    .collect(),
            })
        })
        .await
        .map_err(|e| to_mcp_error(e))?
        .map_err(to_mcp_error)?;

        let mut lines = vec![
            "QMD Index Status:".to_string(),
            format!("  Total documents: {}", result.total_documents),
            format!("  Needs embedding: {}", result.needs_embedding),
            format!(
                "  Vector index: {}",
                if result.has_vector_index { "yes" } else { "no" }
            ),
            format!("  Collections: {}", result.collections.len()),
        ];
        for col in &result.collections {
            lines.push(format!("    - {} ({} docs)", col.name, col.documents));
        }

        Ok(CallToolResult::success(vec![Content::text(
            lines.join("\n"),
        )]))
    }
}

#[tool_handler]
impl ServerHandler for QmdMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::LATEST,
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            server_info: Implementation {
                name: "qmd".into(),
                version: env!("CARGO_PKG_VERSION").into(),
                title: None,
                icons: None,
                website_url: None,
            },
            instructions: Some(
                "QMD - Quick Markdown Search. A local search engine for markdown knowledge bases. \
                 Use 'search' for keyword lookups, 'get' to retrieve documents, 'status' to check index."
                    .into(),
            ),
        }
    }
}
