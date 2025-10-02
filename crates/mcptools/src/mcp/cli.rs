#[derive(Debug, clap::Parser)]
#[command(name = "mcp")]
#[command(about = "Model Context Protocol server")]
pub struct App {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, clap::Subcommand)]
pub enum Commands {
    /// Start MCP server with stdio transport
    #[clap(name = "stdio")]
    Stdio,

    /// Start MCP server with SSE transport (HTTP)
    #[clap(name = "sse")]
    Sse(SseOptions),
}

#[derive(Debug, clap::Args)]
pub struct SseOptions {
    /// Port to listen on
    #[arg(short, long, default_value = "3000")]
    pub port: u16,

    /// Host to bind to
    #[arg(long, default_value = "127.0.0.1")]
    pub host: String,
}
