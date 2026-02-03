//! QMD MCP Server - Model Context Protocol server for QMD search engine.
//!
//! This crate provides an MCP server that exposes QMD's search and document
//! retrieval capabilities to AI assistants via the Model Context Protocol.
//!
//! ## Features
//!
//! - **Tools**: search, get, status
//!
//! ## Usage
//!
//! ```bash
//! qmd-mcp
//! ```

pub mod server;

pub use server::QmdMcpServer;
