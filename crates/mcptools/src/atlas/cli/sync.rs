use crate::atlas::cli::index::IndexOptions;
use crate::prelude::*;

#[derive(Debug, clap::Parser)]
pub struct SyncOptions {}

pub async fn run(_opts: SyncOptions, global: crate::Global) -> Result<()> {
    crate::prelude::eprintln!("Refreshing structural index...");
    super::index::run(
        IndexOptions {
            incremental: false,
            dry_run: false,
            stdin: false,
        },
        global,
    )
    .await
}
