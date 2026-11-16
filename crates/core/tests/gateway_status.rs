use mcptools_core::gateway::Upstream;
use mcptools_core::gateway_status::*;
use serde_json::{json, Value};

fn upstream(value: Value) -> Upstream {
    serde_json::from_value(value).unwrap()
}

fn oauth(expires_at: Option<u64>, refreshable: bool) -> Credential {
    Credential::Valid {
        expires_at,
        refreshable,
    }
}

#[test]
fn transport_and_auth_kind_follow_config() {
    let stdio = upstream(json!({"prefix": "a", "command": "x"}));
    let open = upstream(json!({"prefix": "b", "url": "http://h/mcp"}));
    let header = upstream(
        json!({"prefix": "c", "url": "http://h/mcp", "headers": {"Authorization": "Bearer ${T}"}}),
    );
    let flow = upstream(json!({"prefix": "d", "url": "http://h/mcp", "auth": "oauth"}));
    assert_eq!(transport(&stdio), Transport::Stdio);
    assert_eq!(transport(&open), Transport::Http);
    assert_eq!(auth_kind(&stdio), AuthKind::None);
    assert_eq!(auth_kind(&open), AuthKind::None);
    assert_eq!(auth_kind(&header), AuthKind::Header);
    assert_eq!(auth_kind(&flow), AuthKind::Oauth);
}

#[test]
fn offline_states_for_oauth_credentials() {
    let now = 1000;
    let cases = [
        (Credential::Missing, State::NotLoggedIn),
        (oauth(None, false), State::Ok),
        (oauth(Some(2000), false), State::Ok),
        (oauth(Some(1000), true), State::Refreshable),
        (oauth(Some(10), true), State::Refreshable),
        (oauth(Some(10), false), State::Expired),
    ];
    for (credential, expected) in cases {
        assert_eq!(
            offline_state(AuthKind::Oauth, credential, now),
            expected,
            "{credential:?}"
        );
    }
}

#[test]
fn offline_non_oauth_is_ok_without_credentials() {
    for auth in [AuthKind::Header, AuthKind::None] {
        assert_eq!(offline_state(auth, Credential::Missing, 5), State::Ok);
    }
}

#[test]
fn probes_skip_oauth_without_usable_credentials() {
    assert!(!needs_probe(AuthKind::Oauth, Credential::Missing, 5));
    assert!(!needs_probe(AuthKind::Oauth, oauth(Some(1), false), 5));
    assert!(needs_probe(AuthKind::Oauth, oauth(Some(1), true), 5));
    assert!(needs_probe(AuthKind::Oauth, oauth(None, false), 5));
    assert!(needs_probe(AuthKind::Header, Credential::Missing, 5));
    assert!(needs_probe(AuthKind::None, Credential::Missing, 5));
}

#[test]
fn live_states_map_probe_outcomes() {
    let valid = oauth(None, false);
    let expired = oauth(Some(1), true);
    let rejected = Probe::Rejected {
        status: Some(403),
        message: Some("app not enabled for MCP".into()),
    };
    assert_eq!(
        live_state(AuthKind::Oauth, valid, Probe::Initialized, 5),
        State::Ok
    );
    assert_eq!(
        live_state(AuthKind::Oauth, expired, Probe::Initialized, 5),
        State::Ok
    );
    assert_eq!(
        live_state(AuthKind::None, Credential::Missing, Probe::Unreachable, 5),
        State::Unreachable
    );
    assert_eq!(
        live_state(AuthKind::Oauth, expired, Probe::LoginRequired, 5),
        State::Expired
    );
    assert_eq!(
        live_state(AuthKind::Oauth, valid, Probe::LoginRequired, 5),
        State::Expired
    );
    assert_eq!(
        live_state(
            AuthKind::Oauth,
            Credential::Missing,
            Probe::LoginRequired,
            5
        ),
        State::NotLoggedIn
    );
    assert_eq!(
        live_state(AuthKind::Header, Credential::Missing, rejected, 5),
        State::Failed("HTTP 403: app not enabled for MCP".into())
    );
    assert_eq!(
        live_state(
            AuthKind::Header,
            Credential::Missing,
            Probe::Rejected {
                status: Some(500),
                message: None
            },
            5
        ),
        State::Failed("HTTP 500".into())
    );
    assert_eq!(
        live_state(AuthKind::None, Credential::Missing, Probe::Incompatible, 5),
        State::Failed("incompatible MCP protocol".into())
    );
    assert_eq!(
        live_state(AuthKind::Oauth, Credential::Missing, Probe::Initialized, 5),
        State::NotLoggedIn
    );
    assert_eq!(
        live_state(
            AuthKind::Oauth,
            oauth(Some(1), false),
            Probe::Initialized,
            5
        ),
        State::Expired
    );
}

#[test]
fn safe_reason_extracts_message_and_redacts_secrets() {
    let secrets = vec!["tok-123".to_owned()];
    assert_eq!(
        safe_reason(
            403,
            r#"{"error":{"message":"app not enabled for MCP"}}"#,
            &secrets
        ),
        "app not enabled for MCP"
    );
    assert_eq!(
        safe_reason(400, r#"{"message":"bad tok-123 here"}"#, &secrets),
        "bad [redacted] here"
    );
    assert_eq!(safe_reason(502, "<html>gateway</html>", &secrets), "");
    assert_eq!(safe_reason(401, r#"{"error":"nope"}"#, &secrets), "nope");
    let long = format!(r#"{{"message":"{}"}}"#, "x".repeat(500));
    assert!(safe_reason(400, &long, &secrets).chars().count() <= 200);
    assert_eq!(
        safe_reason(400, "{\"message\":\"a\\nb\\u0007c\"}", &secrets),
        "a b c"
    );
}

fn rows() -> Vec<Row> {
    vec![
        row(
            &upstream(json!({"prefix": "local", "command": "x"})),
            State::Ok,
        ),
        row(
            &upstream(json!({"prefix": "slack", "url": "http://h/mcp", "auth": "oauth"})),
            State::Failed("HTTP 403: app not enabled for MCP".into()),
        ),
        row(
            &upstream(
                json!({"prefix": "gh", "url": "http://h/mcp", "headers": {"Authorization": "Bearer s3cret"}}),
            ),
            State::Unreachable,
        ),
        row(
            &upstream(json!({"prefix": "sentry", "url": "http://h/mcp", "auth": "oauth"})),
            State::NotLoggedIn,
        ),
    ]
}

#[test]
fn rows_carry_labels_and_reasons() {
    let rows = rows();
    assert_eq!(rows[0].name, "local");
    assert_eq!(rows[0].transport, Transport::Stdio);
    assert_eq!(rows[0].auth, AuthKind::None);
    assert_eq!(rows[0].state, "ok");
    assert_eq!(rows[0].reason, None);
    assert_eq!(rows[1].state, "failed");
    assert_eq!(
        rows[1].reason.as_deref(),
        Some("HTTP 403: app not enabled for MCP")
    );
    assert_eq!(rows[2].state, "unreachable");
    assert_eq!(rows[3].state, "not_logged_in");
}

#[test]
fn all_ok_requires_every_row_ok() {
    let rows = rows();
    assert!(!all_ok(&rows));
    assert!(all_ok(&rows[..1]));
    assert!(all_ok(&[]));
}

#[test]
fn table_has_one_line_per_upstream_plus_header() {
    let table = render_table(&rows());
    let lines: Vec<_> = table.lines().collect();
    assert_eq!(lines.len(), 5, "{table}");
    assert!(lines[0].starts_with("NAME"));
    assert!(lines[2].contains("slack") && lines[2].contains("http") && lines[2].contains("oauth"));
    assert!(lines[2].contains("failed") && lines[2].contains("app not enabled for MCP"));
    assert!(!table.contains("s3cret"));
}

#[test]
fn json_is_an_array_of_rows() {
    let value: Value = serde_json::from_str(&render_json(&rows())).unwrap();
    let array = value.as_array().unwrap();
    assert_eq!(array.len(), 4);
    assert_eq!(array[1]["name"], "slack");
    assert_eq!(array[1]["transport"], "http");
    assert_eq!(array[1]["auth"], "oauth");
    assert_eq!(array[1]["state"], "failed");
    assert_eq!(array[1]["reason"], "HTTP 403: app not enabled for MCP");
    assert!(array[0]["reason"].is_null());
}

#[test]
fn login_plan_skips_ok_and_flags_non_oauth() {
    let plan = login_plan(&rows());
    assert_eq!(
        plan,
        vec![
            ("local".to_owned(), LoginAction::SkipOk),
            ("slack".to_owned(), LoginAction::Login),
            ("gh".to_owned(), LoginAction::NotRenewable),
            ("sentry".to_owned(), LoginAction::Login),
        ]
    );
}

#[test]
fn login_summary_lists_each_outcome_and_success_needs_no_failure() {
    let results = vec![
        ("a".to_owned(), LoginResult::LoggedIn),
        (
            "b".to_owned(),
            LoginResult::Failed("callback timed out".into()),
        ),
        ("c".to_owned(), LoginResult::SkippedOk),
        ("d".to_owned(), LoginResult::NotRenewable),
    ];
    let summary = render_login_summary(&results);
    assert_eq!(summary.lines().count(), 4, "{summary}");
    assert!(summary.contains("a") && summary.contains("logged in"));
    assert!(summary.contains("b") && summary.contains("failed: callback timed out"));
    assert!(summary.contains("c") && summary.contains("skipped"));
    assert!(summary.contains("d") && summary.contains("not renewable"));
    assert!(!login_succeeded(&results));
    assert!(login_succeeded(&results[2..]));
    assert!(login_succeeded(&results[..1]));
}
