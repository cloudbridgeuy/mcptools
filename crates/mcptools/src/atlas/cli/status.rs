use crate::prelude::*;
use mcptools_core::atlas::format_status;

use crate::atlas::cli::index;
use crate::atlas::data::repository_status;

#[derive(Debug, clap::Args)]
pub struct StatusOptions {
    #[arg(long)]
    pub json: bool,
}

pub async fn run(opts: StatusOptions, _global: crate::Global) -> Result<()> {
    let result = index::find_git_root().and_then(|root| repository_status(&root));
    match result {
        Ok(status) => {
            crate::prelude::println!("{}", format_status(&status, opts.json));
            Ok(())
        }
        Err(error) => {
            if opts.json {
                let status = mcptools_core::atlas::IndexStatus {
                    outcome: if error
                        .downcast_ref::<crate::atlas::data::StatusConfigurationFailure>()
                        .is_some()
                    {
                        mcptools_core::atlas::StatusOutcome::ConfigFailure
                    } else {
                        mcptools_core::atlas::StatusOutcome::ExecutionFailure
                    },
                    structural: mcptools_core::atlas::StructuralState::Unknown,
                    guidance: Some(
                        "Check the repository, Atlas configuration, and index permissions".into(),
                    ),
                    ..Default::default()
                };
                crate::prelude::println!("{}", format_status(&status, true));
            }
            Err(error)
        }
    }
}
