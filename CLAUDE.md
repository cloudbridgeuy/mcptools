# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

MCPTOOLS is a Rust workspace providing MCP (Model Context Protocol) exposed tools for LLM Coding Agents. Currently implements AWS shortcuts, with primary focus on KMS (Key Management Service) operations.

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

- `main.rs` - Entry point, CLI app structure with global options (AWS region, profile, verbose)
- `prelude.rs` - Common imports and utilities (Result type, logging macros, table formatting)
- `error.rs` - Custom error types using thiserror
- `kms.rs` - AWS KMS operations (list keys, get policies)

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
use crate::prelude::*;
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
use crate::prelude::*;

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
        aprintln!("Verbose mode enabled");
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
pub use anstream::eprintln as aeprintln;
pub use anstream::println as aprintln;
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
