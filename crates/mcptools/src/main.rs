#![allow(unused)]

use crate::prelude::*;
use clap::{CommandFactory, FromArgMatches, Parser};
use mcptools_core::sandbox::Limits;
use std::time::Duration;

mod agent;
mod atlassian;
mod chrome_reaper;
mod config;
mod diagram;
mod error;
mod find_tools;
mod gateway;
mod hn;
mod images;
mod jev;
mod lane;
mod linear;
mod llm_stream;
mod mcp;
mod md;
mod open;
mod pdf;
mod plan_store;
mod prelude;
mod sandbox;
mod upgrade;

#[derive(Debug, clap::Parser)]
#[command(
    author,
    version = env!("MCPTOOLS_VERSION"),
    about,
    long_about = "MCP tools for web content, HackerNews, and Atlassian integrations"
)]
pub struct App {
    #[command(subcommand)]
    pub command: SubCommands,

    #[clap(flatten)]
    global: Global,
}

#[derive(Debug, Clone, clap::Parser)]
pub struct Global {
    /// Whether to display additional information.
    #[clap(long, env = "MCPTOOLS_VERBOSE", global = true, default_value = "false")]
    pub verbose: bool,

    #[clap(
        long,
        env = "MCPTOOLS_EXECUTE_TIMEOUT_SECS",
        global = true,
        default_value_t = 30,
        value_name = "SECS",
        value_parser = clap::value_parser!(u64).range(1..)
    )]
    pub execute_timeout_secs: u64,

    #[clap(
        long,
        env = "MCPTOOLS_EXECUTE_MEMORY_MB",
        global = true,
        default_value_t = 64,
        value_name = "MB",
        value_parser = clap::value_parser!(u64).range(1..)
    )]
    pub execute_memory_mb: u64,

    #[clap(
        long,
        env = "MCPTOOLS_EXECUTE_OUTPUT_KB",
        global = true,
        default_value_t = 256,
        value_name = "KB",
        value_parser = clap::value_parser!(u64).range(1..)
    )]
    pub execute_output_kb: u64,
}

impl Global {
    pub fn sandbox_limits(&self) -> Limits {
        Limits {
            timeout: Duration::from_secs(self.execute_timeout_secs),
            memory_bytes: (self.execute_memory_mb * 1024 * 1024) as usize,
            output_bytes: (self.execute_output_kb * 1024) as usize,
        }
    }
}

#[derive(Debug, clap::Parser)]
pub enum SubCommands {
    Config(crate::config::App),
    #[command(hide = true)]
    UpstreamReaper(crate::gateway::ReaperArgs),
    #[command(hide = true)]
    ChromeReaper(crate::chrome_reaper::App),
    Diagram(crate::diagram::App),
    /// Agent setup, status, and uninstall
    Agent(crate::agent::App),

    /// Atlassian (Jira, Confluence) operations
    Atlassian(Box<crate::atlassian::App>),

    #[command(about = "Find the best-fitting tools for a task")]
    FindTools(crate::find_tools::App),

    /// HackerNews (news.ycombinator.com) operations
    HN(crate::hn::App),

    #[command(about = mcptools_core::images::about())]
    Images(crate::images::App),

    Lane(crate::lane::App),

    /// Model Context Protocol server
    MCP(crate::mcp::App),

    /// Convert web pages to Markdown using headless Chrome
    MD(crate::md::App),

    /// PDF document navigation and extraction
    Pdf(crate::pdf::App),

    Linear(crate::linear::App),

    /// Upgrade mcptools to the latest version
    Upgrade(crate::upgrade::App),
}

fn main() -> Result<()> {
    env_logger::init();
    color_eyre::install()?;

    let command = App::command();
    let hidden = command
        .get_arguments()
        .filter(|arg| arg.get_id().as_str().starts_with("execute_"))
        .cloned()
        .map(|arg| arg.hide(true))
        .collect::<Vec<_>>();
    let matches = command
        .mut_subcommand("images", |images| images.args(hidden))
        .get_matches();
    let mut app = App::from_arg_matches(&matches)?;
    let defaulted =
        |name| matches.value_source(name) == Some(clap::parser::ValueSource::DefaultValue);
    if let SubCommands::MCP(ref mut mcp) = app.command {
        let source = matches.subcommand_matches("mcp").expect("mcp matches");
        let supplied = mcptools_core::config::Mcp {
            discovery: (source.value_source("discovery")
                != Some(clap::parser::ValueSource::DefaultValue))
            .then_some(mcp.flags.discovery),
            code_mode: (source.value_source("code_mode")
                != Some(clap::parser::ValueSource::DefaultValue))
            .then_some(mcp.flags.code_mode),
            execute_timeout_secs: (!defaulted("execute_timeout_secs"))
                .then_some(app.global.execute_timeout_secs),
            execute_memory_mb: (!defaulted("execute_memory_mb"))
                .then_some(app.global.execute_memory_mb),
            execute_output_kb: (!defaulted("execute_output_kb"))
                .then_some(app.global.execute_output_kb),
        };
        let file = if supplied.needs_file() {
            crate::config::optional()?.unwrap_or_default().mcp
        } else {
            Default::default()
        };
        let resolved = supplied.with_fallback(file);
        mcp.flags.discovery = resolved.discovery.unwrap_or(false);
        mcp.flags.code_mode = resolved.code_mode.unwrap_or(false);
        app.global.execute_timeout_secs = resolved.execute_timeout_secs.unwrap_or(30);
        app.global.execute_memory_mb = resolved.execute_memory_mb.unwrap_or(64);
        app.global.execute_output_kb = resolved.execute_output_kb.unwrap_or(256);
    }
    if let SubCommands::MD(ref mut md) = app.command {
        if matches!(
            md.command,
            crate::md::Commands::Fetch(_) | crate::md::Commands::Toc(_)
        ) {
            let source = matches
                .subcommand_matches("md")
                .and_then(|m| m.subcommand())
                .map(|(_, m)| m)
                .expect("md matches");
            if source.value_source("timeout") == Some(clap::parser::ValueSource::DefaultValue) {
                let timeout = crate::config::optional()?
                    .unwrap_or_default()
                    .md
                    .timeout
                    .unwrap_or(30);
                match &mut md.command {
                    crate::md::Commands::Fetch(options) => options.timeout = timeout,
                    crate::md::Commands::Toc(options) => options.timeout = timeout,
                    crate::md::Commands::Render(_) => unreachable!(),
                }
            }
        }
    }
    if defaulted("verbose")
        && matches!(
            app.command,
            SubCommands::MCP(_)
                | SubCommands::HN(_)
                | SubCommands::Atlassian(_)
                | SubCommands::Linear(_)
        )
    {
        let file = crate::config::optional().ok().flatten().unwrap_or_default();
        app.global.verbose =
            mcptools_core::config::resolve(None, None, file.general.verbose, Some(false))
                .unwrap_or(false);
    }

    if let SubCommands::ChromeReaper(sub_app) = app.command {
        return crate::chrome_reaper::run(sub_app);
    }
    if let SubCommands::UpstreamReaper(args) = app.command {
        return crate::gateway::reap(args);
    }

    run(app)
}

#[tokio::main]
async fn run(app: App) -> Result<()> {
    if matches!(app.command, SubCommands::MCP(_) | SubCommands::FindTools(_)) {
        crate::gateway::initialize().await?;
    }
    if matches!(app.command, SubCommands::MCP(_)) {
        crate::chrome_reaper::sweep()?;
    }

    let result = match app.command {
        SubCommands::Config(args) => crate::config::run(args),
        SubCommands::UpstreamReaper(args) => crate::gateway::reap(args),
        SubCommands::ChromeReaper(sub_app) => crate::chrome_reaper::run(sub_app),
        SubCommands::Diagram(sub_app) => crate::diagram::run(sub_app).await,
        SubCommands::Agent(sub_app) => crate::agent::run(sub_app, app.global).await,
        SubCommands::Atlassian(sub_app) => crate::atlassian::run(*sub_app, app.global).await,
        SubCommands::FindTools(sub_app) => crate::find_tools::run(sub_app, app.global).await,
        SubCommands::HN(sub_app) => crate::hn::run(sub_app, app.global).await,
        SubCommands::Images(sub_app) => crate::images::run(sub_app, app.global).await,
        SubCommands::Lane(sub_app) => crate::lane::run(sub_app, app.global).await,
        SubCommands::MCP(sub_app) => crate::mcp::run(sub_app, app.global).await,
        SubCommands::MD(sub_app) => crate::md::run(sub_app, app.global).await,
        SubCommands::Pdf(sub_app) => crate::pdf::run(sub_app, app.global).await,
        SubCommands::Linear(sub_app) => crate::linear::run(sub_app, app.global).await,
        SubCommands::Upgrade(sub_app) => crate::upgrade::run(sub_app, app.global).await,
    }
    .map_err(|err: color_eyre::eyre::Report| eyre!(err));
    crate::gateway::shutdown().await;
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sandbox_limits_from_flags() {
        let global = Global::try_parse_from([
            "mcptools",
            "--execute-timeout-secs",
            "5",
            "--execute-memory-mb",
            "2",
            "--execute-output-kb",
            "3",
        ])
        .unwrap();

        let limits = global.sandbox_limits();
        assert_eq!(limits.timeout, Duration::from_secs(5));
        assert_eq!(limits.memory_bytes, 2 * 1024 * 1024);
        assert_eq!(limits.output_bytes, 3 * 1024);
    }

    #[test]
    fn sandbox_limits_without_flags_are_default() {
        let limits = Global::try_parse_from(["mcptools"])
            .unwrap()
            .sandbox_limits();
        let default = Limits::default();
        assert_eq!(limits.timeout, default.timeout);
        assert_eq!(limits.memory_bytes, default.memory_bytes);
        assert_eq!(limits.output_bytes, default.output_bytes);
    }

    #[test]
    fn zero_execute_memory_is_rejected() {
        assert!(Global::try_parse_from(["mcptools", "--execute-memory-mb", "0"]).is_err());
    }
    #[test]
    fn secret_environment_arguments_hide_values() {
        let mut commands = vec![super::App::command()];
        let mut failures = Vec::new();
        while let Some(command) = commands.pop() {
            for arg in command.get_arguments() {
                if let Some(env) = arg.get_env().and_then(|name| name.to_str()) {
                    if env != "JIRA_ISSUE_KEY"
                        && ["KEY", "TOKEN", "PASSWORD", "SECRET"]
                            .iter()
                            .any(|marker| env.contains(*marker))
                        && !arg.is_hide_env_values_set()
                    {
                        failures.push(format!("{}: {env}", command.get_name()));
                    }
                }
            }
            commands.extend(command.get_subcommands().cloned());
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }
}
