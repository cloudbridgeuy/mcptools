use std::process::Command;

#[test]
fn help_never_displays_secret_environment_values() {
    let home = tempfile::tempdir().unwrap();
    let secrets = [
        "OPENAI_API_KEY",
        "ATLASSIAN_API_TOKEN",
        "JIRA_API_TOKEN",
        "CONFLUENCE_API_TOKEN",
        "BITBUCKET_APP_PASSWORD",
        "LINEAR_API_KEY",
        "JEV_API_KEY",
        "OPENCODE_API_KEY",
        "OPENROUTER_API_KEY",
        "AI_GATEWAY_API_KEY",
        "TYPESAFE_API_KEY",
        "GITHUB_TOKEN",
        "DB_PASSWORD",
        "MCPTOOLS_TEST_API_KEY",
        "MY_SECRET",
        "EMPTY_SECRET",
    ];
    let source = include_str!("../src/main.rs");
    let commands = source.split("pub enum SubCommands {").nth(1).unwrap();
    let commands = commands.split("\n}").next().unwrap();
    let mut paths = vec![Vec::<String>::new()];
    for (index, _) in commands.match_indices("#[command(hide = true)]") {
        let variant = commands[index..]
            .lines()
            .nth(1)
            .unwrap()
            .trim()
            .split('(')
            .next()
            .unwrap();
        let mut name = String::new();
        for (index, character) in variant.chars().enumerate() {
            if character.is_ascii_uppercase() && index > 0 {
                name.push('-');
            }
            name.push(character.to_ascii_lowercase());
        }
        paths.push(vec![name]);
    }
    let mut failures = Vec::new();
    while let Some(path) = paths.pop() {
        for flag in ["-h", "--help"] {
            let mut command = Command::new(env!("CARGO_BIN_EXE_mcptools"));
            command
                .env_clear()
                .env("HOME", home.path())
                .env("MCPTOOLS_CONFIG", home.path().join("config.toml"))
                .current_dir(home.path())
                .args(&path)
                .arg(flag);
            for name in secrets {
                command.env(name, format!("dummy-secret-value-{name}"));
            }
            let output = command.output().unwrap();
            if !(output.status.success()
                || (path.last().is_some_and(|name| name == "help")
                    && output.status.code() == Some(2)))
            {
                failures.push(format!("{} {flag}: help failed", path.join(" ")));
            }
            let stdout = String::from_utf8(output.stdout).unwrap();
            let stderr = String::from_utf8(output.stderr).unwrap();
            for name in secrets {
                let value = format!("dummy-secret-value-{name}");
                if stdout.contains(&value) || stderr.contains(&value) {
                    failures.push(format!("{} {flag}: {name}", path.join(" ")));
                }
            }
            if flag == "--help" && path.last().is_none_or(|name| name != "help") {
                if let Some(commands) = stdout.split_once("Commands:\n").map(|(_, rest)| rest) {
                    for line in commands.lines().take_while(|line| !line.is_empty()) {
                        if let Some(entry) = line.strip_prefix("  ") {
                            if !entry.starts_with(char::is_whitespace) {
                                if let Some(name) = entry.split_whitespace().next() {
                                    let mut child = path.clone();
                                    child.push(name.to_string());
                                    paths.push(child);
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
