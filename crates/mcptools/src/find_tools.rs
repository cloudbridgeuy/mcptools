use crate::prelude::{println, *};

#[derive(Debug, clap::Parser)]
#[command(name = "find-tools")]
#[command(about = "Find the best-fitting tools for a task")]
pub struct App {
    #[arg(help = "Natural-language description of the task")]
    pub task: String,
    #[arg(short = 'k', long, default_value_t = mcptools_core::find_tools::DEFAULT_K, help = "Maximum number of tools to return")]
    pub k: usize,
}

pub async fn run(app: App, _global: crate::Global) -> Result<()> {
    let result = crate::mcp::find_tools(&app.task, app.k)
        .await
        .map_err(|e| eyre!(e.to_string()))?;
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}
