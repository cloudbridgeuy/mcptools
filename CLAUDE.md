# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

MCPTOOLS is a Rust workspace providing MCP (Model Context Protocol) exposed tools for LLM Coding Agents. Currently implements:
- AWS shortcuts (KMS operations)
- Atlassian tools (Jira and Confluence search and management)
- Web content fetching and parsing

## Workspace Structure

This is a Cargo workspace with two main components:

- `crates/mcptools/` - Main binary application (AWS CLI shortcuts)
- `xtask/` - Build automation and project tasks (cargo-xtask pattern)

The project follows the cargo-xtask pattern for build automation. The `xtask` crate provides custom build commands accessible via `cargo xtask`.

## Common Commands

### Building and Running

```bash
# Build the project
cargo build

# Run the main binary
cargo run --bin mcptools -- <command>

# Build release binary
cargo build --release
```

### Testing

```bash
# Run all tests
cargo test

# Run tests with bacon (continuous testing)
bacon test
```

### Linting and Checking

```bash
# Check code without building
cargo check

# Check all targets
cargo check --all-targets

# Run clippy
cargo clippy --all-targets

# Use bacon for continuous checking (default)
bacon

# Use bacon for continuous clippy
bacon clippy
```

### Bacon Integration

The project uses [bacon](https://github.com/Canop/bacon) for continuous checking. Available bacon jobs:

- `bacon` or `bacon check` - Run cargo check (default)
- `bacon check-all` - Check all targets
- `bacon clippy` - Run clippy
- `bacon test` - Run tests
- `bacon doc` - Build documentation
- `bacon doc-open` - Build and open documentation
- `bacon run` - Run the application

### xtask Commands

Custom build tasks via cargo-xtask (Note: Cargo.toml has syntax issues that need fixing):

```bash
# Install binary to a path
cargo xtask install --name <binary> --path <install-path>

# Build release binaries (supports cross-compilation)
cargo xtask release --binary mcptools

# Build documentation site
cargo xtask build-docs

# Deploy documentation to GCP
cargo xtask deploy-docs

# Run documentation development server
cargo xtask dev-docs
```

### Installation Scripts

```bash
# Install git hooks
./scripts/install-hooks.sh

# Install binary (download latest release)
./scripts/install.sh
```

## Code Architecture

### Main Application (`crates/mcptools`)

The application uses a modular CLI structure with clap for argument parsing:

- `main.rs` - Entry point, CLI app structure with global options (AWS region, profile, verbose, Atlassian credentials)
- `prelude.rs` - Common imports and utilities (Result type, logging macros, table formatting)
- `error.rs` - Custom error types using thiserror
- `atlassian/` - Atlassian (Jira/Confluence) operations
  - `mod.rs` - Authentication config and HTTP client setup
  - `jira/mod.rs` - Jira operations (list/search issues)
  - `confluence/mod.rs` - Confluence operations (search pages)
- `mcp/tools/atlassian.rs` - MCP tool handlers for Jira and Confluence

**CLI Structure:**

- Global options: `--region`, `--profile`, `--verbose` (also available as env vars)
- Subcommands follow pattern: `mcptools <service> <operation>`
- Example: `mcptools kms list-keys`

**Key patterns:**

- Async runtime: Uses tokio
- Error handling: color-eyre for rich error reports
- AWS SDK: Configured via `get_sdk_config_from_global()` helper
- Output formatting: prettytable for tabular data, anstream for colored output
- Logging: env_logger, controlled by `RUST_LOG` environment variable

### xtask (`xtask/`)

Build automation following the cargo-xtask pattern:

- `cli.rs` - Command definitions (install, release, build-docs, deploy-docs, dev-docs)
- `scripts/` - Implementation modules for each command
- Uses duct for running child processes

## Development Notes

### Cargo.toml Issue

The root `Cargo.toml` currently has a syntax error - workspace manifests should not have a `[dependencies]` section. Dependencies should only be in `[workspace.dependencies]` for shared dependencies across the workspace.

### AWS Configuration

The application respects standard AWS configuration:

- Environment variables: `AWS_REGION`, `AWS_PROFILE`
- CLI flags: `--region`, `--profile`
- Default region: `us-east-1`
- Default profile: `default`

### Atlassian Configuration

The Atlassian module requires three environment variables:

- `ATLASSIAN_BASE_URL` (e.g., `https://your-domain.atlassian.net`)
- `ATLASSIAN_EMAIL` (your Atlassian email)
- `ATLASSIAN_API_TOKEN` (API token from https://id.atlassian.com/manage-profile/security/api-tokens)

See `ATLASSIAN_SETUP.md` for detailed setup instructions.

### Adding New AWS Services

To add a new AWS service (e.g., S3):

1. Create `src/<service>.rs` with the service module
2. Add module declaration in `main.rs`
3. Add variant to `SubCommands` enum
4. Implement `run()` function following the KMS pattern
5. Use `get_sdk_config_from_global()` for AWS client configuration

### Code Style

- Edition: Rust 2021
- Debug info disabled in dev profile for faster builds
- Incremental compilation enabled in release
- Uses workspace dependencies for consistency

## Web Fetching with MCP Tools

This section provides guidance for Claude Code on how to properly fetch web content using the MCP tools provided by this project.

### Available Tools

- `mcp__mcptools__md_toc` - Fetches the table of contents / structure of a webpage
- `mcp__mcptools__md_fetch` - Fetches webpage content, optionally filtered by CSS selectors

### Proper Web Fetching Workflow

**IMPORTANT:** Always follow this two-step process when fetching web content:

#### Step 1: Get Page Structure with `md_toc`

Before fetching any content, first use `mcp__mcptools__md_toc` to understand the page structure:

```
Purpose: Retrieve the table of contents or structural outline of the webpage
Returns: Hierarchical list of sections, headings, and major content blocks
Use case: Identify which sections contain the information you need
```

This step allows you to:
- Understand the page layout before fetching full content
- Identify relevant CSS selectors for targeted content extraction
- Determine which sections are needed for your task
- Avoid fetching unnecessary content

#### Step 2: Fetch Targeted Content with `md_fetch`

After analyzing the TOC, use `mcp__mcptools__md_fetch` with appropriate selectors:

```
Purpose: Fetch specific sections of the webpage using CSS selectors
Parameters:
  - url: The webpage URL
  - selector: CSS selector to filter content (e.g., "article", "div.content", "main")
  - strategy: Selection strategy when multiple elements match (first, last, all, n)
  - page: Page number for pagination (default: 1)
  - limit: Characters per page for pagination (default: 1000)

Returns: Markdown-formatted content from the selected elements
```

**Best Practices:**

1. **Use specific selectors**: Based on the TOC, identify the most specific CSS selector that targets only the content you need
2. **Prefer narrow selectors over broad ones**: Use `article.blog-post` instead of `body` to reduce noise
3. **Leverage pagination**: If content is large, use the `page` and `limit` parameters to fetch content in manageable chunks
4. **Combine selectors**: Use strategy parameter to handle multiple matching elements appropriately

#### Example Workflow

```
Task: Extract installation instructions from a project's documentation page

Step 1: Fetch TOC
Call: mcp__mcptools__md_toc("https://example.com/docs")
Result:
  - Getting Started
    - Installation
    - Configuration
  - API Reference
  - Examples

Step 2: Analyze TOC
Decision: Need the "Installation" section under "Getting Started"
Selector: Likely "section#installation" or "div.installation"

Step 3: Fetch targeted content
Call: mcp__mcptools__md_fetch(
  url: "https://example.com/docs",
  selector: "section#installation",
  strategy: "first"
)
Result: Markdown content of just the installation section
```

#### Anti-Patterns to Avoid

**DON'T:**
- Skip the TOC step and fetch the entire page blindly
- Use overly broad selectors like `body` or `div` without class/id specificity
- Fetch the full page when you only need one section
- Ignore the TOC structure when determining selectors

**DO:**
- Always call `md_toc` first to understand page structure
- Use the most specific CSS selector possible
- Leverage pagination for large content
- Extract only the sections relevant to your task

### Selector Strategy Guide

When using `md_fetch`, choose the appropriate strategy:

- **`first`** (default): Select the first matching element (most common use case)
- **`last`**: Select the last matching element
- **`all`**: Select all matching elements and concatenate them
- **`n`**: Select the nth matching element (requires `index` parameter)

### Error Handling

If `md_fetch` returns an error about no matching elements:
1. Review the TOC again to verify the page structure
2. Try a broader selector (e.g., `article` instead of `article.specific-class`)
3. Check if the page uses different HTML structure than expected
4. Consider fetching with a more generic selector, then filtering the markdown content

### Site-Specific Selector Guidelines

**LocalStack Documentation** (`https://docs.localstack.cloud/*`):
- Always use `selector: "main"` when fetching content from LocalStack docs
- This ensures you capture the main content area without sidebars or navigation elements

## CLI Patterns and Templates

This section documents the standard patterns used for building CLI applications with clap in this repository.

### Project File Structure

```
crates/your-cli/
├── src/
│   ├── main.rs           # Entry point, App struct, SubCommands enum, main()
│   ├── prelude.rs        # Common imports and utilities
│   ├── error.rs          # Custom error types with thiserror
│   ├── module1.rs        # Feature module (e.g., kms, s3)
│   └── module2.rs        # Another feature module
└── Cargo.toml
```

### 1. Main Entry Point Pattern (`main.rs`)

#### Async Application (with tokio)

```rust
// If eprintln and println are required.
use crate::prelude::{eprintln, println, *};
use clap::Parser;

// Module declarations
mod error;
mod module_name;
mod prelude;

/// Main application struct
#[derive(Debug, clap::Parser)]
#[command(
    author,
    version,
    about,
    long_about = "Detailed description of your application"
)]
pub struct App {
    /// Nested subcommands
    #[command(subcommand)]
    pub command: SubCommands,

    /// Global options available to all subcommands
    #[clap(flatten)]
    global: Global,
}

/// Global options shared across all commands
#[derive(Debug, clap::Args)]
pub struct Global {
    /// Option with environment variable override
    #[clap(long, env = "YOUR_ENV_VAR", global = true, default_value = "default-value")]
    option_name: Option<String>,

    /// Boolean flag with env override
    #[clap(long, env = "YOUR_VERBOSE", global = true, default_value = "false")]
    verbose: bool,
}

/// Top-level subcommands
#[derive(Debug, clap::Parser)]
pub enum SubCommands {
    /// Description of the subcommand
    ModuleName(crate::module_name::App),

    /// Another subcommand
    AnotherModule(crate::another_module::App),
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging and error handling
    env_logger::init();
    color_eyre::install()?;

    // Parse CLI arguments
    let app = App::parse();

    // Dispatch to appropriate module
    match app.command {
        SubCommands::ModuleName(sub_app) => crate::module_name::run(sub_app, app.global).await,
        SubCommands::AnotherModule(sub_app) => crate::another_module::run(sub_app, app.global).await,
    }
    .map_err(|err: color_eyre::eyre::Report| eyre!(err))
}
```

#### Sync Application (without tokio)

```rust
use clap::Parser;
use color_eyre::eyre::Result;

mod cli;
mod scripts;

fn main() -> Result<()> {
    let cli = cli::App::parse();

    match &cli.command {
        Some(command) => match command {
            cli::Commands::CommandOne(args) => scripts::command_one(args),
            cli::Commands::CommandTwo(args) => scripts::command_two(args),
        },
        None => {
            println!("No command specified.");
            std::process::exit(1);
        }
    }
}
```

### 2. Module Pattern (Feature Modules like `kms.rs`, `s3.rs`)

```rust
// If eprintln and println are required.
use crate::prelude::{eprintln, println, *};

/// Module-level app struct
#[derive(Debug, clap::Parser)]
#[command(name = "module-name")]
#[command(about = "Module description")]
pub struct App {
    #[command(subcommand)]
    pub command: Commands,
}

/// Commands within this module
#[derive(Debug, clap::Subcommand)]
pub enum Commands {
    /// Command description
    #[clap(name = "command-name")]
    CommandName(CommandOptions),

    /// Command without options
    #[clap(name = "simple-command")]
    SimpleCommand,
}

/// Options for a specific command
#[derive(Debug, clap::Args, serde::Serialize, serde::Deserialize, Clone)]
pub struct CommandOptions {
    /// Required argument with env override
    #[clap(env = "YOUR_MODULE_ARG")]
    required_arg: String,

    /// Optional argument with short and long flags
    #[arg(short, long)]
    optional_arg: Option<String>,

    /// Flag with default value
    #[arg(short, long, default_value = "default")]
    with_default: String,
}

/// Module entry point - receives app and global options
pub async fn run(app: App, global: crate::Global) -> Result<()> {
    // Access global options if needed
    if global.verbose {
        println!("Verbose mode enabled");
    }

    // Dispatch to command handlers
    match app.command {
        Commands::CommandName(options) => command_handler(options).await,
        Commands::SimpleCommand => simple_handler().await,
    }
}

/// Individual command handler
async fn command_handler(options: CommandOptions) -> Result<()> {
    // Implementation
    Ok(())
}

async fn simple_handler() -> Result<()> {
    // Implementation
    Ok(())
}
```

### 3. CLI Arguments/Options Pattern (`cli.rs` for build tools)

```rust
use clap::{Args, Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "cli-name")]
#[command(about = "CLI description")]
pub struct App {
    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Command description
    CommandName(CommandArgs),
}

#[derive(Args, Debug)]
pub struct CommandArgs {
    /// Required named argument
    #[arg(short, long)]
    pub name: String,

    /// Optional with default
    #[arg(short, long, default_value = "default")]
    pub optional: String,

    /// Boolean flag
    #[arg(long)]
    pub flag: bool,

    /// Negation pattern for boolean flags
    #[arg(long)]
    pub no_feature: bool,
}
```

### 4. Prelude Pattern (`prelude.rs`)

```rust
// Re-export custom error type
pub use crate::error::Error;

// Re-export common dependencies
pub use anstream::eprintln;
pub use anstream::println;
pub use color_eyre::eyre::{eyre, Context, OptionExt, Result};
pub use std::format as f;

// Utility functions
pub fn new_table() -> prettytable::Table {
    let mut table = prettytable::Table::new();
    let format = prettytable::format::FormatBuilder::new()
        .padding(1, 1)
        .build();
    table.set_format(format);
    table
}
```

### 5. Error Pattern (`error.rs`)

```rust
#[derive(thiserror::Error, Debug, serde::Deserialize, serde::Serialize)]
pub enum Error {
    #[error("Generic {0}")]
    Generic(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Invalid input: {0}")]
    InvalidInput(String),
}
```

### 6. Environment Variable Override Pattern

Clap supports automatic environment variable overrides:

```rust
/// Option with env override and default
#[clap(long, env = "APP_OPTION", default_value = "default")]
option: String,

/// Optional value with env override (no default)
#[clap(long, env = "APP_OPTIONAL")]
optional: Option<String>,

/// Boolean flag with env override
#[clap(long, env = "APP_VERBOSE", default_value = "false")]
verbose: bool,

/// Global option available to all subcommands
#[clap(long, env = "APP_CONFIG", global = true)]
config: Option<String>,
```

**Usage:**

```bash
# Via flag
your-cli --option value command

# Via environment variable
APP_OPTION=value your-cli command

# Environment variable takes precedence over default
APP_OPTION=custom your-cli command
```

### 7. Command Naming Conventions

```rust
// Use kebab-case for command names
#[clap(name = "list-keys")]
ListKeys,

#[clap(name = "get-policy")]
GetPolicy(Options),

// Short and long flags
#[arg(short, long)]  // -n, --name
name: String,

#[arg(short = 'o', long = "output")]  // -o, --output
output: String,

// Long flag only
#[arg(long)]  // --verbose
verbose: bool,
```

### 8. Common Patterns Summary

**Option Types:**

- `String` - Required string argument
- `Option<String>` - Optional string argument
- `bool` - Boolean flag (presence = true)
- `Vec<String>` - Multiple values allowed
- `PathBuf` - File system paths

**Attributes:**

- `#[arg(short, long)]` - Both short and long forms
- `#[arg(long, env = "VAR")]` - Environment variable override
- `#[arg(default_value = "val")]` - Default value
- `#[clap(global = true)]` - Available to all subcommands
- `#[clap(flatten)]` - Embed another Args struct
- `#[command(subcommand)]` - Nest subcommands

**Initialization Pattern:**

```rust
#[tokio::main]  // or just fn main() for sync
async fn main() -> Result<()> {
    env_logger::init();      // Initialize logging
    color_eyre::install()?;  // Rich error reports

    let app = App::parse();  // Parse CLI args

    // Dispatch logic...
}
```
