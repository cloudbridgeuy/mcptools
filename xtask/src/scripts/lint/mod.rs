pub mod hooks;

use crate::cli::LintArgs;
use color_eyre::eyre::Result;
use duct::cmd;
use std::fs;
use std::io::Write;

// ---------------------------------------------------------------------------
// Functional Core — pure types and logic, no I/O
// ---------------------------------------------------------------------------

/// Identifier for each check, used to match skip flags and fix-mode overrides.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CheckId {
    Fmt,
    Check,
    Clippy,
    Test,
    Machete,
    Typos,
}

/// A lint check to execute.
struct Check {
    /// Identifier for skip/fix matching.
    id: CheckId,
    /// Human-readable name shown in output.
    name: &'static str,
    /// The program to invoke.
    program: &'static str,
    /// Arguments passed to the program.
    args: &'static [&'static str],
    /// When true, the check is skipped (not failed) if the tool is missing.
    optional: bool,
}

/// The outcome of running a single check.
enum CheckOutcome {
    Passed { output: String },
    Failed { output: String },
    Skipped,
}

struct CheckResult {
    name: String,
    outcome: CheckOutcome,
}

/// The ordered pipeline of checks to run.
const CHECKS: &[Check] = &[
    Check {
        id: CheckId::Fmt,
        name: "cargo fmt --check",
        program: "cargo",
        args: &["fmt", "--check"],
        optional: false,
    },
    Check {
        id: CheckId::Check,
        name: "cargo check --all-targets",
        program: "cargo",
        args: &["check", "--all-targets"],
        optional: false,
    },
    Check {
        id: CheckId::Clippy,
        name: "cargo clippy --all-targets -- -D warnings",
        program: "cargo",
        args: &["clippy", "--all-targets", "--", "-D", "warnings"],
        optional: false,
    },
    Check {
        id: CheckId::Test,
        name: "cargo test --all-targets",
        program: "cargo",
        args: &["test", "--all-targets"],
        optional: false,
    },
    Check {
        id: CheckId::Machete,
        name: "cargo machete",
        program: "cargo",
        args: &["machete"],
        optional: true,
    },
    Check {
        id: CheckId::Typos,
        name: "typos",
        program: "typos",
        args: &[],
        optional: false,
    },
];

/// Determine whether a check should be skipped based on the user's skip flags.
fn should_skip(id: CheckId, args: &LintArgs) -> bool {
    match id {
        CheckId::Fmt => args.no_fmt,
        CheckId::Check => args.no_check,
        CheckId::Clippy => args.no_clippy,
        CheckId::Test => args.no_test,
        CheckId::Machete => args.no_machete,
        CheckId::Typos => args.no_typos,
    }
}

/// Return the effective args for a check, accounting for `--fix` mode.
///
/// In fix mode:
/// - `fmt` drops `--check` so formatting is applied directly.
/// - `clippy` appends `--fix --allow-dirty` before the `--` separator.
/// - All other checks use their default args unchanged.
///
/// Returns `None` when the default static args should be used as-is.
fn fix_args(id: CheckId) -> Option<Vec<&'static str>> {
    match id {
        CheckId::Fmt => Some(vec!["fmt"]),
        CheckId::Clippy => Some(vec![
            "clippy",
            "--all-targets",
            "--fix",
            "--allow-dirty",
            "--",
            "-D",
            "warnings",
        ]),
        _ => None,
    }
}

/// Build a display name for a check from its program and effective args.
fn check_display_name(program: &str, args: &[&str]) -> String {
    format!("{} {}", program, args.join(" "))
}

/// Determine whether command output indicates the tool is not installed.
fn is_tool_not_found(output: &str) -> bool {
    let lower = output.to_lowercase();
    lower.contains("not found")
        || lower.contains("no such file or directory")
        || lower.contains("unrecognized subcommand")
        || lower.contains("no such command")
}

/// Determine the outcome of a check from its exit status and output.
fn determine_outcome(success: bool, output: String, optional: bool) -> CheckOutcome {
    if optional && !success && is_tool_not_found(&output) {
        return CheckOutcome::Skipped;
    }
    if success {
        CheckOutcome::Passed { output }
    } else {
        CheckOutcome::Failed { output }
    }
}

/// Format a single check result as a log entry.
fn format_log_entry(result: &CheckResult) -> String {
    match &result.outcome {
        CheckOutcome::Skipped => {
            format!("=== {} ===\n[skipped — tool not installed]\n", result.name)
        }
        CheckOutcome::Passed { output } | CheckOutcome::Failed { output } => {
            format!("=== {} ===\n{}\n", result.name, output)
        }
    }
}

/// Build the final log path line printed on every run.
fn format_log_path_line(log_path: &str) -> String {
    format!("log: {log_path}")
}

// ---------------------------------------------------------------------------
// Imperative Shell — I/O, side effects, orchestration
// ---------------------------------------------------------------------------

/// Run the full lint pipeline (or dispatch to hooks management).
pub fn run(args: &LintArgs) -> Result<()> {
    // Dispatch hooks management flags — early return.
    if args.install_hooks {
        return hooks::install_hooks();
    }
    if args.uninstall_hooks {
        return hooks::uninstall_hooks();
    }
    if args.hooks_status {
        return hooks::show_status();
    }
    if args.test_hooks {
        return hooks::test_hooks();
    }

    // --staged-only implies --fix (formatting is applied, not just checked).
    let fix = args.fix || args.staged_only;

    // Capture staged .rs files BEFORE the pipeline runs, since cargo fmt
    // may modify files and change what git reports as staged.
    let staged_files = if args.staged_only {
        Some(collect_staged_rust_files()?)
    } else {
        None
    };

    let log_path = resolve_log_path()?;
    let mut log_file = fs::File::create(&log_path)?;

    let mut failed_check: Option<String> = None;

    for check in CHECKS {
        if should_skip(check.id, args) {
            continue;
        }

        let effective_args: Option<Vec<&str>> = if fix { fix_args(check.id) } else { None };

        let effective_name = match &effective_args {
            Some(overrides) => check_display_name(check.program, overrides),
            None => check.name.to_string(),
        };

        let result = run_check(check, effective_name, effective_args.as_deref())?;

        // Write every result to the log, regardless of outcome.
        write!(log_file, "{}", format_log_entry(&result))?;

        match result.outcome {
            CheckOutcome::Skipped => {
                if args.verbose {
                    println!("[skip] {} (not installed)", result.name);
                }
            }
            CheckOutcome::Passed { ref output } => {
                if args.verbose {
                    print!("{output}");
                }
            }
            CheckOutcome::Failed { ref output } => {
                print!("{output}");
                failed_check = Some(result.name.clone());
                break;
            }
        }
    }

    // Always print the log path as the very last line.
    let log_path_line = format_log_path_line(&log_path);

    if let Some(name) = failed_check {
        println!("\nlint failed at: {name}");
        println!("{log_path_line}");
        drop(log_file);
        std::process::exit(1);
    }

    // Re-stage .rs files when running from the pre-commit hook.
    if let Some(files) = staged_files {
        restage_files(&files)?;
    }

    println!("{log_path_line}");
    Ok(())
}

/// Execute a single check and return its result. Handles optional-tool logic.
///
/// When `override_args` is `Some`, those args are used instead of the check's
/// default args (used for `--fix` mode).
fn run_check(check: &Check, name: String, override_args: Option<&[&str]>) -> Result<CheckResult> {
    let args: &[&str] = override_args.unwrap_or(check.args);

    let output = cmd(check.program, args)
        .stderr_to_stdout()
        .stdout_capture()
        .unchecked()
        .run()?;

    let text = String::from_utf8_lossy(&output.stdout).into_owned();

    let outcome = determine_outcome(output.status.success(), text, check.optional);
    Ok(CheckResult { name, outcome })
}

/// Resolve the absolute path to the log file inside `target/`.
fn resolve_log_path() -> Result<String> {
    let target_dir = std::env::current_dir()?.join("target");
    fs::create_dir_all(&target_dir)?;
    let log_path = target_dir.join("xtask-lint.log");
    Ok(log_path.to_string_lossy().into_owned())
}

/// Collect staged .rs files (added/copied/modified) from the git index.
///
/// Must be called BEFORE the lint pipeline runs so the list reflects the
/// original staged state, not what `cargo fmt` may have changed.
fn collect_staged_rust_files() -> Result<Vec<String>> {
    let output = cmd!(
        "git",
        "diff",
        "--cached",
        "--name-only",
        "--diff-filter=ACM"
    )
    .stdout_capture()
    .run()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(stdout
        .lines()
        .filter(|l| l.ends_with(".rs"))
        .map(String::from)
        .collect())
}

/// Re-stage a pre-collected list of .rs files.
///
/// Called after a successful pipeline run in `--staged-only` mode so that
/// formatting changes applied by `cargo fmt` are included in the commit.
fn restage_files(files: &[String]) -> Result<()> {
    if !files.is_empty() {
        let mut args = vec!["add"];
        args.extend(files.iter().map(|s| s.as_str()));
        cmd("git", &args).run()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_tool_not_found_detects_missing_tool() {
        assert!(is_tool_not_found("error: no such command: `machete`\n"));
        assert!(is_tool_not_found("bash: cargo-machete: not found\n"));
        assert!(is_tool_not_found(
            "error[E0463]: No such file or directory\n"
        ));
        assert!(is_tool_not_found(
            "error: unrecognized subcommand 'machete'\n"
        ));
    }

    #[test]
    fn test_is_tool_not_found_ignores_normal_errors() {
        assert!(!is_tool_not_found("error[E0308]: mismatched types\n"));
        assert!(!is_tool_not_found("warning: unused variable\n"));
        assert!(!is_tool_not_found(""));
    }

    #[test]
    fn test_format_log_entry_passed() {
        let result = CheckResult {
            name: "cargo fmt --check".to_string(),
            outcome: CheckOutcome::Passed {
                output: "all good\n".to_string(),
            },
        };
        let entry = format_log_entry(&result);
        assert!(entry.contains("=== cargo fmt --check ==="));
        assert!(entry.contains("all good"));
    }

    #[test]
    fn test_format_log_entry_failed() {
        let result = CheckResult {
            name: "cargo clippy".to_string(),
            outcome: CheckOutcome::Failed {
                output: "error: something wrong\n".to_string(),
            },
        };
        let entry = format_log_entry(&result);
        assert!(entry.contains("=== cargo clippy ==="));
        assert!(entry.contains("error: something wrong"));
    }

    #[test]
    fn test_format_log_entry_skipped() {
        let result = CheckResult {
            name: "cargo machete".to_string(),
            outcome: CheckOutcome::Skipped,
        };
        let entry = format_log_entry(&result);
        assert!(entry.contains("=== cargo machete ==="));
        assert!(entry.contains("[skipped"));
    }

    #[test]
    fn test_format_log_path_line() {
        let line = format_log_path_line("/abs/path/target/xtask-lint.log");
        assert_eq!(line, "log: /abs/path/target/xtask-lint.log");
    }

    /// Helper: build a LintArgs with all flags defaulted to false.
    fn default_lint_args() -> LintArgs {
        LintArgs {
            verbose: false,
            no_fmt: false,
            no_check: false,
            no_clippy: false,
            no_test: false,
            no_machete: false,
            no_typos: false,
            fix: false,
            staged_only: false,
            install_hooks: false,
            uninstall_hooks: false,
            hooks_status: false,
            test_hooks: false,
        }
    }

    #[test]
    fn test_should_skip_respects_each_flag() {
        let mut args = default_lint_args();

        // Nothing skipped by default.
        assert!(!should_skip(CheckId::Fmt, &args));
        assert!(!should_skip(CheckId::Check, &args));
        assert!(!should_skip(CheckId::Clippy, &args));
        assert!(!should_skip(CheckId::Test, &args));
        assert!(!should_skip(CheckId::Machete, &args));
        assert!(!should_skip(CheckId::Typos, &args));

        // Each flag skips only its corresponding check.
        args.no_fmt = true;
        assert!(should_skip(CheckId::Fmt, &args));
        assert!(!should_skip(CheckId::Check, &args));

        args.no_check = true;
        assert!(should_skip(CheckId::Check, &args));

        args.no_clippy = true;
        assert!(should_skip(CheckId::Clippy, &args));

        args.no_test = true;
        assert!(should_skip(CheckId::Test, &args));

        args.no_machete = true;
        assert!(should_skip(CheckId::Machete, &args));

        args.no_typos = true;
        assert!(should_skip(CheckId::Typos, &args));
    }

    #[test]
    fn test_fix_args_fmt_drops_check_flag() {
        let args = fix_args(CheckId::Fmt).expect("fmt should have fix args");
        assert_eq!(args, vec!["fmt"]);
        assert!(!args.contains(&"--check"));
    }

    #[test]
    fn test_fix_args_clippy_adds_fix_allow_dirty() {
        let args = fix_args(CheckId::Clippy).expect("clippy should have fix args");
        assert!(args.contains(&"--fix"));
        assert!(args.contains(&"--allow-dirty"));
        // Should still have the -D warnings after --
        assert!(args.contains(&"-D"));
        assert!(args.contains(&"warnings"));
    }

    #[test]
    fn test_fix_args_returns_none_for_unmodified_checks() {
        assert!(fix_args(CheckId::Check).is_none());
        assert!(fix_args(CheckId::Test).is_none());
        assert!(fix_args(CheckId::Machete).is_none());
        assert!(fix_args(CheckId::Typos).is_none());
    }

    #[test]
    fn test_determine_outcome_passed() {
        let outcome = determine_outcome(true, "all good\n".to_string(), false);
        assert!(matches!(outcome, CheckOutcome::Passed { output } if output == "all good\n"));
    }

    #[test]
    fn test_determine_outcome_failed() {
        let outcome = determine_outcome(false, "error: something wrong\n".to_string(), false);
        assert!(
            matches!(outcome, CheckOutcome::Failed { output } if output == "error: something wrong\n")
        );
    }

    #[test]
    fn test_determine_outcome_skipped_optional_tool_not_found() {
        let outcome = determine_outcome(
            false,
            "error: no such command: `machete`\n".to_string(),
            true,
        );
        assert!(matches!(outcome, CheckOutcome::Skipped));
    }

    #[test]
    fn test_determine_outcome_optional_but_real_failure() {
        // Optional tool that IS installed but fails with a real error should still fail.
        let outcome =
            determine_outcome(false, "error[E0308]: mismatched types\n".to_string(), true);
        assert!(
            matches!(outcome, CheckOutcome::Failed { output } if output == "error[E0308]: mismatched types\n")
        );
    }

    #[test]
    fn test_check_display_name() {
        assert_eq!(check_display_name("cargo", &["fmt"]), "cargo fmt");
        assert_eq!(
            check_display_name(
                "cargo",
                &[
                    "clippy",
                    "--all-targets",
                    "--fix",
                    "--allow-dirty",
                    "--",
                    "-D",
                    "warnings"
                ]
            ),
            "cargo clippy --all-targets --fix --allow-dirty -- -D warnings"
        );
    }

    #[test]
    fn test_typos_check_configuration() {
        let typos_check = CHECKS.iter().find(|c| c.id == CheckId::Typos);
        assert!(typos_check.is_some(), "typos check should exist");
        let check = typos_check.unwrap();
        assert_eq!(check.program, "typos");
        assert_eq!(check.args, &[] as &[&str]);
        assert!(!check.optional, "typos should not be optional");
    }
}
