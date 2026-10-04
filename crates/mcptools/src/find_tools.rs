use crate::mcp::tools::find_tools::{query, FindToolsArgs};
use crate::prelude::{println, *};

#[derive(Debug, clap::Parser)]
#[command(name = "find-tools")]
#[command(
    about = "Find the best-fitting tools for a task, list domains, or list tools in a domain"
)]
pub struct App {
    #[arg(help = "Natural-language description of the task")]
    pub task: Option<String>,
    #[arg(
        short = 'k',
        long,
        help = "Maximum number of tools to return (only valid with task)"
    )]
    pub k: Option<usize>,
    #[arg(long, help = "Domain prefix to list tools for")]
    pub domain: Option<String>,
    #[arg(long, help = "List domain prefixes instead of ranking")]
    pub list_domains: bool,
}

pub async fn run(app: App, _global: crate::Global) -> Result<()> {
    let list_domains = if app.list_domains { Some(true) } else { None };
    let args = FindToolsArgs {
        task: app.task,
        k: app.k,
        domain: app.domain,
        list_domains,
    };
    let result = query(args, false).await.map_err(|e| eyre!(e.to_string()))?;
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}
