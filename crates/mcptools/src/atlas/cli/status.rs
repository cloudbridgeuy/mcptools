use crate::prelude::*;
use mcptools_core::atlas::format_status;

use crate::atlas::cli::index;
use crate::atlas::config::load_config;
use crate::atlas::data::atlas_status_data;
use crate::atlas::db::Database;

#[derive(Debug, clap::Args)]
pub struct StatusOptions {
    /// Output as JSON
    #[arg(long)]
    pub json: bool,
}

pub async fn run(opts: StatusOptions, _global: crate::Global) -> Result<()> {
    let root = index::find_git_root()?;
    let config = load_config(&root)?;
    let db = Database::open(&config.db_path.resolve(&root))?;
    let status = atlas_status_data(&db, &config, &root)?;
    crate::prelude::println!("{}", format_status(&status, opts.json));
    Ok(())
}
