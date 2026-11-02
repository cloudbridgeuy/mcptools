pub mod retention;
mod serve;

use std::net::IpAddr;

#[derive(Debug, clap::Parser)]
#[command(about = "Serve stored plan diagrams")]
pub struct App {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, clap::Subcommand)]
enum Commands {
    Serve {
        #[arg(long)]
        host: IpAddr,
        #[arg(long, default_value_t = 5199)]
        port: u16,
    },
}

pub async fn run(app: App) -> crate::prelude::Result<()> {
    match app.command {
        Commands::Serve { host, port } => serve::run(host, port).await,
    }
}
