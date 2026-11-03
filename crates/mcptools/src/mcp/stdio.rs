use crate::prelude::{eprintln, *};
use std::io::BufRead;
use tokio::io::AsyncWriteExt;

pub async fn run_stdio(global: crate::Global, flags: crate::mcp::ServeFlags) -> Result<()> {
    let shutdown = crate::md::BrowserShutdown::new();
    #[cfg(unix)]
    let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    #[cfg(unix)]
    let mut interrupt = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())?;
    #[cfg(unix)]
    let result = tokio::select! {
        result = serve(global, flags) => result,
        _ = terminate.recv() => Ok(()),
        _ = interrupt.recv() => Ok(()),
    };
    #[cfg(not(unix))]
    let result = tokio::select! {
        result = serve(global, flags) => result,
        result = tokio::signal::ctrl_c() => { result?; Ok(()) },
    };
    drop(shutdown);
    result
}

async fn serve(global: crate::Global, flags: crate::mcp::ServeFlags) -> Result<()> {
    if global.verbose {
        eprintln!("Starting MCP server with stdio transport...");
        eprintln!();
    }

    let mut stdout = tokio::io::stdout();
    let (sender, mut lines) = tokio::sync::mpsc::channel(1);
    std::thread::spawn(move || {
        let stdin = std::io::stdin();
        let mut reader = stdin.lock();
        loop {
            let mut line = String::new();
            match reader.read_line(&mut line) {
                Ok(0) => break,
                Ok(_) => {
                    if sender.blocking_send(Ok(line)).is_err() {
                        break;
                    }
                }
                Err(error) => {
                    let _ = sender.blocking_send(Err(error));
                    break;
                }
            }
        }
    });

    while let Some(line) = lines.recv().await {
        let line = line?;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        if global.verbose {
            eprintln!("Received: {trimmed}");
        }

        let Some(response) = super::handle_request(trimmed, &global, flags).await else {
            continue;
        };
        let response_json = serde_json::to_string(&response)?;

        if global.verbose {
            eprintln!("Sending: {response_json}");
        }

        stdout.write_all(response_json.as_bytes()).await?;
        stdout.write_all(b"\n").await?;
        stdout.flush().await?;
    }

    Ok(())
}
