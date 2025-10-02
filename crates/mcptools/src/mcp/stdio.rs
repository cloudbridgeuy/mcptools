use crate::prelude::{eprintln, *};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

pub async fn run_stdio(global: crate::Global) -> Result<()> {
    if global.verbose {
        eprintln!("Starting MCP server with stdio transport...");
        eprintln!();
    }

    let stdin = tokio::io::stdin();
    let mut stdout = tokio::io::stdout();
    let mut reader = BufReader::new(stdin);
    let mut line = String::new();

    loop {
        line.clear();
        let bytes_read = reader.read_line(&mut line).await?;

        if bytes_read == 0 {
            break; // EOF
        }

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        if global.verbose {
            eprintln!("Received: {trimmed}");
        }

        let response = super::handle_request(trimmed, &global).await;
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
