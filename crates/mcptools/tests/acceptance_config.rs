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

#[test]
fn jev_file_and_invalid_fields_are_checked_without_exposing_secrets() {
    let home = tempfile::tempdir().unwrap();
    let path = home.path().join("config.toml");
    std::fs::write(&path, "[jev]\nprovider = 'opencode'\nendpoint = 'https://example.test'\nmodel = 'model'\napi_key = 'secret-value'\n").unwrap();
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
    for text in [
        "[jev]\nunknown = 'secret-value'\n",
        "[jev]\napi_key = ['secret-value']\n",
        "[jev]\nprovider = 'secret-value'\n",
    ] {
        std::fs::write(&path, text).unwrap();
        let checked = command(home.path())
            .env("MCPTOOLS_CONFIG", &path)
            .args(["config", "check"])
            .output()
            .unwrap();
        assert!(!checked.status.success());
        assert!(!String::from_utf8_lossy(&checked.stderr).contains("secret-value"));
        assert!(!String::from_utf8_lossy(&checked.stdout).contains("secret-value"));
    }
}

mod common;

#[tokio::test]
async fn jev_runtime_file_and_environment_configuration() {
    let home = tempfile::tempdir().unwrap();
    let path = home.path().join("config.toml");
    std::fs::write(&path, "[jev]\nprovider = 'opencode'\n").unwrap();
    let config_path = path.to_str().unwrap();
    let mut server = common::spawn_server(&[("MCPTOOLS_CONFIG", config_path)]).unwrap();
    let response = server
        .tools_call("find_tools", serde_json::json!({"task":"close ticket"}))
        .await
        .to_string();
    assert!(response.contains("invalid_config"), "{response}");
    std::fs::write(
        &path,
        "[jev]\nprovider = 'opencode'\napi_key = 'file-key'\n",
    )
    .unwrap();
    let mut server = common::spawn_server(&[
        ("MCPTOOLS_CONFIG", config_path),
        ("JEV_PROVIDER", "unknown"),
    ])
    .unwrap();
    let response = server
        .tools_call("find_tools", serde_json::json!({"task":"close ticket"}))
        .await
        .to_string();
    assert!(response.contains("invalid_config"), "{response}");
    assert!(!response.contains("file-key"));
    std::fs::write(
        &path,
        "[jev]\nprovider = 'opencode'\nendpoint = 'invalid://file'\n",
    )
    .unwrap();
    let mut server = common::spawn_server(&[
        ("MCPTOOLS_CONFIG", config_path),
        ("OPENCODE_API_KEY", "provider-key"),
    ])
    .unwrap();
    let response = server
        .tools_call("find_tools", serde_json::json!({"task":"close ticket"}))
        .await
        .to_string();
    assert!(response.contains("unreachable"), "{response}");
    assert!(!response.contains("provider-key"));
    std::fs::write(&path, "[jev]\nprovider = 'opencode'\nendpoint = 'invalid://file'\nmodel = 'file'\napi_key = 'file-key'\n").unwrap();
    let mut server = common::spawn_server(&[("MCPTOOLS_CONFIG", config_path)]).unwrap();
    let response = server
        .tools_call("find_tools", serde_json::json!({"task":"close ticket"}))
        .await
        .to_string();
    assert!(response.contains("unreachable"), "{response}");
    assert!(!response.contains("file-key"));
    let mut server = common::spawn_server(&[
        ("MCPTOOLS_CONFIG", config_path),
        ("JEV_PROVIDER", "vercel"),
        ("JEV_ENDPOINT", "invalid://env"),
        ("JEV_MODEL", "env"),
        ("AI_GATEWAY_API_KEY", "env-key"),
    ])
    .unwrap();
    std::fs::write(&path, "invalid = true\n").unwrap();
    let response = server
        .tools_call("find_tools", serde_json::json!({"task":"close ticket"}))
        .await
        .to_string();
    assert!(response.contains("unreachable"), "{response}");
    assert!(!response.contains("env-key"));
}

#[test]
fn images_file_and_precedence_and_invalid_fields() {
    let home = tempfile::tempdir().unwrap();
    let path = home.path().join("config.toml");
    std::fs::write(
        &path,
        "[images]\napi = 'openai'\napi_key = 'file-key'\nsize = '1024x1024'\nquality = 'high'\n",
    )
    .unwrap();
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
    for field in ["api", "size", "quality"] {
        std::fs::write(&path, format!("[images]\n{field} = 'secret-value'\n")).unwrap();
        let checked = command(home.path())
            .env("MCPTOOLS_CONFIG", &path)
            .args(["config", "check"])
            .output()
            .unwrap();
        assert!(!checked.status.success());
        assert!(!String::from_utf8_lossy(&checked.stderr).contains("secret-value"));
    }
}

#[test]
fn images_auth_prefers_own_then_falls_back_to_legacy() {
    let home = tempfile::tempdir().unwrap();
    let own = home.path().join("own");
    let legacy = home.path().join(".config/llm-stream");
    std::fs::create_dir_all(&own).unwrap();
    std::fs::create_dir_all(&legacy).unwrap();
    std::fs::write(own.join("auth.json"), "own-invalid").unwrap();
    std::fs::write(legacy.join("auth.json"), "legacy-invalid").unwrap();
    let run = || {
        command(home.path())
            .env("MCPTOOLS_CONFIG", own.join("config.toml"))
            .env_remove("MCPTOOLS_IMAGES_API")
            .env_remove("LLM_STREAM_CONFIG_DIR")
            .args(["images", "generate", "test"])
            .output()
            .unwrap()
    };
    let own_error = String::from_utf8_lossy(&run().stderr).into_owned();
    assert!(
        own_error.contains(own.join("auth.json").to_str().unwrap()),
        "{own_error}"
    );
    std::fs::remove_file(own.join("auth.json")).unwrap();
    let legacy_error = String::from_utf8_lossy(&run().stderr).into_owned();
    assert!(
        legacy_error.contains(legacy.join("auth.json").to_str().unwrap()),
        "{legacy_error}"
    );
}

#[tokio::test]
async fn images_file_env_and_flag_precedence() {
    use wiremock::{Mock, MockServer, ResponseTemplate};
    let server = MockServer::start().await;
    Mock::given(wiremock::matchers::method("POST"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(
                serde_json::json!({"created": 1, "data": [{"b64_json": "R0lGODlh"}]}),
            ),
        )
        .expect(3)
        .mount(&server)
        .await;
    let home = tempfile::tempdir().unwrap();
    let path = home.path().join("config.toml");
    std::fs::write(&path, format!("[images]\napi = 'openai'\napi_key = 'file-key'\nbase_url = '{}'\nmodel = 'file-model'\nsize = '1024x1024'\nquality = 'high'\n", server.uri())).unwrap();
    for mode in 0..3 {
        let mut cmd = command(home.path());
        cmd.env("MCPTOOLS_CONFIG", &path)
            .env_remove("MCPTOOLS_IMAGES_API")
            .env_remove("OPENAI_API_KEY")
            .env_remove("OPENAI_BASE_URL")
            .env_remove("OPENAI_IMAGES_MODEL")
            .env_remove("MCPTOOLS_IMAGES_MAINLINE")
            .args([
                "images",
                "generate",
                "test",
                "--output-dir",
                home.path().to_str().unwrap(),
            ]);
        if mode > 0 {
            cmd.env("OPENAI_IMAGES_MODEL", "env-model")
                .env("OPENAI_API_KEY", "env-key");
        }
        if mode == 2 {
            cmd.args([
                "--model",
                "flag-model",
                "--api-key",
                "flag-key",
                "--quality",
                "low",
            ]);
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
    let requests = server.received_requests().await.unwrap();
    for (request, (model, key, quality)) in requests.iter().zip([
        ("file-model", "file-key", "high"),
        ("env-model", "env-key", "high"),
        ("flag-model", "flag-key", "low"),
    ]) {
        let body: serde_json::Value = serde_json::from_slice(&request.body).unwrap();
        assert_eq!(body["model"], model);
        assert_eq!(body["size"], "1024x1024");
        assert_eq!(body["quality"], quality);
        assert_eq!(
            request
                .headers
                .get("authorization")
                .unwrap()
                .to_str()
                .unwrap(),
            format!("Bearer {key}")
        );
    }
}

#[tokio::test]
async fn images_complete_backend_settings_skip_invalid_config() {
    use wiremock::{Mock, MockServer, ResponseTemplate};
    let server = MockServer::start().await;
    Mock::given(wiremock::matchers::method("POST"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!({"created": 1, "data": []})),
        )
        .expect(3)
        .mount(&server)
        .await;
    let home = tempfile::tempdir().unwrap();
    let path = home.path().join("config.toml");
    std::fs::write(&path, "invalid = true\n").unwrap();
    let image = home.path().join("input.png");
    std::fs::write(&image, b"image").unwrap();
    for operation in ["generate", "edit", "vary"] {
        let mut cmd = command(home.path());
        cmd.env("MCPTOOLS_CONFIG", &path)
            .env("MCPTOOLS_IMAGES_API", "openai")
            .env("OPENAI_API_KEY", "key")
            .env("OPENAI_BASE_URL", server.uri())
            .env("OPENAI_IMAGES_MODEL", "model")
            .env_remove("MCPTOOLS_IMAGES_MAINLINE")
            .args(["images", operation]);
        if operation != "vary" {
            cmd.arg("test");
        }
        if operation != "generate" {
            cmd.args(["--image", image.to_str().unwrap()]);
        }
        cmd.args(["--size", "auto", "--quality", "auto"]);
        let output = tokio::task::spawn_blocking(move || cmd.output().unwrap())
            .await
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let output = command(home.path())
        .env("MCPTOOLS_CONFIG", &path)
        .env_remove("LLM_STREAM_CONFIG_DIR")
        .env_remove("OPENAI_API_KEY")
        .env_remove("OPENAI_BASE_URL")
        .args([
            "images",
            "generate",
            "test",
            "--api",
            "chatgpt",
            "--mainline",
            "mainline",
            "--model",
            "model",
            "--size",
            "auto",
            "--quality",
            "auto",
        ])
        .output()
        .unwrap();
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(error.contains("not signed in"), "{error}");
    assert!(!error.contains("Invalid config"), "{error}");
}

#[test]
fn runtime_sections_are_checked() {
    let home = tempfile::tempdir().unwrap();
    let path = home.path().join("config.toml");
    std::fs::write(&path, "[mcp]\ndiscovery = true\ncode_mode = false\nexecute_timeout_secs = 9\nexecute_memory_mb = 32\nexecute_output_kb = 8\n[general]\nverbose = true\n[diagram]\nhost = '127.0.0.1'\nport = 5199\nshare_seconds = 600\nttl_seconds = 3600\nidle_check_seconds = 60\nidle_grace_seconds = 120\n[md]\ntimeout = 15\n").unwrap();
    let output = command(home.path())
        .env("MCPTOOLS_CONFIG", &path)
        .args(["config", "check"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    for text in [
        "[mcp]\nunknown = true",
        "[mcp]\ndiscovery = 'yes'",
        "[mcp]\nexecute_timeout_secs = 0",
        "[mcp]\nexecute_memory_mb = 0",
        "[mcp]\nexecute_output_kb = 0",
        "[diagram]\nport = 65536",
        "[diagram]\nshare_seconds = 59",
        "[md]\ntimeout = '30'",
    ] {
        std::fs::write(&path, text).unwrap();
        let output = command(home.path())
            .env("MCPTOOLS_CONFIG", &path)
            .args(["config", "check"])
            .output()
            .unwrap();
        assert!(!output.status.success(), "accepted {text}");
    }
}

fn mcp_runtime_request(
    home: &Path,
    config: &Path,
    env: &[(&str, &str)],
    flags: &[&str],
    method: &str,
    params: serde_json::Value,
) -> serde_json::Value {
    use std::io::{BufRead, BufReader, Write};
    use std::process::Stdio;
    let mut cmd = command(home);
    cmd.env("MCPTOOLS_CONFIG", config)
        .args(["mcp", "stdio"])
        .args(flags)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped());
    for key in [
        "MCPTOOLS_DISCOVERY",
        "MCPTOOLS_CODE_MODE",
        "MCPTOOLS_EXECUTE_TIMEOUT_SECS",
        "MCPTOOLS_EXECUTE_MEMORY_MB",
        "MCPTOOLS_EXECUTE_OUTPUT_KB",
    ] {
        cmd.env_remove(key);
    }
    for (key, value) in env {
        cmd.env(key, value);
    }
    let mut child = cmd.spawn().unwrap();
    let mut input = child.stdin.take().unwrap();
    writeln!(
        input,
        "{}",
        serde_json::json!({"jsonrpc":"2.0", "id":1, "method":method, "params":params})
    )
    .unwrap();
    let mut line = String::new();
    BufReader::new(child.stdout.take().unwrap())
        .read_line(&mut line)
        .unwrap();
    drop(input);
    assert!(child.wait().unwrap().success());
    serde_json::from_str(&line).unwrap()
}

#[test]
fn mcp_file_env_and_flag_precedence() {
    let home = tempfile::tempdir().unwrap();
    let path = home.path().join("config.toml");
    std::fs::write(&path, "[mcp]\ndiscovery = true\nexecute_output_kb = 1\n").unwrap();
    let request = |env: &[(&str, &str)], flags: &[&str]| {
        mcp_runtime_request(
            home.path(),
            &path,
            env,
            flags,
            "tools/list",
            serde_json::json!({}),
        )["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|tool| tool["name"].as_str().unwrap().to_owned())
            .collect::<Vec<_>>()
    };
    let file = request(&[], &[]);
    assert!(file.contains(&"find_tools".into()) && !file.contains(&"jira_search".into()));
    let env = request(&[("MCPTOOLS_DISCOVERY", "off")], &[]);
    assert!(env.contains(&"jira_search".into()));
    let flag = request(&[("MCPTOOLS_DISCOVERY", "off")], &["--discovery"]);
    assert!(!flag.contains(&"jira_search".into()));
    std::fs::write(&path, "[mcp]\ncode_mode = true\nexecute_output_kb = 1\n").unwrap();
    let code_mode = request(&[], &[]);
    assert!(code_mode.contains(&"execute".into()) && !code_mode.contains(&"jira_search".into()));
    let code_mode_env = request(&[("MCPTOOLS_CODE_MODE", "false")], &[]);
    assert!(code_mode_env.contains(&"jira_search".into()));
    let code_mode_flag = request(&[("MCPTOOLS_CODE_MODE", "false")], &["--code-mode"]);
    assert!(!code_mode_flag.contains(&"jira_search".into()));
    let output = |env: &[(&str, &str)], flags: &[&str]| {
        mcp_runtime_request(
            home.path(),
            &path,
            env,
            flags,
            "tools/call",
            serde_json::json!({"name":"execute", "arguments":{"code":"'x'.repeat(1500)"}}),
        )
        .to_string()
    };
    assert!(output(&[], &[]).contains("OutputLimit"));
    assert!(!output(&[("MCPTOOLS_EXECUTE_OUTPUT_KB", "2")], &[]).contains("OutputLimit"));
    assert!(output(
        &[("MCPTOOLS_EXECUTE_OUTPUT_KB", "2")],
        &["--execute-output-kb", "1"]
    )
    .contains("OutputLimit"));
}

#[test]
fn md_timeout_env_and_flag_precede_file() {
    let home = tempfile::tempdir().unwrap();
    let path = home.path().join("config.toml");
    std::fs::write(&path, "[md]\ntimeout = 10\n").unwrap();
    for flags in [vec![], vec!["--timeout", "0"]] {
        let mut cmd = command(home.path());
        cmd.env("MCPTOOLS_CONFIG", &path)
            .env("MD_TIMEOUT", if flags.is_empty() { "0" } else { "10" })
            .args(["md", "fetch", "http://example.test"])
            .args(flags);
        let output = cmd.output().unwrap();
        assert!(String::from_utf8_lossy(&output.stderr).contains("timed out after 0 seconds"));
    }
}

#[test]
fn lane_repos_is_rejected_by_config_check() {
    let home = tempfile::tempdir().unwrap();
    let path = home.path().join("config.toml");
    for text in [
        "lane_repos = ['/tmp']",
        "lane-repos = ['/tmp']",
        "[mcp]\nlane_repos = ['/tmp']",
        "[mcp]\nlane-repos = ['/tmp']",
        "[lane]\nrepos = ['/tmp']",
        "[general]\nlane_repos = ['/tmp']",
        "[diagram]\nlane_repos = ['/tmp']",
        "[md]\nlane_repos = ['/tmp']",
    ] {
        std::fs::write(&path, text).unwrap();
        let output = command(home.path())
            .env("MCPTOOLS_CONFIG", &path)
            .args(["config", "check"])
            .output()
            .unwrap();
        assert!(!output.status.success(), "accepted {text}");
    }
}

#[test]
fn general_verbose_file_and_environment_precedence() {
    let home = tempfile::tempdir().unwrap();
    let path = home.path().join("config.toml");
    std::fs::write(&path, "[general]\nverbose = true\n").unwrap();
    for (env, expected) in [(None, true), (Some("false"), false)] {
        let mut cmd = command(home.path());
        cmd.env("MCPTOOLS_CONFIG", &path)
            .env_remove("LINEAR_API_KEY")
            .env_remove("MCPTOOLS_VERBOSE")
            .args(["linear", "auth", "status"]);
        if let Some(value) = env {
            cmd.env("MCPTOOLS_VERBOSE", value);
        }
        let output = cmd.output().unwrap();
        assert_eq!(
            String::from_utf8_lossy(&output.stdout).contains("Running Linear command"),
            expected
        );
    }
}

#[test]
fn diagram_serve_uses_file_env_and_flag_ports() {
    use std::process::Stdio;
    use std::time::{Duration, Instant};
    let home = tempfile::tempdir().unwrap();
    let path = home.path().join("config.toml");
    let port = || {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.local_addr().unwrap().port()
    };
    for mode in 0..3 {
        let file_port = port();
        let env_port = port();
        let flag_port = port();
        std::fs::write(&path, format!("[diagram]\nport = {file_port}\n")).unwrap();
        let mut cmd = command(home.path());
        cmd.env("MCPTOOLS_CONFIG", &path)
            .env("XDG_DATA_HOME", home.path())
            .env_remove("MCPTOOLS_DIAGRAM_PORT")
            .args(["diagram", "serve", "--host", "127.0.0.1"])
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        if mode > 0 {
            cmd.env("MCPTOOLS_DIAGRAM_PORT", env_port.to_string());
        }
        if mode == 2 {
            cmd.args(["--port", &flag_port.to_string()]);
        }
        let mut child = cmd.spawn().unwrap();
        let record = home.path().join("mcptools/diagrams/.daemon-address!");
        let deadline = Instant::now() + Duration::from_secs(5);
        let address = loop {
            if let Ok(address) = std::fs::read_to_string(&record) {
                break address;
            }
            assert!(
                child.try_wait().unwrap().is_none() && Instant::now() < deadline,
                "daemon failed to start"
            );
            std::thread::sleep(Duration::from_millis(20));
        };
        child.kill().unwrap();
        child.wait().unwrap();
        std::fs::remove_file(record).unwrap();
        assert_eq!(
            address.parse::<std::net::SocketAddr>().unwrap().port(),
            [file_port, env_port, flag_port][mode]
        );
    }
}

#[cfg(unix)]
#[test]
fn md_timeout_file_env_and_flag_precedence() {
    use std::os::unix::fs::PermissionsExt;
    let home = tempfile::tempdir().unwrap();
    let path = home.path().join("config.toml");
    let chrome = home.path().join("chrome");
    std::fs::write(&path, "[md]\ntimeout = 1\n").unwrap();
    std::fs::write(&chrome, "#!/bin/sh\nexec sleep 30\n").unwrap();
    std::fs::set_permissions(&chrome, std::fs::Permissions::from_mode(0o755)).unwrap();
    for mode in 0..3 {
        let mut cmd = command(home.path());
        cmd.env("MCPTOOLS_CONFIG", &path)
            .env("CHROME", &chrome)
            .env_remove("MD_TIMEOUT")
            .args(["md", "fetch", "http://example.test"]);
        if mode > 0 {
            cmd.env("MD_TIMEOUT", "2");
        }
        if mode == 2 {
            cmd.args(["--timeout", "3"]);
        }
        let output = cmd.output().unwrap();
        let error = String::from_utf8_lossy(&output.stderr);
        assert!(
            error.contains(&format!("timed out after {} seconds", mode + 1)),
            "{error}"
        );
    }
}
