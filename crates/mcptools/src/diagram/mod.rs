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
        #[arg(
            long,
            conflicts_with = "tailscale",
            required_unless_present = "tailscale"
        )]
        host: Option<IpAddr>,
        #[arg(long)]
        tailscale: bool,
        #[arg(long, default_value_t = 5199)]
        port: u16,
    },
}

pub async fn run(app: App) -> crate::prelude::Result<()> {
    match app.command {
        Commands::Serve {
            host,
            tailscale,
            port,
        } => {
            let host = if tailscale {
                let output = std::process::Command::new("tailscale")
                    .args(["ip", "-4"])
                    .output()?;
                if !output.status.success() {
                    return Err(color_eyre::eyre::eyre!("tailscale ip -4 failed"));
                }
                mcptools_core::diagram_endpoint::tailscale_ip(&String::from_utf8_lossy(
                    &output.stdout,
                ))
                .ok_or_else(|| color_eyre::eyre::eyre!("no Tailscale IPv4 address"))?
                .into()
            } else {
                host.expect("host required without tailscale")
            };
            serve::run(host, port).await
        }
    }
}
