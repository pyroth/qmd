//! QMD MCP Server - Entry point with stdio transport.

use anyhow::Result;
use clap::Parser;
use rmcp::ServiceExt;
use tracing_subscriber::{EnvFilter, fmt, prelude::*};

use qmd_mcp::QmdMcpServer;

/// QMD MCP Server - Model Context Protocol server for QMD search engine.
#[derive(Parser, Debug)]
#[command(name = "qmd-mcp")]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Enable verbose logging.
    #[arg(short, long)]
    verbose: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Initialize logging to stderr (stdout is used for MCP communication)
    let filter = if args.verbose {
        EnvFilter::new("debug")
    } else {
        EnvFilter::new("warn")
    };

    tracing_subscriber::registry()
        .with(fmt::layer().with_writer(std::io::stderr))
        .with(filter)
        .init();

    // Create QMD MCP server
    let server = QmdMcpServer::new();

    tracing::info!("Starting QMD MCP server with stdio transport");

    // Serve using stdio transport
    let service = server.serve(rmcp::transport::stdio()).await?;

    // Wait for the service to complete
    service.waiting().await?;

    Ok(())
}
