use crate::prelude::*;
use headless_chrome::{Browser, LaunchOptions, Tab};
use std::{
    ffi::OsStr,
    process::{Command, Stdio},
    sync::{mpsc, Arc, OnceLock, Weak},
    time::{Duration, Instant},
};

use super::{fetch_with_browser, FetchConfig, FetchOutput};

struct Request {
    config: FetchConfig,
    start: Instant,
    deadline: Instant,
    reply: mpsc::SyncSender<Result<FetchOutput>>,
    runtime: Option<tokio::runtime::Handle>,
}

pub(super) fn launch_options() -> LaunchOptions<'static> {
    LaunchOptions {
        headless: false,
        args: vec![OsStr::new("--headless=new")],
        ignore_certificate_errors: false,
        ..LaunchOptions::default()
    }
}

fn launch(deadline: Instant, timeout: u64) -> Result<Browser> {
    use mcptools_core::md::{parse_chrome_major, validate_chrome_major};

    let path = std::env::var_os("CHROME")
        .map(std::path::PathBuf::from)
        .map(Ok)
        .unwrap_or_else(headless_chrome::browser::default_executable)
        .map_err(|error| {
        eyre!("Chrome not found: {error}. Install current stable Chrome. CHROME=<path> selects a binary.")
    })?;
    let mut child = Command::new(&path)
        .arg("--version")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| {
            eyre!("Cannot detect Chrome version: {error}. CHROME=<path> selects a binary.")
        })?;
    loop {
        if child.try_wait()?.is_some() {
            break;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(eyre!("Fetch timed out after {timeout} seconds"));
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let output = child.wait_with_output()?;
    if !output.status.success() {
        return Err(eyre!("Cannot detect Chrome version. Install current stable Chrome. CHROME=<path> selects a binary."));
    }
    let major = parse_chrome_major(&String::from_utf8_lossy(&output.stdout)).map_err(|error| {
        eyre!("{error}. Install current stable Chrome. CHROME=<path> selects a binary.")
    })?;
    validate_chrome_major(major).map_err(|error| eyre!(error))?;
    if Instant::now() >= deadline {
        return Err(eyre!("Fetch timed out after {timeout} seconds"));
    }
    Browser::new(LaunchOptions {
        path: Some(path),
        ..launch_options()
    })
    .map_err(|error| eyre!("Failed to launch browser: {error}"))
}

struct RequestTab(Arc<Tab>);

impl Drop for RequestTab {
    fn drop(&mut self) {
        let _ = self.0.close(true);
    }
}

fn serve(receiver: mpsc::Receiver<Request>) {
    let mut browser: Weak<Browser> = Weak::new();
    let mut keeper: Option<tokio::task::JoinHandle<()>> = None;
    let mut synchronous_browser = None;
    loop {
        let request = match receiver.recv_timeout(Duration::from_secs(10)) {
            Ok(request) => request,
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if let Some(browser) = browser.upgrade() {
                    if let Ok(tab) = browser.new_tab() {
                        let _ = tab.close(true);
                    }
                }
                continue;
            }
        };
        let result = (|| {
            if Instant::now() >= request.deadline {
                return Err(eyre!(
                    "Fetch timed out after {} seconds",
                    request.config.timeout
                ));
            }
            let active = browser
                .upgrade()
                .filter(|browser| browser.get_version().is_ok());
            if Instant::now() >= request.deadline {
                return Err(eyre!(
                    "Fetch timed out after {} seconds",
                    request.config.timeout
                ));
            }
            let active = match active {
                Some(active) => active,
                None => {
                    if let Some(keeper) = keeper.take() {
                        keeper.abort();
                    }
                    synchronous_browser.take();
                    let active = Arc::new(launch(request.deadline, request.config.timeout)?);
                    browser = Arc::downgrade(&active);
                    let owned = Arc::clone(&active);
                    if let Some(runtime) = request.runtime {
                        keeper = Some(runtime.spawn(async move {
                            std::future::pending::<()>().await;
                            drop(owned);
                        }));
                    } else {
                        synchronous_browser = Some(owned);
                    }
                    active
                }
            };
            if Instant::now() >= request.deadline {
                return Err(eyre!(
                    "Fetch timed out after {} seconds",
                    request.config.timeout
                ));
            }
            let tab = RequestTab(
                active
                    .new_tab()
                    .map_err(|error| eyre!("Failed to create new tab: {error}"))?,
            );
            fetch_with_browser(request.config, request.start, request.deadline, &tab.0)
        })();
        let _ = request.reply.send(result);
    }
}

pub(super) fn fetch(config: FetchConfig, start: Instant, deadline: Instant) -> Result<FetchOutput> {
    static WORKER: OnceLock<mpsc::SyncSender<Request>> = OnceLock::new();
    let worker = WORKER.get_or_init(|| {
        let (sender, receiver) = mpsc::sync_channel(64);
        std::thread::spawn(move || serve(receiver));
        sender
    });
    let timeout = config.timeout;
    let (reply, receiver) = mpsc::sync_channel(1);
    let runtime = tokio::runtime::Handle::try_current().ok();
    worker
        .try_send(Request {
            config,
            start,
            deadline,
            reply,
            runtime,
        })
        .map_err(|error| eyre!("Browser worker unavailable: {error}"))?;
    receiver
        .recv_timeout(deadline.saturating_duration_since(Instant::now()))
        .map_err(|_| eyre!("Fetch timed out after {timeout} seconds"))?
}
