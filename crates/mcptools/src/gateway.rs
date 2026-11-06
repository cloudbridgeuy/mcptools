mod http;
mod oauth;

pub use oauth::{run, App};

use crate::prelude::*;
use mcptools_core::gateway::{self, Cache, GatewayTool, Upstream, UpstreamTool};
use serde_json::{json, Value};
use std::path::Path;
use std::process::Stdio;
use std::sync::OnceLock;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::Mutex;

static GATEWAY: OnceLock<Gateway> = OnceLock::new();
const MESSAGE_LIMIT: u64 = 4 * 1024 * 1024;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

struct Gateway {
    tools: Vec<GatewayTool>,
    upstreams: Vec<Upstream>,
    clients: Vec<Mutex<Option<Client>>>,
}

struct StdioClient {
    child: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
    reaper: Child,
    reaper_output: ChildStdout,
    lease: Option<ChildStdin>,
    next_id: u64,
}

pub fn tools() -> &'static [GatewayTool] {
    GATEWAY.get().map_or(&[], |gateway| &gateway.tools)
}

pub async fn initialize() -> Result<()> {
    let path = crate::config::path()?;
    let Some(config) = crate::config::load(&path)? else {
        return Ok(());
    };
    let config = config.gateway;
    if config.upstreams.is_empty() {
        return Ok(());
    }
    #[cfg(not(unix))]
    if config
        .upstreams
        .iter()
        .any(|upstream| upstream.url.is_none())
    {
        return Err(eyre!("Local upstreams require Unix process supervision"));
    }
    let cache_path = mcptools_core::config::cache_path(&path)?;
    let cached = match std::fs::read(&cache_path) {
        Ok(bytes) => serde_json::from_slice::<Cache>(&bytes).ok(),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(error.into()),
    };
    let (discovered, clients, refresh) =
        if let Some(cache) = cached.filter(|cache| gateway::reusable_cache(&config, cache)) {
            let clients = config.upstreams.iter().map(|_| Mutex::new(None)).collect();
            (cache.tools, clients, false)
        } else {
            let mut discovered = Vec::new();
            let mut clients: Vec<Mutex<Option<Client>>> = Vec::new();
            for upstream in &config.upstreams {
                let mut client = match Client::start(upstream).await {
                    Ok(client) => client,
                    Err(error) => {
                        stop_clients(clients).await;
                        return Err(error);
                    }
                };
                let tools = match client.discover().await {
                    Ok(tools) => tools,
                    Err(error) => {
                        client.stop().await;
                        stop_clients(clients).await;
                        return Err(error);
                    }
                };
                discovered.push(tools);
                clients.push(Mutex::new(Some(client)));
            }
            (discovered, clients, true)
        };
    let result = (|| -> Result<Vec<GatewayTool>> {
        let tools =
            gateway::merge_tools(&config, &discovered, crate::mcp::tools::reserved_names())?;
        if refresh {
            let cache = Cache {
                version: 1,
                fingerprint: gateway::fingerprint(&config),
                tools: discovered,
            };
            let mut staged = tempfile::NamedTempFile::new_in(
                path.parent()
                    .filter(|path| !path.as_os_str().is_empty())
                    .unwrap_or(Path::new(".")),
            )?;
            serde_json::to_writer(&mut staged, &cache)?;
            staged.as_file_mut().sync_all()?;
            staged.persist(&cache_path).map_err(|error| eyre!(error))?;
        }
        Ok(tools)
    })();
    let tools = match result {
        Ok(tools) => tools,
        Err(error) => {
            stop_clients(clients).await;
            return Err(error);
        }
    };
    GATEWAY
        .set(Gateway {
            tools,
            upstreams: config.upstreams,
            clients,
        })
        .map_err(|_| eyre!("Gateway already initialized"))?;
    Ok(())
}

async fn stop_clients(clients: Vec<Mutex<Option<Client>>>) {
    for client in clients {
        if let Some(client) = client.into_inner() {
            client.stop().await;
        }
    }
}

pub async fn shutdown() {
    if let Some(gateway) = GATEWAY.get() {
        for client in &gateway.clients {
            if let Some(client) = client.lock().await.take() {
                client.stop().await;
            }
        }
    }
}

pub async fn call(name: &str, arguments: Option<Value>) -> Result<Value> {
    let gateway = GATEWAY
        .get()
        .ok_or_else(|| eyre!("Gateway not initialized"))?;
    let tool = gateway
        .tools
        .iter()
        .find(|tool| tool.name == name)
        .ok_or_else(|| eyre!("Unknown upstream tool"))?;
    let arguments = arguments.unwrap_or_else(|| json!({}));
    if !arguments.is_object() {
        return Err(eyre!("Upstream arguments must be an object"));
    }
    let mut slot = gateway.clients[tool.upstream].lock().await;
    if slot.is_none() {
        *slot = Some(Client::start(&gateway.upstreams[tool.upstream]).await?);
    }
    let mut client = slot
        .take()
        .ok_or_else(|| eyre!("Missing upstream client"))?;
    let result = client
        .request(
            "tools/call",
            json!({"name": tool.original_name, "arguments": arguments}),
        )
        .await;
    if result.is_ok() {
        *slot = Some(client);
    } else {
        client.stop().await;
    }
    result
}

enum Client {
    Stdio(Box<StdioClient>),
    Http(Box<http::Client>),
}

impl Client {
    async fn start(upstream: &Upstream) -> Result<Self> {
        if upstream.url.is_some() {
            Ok(Self::Http(Box::new(http::Client::start(upstream).await?)))
        } else {
            Ok(Self::Stdio(Box::new(StdioClient::start(upstream).await?)))
        }
    }

    async fn request(&mut self, method: &str, params: Value) -> Result<Value> {
        match self {
            Self::Stdio(client) => client.request(method, params).await,
            Self::Http(client) => client.request(method, params).await,
        }
    }

    async fn discover(&mut self) -> Result<Vec<UpstreamTool>> {
        let mut tools = Vec::new();
        let mut cursor = None;
        let mut cursors = std::collections::BTreeSet::new();
        for _ in 0..100 {
            let params = cursor
                .as_ref()
                .map_or_else(|| json!({}), |cursor| json!({"cursor": cursor}));
            let result = self.request("tools/list", params).await?;
            let page: Vec<UpstreamTool> = serde_json::from_value(result["tools"].clone())
                .map_err(|_| eyre!("Invalid upstream tools/list"))?;
            tools.extend(page);
            match result.get("nextCursor").filter(|value| !value.is_null()) {
                None => return Ok(tools),
                Some(Value::String(next)) if !next.is_empty() && cursors.insert(next.clone()) => {
                    cursor = Some(next.clone())
                }
                _ => return Err(eyre!("Invalid or repeated upstream cursor")),
            }
        }
        Err(eyre!("Upstream discovery exceeded 100 pages"))
    }

    async fn stop(self) {
        match self {
            Self::Stdio(client) => (*client).stop().await,
            Self::Http(client) => (*client).stop().await,
        }
    }
}

impl StdioClient {
    async fn start(upstream: &Upstream) -> Result<Self> {
        let mut command = Command::new(
            upstream
                .command
                .as_deref()
                .ok_or_else(|| eyre!("Missing upstream command"))?,
        );
        command
            .args(&upstream.args)
            .envs(&upstream.env)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(false);
        let mut child = command
            .spawn()
            .map_err(|_| eyre!("Cannot start upstream {}", upstream.prefix))?;
        let pid = child.id().ok_or_else(|| eyre!("Missing upstream PID"))?;
        let reaper = (|| -> Result<Child> {
            let identity = crate::chrome_reaper::inspect(pid)?
                .ok_or_else(|| eyre!("Upstream exited during launch"))?;
            Ok(Command::new(std::env::current_exe()?)
                .arg("upstream-reaper")
                .arg("--identity")
                .arg(serde_json::to_string(&identity)?)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()?)
        })();
        let mut reaper = match reaper {
            Ok(reaper) => reaper,
            Err(error) => {
                let _ = child.kill().await;
                let _ = child.wait().await;
                return Err(error);
            }
        };
        let lease = reaper.stdin.take();
        let reaper_output = reaper
            .stdout
            .take()
            .ok_or_else(|| eyre!("Missing reaper stdout"))?;
        let input = child
            .stdin
            .take()
            .ok_or_else(|| eyre!("Missing upstream stdin"))?;
        let output = BufReader::new(
            child
                .stdout
                .take()
                .ok_or_else(|| eyre!("Missing upstream stdout"))?,
        );
        let mut client = Self {
            child,
            input,
            output,
            reaper,
            reaper_output,
            lease,
            next_id: 0,
        };
        let initialized = client
            .request(
                "initialize",
                json!({
                    "protocolVersion": "2025-06-18",
                    "capabilities": {},
                    "clientInfo": {"name": "mcptools", "version": env!("MCPTOOLS_VERSION")}
                }),
            )
            .await?;
        if !matches!(
            initialized["protocolVersion"].as_str(),
            Some("2024-11-05" | "2025-03-26" | "2025-06-18")
        ) || !initialized["capabilities"]["tools"].is_object()
        {
            return Err(eyre!(
                "Upstream does not support a compatible tools protocol"
            ));
        }
        client
            .send(json!({"jsonrpc": "2.0", "method": "notifications/initialized"}))
            .await?;
        Ok(client)
    }

    async fn send(&mut self, message: Value) -> Result<()> {
        let mut bytes = serde_json::to_vec(&message)?;
        if bytes.len() as u64 > MESSAGE_LIMIT {
            return Err(eyre!("Upstream message exceeds 4 MiB"));
        }
        bytes.push(b'\n');
        self.input.write_all(&bytes).await?;
        self.input.flush().await?;
        Ok(())
    }

    async fn request(&mut self, method: &str, params: Value) -> Result<Value> {
        tokio::time::timeout(REQUEST_TIMEOUT, async {
            let result = self.exchange(method, params).await?;
            self.lease
                .as_mut()
                .ok_or_else(|| eyre!("Missing reaper lease"))?
                .write_all(&[1])
                .await?;
            let mut acknowledgement = [0];
            self.reaper_output.read_exact(&mut acknowledgement).await?;
            Ok(result)
        })
        .await
        .map_err(|_| eyre!("Upstream request timed out; outcome may be uncertain; not retried"))?
    }

    async fn exchange(&mut self, method: &str, params: Value) -> Result<Value> {
        self.next_id += 1;
        let id = self.next_id;
        self.send(json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}))
            .await?;
        loop {
            let mut bytes = Vec::new();
            let count = (&mut self.output)
                .take(MESSAGE_LIMIT + 1)
                .read_until(b'\n', &mut bytes)
                .await?;
            if count == 0 || count as u64 > MESSAGE_LIMIT || bytes.last() != Some(&b'\n') {
                return Err(eyre!(
                    "Upstream closed or exceeded the 4 MiB message limit; outcome may be uncertain"
                ));
            }
            let response: Value =
                serde_json::from_slice(&bytes).map_err(|_| eyre!("Invalid upstream JSON"))?;
            if response["jsonrpc"] != "2.0" {
                return Err(eyre!("Invalid upstream JSON-RPC version"));
            }
            if response.get("method").is_some() {
                if let Some(id) = response.get("id") {
                    let result = if response["method"] == "ping" {
                        json!({"result": {}})
                    } else {
                        json!({"error": {"code": -32601, "message": "Client capability not supported"}})
                    };
                    let mut reply = result;
                    reply["jsonrpc"] = json!("2.0");
                    reply["id"] = id.clone();
                    self.send(reply).await?;
                }
                continue;
            }
            if response["id"] != id {
                return Err(eyre!("Unexpected upstream response ID"));
            }
            if response.get("error").is_some() {
                return Err(eyre!("Upstream JSON-RPC error; not retried"));
            }
            return response
                .get("result")
                .cloned()
                .ok_or_else(|| eyre!("Missing upstream result"));
        }
    }

    async fn stop(mut self) {
        self.lease.take();
        let _ = tokio::time::timeout(Duration::from_secs(3), self.reaper.wait()).await;
        let _ = self.child.kill().await;
        let _ = self.child.wait().await;
    }
}

impl Drop for StdioClient {
    fn drop(&mut self) {
        self.lease.take();
    }
}

#[derive(Debug, clap::Args)]
pub struct ReaperArgs {
    #[arg(long)]
    identity: String,
}

fn record_tree(
    identity: &mcptools_core::chrome::ProcessIdentity,
    processes: &mut Vec<mcptools_core::chrome::ProcessIdentity>,
) -> Result<()> {
    processes.extend(crate::chrome_reaper::process_tree(identity)?);
    let mut live = Vec::new();
    for process in processes.drain(..) {
        if crate::chrome_reaper::inspect(process.pid)?.as_ref() == Some(&process)
            && !live.contains(&process)
        {
            live.push(process);
        }
    }
    *processes = live;
    Ok(())
}

pub fn reap(args: ReaperArgs) -> Result<()> {
    use std::io::{Read, Write};
    let mut identity: mcptools_core::chrome::ProcessIdentity =
        serde_json::from_str(&args.identity)?;
    let mut processes = vec![identity.clone()];
    let mut bytes = [0; 1];
    while std::io::stdin().read(&mut bytes)? != 0 {
        if let Some(current) = crate::chrome_reaper::inspect(identity.pid)?
            .filter(|current| current.started == identity.started)
        {
            identity = current;
            record_tree(&identity, &mut processes)?;
        }
        if std::io::stdout()
            .write_all(&[1])
            .and_then(|()| std::io::stdout().flush())
            .is_err()
        {
            break;
        }
    }
    #[cfg(unix)]
    {
        if crate::chrome_reaper::inspect(identity.pid)?.as_ref() == Some(&identity) {
            record_tree(&identity, &mut processes)?;
        }
        crate::chrome_reaper::signal(&identity, "-TERM")?;
        std::thread::sleep(Duration::from_millis(500));
        for process in processes.iter().rev() {
            crate::chrome_reaper::signal(process, "-TERM")?;
        }
        std::thread::sleep(Duration::from_millis(100));
        for process in processes.iter().rev() {
            crate::chrome_reaper::signal(process, "-KILL")?;
        }
    }
    Ok(())
}
