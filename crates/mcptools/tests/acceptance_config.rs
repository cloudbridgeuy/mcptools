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
