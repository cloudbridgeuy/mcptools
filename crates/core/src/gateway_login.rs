use crate::gateway::{Config, Upstream};
use crate::gateway_status::Row;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

const REASON_LIMIT: usize = 200;

#[derive(Debug, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StatusArgs {
    #[serde(default)]
    pub offline: bool,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct StatusOutput {
    pub upstreams: Vec<Row>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LoginArgs {
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoginState {
    Starting,
    Pending { url: String },
    Ok,
    Failed { reason: String },
}

#[derive(Debug, PartialEq, Eq)]
pub enum Begin {
    Spawn,
    Reuse(LoginState),
}

#[derive(Debug, Default)]
pub struct LoginBook {
    entries: BTreeMap<String, LoginState>,
}

impl LoginBook {
    pub const fn new() -> Self {
        Self {
            entries: BTreeMap::new(),
        }
    }

    pub fn begin(&mut self, name: &str) -> Begin {
        match self.entries.get(name) {
            Some(state @ (LoginState::Starting | LoginState::Pending { .. })) => {
                Begin::Reuse(state.clone())
            }
            _ => {
                self.entries.insert(name.to_owned(), LoginState::Starting);
                Begin::Spawn
            }
        }
    }

    pub fn pending(&mut self, name: &str, url: String) {
        if matches!(self.entries.get(name), Some(LoginState::Starting)) {
            self.entries
                .insert(name.to_owned(), LoginState::Pending { url });
        }
    }

    pub fn finish(&mut self, name: &str, outcome: Result<(), String>) {
        let state = match outcome {
            Ok(()) => LoginState::Ok,
            Err(reason) => LoginState::Failed {
                reason: sanitize(&reason),
            },
        };
        self.entries.insert(name.to_owned(), state);
    }

    pub fn state(&self, name: &str) -> Option<&LoginState> {
        self.entries.get(name)
    }
}

fn sanitize(reason: &str) -> String {
    reason
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .take(REASON_LIMIT)
        .collect::<String>()
        .trim()
        .to_owned()
}

#[derive(Debug, PartialEq, Eq)]
pub struct Refusal(pub String);

pub fn oauth_upstream<'a>(config: &'a Config, name: &str) -> Result<&'a Upstream, Refusal> {
    let upstream = config
        .upstreams
        .iter()
        .find(|upstream| upstream.prefix == name)
        .ok_or_else(|| Refusal("Unknown OAuth upstream".to_owned()))?;
    if upstream.auth.as_deref() == Some("oauth") && upstream.url.is_some() {
        Ok(upstream)
    } else {
        Err(Refusal("Unknown OAuth upstream".to_owned()))
    }
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct LoginOutput {
    pub name: String,
    pub state: String,
    pub url: Option<String>,
    pub reason: Option<String>,
}

pub fn login_output(name: &str, state: &LoginState) -> LoginOutput {
    let (label, url, reason) = match state {
        LoginState::Starting => ("pending", None, None),
        LoginState::Pending { url } => ("pending", Some(url.clone()), None),
        LoginState::Ok => ("ok", None, None),
        LoginState::Failed { reason } => ("failed", None, Some(reason.clone())),
    };
    LoginOutput {
        name: name.to_owned(),
        state: label.to_owned(),
        url,
        reason,
    }
}

pub fn overlay(rows: Vec<Row>, book: &LoginBook) -> Vec<Row> {
    rows.into_iter()
        .map(|mut row| {
            match book.state(&row.name) {
                Some(LoginState::Starting | LoginState::Pending { .. }) => {
                    row.state = "pending".to_owned();
                    row.reason = None;
                }
                Some(LoginState::Failed { reason }) if row.state != "ok" => {
                    row.state = "failed".to_owned();
                    row.reason = Some(reason.clone());
                }
                _ => {}
            }
            row
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gateway_status::{AuthKind, Transport};

    fn row(name: &str, state: &str) -> Row {
        Row {
            name: name.to_owned(),
            transport: Transport::Http,
            auth: AuthKind::Oauth,
            state: state.to_owned(),
            reason: None,
        }
    }

    #[test]
    fn begin_reuses_unfinished_login_only() {
        let mut book = LoginBook::default();
        assert_eq!(book.begin("a"), Begin::Spawn);
        assert_eq!(book.begin("a"), Begin::Reuse(LoginState::Starting));
        book.pending("a", "https://x".to_owned());
        assert_eq!(
            book.begin("a"),
            Begin::Reuse(LoginState::Pending {
                url: "https://x".to_owned()
            })
        );
        book.finish("a", Err("boom".to_owned()));
        assert_eq!(book.begin("a"), Begin::Spawn);
    }

    #[test]
    fn failed_reason_is_single_line_and_bounded() {
        let mut book = LoginBook::default();
        book.finish("a", Err(format!("x\ny{}", "z".repeat(500))));
        let Some(LoginState::Failed { reason }) = book.state("a") else {
            panic!("expected failed");
        };
        assert!(!reason.contains('\n'));
        assert_eq!(reason.chars().count(), REASON_LIMIT);
    }

    #[test]
    fn overlay_marks_pending_and_failed_but_keeps_ok_rows() {
        let mut book = LoginBook::default();
        book.begin("p");
        book.finish("f", Err("denied".to_owned()));
        book.finish("g", Err("denied".to_owned()));
        let rows = overlay(
            vec![
                row("p", "not_logged_in"),
                row("f", "not_logged_in"),
                row("g", "ok"),
            ],
            &book,
        );
        assert_eq!(rows[0].state, "pending");
        assert_eq!(rows[1].state, "failed");
        assert_eq!(rows[1].reason.as_deref(), Some("denied"));
        assert_eq!(rows[2].state, "ok");
    }

    #[test]
    fn output_labels_states() {
        assert_eq!(login_output("a", &LoginState::Starting).state, "pending");
        let pending = login_output(
            "a",
            &LoginState::Pending {
                url: "https://x".to_owned(),
            },
        );
        assert_eq!(pending.url.as_deref(), Some("https://x"));
        assert_eq!(login_output("a", &LoginState::Ok).state, "ok");
    }
}
