use crate::prelude::*;

pub fn add_argv(name: &str, command: &str, args: &[String]) -> Vec<String> {
    let mut argv = vec![
        "mcp".to_string(),
        "add".to_string(),
        name.to_string(),
        "--".to_string(),
        command.to_string(),
    ];
    argv.extend(args.iter().cloned());
    argv
}

pub fn remove_argv(name: &str) -> Vec<String> {
    vec!["mcp".to_string(), "remove".to_string(), name.to_string()]
}

pub fn describe_add(name: &str, command: &str, args: &[String]) -> String {
    let mut parts = vec![
        "codex".to_string(),
        "mcp".to_string(),
        "add".to_string(),
        name.to_string(),
        "--".to_string(),
        command.to_string(),
    ];
    parts.extend(args.iter().cloned());
    parts.join(" ")
}

pub fn describe_remove(name: &str) -> String {
    f!("codex mcp remove {name}")
}

fn run_codex(argv: &[String]) -> Result<()> {
    let status = std::process::Command::new("codex").args(argv).status()?;
    if status.success() {
        Ok(())
    } else {
        Err(eyre!(f!("codex {} exited with {status}", argv.join(" "))))
    }
}

pub fn codex_mcp_add(name: &str, command: &str, args: &[String]) -> Result<()> {
    run_codex(&add_argv(name, command, args))
}

pub fn codex_mcp_remove(name: &str) -> Result<()> {
    run_codex(&remove_argv(name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_argv_places_separator_before_command() {
        let argv = add_argv(
            "mcptools",
            "/tmp/t/bin/mcptools",
            &["mcp".to_string(), "stdio".to_string()],
        );
        assert_eq!(
            argv,
            vec![
                "mcp",
                "add",
                "mcptools",
                "--",
                "/tmp/t/bin/mcptools",
                "mcp",
                "stdio"
            ]
        );
    }

    #[test]
    fn add_argv_keeps_spaced_command_whole() {
        let argv = add_argv(
            "mcptools",
            "/tmp/t 5/bin/mcptools",
            &["mcp".to_string(), "stdio".to_string()],
        );
        assert!(argv.contains(&"/tmp/t 5/bin/mcptools".to_string()));
        assert_eq!(argv.iter().filter(|item| item.as_str() == "--").count(), 1);
    }

    #[test]
    fn add_argv_carries_no_secrets() {
        std::env::set_var("LINEAR_API_KEY", "secret-value");
        let argv = add_argv(
            "mcptools",
            "mcptools",
            &["mcp".to_string(), "stdio".to_string()],
        );
        assert!(!argv.iter().any(|item| item.contains("secret-value")));
        std::env::remove_var("LINEAR_API_KEY");
    }

    #[test]
    fn remove_argv_names_server_only() {
        assert_eq!(remove_argv("mcptools"), vec!["mcp", "remove", "mcptools"]);
    }

    #[test]
    fn describe_add_renders_full_argv() {
        assert_eq!(
            describe_add(
                "mcptools",
                "mcptools",
                &["mcp".to_string(), "stdio".to_string()]
            ),
            "codex mcp add mcptools -- mcptools mcp stdio"
        );
    }

    #[test]
    fn describe_add_holds_no_secrets() {
        std::env::set_var("LINEAR_API_KEY", "secret-value");
        let text = describe_add(
            "mcptools",
            "mcptools",
            &["mcp".to_string(), "stdio".to_string()],
        );
        assert!(!text.contains("secret-value"));
        std::env::remove_var("LINEAR_API_KEY");
    }

    #[test]
    fn describe_remove_renders_name() {
        assert_eq!(describe_remove("mcptools"), "codex mcp remove mcptools");
    }
}
