use crate::gateway::Upstream;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Transport {
    Stdio,
    Http,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthKind {
    Oauth,
    Header,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Credential {
    Missing,
    Valid {
        expires_at: Option<u64>,
        refreshable: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Probe {
    Initialized,
    Unreachable,
    LoginRequired,
    Rejected {
        status: Option<u16>,
        message: Option<String>,
    },
    Incompatible,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum State {
    Ok,
    Expired,
    Refreshable,
    NotLoggedIn,
    Failed(String),
    Unreachable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Row {
    pub name: String,
    pub transport: Transport,
    pub auth: AuthKind,
    pub state: String,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoginAction {
    Login,
    SkipOk,
    NotRenewable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoginResult {
    LoggedIn,
    Failed(String),
    SkippedOk,
    NotRenewable,
}

const REASON_LIMIT: usize = 200;

pub fn transport(upstream: &Upstream) -> Transport {
    if upstream.url.is_some() {
        Transport::Http
    } else {
        Transport::Stdio
    }
}

pub fn auth_kind(upstream: &Upstream) -> AuthKind {
    if upstream.auth.as_deref() == Some("oauth") {
        AuthKind::Oauth
    } else if upstream.headers.is_empty() {
        AuthKind::None
    } else {
        AuthKind::Header
    }
}

fn expired(credential: Credential, now: u64) -> bool {
    matches!(credential, Credential::Valid { expires_at: Some(at), .. } if at <= now)
}

fn refreshable(credential: Credential) -> bool {
    matches!(
        credential,
        Credential::Valid {
            refreshable: true,
            ..
        }
    )
}

pub fn offline_state(auth: AuthKind, credential: Credential, now: u64) -> State {
    if auth != AuthKind::Oauth {
        return State::Ok;
    }
    match credential {
        Credential::Missing => State::NotLoggedIn,
        _ if !expired(credential, now) => State::Ok,
        _ if refreshable(credential) => State::Refreshable,
        _ => State::Expired,
    }
}

pub fn needs_probe(auth: AuthKind, credential: Credential, now: u64) -> bool {
    auth != AuthKind::Oauth
        || matches!(
            offline_state(auth, credential, now),
            State::Ok | State::Refreshable
        )
}

pub fn live_state(auth: AuthKind, credential: Credential, probe: Probe, now: u64) -> State {
    if auth == AuthKind::Oauth && !needs_probe(auth, credential, now) {
        return offline_state(auth, credential, now);
    }
    match probe {
        Probe::Initialized => State::Ok,
        Probe::Unreachable => State::Unreachable,
        Probe::LoginRequired if credential == Credential::Missing => State::NotLoggedIn,
        Probe::LoginRequired => State::Expired,
        Probe::Incompatible => State::Failed("incompatible MCP protocol".to_owned()),
        Probe::Rejected { status, message } => {
            let head = status.map_or_else(|| "rejected".to_owned(), |code| format!("HTTP {code}"));
            State::Failed(match message.filter(|text| !text.is_empty()) {
                Some(text) => format!("{head}: {text}"),
                None => head,
            })
        }
    }
}

pub fn safe_reason(_status: u16, body: &str, secrets: &[String]) -> String {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(body) else {
        return String::new();
    };
    let text = [
        &value["error"]["message"],
        &value["message"],
        &value["error"],
        &value["error_description"],
    ]
    .into_iter()
    .find_map(|candidate| candidate.as_str())
    .unwrap_or_default();
    let mut text = text.to_owned();
    for secret in secrets.iter().filter(|secret| !secret.is_empty()) {
        text = text.replace(secret.as_str(), "[redacted]");
    }
    text.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .take(REASON_LIMIT)
        .collect::<String>()
        .trim()
        .to_owned()
}

impl State {
    fn label(&self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Expired => "expired",
            Self::Refreshable => "refreshable",
            Self::NotLoggedIn => "not_logged_in",
            Self::Failed(_) => "failed",
            Self::Unreachable => "unreachable",
        }
    }
}

pub fn row(upstream: &Upstream, state: State) -> Row {
    Row {
        name: upstream.prefix.clone(),
        transport: transport(upstream),
        auth: auth_kind(upstream),
        state: state.label().to_owned(),
        reason: match state {
            State::Failed(reason) => Some(reason),
            _ => None,
        },
    }
}

pub fn all_ok(rows: &[Row]) -> bool {
    rows.iter().all(|row| row.state == "ok")
}

fn word<T: Serialize>(value: &T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_default()
}

pub fn render_table(rows: &[Row]) -> String {
    let cells: Vec<[String; 4]> = rows
        .iter()
        .map(|row| {
            let state = match &row.reason {
                Some(reason) if !reason.is_empty() => format!("{}: {reason}", row.state),
                _ => row.state.clone(),
            };
            [
                row.name.clone(),
                word(&row.transport),
                word(&row.auth),
                state,
            ]
        })
        .collect();
    let header = ["NAME", "TRANSPORT", "AUTH", "STATE"].map(str::to_owned);
    let widths: Vec<usize> = (0..3)
        .map(|column| {
            std::iter::once(&header)
                .chain(&cells)
                .map(|line| line[column].chars().count())
                .max()
                .unwrap_or(0)
        })
        .collect();
    std::iter::once(&header)
        .chain(&cells)
        .map(|line| {
            format!(
                "{:<w0$}  {:<w1$}  {:<w2$}  {}",
                line[0],
                line[1],
                line[2],
                line[3],
                w0 = widths[0],
                w1 = widths[1],
                w2 = widths[2]
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn render_json(rows: &[Row]) -> String {
    serde_json::to_string_pretty(rows).unwrap_or_else(|_| "[]".to_owned())
}

pub fn login_plan(rows: &[Row]) -> Vec<(String, LoginAction)> {
    rows.iter()
        .map(|row| {
            let action = match (row.state.as_str(), row.auth) {
                ("ok", _) => LoginAction::SkipOk,
                (_, AuthKind::Oauth) => LoginAction::Login,
                _ => LoginAction::NotRenewable,
            };
            (row.name.clone(), action)
        })
        .collect()
}

pub fn render_login_summary(results: &[(String, LoginResult)]) -> String {
    results
        .iter()
        .map(|(name, result)| match result {
            LoginResult::LoggedIn => format!("{name}: logged in"),
            LoginResult::Failed(reason) => format!("{name}: failed: {reason}"),
            LoginResult::SkippedOk => format!("{name}: skipped (ok)"),
            LoginResult::NotRenewable => format!("{name}: not renewable by login"),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn login_succeeded(results: &[(String, LoginResult)]) -> bool {
    results
        .iter()
        .all(|(_, result)| !matches!(result, LoginResult::Failed(_)))
}
