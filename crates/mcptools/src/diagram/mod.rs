pub mod retention;
mod serve;

use std::net::IpAddr;
use std::time::Duration;

async fn ready() -> bool {
    let Ok((host, port)) = crate::plan_store::endpoint() else {
        return false;
    };
    let address = format!(
        "{}:{port}",
        if host.contains(':') && !host.starts_with('[') {
            format!("[{host}]")
        } else {
            host
        }
    );
    let Ok(client) = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_millis(200))
        .build()
    else {
        return false;
    };
    client
        .get(format!("http://{address}/"))
        .send()
        .await
        .is_ok_and(|response| {
            response.status() == reqwest::StatusCode::NOT_FOUND
                && response
                    .headers()
                    .get("x-content-type-options")
                    .is_some_and(|value| value == "nosniff")
                && response
                    .headers()
                    .get("referrer-policy")
                    .is_some_and(|value| value == "no-referrer")
        })
}

pub async fn ensure_running() -> bool {
    if ready().await {
        return true;
    }
    let result = (|| -> crate::prelude::Result<()> {
        let directory = crate::plan_store::data_dir()?.join("diagrams");
        std::fs::create_dir_all(&directory)?;
        let log = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(directory.join("daemon.log"))?;
        let port = port(None)?.to_string();
        let mut command = std::process::Command::new(std::env::current_exe()?);
        command
            .args(["diagram", "serve", "--tailscale", "--port", &port])
            .stdin(std::process::Stdio::null())
            .stdout(log.try_clone()?)
            .stderr(log);
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
        }
        let mut child = command.spawn()?;
        std::thread::spawn(move || {
            let _ = child.wait();
        });
        Ok(())
    })();
    if result.is_err() {
        return false;
    }
    let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
    while tokio::time::Instant::now() < deadline {
        if ready().await {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    false
}

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
        #[arg(long)]
        port: Option<u16>,
    },
}

fn port(value: Option<u16>) -> crate::prelude::Result<u16> {
    if let Some(value) = value {
        return Ok(value);
    }
    match std::env::var("MCPTOOLS_DIAGRAM_PORT") {
        Ok(value) => Ok(value.parse()?),
        Err(std::env::VarError::NotPresent) => Ok(crate::config::optional()?
            .unwrap_or_default()
            .diagram
            .port
            .unwrap_or(5199)),
        Err(error) => Err(error.into()),
    }
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
            serve::run(host, self::port(port)?).await
        }
    }
}
