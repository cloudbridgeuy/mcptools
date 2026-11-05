use std::path::Path;
use std::process::Command;

fn command(home: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_mcptools"));
    command
        .env("HOME", home)
        .env_remove("XDG_CONFIG_HOME")
        .env_remove("MCPTOOLS_CONFIG")
        .current_dir(home);
    command
}

#[test]
fn default_config_path_and_xdg_resolution() {
    let home = tempfile::tempdir().unwrap();
    let default = command(home.path())
        .args(["config", "path"])
        .output()
        .unwrap();
    assert!(default.status.success());
    assert_eq!(
        String::from_utf8(default.stdout).unwrap().trim(),
        home.path()
            .join(".config/mcptools/config.toml")
            .to_str()
            .unwrap()
    );
    let xdg = home.path().join("custom-config");
    let resolved = command(home.path())
        .env("XDG_CONFIG_HOME", &xdg)
        .args(["config", "path"])
        .output()
        .unwrap();
    assert!(resolved.status.success());
    assert_eq!(
        String::from_utf8(resolved.stdout).unwrap().trim(),
        xdg.join("mcptools/config.toml").to_str().unwrap()
    );
}

#[test]
fn config_file_override_precedes_default_and_resolves_relative_paths() {
    let home = tempfile::tempdir().unwrap();
    let overridden = command(home.path())
        .env("MCPTOOLS_CONFIG", "other.toml")
        .args(["config", "path"])
        .output()
        .unwrap();
    assert!(overridden.status.success());
    assert_eq!(
        String::from_utf8(overridden.stdout).unwrap().trim(),
        std::fs::canonicalize(home.path())
            .unwrap()
            .join("other.toml")
            .to_str()
            .unwrap()
    );
}

#[test]
fn missing_config_uses_defaults_without_writing_files() {
    let home = tempfile::tempdir().unwrap();
    let checked = command(home.path())
        .args(["config", "check"])
        .output()
        .unwrap();
    assert!(checked.status.success());
    assert!(String::from_utf8_lossy(&checked.stdout).contains("defaults apply"));
    let catalog = command(home.path())
        .args(["mcp", "catalog"])
        .output()
        .unwrap();
    assert!(catalog.status.success());
    assert!(String::from_utf8_lossy(&catalog.stdout).contains("jira_search"));
    assert_eq!(std::fs::read_dir(home.path()).unwrap().count(), 0);
    let checked = command(home.path())
        .env("MCPTOOLS_CONFIG", "missing.toml")
        .args(["config", "check"])
        .output()
        .unwrap();
    assert!(checked.status.success());
}

#[test]
fn toml_gateway_section_and_defaults_validate_without_spawning() {
    let home = tempfile::tempdir().unwrap();
    let directory = home.path().join(".config/mcptools");
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(directory.join("config.toml"), "[[gateway.upstreams]]\nprefix = 'context7'\ncommand = 'executable-must-not-run'\nargs = ['-y', '@upstash/context7-mcp']\nreadOnly = ['resolve-library-id', 'query-docs']\n[gateway.upstreams.env]\nTOKEN = 'literal'\n").unwrap();
    let checked = command(home.path())
        .args(["config", "check"])
        .output()
        .unwrap();
    assert!(
        checked.status.success(),
        "{}",
        String::from_utf8_lossy(&checked.stderr)
    );
    assert!(String::from_utf8_lossy(&checked.stdout).contains("ok:"));
    assert_eq!(std::fs::read_dir(&directory).unwrap().count(), 1);
}

#[test]
fn punctuated_tool_names_have_quoted_callable_declarations() {
    let text = mcptools_core::ts_decl::tool_declaration(
        "context7_resolve-library-id",
        "Read tool",
        &serde_json::json!({"type":"object"}),
        &serde_json::json!({"type":"object"}),
    );
    assert!(text.contains("interface Context7ResolveLibraryIdTool"));
    assert!(text.contains("\"context7_resolve-library-id\"(input:"));
    assert!(!text.contains("declare function context7_resolve-library-id"));
}

#[test]
fn config_check_reports_path_key_and_parse_location_without_secrets() {
    let home = tempfile::tempdir().unwrap();
    let path = home.path().join("broken.toml");
    for (content, expected) in [
        (
            "[[gateway.upstreams]]\nprefix = 'bad_prefix'\ncommand = 'secret-value'\n",
            "prefix",
        ),
        ("[gateway]\nunexpected = []\n", "unexpected"),
        ("[[gateway.upstreams]\n", "line"),
    ] {
        std::fs::write(&path, content).unwrap();
        let checked = command(home.path())
            .env("MCPTOOLS_CONFIG", &path)
            .args(["config", "check"])
            .output()
            .unwrap();
        assert!(!checked.status.success());
        let error = String::from_utf8_lossy(&checked.stderr);
        assert!(error.contains(path.to_str().unwrap()), "{error}");
        assert!(error.contains(expected), "{error}");
        assert!(!error.contains("secret-value"));
    }
}

#[test]
fn atlassian_file_only_and_invalid_fields() {
    let home = tempfile::tempdir().unwrap();
    let path = home.path().join("config.toml");
    std::fs::write(&path, "[atlassian]\nbase_url = 'https://example.test'\nemail = 'a@example.test'\napi_token = 'secret-value'\n[atlassian.jira]\nboard_id = 42\n[atlassian.confluence]\nemail = 'b@example.test'\n[atlassian.bitbucket]\nusername = 'user'\napp_password = 'secret-value'\nbase_url = 'https://bitbucket.test'\n").unwrap();
    let checked = command(home.path())
        .env("MCPTOOLS_CONFIG", &path)
        .args(["config", "check"])
        .output()
        .unwrap();
    assert!(
        checked.status.success(),
        "{}",
        String::from_utf8_lossy(&checked.stderr)
    );
    std::fs::write(&path, "[atlassian.jira]\nboard_id = 'secret-value'\n").unwrap();
    let checked = command(home.path())
        .env("MCPTOOLS_CONFIG", &path)
        .args(["config", "check"])
        .output()
        .unwrap();
    assert!(!checked.status.success());
    assert!(!String::from_utf8_lossy(&checked.stderr).contains("secret-value"));
}

#[test]
fn linear_missing_key_error_names_variable() {
    let home = tempfile::tempdir().unwrap();
    let output = command(home.path())
        .env("MCPTOOLS_CONFIG", home.path().join("missing.toml"))
        .env_remove("LINEAR_API_KEY")
        .args(["linear", "auth", "status"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr)
        .contains("LINEAR_API_KEY environment variable not set"));
}

#[cfg(unix)]
#[test]
fn linear_non_unicode_environment_key_does_not_fall_back_to_file() {
    use std::os::unix::ffi::OsStringExt;
    let home = tempfile::tempdir().unwrap();
    let path = home.path().join("config.toml");
    std::fs::write(&path, "[linear]\napi_key = \"file\\nkey\"\n").unwrap();
    let output = command(home.path())
        .env("MCPTOOLS_CONFIG", &path)
        .env("LINEAR_API_KEY", std::ffi::OsString::from_vec(vec![0xff]))
        .args(["linear", "auth", "status"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr)
        .contains("LINEAR_API_KEY environment variable not set"));
}

#[test]
fn linear_file_key_and_secret_safe_config_check() {
    let home = tempfile::tempdir().unwrap();
    let path = home.path().join("config.toml");
    std::fs::write(&path, "[linear]\napi_key = 'secret-value'\n").unwrap();
    let checked = command(home.path())
        .env("MCPTOOLS_CONFIG", &path)
        .args(["config", "check"])
        .output()
        .unwrap();
    assert!(
        checked.status.success(),
        "{}",
        String::from_utf8_lossy(&checked.stderr)
    );
    assert!(!String::from_utf8_lossy(&checked.stdout).contains("secret-value"));
    assert!(!String::from_utf8_lossy(&checked.stderr).contains("secret-value"));
    for (text, expected) in [
        ("[linear]\nunknown = 'secret-value'\n", "unknown"),
        (
            "[linear]\napi_key = ['secret-value']\n",
            "expected a string",
        ),
    ] {
        std::fs::write(&path, text).unwrap();
        let checked = command(home.path())
            .env("MCPTOOLS_CONFIG", &path)
            .args(["config", "check"])
            .output()
            .unwrap();
        assert!(!checked.status.success());
        let output = format!(
            "{}{}",
            String::from_utf8_lossy(&checked.stdout),
            String::from_utf8_lossy(&checked.stderr)
        );
        assert!(output.contains(expected), "{output}");
        assert!(!output.contains("secret-value"));
    }
}

#[test]
fn linear_file_fallback_and_environment_override() {
    let home = tempfile::tempdir().unwrap();
    let path = home.path().join("config.toml");
    std::fs::write(&path, "[linear]\napi_key = \"file\\nkey\"\n").unwrap();
    let file = command(home.path())
        .env("MCPTOOLS_CONFIG", &path)
        .env_remove("LINEAR_API_KEY")
        .args(["linear", "auth", "status"])
        .output()
        .unwrap();
    assert!(String::from_utf8_lossy(&file.stderr).contains("Invalid Linear API key"));
    std::fs::write(&path, "[linear]\napi_key = ' '\n").unwrap();
    let env = command(home.path())
        .env("MCPTOOLS_CONFIG", &path)
        .env("LINEAR_API_KEY", "env\nkey")
        .args(["linear", "auth", "status"])
        .output()
        .unwrap();
    assert!(String::from_utf8_lossy(&env.stderr).contains("Invalid Linear API key"));
    std::fs::write(&path, "invalid = true\n").unwrap();
    let env = command(home.path())
        .env("MCPTOOLS_CONFIG", &path)
        .env("LINEAR_API_KEY", "env\nkey")
        .args(["linear", "auth", "status"])
        .output()
        .unwrap();
    assert!(String::from_utf8_lossy(&env.stderr).contains("Invalid Linear API key"));
    let blank = command(home.path())
        .env("MCPTOOLS_CONFIG", &path)
        .env("LINEAR_API_KEY", " ")
        .args(["linear", "auth", "status"])
        .output()
        .unwrap();
    assert!(String::from_utf8_lossy(&blank.stderr)
        .contains("LINEAR_API_KEY environment variable not set"));
}

#[cfg(unix)]
#[test]
fn atlassian_config_permission_warning() {
    use std::os::unix::fs::PermissionsExt;
    let home = tempfile::tempdir().unwrap();
    let path = home.path().join("config.toml");
    std::fs::write(&path, "[atlassian]\napi_token = 'secret-value'\n").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
    let checked = command(home.path())
        .env("MCPTOOLS_CONFIG", &path)
        .args(["config", "check"])
        .output()
        .unwrap();
    assert!(checked.status.success());
    let output = format!(
        "{}{}",
        String::from_utf8_lossy(&checked.stdout),
        String::from_utf8_lossy(&checked.stderr)
    );
    assert!(output.contains("chmod 600"), "{output}");
    assert!(!output.contains("secret-value"));
    for mode in [0o640, 0o604, 0o600] {
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode)).unwrap();
        let checked = command(home.path())
            .env("MCPTOOLS_CONFIG", &path)
            .args(["config", "check"])
            .output()
            .unwrap();
        assert!(checked.status.success());
        assert_eq!(
            String::from_utf8_lossy(&checked.stderr).contains("chmod 600"),
            mode != 0o600
        );
    }
}

#[tokio::test]
async fn jira_file_service_and_environment_shared_precedence() {
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let stub = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/rest/agile/1.0/board/42/sprint"))
        .and(header(
            "authorization",
            "Basic c2VydmljZS1lbWFpbDpzZXJ2aWNlLXRva2Vu",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            serde_json::json!({"maxResults": 50, "startAt": 0, "values": [], "isLast": true}),
        ))
        .expect(1)
        .mount(&stub)
        .await;
    Mock::given(method("GET"))
        .and(path("/rest/agile/1.0/board/42/sprint"))
        .and(header(
            "authorization",
            "Basic ZW52LWVtYWlsOmVudi10b2tlbg==",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            serde_json::json!({"maxResults": 50, "startAt": 0, "values": [], "isLast": true}),
        ))
        .expect(3)
        .mount(&stub)
        .await;
    let home = tempfile::tempdir().unwrap();
    let config = home.path().join("config.toml");
    std::fs::write(&config, format!("[atlassian]\nbase_url = 'http://127.0.0.1:1'\nemail = 'shared'\napi_token = 'shared'\n[atlassian.jira]\nbase_url = '{}'\nemail = 'service-email'\napi_token = 'service-token'\nboard_id = 42\n", stub.uri())).unwrap();
    for mode in 0..4 {
        let mut cmd = command(home.path());
        for key in [
            "ATLASSIAN_BASE_URL",
            "ATLASSIAN_EMAIL",
            "ATLASSIAN_API_TOKEN",
            "JIRA_BASE_URL",
            "JIRA_EMAIL",
            "JIRA_API_TOKEN",
            "JIRA_BOARD_ID",
        ] {
            cmd.env_remove(key);
        }
        cmd.env("MCPTOOLS_CONFIG", &config)
            .args(["atlassian", "jira", "sprint", "list"]);
        if mode > 0 {
            if mode >= 2 {
                cmd.env("MCPTOOLS_CONFIG", home.path().join("missing.toml"))
                    .env("JIRA_BOARD_ID", "42");
            }
            if mode == 3 {
                cmd.env_remove("HOME")
                    .env_remove("XDG_CONFIG_HOME")
                    .env_remove("MCPTOOLS_CONFIG");
            }
            cmd.env("ATLASSIAN_BASE_URL", stub.uri())
                .env("ATLASSIAN_EMAIL", "env-email")
                .env("ATLASSIAN_API_TOKEN", "env-token");
        }
        let output = tokio::task::spawn_blocking(move || cmd.output().unwrap())
            .await
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    for subcommand in ["path", "check"] {
        let output = command(home.path())
            .env_remove("HOME")
            .args(["config", subcommand])
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("Cannot resolve config path"));
    }
}
