//! QMD MCP Server - Model Context Protocol server for QMD search engine.
//!
//! This crate provides an MCP server that exposes QMD's search and document
//! retrieval capabilities to AI assistants via the Model Context Protocol.
//!
//! ## Features
//!
//! - **Tools**: search, get, status
//! - **Transports**: stdio (local) and HTTP (remote)
//!
//! ## Usage
//!
//! ```bash
//! # Start with stdio transport (default)
//! qmd-mcp
//!
//! # Start with HTTP transport
//! qmd-mcp --transport http --port 8080
//! ```

pub mod server;

pub use server::QmdMcpServer;
