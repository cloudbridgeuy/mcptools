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
