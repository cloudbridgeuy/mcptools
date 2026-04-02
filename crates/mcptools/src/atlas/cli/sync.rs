use crate::atlas::cli::index::IndexOptions;
use crate::prelude::*;

#[derive(Debug, clap::Parser)]
pub struct SyncOptions {
    /// Number of parallel LLM workers for file descriptions
    #[clap(long, default_value = "1")]
    pub parallel: usize,
}

pub async fn run(opts: SyncOptions, global: crate::Global) -> Result<()> {
    crate::prelude::eprintln!("Re-indexing from scratch...");
    super::index::run(
        IndexOptions {
            parallel: opts.parallel,
            incremental: false,
        },
        global,
    )
    .await
}
