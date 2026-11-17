use super::{oauth, GATEWAY};
use crate::prelude::*;
use mcptools_core::gateway::Upstream;
use mcptools_core::gateway_login::{self as login, Begin, LoginBook, LoginOutput, LoginState};
use mcptools_core::gateway_status::Row;
use std::sync::{Mutex, PoisonError};
use std::time::Duration;
use tokio::sync::Notify;
use tokio::time::Instant;

static LOGINS: Mutex<LoginBook> = Mutex::new(LoginBook::new());
static CHANGED: Notify = Notify::const_new();
const URL_WAIT: Duration = Duration::from_secs(5);

fn with_book<T>(change: impl FnOnce(&mut LoginBook) -> T) -> T {
    change(&mut LOGINS.lock().unwrap_or_else(PoisonError::into_inner))
}

pub(super) fn overlay(rows: Vec<Row>) -> Vec<Row> {
    with_book(|book| login::overlay(rows, book))
}

async fn run(upstream: Upstream) {
    let name = upstream.prefix.clone();
    let outcome = async {
        let (url, finish) = oauth::begin_login(&upstream)
            .await
            .map_err(|error| error.to_string())?;
        with_book(|book| book.pending(&name, url.clone()));
        CHANGED.notify_waiters();
        oauth::open_browser(&url);
        finish.await.map_err(|error| error.to_string())?;
        super::reconnect(&name)
            .await
            .map_err(|error| format!("Logged in but reconnect failed: {error}"))
    }
    .await;
    with_book(|book| book.finish(&name, outcome));
    CHANGED.notify_waiters();
}

pub(super) async fn start(name: &str) -> Result<LoginOutput> {
    let gateway = GATEWAY
        .get()
        .ok_or_else(|| eyre!("Gateway not initialized"))?;
    let upstream = login::oauth_upstream(&gateway.config, name)
        .map_err(|refusal| eyre!(refusal.0))?
        .clone();
    if with_book(|book| book.begin(name)) == Begin::Spawn {
        tokio::spawn(run(upstream));
    }
    let deadline = Instant::now() + URL_WAIT;
    loop {
        let notified = CHANGED.notified();
        tokio::pin!(notified);
        notified.as_mut().enable();
        let state = with_book(|book| book.state(name).cloned()).unwrap_or(LoginState::Starting);
        if state != LoginState::Starting
            || tokio::time::timeout_at(deadline, notified).await.is_err()
        {
            return Ok(login::login_output(name, &state));
        }
    }
}
