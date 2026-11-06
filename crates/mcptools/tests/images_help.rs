use std::process::Command;

fn help(args: &[&str]) -> String {
    let home = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_mcptools"))
        .env_clear()
        .env("HOME", home.path())
        .current_dir(home.path())
        .args(args)
        .arg("--help")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

fn entry(help: &str, name: &str) -> String {
    let mut lines = help.lines();
    let line = lines
        .find(|line| {
            let line = line.trim_start();
            line.strip_prefix(name)
                .is_some_and(|rest| rest.is_empty() || rest.starts_with(char::is_whitespace))
                || line.starts_with("-n, ") && name == "--number"
                || line.starts_with("-o, ") && name == "--output"
                || line.starts_with("-h, ") && name == "--help"
        })
        .unwrap_or_else(|| panic!("missing {name} in {help}"));
    let description = line
        .trim_start()
        .split_once("  ")
        .map(|(_, description)| description.trim())
        .unwrap_or("");
    let mut text = description.to_string();
    for line in lines.take_while(|line| line.starts_with("          ")) {
        text.push(' ');
        text.push_str(line.trim());
    }
    text.trim().to_string()
}

fn described(help: &str, names: &[&str]) {
    for name in names {
        let description = entry(help, name);
        assert!(
            !description.is_empty() && !description.starts_with('['),
            "missing description for {name}: {help}"
        );
    }
}

#[test]
fn images_commands_are_described() {
    let text = help(&["images"]);
    described(&text, &["generate", "edit", "vary", "--verbose", "--help"]);
    assert!(
        text.contains(mcptools_core::images::DEFAULT_MODEL),
        "{text}"
    );
    let root = help(&[]);
    assert!(
        root.contains(mcptools_core::images::DEFAULT_MODEL),
        "{root}"
    );
    assert!(!root.contains("Images 2.5"), "{root}");
}

#[test]
fn images_options_are_described() {
    let shared = [
        "--model",
        "--size",
        "--quality",
        "--format",
        "--compression",
        "--background",
        "--moderation",
        "--output",
        "--output-dir",
        "--json",
        "--open",
        "--preview",
        "--api",
        "--config-dir",
        "--api-key",
        "--base-url",
        "--mainline",
        "--number",
        "--verbose",
        "--help",
    ];
    for (command, specific) in [
        (
            "generate",
            &["<PROMPT>", "--frames", "--frame-ms", "--complexity"][..],
        ),
        (
            "edit",
            &[
                "<PROMPT>",
                "--image",
                "--mask",
                "--markup",
                "--input-fidelity",
            ][..],
        ),
        ("vary", &["--prompt", "--image", "--input-fidelity"][..]),
    ] {
        let text = help(&["images", command]);
        described(&text, &shared);
        described(&text, specific);
    }
}

#[test]
fn images_help_explains_values_and_sizes() {
    for command in ["generate", "edit", "vary"] {
        let text = help(&["images", command]);
        for (option, values) in [
            (
                "--quality",
                &["auto", "low", "medium", "high", "xhigh", "max"][..],
            ),
            ("--format", &["png", "jpeg", "webp", "gif", "svg"][..]),
            ("--background", &["auto", "transparent", "opaque"][..]),
            ("--moderation", &["auto", "low"][..]),
            ("--api", &["chatgpt", "openai"][..]),
            (
                "--size",
                &["auto", "1024x1024", "1536x1024", "1024x1536", "16"][..],
            ),
        ] {
            let description = entry(&text, option).to_lowercase();
            for value in values {
                assert!(
                    description.contains(value),
                    "{option} missing {value}: {description}"
                );
            }
        }
        let size = entry(&text, "--size").to_lowercase();
        assert!(
            size.contains("custom") && size.contains("multiple"),
            "{size}"
        );
        let (option, values) = if command == "generate" {
            ("--complexity", &["low", "medium", "high"][..])
        } else {
            ("--input-fidelity", &["low", "high"][..])
        };
        let description = entry(&text, option).to_lowercase();
        for value in values {
            assert!(
                description.contains(value),
                "{option} missing {value}: {description}"
            );
        }
    }
}

#[test]
fn images_help_has_examples() {
    for (command, values) in [
        (
            "generate",
            &["phone", "portrait", "1024x1536", "--format gif --frames"][..],
        ),
        ("edit", &["example", "--mask mask.png"][..]),
        ("vary", &["example", "images vary", "--image"][..]),
    ] {
        let text = help(&["images", command]).to_lowercase();
        for value in values {
            assert!(text.contains(value), "missing {value}: {text}");
        }
    }
}

#[test]
fn sandbox_options_are_only_on_mcp_help() {
    for args in [
        &["images"][..],
        &["images", "generate"],
        &["images", "edit"],
        &["images", "vary"],
    ] {
        let text = help(args);
        assert!(!text.contains("--execute-"), "{text}");
    }
    let text = help(&["mcp"]);
    for option in [
        "--execute-timeout-secs",
        "--execute-memory-mb",
        "--execute-output-kb",
    ] {
        assert!(text.contains(option), "{text}");
    }
}
