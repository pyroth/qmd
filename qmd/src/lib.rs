//! QMD - Query Markdown Documents
//!
//! A full-text search tool for markdown files with collection management,
//! context annotations, vector search, and virtual path support.
//!
//! ## Features
//!
//! - Full-text search with BM25 ranking
//! - Vector semantic search with local embeddings
//! - Query expansion and RRF fusion
//! - Automatic model download from `HuggingFace`
//! - Fuzzy file matching
//! - Index health monitoring

pub mod cli;
pub mod collections;
pub mod config;
pub mod error;
pub mod formatter;
pub mod llm;
pub mod store;

pub use cli::{Cli, Commands};
pub use error::{QmdError, Result};
pub use llm::{
    BatchRerankResult, CHUNK_OVERLAP_TOKENS, CHUNK_SIZE_TOKENS, Chunk, Cursor, EmbeddingEngine,
    EmbeddingResult, GenerationEngine, GenerationResult, IndexHealth, Progress, PullResult,
    QueryType, Queryable, RerankDocument, RerankEngine, RerankResult, RrfResult, SnippetResult,
    TokenChunk, chunk_document, chunk_document_by_tokens, expand_query_simple, extract_snippet,
    format_doc_for_embedding, format_eta, format_query_for_embedding, hybrid_search_rrf,
    pull_model, pull_models, reciprocal_rank_fusion, render_progress_bar, resolve_model,
};
pub use store::{Store, find_similar_files, match_files_by_glob};
