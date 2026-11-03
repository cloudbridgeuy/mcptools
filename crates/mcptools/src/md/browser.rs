use crate::prelude::*;
use headless_chrome::{Browser, LaunchOptions, Tab};
use std::{
    ffi::OsStr,
    process::{Command, Stdio},
    sync::{mpsc, Arc, Condvar, Mutex, OnceLock, Weak},
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
    let guard = crate::chrome_reaper::LaunchGuard::prepare()?;
    let browser = Browser::new(LaunchOptions {
        path: Some(path),
        user_data_dir: Some(guard.profile.clone()),
        ..launch_options()
    })
    .map_err(|error| eyre!("Failed to launch browser: {error}"))?;
    guard.launched(
        browser
            .get_process_id()
            .ok_or_else(|| eyre!("Missing Chrome pid"))?,
    )?;
    Ok(browser)
}

struct RequestTab(Arc<Tab>);

#[derive(Default)]
struct Lifecycle {
    stopped: bool,
    stdio: bool,
    busy: bool,
    browser: Option<Arc<Browser>>,
}

fn lifecycle() -> &'static (Mutex<Lifecycle>, Condvar) {
    static LIFECYCLE: OnceLock<(Mutex<Lifecycle>, Condvar)> = OnceLock::new();
    LIFECYCLE.get_or_init(|| (Mutex::new(Lifecycle::default()), Condvar::new()))
}

pub(super) fn stopped() -> bool {
    lifecycle()
        .0
        .lock()
        .map(|state| state.stopped)
        .unwrap_or(true)
}

pub(crate) struct Shutdown;

struct Busy;

impl Drop for Busy {
    fn drop(&mut self) {
        let (lock, changed) = lifecycle();
        if let Ok(mut state) = lock.lock() {
            state.busy = false;
            changed.notify_all();
        }
    }
}

impl Shutdown {
    pub(crate) fn new() -> Self {
        lifecycle().0.lock().expect("browser lifecycle").stdio = true;
        Self
    }
}

impl Drop for Shutdown {
    fn drop(&mut self) {
        let (lock, changed) = lifecycle();
        let Ok(mut state) = lock.lock() else { return };
        state.stopped = true;
        while state.busy && state.browser.is_none() {
            state = changed.wait(state).expect("browser lifecycle");
        }
        let browser = state.browser.take();
        drop(state);
        if let Some(browser) = &browser {
            if let Ok(url) = browser.get_ws_url().parse() {
                if let Ok(transport) = headless_chrome::browser::transport::Transport::new(
                    url,
                    browser.get_process_id(),
                    Duration::from_secs(2),
                ) {
                    let _ = transport.call_method_on_browser(
                        headless_chrome::protocol::cdp::Browser::Close(None),
                    );
                    transport.shutdown();
                }
            }
        }
        let mut state = lock.lock().expect("browser lifecycle");
        while state.busy {
            state = changed.wait(state).expect("browser lifecycle");
        }
        drop(state);
        drop(browser);
    }
}

impl Drop for RequestTab {
    fn drop(&mut self) {
        if !stopped() {
            let _ = self.0.close(true);
        }
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
                let busy = {
                    let mut state = lifecycle().0.lock().expect("browser lifecycle");
                    if state.stopped {
                        break;
                    }
                    state.busy = true;
                    Busy
                };
                if let Some(browser) = browser.upgrade() {
                    if let Ok(tab) = browser.new_tab() {
                        let _ = tab.close(true);
                    }
                }
                drop(busy);
                continue;
            }
        };
        {
            let (lock, _) = lifecycle();
            let mut state = lock.lock().expect("browser lifecycle");
            if state.stopped {
                let _ = request.reply.send(Err(eyre!("Browser shutdown")));
                continue;
            }
            state.busy = true;
        }
        let busy = Busy;
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
                    let (lock, changed) = lifecycle();
                    let mut state = lock
                        .lock()
                        .map_err(|_| eyre!("Browser lifecycle unavailable"))?;
                    if state.stopped {
                        return Err(eyre!("Browser shutdown"));
                    }
                    if state.stdio {
                        state.browser = Some(owned);
                        changed.notify_all();
                    } else if let Some(runtime) = request.runtime {
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
        drop(busy);
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

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn unwind_closes_owned_browser() {
        let path = match headless_chrome::browser::default_executable() {
            Ok(path) => path,
            Err(error) => {
                std::eprintln!("skipping unwind_closes_owned_browser: Chrome not found: {error}");
                return;
            }
        };
        let output = Command::new(path)
            .arg("--version")
            .output()
            .expect("Chrome version");
        if let Err(error) =
            mcptools_core::md::parse_chrome_major(&String::from_utf8_lossy(&output.stdout))
                .and_then(mcptools_core::md::validate_chrome_major)
        {
            std::eprintln!("skipping unwind_closes_owned_browser: {error}");
            return;
        }
        let browser =
            Arc::new(launch(Instant::now() + Duration::from_secs(20), 20).expect("launch Chrome"));
        let pid = browser
            .get_process_id()
            .expect("browser process")
            .to_string();
        lifecycle().0.lock().expect("browser lifecycle").browser = Some(browser);
        let result = std::panic::catch_unwind(|| {
            let _shutdown = Shutdown::new();
            panic!("test unwind");
        });
        assert!(result.is_err());
        assert!(!Command::new("kill")
            .args(["-0", &pid])
            .stderr(Stdio::null())
            .status()
            .expect("check Chrome")
            .success());
    }
}
