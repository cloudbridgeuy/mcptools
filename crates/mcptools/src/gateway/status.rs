use super::{http, oauth};
use crate::prelude::*;
use mcptools_core::gateway::{self, Upstream};
use mcptools_core::gateway_status::{
    self as status, AuthKind, Credential, LoginAction, LoginResult, Probe, Row,
};

fn classify(error: &color_eyre::eyre::Report) -> Probe {
    if error.downcast_ref::<oauth::LoginRequired>().is_some() {
        return Probe::LoginRequired;
    }
    if let Some(rejected) = error.downcast_ref::<http::Rejected>() {
        return Probe::Rejected {
            status: Some(rejected.status),
            message: Some(rejected.reason.clone()),
        };
    }
    if error.downcast_ref::<gateway::Error>().is_some() {
        return Probe::Rejected {
            status: None,
            message: Some(error.to_string()),
        };
    }
    if error.to_string().contains("compatible") {
        return Probe::Incompatible;
    }
    Probe::Unreachable
}

async fn probe(upstream: &Upstream) -> Probe {
    match tokio::time::timeout(super::request_timeout(), super::Client::start(upstream)).await {
        Ok(Ok(client)) => {
            client.stop().await;
            Probe::Initialized
        }
        Ok(Err(error)) => classify(&error),
        Err(_) => Probe::Unreachable,
    }
}

async fn collect(config: &gateway::Config, live: bool) -> Vec<Row> {
    let now = oauth::now();
    let mut rows = Vec::new();
    for upstream in &config.upstreams {
        let auth = status::auth_kind(upstream);
        let credential = if auth == AuthKind::Oauth {
            oauth::credential(upstream)
        } else {
            Credential::Missing
        };
        let state = if live && status::needs_probe(auth, credential, now) {
            status::live_state(auth, credential, probe(upstream).await, now)
        } else {
            status::offline_state(auth, credential, now)
        };
        rows.push(status::row(upstream, state));
    }
    rows
}

pub(super) async fn report(config: &gateway::Config, json: bool, offline: bool) -> Result<()> {
    let rows = collect(config, !offline).await;
    if json {
        std::println!("{}", status::render_json(&rows));
    } else {
        std::println!("{}", status::render_table(&rows));
    }
    if status::all_ok(&rows) {
        Ok(())
    } else {
        Err(eyre!("Some gateway upstreams are not ok"))
    }
}

pub(super) async fn login_all(config: &gateway::Config) -> Result<()> {
    let rows = collect(config, true).await;
    let mut results = Vec::new();
    for (name, action) in status::login_plan(&rows) {
        let result = match action {
            LoginAction::SkipOk => LoginResult::SkippedOk,
            LoginAction::NotRenewable => LoginResult::NotRenewable,
            LoginAction::Login => {
                let upstream = config
                    .upstreams
                    .iter()
                    .find(|upstream| upstream.prefix == name)
                    .ok_or_else(|| eyre!("Unknown OAuth upstream"))?;
                match oauth::login_upstream(upstream).await {
                    Ok(()) => LoginResult::LoggedIn,
                    Err(error) => LoginResult::Failed(error.to_string()),
                }
            }
        };
        results.push((name, result));
    }
    std::println!("{}", status::render_login_summary(&results));
    if status::login_succeeded(&results) {
        Ok(())
    } else {
        Err(eyre!("Some gateway logins failed"))
    }
}
