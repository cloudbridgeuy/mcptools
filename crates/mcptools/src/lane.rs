use std::ffi::OsString;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use mcptools_core::lane::{CreateArgs, CreateOutput, ListOutput, State};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::Command;

type Result<T> = std::result::Result<T, String>;

const OUTPUT_LIMIT: usize = 1024 * 1024;
const TIMEOUT: Duration = Duration::from_secs(20);

pub struct Runner {
    home: tempfile::TempDir,
    path: OsString,
    git: OsString,
    wt: OsString,
    timeout: Duration,
}

impl Runner {
    pub fn new() -> Result<Self> {
        if !cfg!(unix) {
            return Err("Lane tools currently require Unix filesystem identity".into());
        }
        Ok(Self {
            home: tempfile::tempdir().map_err(|e| e.to_string())?,
            path: std::env::var_os("PATH").unwrap_or_default(),
            git: "git".into(),
            wt: "wt".into(),
            timeout: TIMEOUT,
        })
    }

    fn command(&self, binary: &OsString, repo: &Path, args: &[&str]) -> Command {
        let mut command = Command::new(binary);
        command
            .env_clear()
            .env("PATH", &self.path)
            .env("HOME", self.home.path())
            .env("XDG_CONFIG_HOME", self.home.path())
            .env("LC_ALL", "C")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_OPTIONAL_LOCKS", "0")
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_NO_LAZY_FETCH", "1")
            .env("WORKTRUNK_PROJECT_CONFIG_PATH", "")
            .env(
                "WORKTRUNK_SYSTEM_CONFIG_PATH",
                self.home.path().join("absent"),
            )
            .current_dir(repo)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        let overrides = [
            ("core.hooksPath", "/dev/null"),
            ("core.fsmonitor", "false"),
            ("core.untrackedCache", "false"),
            ("maintenance.auto", "false"),
            ("gc.auto", "0"),
            ("diff.external", ""),
            ("log.showSignature", "false"),
            ("core.logAllRefUpdates", "true"),
        ];
        command.env("GIT_CONFIG_COUNT", overrides.len().to_string());
        for (index, (key, value)) in overrides.iter().enumerate() {
            command
                .env(format!("GIT_CONFIG_KEY_{index}"), key)
                .env(format!("GIT_CONFIG_VALUE_{index}"), value);
        }
        #[cfg(unix)]
        command.process_group(0);
        command
    }

    async fn run(&self, binary: &OsString, repo: &Path, args: &[&str]) -> Result<Output> {
        let child = self
            .command(binary, repo, args)
            .spawn()
            .map_err(|e| e.to_string())?;
        let mut process = Process {
            pid: child.id(),
            child,
        };
        let mut stdout = process.child.stdout.take().ok_or("Missing stdout pipe")?;
        let mut stderr = process.child.stderr.take().ok_or("Missing stderr pipe")?;
        let count = AtomicUsize::new(0);
        let operation = futures::future::try_join3(
            read_capped(&mut stdout, &count),
            read_capped(&mut stderr, &count),
            async { process.child.wait().await.map_err(|e| e.to_string()) },
        );
        let result = tokio::time::timeout(self.timeout, operation).await;
        match result {
            Ok(Ok((stdout, stderr, status))) => {
                process.pid = None;
                Ok(Output {
                    stdout: String::from_utf8(stdout).map_err(|_| "Non-UTF-8 subprocess output")?,
                    stderr: String::from_utf8_lossy(&stderr).into_owned(),
                    code: status.code(),
                })
            }
            failure => {
                process.kill_group();
                let _ = process.child.start_kill();
                let _ = process.child.wait().await;
                Err(match failure {
                    Err(_) => "Lane subprocess timed out".into(),
                    Ok(Err(error)) => error,
                    _ => unreachable!(),
                })
            }
        }
    }

    async fn git(&self, repo: &Path, args: &[&str]) -> Result<String> {
        self.run(&self.git, repo, args).await?.success()
    }

    async fn executable_config(&self, repo: &Path) -> Result<()> {
        let output = self.run(&self.git, repo, &[
            "config", "--get-regexp",
            r"^(filter\..*\.(clean|smudge|process)|diff\..*\.(command|textconv)|merge\..*\.driver|include\.path|includeif\..*\.path|extensions\.partialclone|remote\..*\.promisor)$",
        ]).await?;
        match output.code {
            Some(1) if output.stdout.is_empty() => Ok(()),
            Some(0) => Err(
                "Configured Git filters, diff commands, merge drivers, includes, or promisor remotes are unsupported"
                    .into(),
            ),
            _ => Err(format!(
                "Cannot inspect Git configuration: {}",
                output.stderr
            )),
        }
    }
}

struct Process {
    child: tokio::process::Child,
    pid: Option<u32>,
}

#[cfg(unix)]
extern "C" {
    fn kill(pid: i32, sig: i32) -> i32;
}

impl Process {
    fn kill_group(&self) {
        #[cfg(unix)]
        if let Some(pid) = self.pid {
            unsafe { kill(-(pid as i32), 9) };
        }
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        self.kill_group();
    }
}

#[derive(Debug)]
struct Output {
    stdout: String,
    stderr: String,
    code: Option<i32>,
}

impl Output {
    fn success(self) -> Result<String> {
        if self.code == Some(0) {
            Ok(self.stdout)
        } else {
            Err(format!(
                "Lane subprocess failed (exit {:?}): {}; stdout: {}",
                self.code,
                self.stderr.trim(),
                self.stdout.trim()
            ))
        }
    }
}

async fn read_capped(pipe: &mut (impl AsyncRead + Unpin), count: &AtomicUsize) -> Result<Vec<u8>> {
    let mut output = Vec::new();
    let mut buffer = [0; 8192];
    loop {
        let length = pipe.read(&mut buffer).await.map_err(|e| e.to_string())?;
        if length == 0 {
            return Ok(output);
        }
        if count.fetch_add(length, Ordering::Relaxed) + length > OUTPUT_LIMIT {
            return Err("Lane subprocess exceeded combined 1 MiB output limit".into());
        }
        output.extend_from_slice(&buffer[..length]);
    }
}

fn text(path: &Path) -> Result<&str> {
    path.to_str()
        .ok_or_else(|| "Non-UTF-8 paths are unsupported".into())
}

fn canonical(path: &Path) -> Result<PathBuf> {
    path.canonicalize()
        .map_err(|e| format!("Cannot resolve {}: {e}", path.display()))
}

fn contained(path: &Path, root: &Path) -> Result<()> {
    let mut ancestor = path;
    while !ancestor.try_exists().map_err(|e| e.to_string())? {
        if std::fs::symlink_metadata(ancestor).is_ok() {
            return Err("Dangling symlink in lane storage path".into());
        }
        ancestor = ancestor.parent().ok_or("No existing path ancestor")?;
    }
    if !canonical(ancestor)?.starts_with(root) {
        return Err("Lane storage path escapes permitted root".into());
    }
    Ok(())
}

fn available_lane_path(path: &Path, root: &Path) -> Result<()> {
    contained(path, root)?;
    for ancestor in path.ancestors().take_while(|ancestor| *ancestor != root) {
        let metadata = match std::fs::symlink_metadata(ancestor) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error.to_string()),
        };
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err("Lane path ancestors must be real directories, not symlinks".into());
        }
        if ancestor == path {
            return Err("Lane path already exists".into());
        }
    }
    Ok(())
}

fn reject_storage_symlinks(path: &Path) -> Result<()> {
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.to_string()),
    };
    if metadata.file_type().is_symlink() {
        return Err("Symlinks in Git or Worktrunk storage are unsupported".into());
    }
    if metadata.is_dir() {
        for entry in std::fs::read_dir(path).map_err(|error| error.to_string())? {
            reject_storage_symlinks(&entry.map_err(|error| error.to_string())?.path())?;
        }
    }
    Ok(())
}

pub fn permitted_repo(repo: &str, allowed: Option<OsString>) -> Result<PathBuf> {
    let allowed = allowed
        .filter(|value| !value.is_empty())
        .ok_or("MCPTOOLS_LANE_REPOS is unset or empty; lane access denied")?;
    if repo.is_empty() || !Path::new(repo).is_absolute() {
        return Err("repo must be an absolute repository root".into());
    }
    let repo = canonical(Path::new(repo))?;
    if allowed == "*" {
        return Ok(repo);
    }
    let roots = std::env::split_paths(&allowed)
        .map(|path| {
            if !path.is_absolute() || path.as_os_str().is_empty() {
                return Err("Allowed lane repository roots must be absolute".into());
            }
            canonical(&path)
        })
        .collect::<Result<Vec<_>>>()?;
    if !roots.contains(&repo) {
        return Err("Repository is not in MCPTOOLS_LANE_REPOS".into());
    }
    Ok(repo)
}

struct Repository {
    root: PathBuf,
    common: PathBuf,
    registry: PathBuf,
}

impl Repository {
    async fn open(runner: &Runner, root: PathBuf) -> Result<Self> {
        let bare = runner
            .git(&root, &["rev-parse", "--is-bare-repository"])
            .await?;
        let actual = match bare.trim() {
            "true" => {
                runner
                    .git(&root, &["rev-parse", "--absolute-git-dir"])
                    .await?
            }
            "false" => runner.git(&root, &["rev-parse", "--show-toplevel"]).await?,
            _ => return Err("Unsupported Git repository identity".into()),
        };
        if canonical(Path::new(actual.trim()))? != root {
            return Err("repo must be the repository root, not a descendant".into());
        }
        let common = runner
            .git(
                &root,
                &["rev-parse", "--path-format=absolute", "--git-common-dir"],
            )
            .await?;
        let common = canonical(Path::new(common.trim()))?;
        if !common.starts_with(&root) {
            return Err("Git common directory is outside permitted repository".into());
        }
        let registry = common.join("mcptools-lanes");
        contained(&registry, &common)?;
        Ok(Self {
            root,
            common,
            registry,
        })
    }

    fn records(&self) -> Result<Vec<Record>> {
        contained(&self.registry, &self.common)?;
        if !self.registry.exists() {
            return Ok(Vec::new());
        }
        std::fs::read_dir(&self.registry)
            .map_err(|e| e.to_string())?
            .map(|entry| {
                let path = entry.map_err(|e| e.to_string())?.path();
                if path
                    .file_name()
                    .and_then(|s| s.to_str())
                    .is_some_and(|s| s.starts_with(".tmp"))
                {
                    return Ok(None);
                }
                let metadata = std::fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
                if !metadata.is_file() {
                    return Err(
                        "Ownership registry entries must be regular files, not symlinks".into(),
                    );
                }
                let mut bytes = Vec::new();
                std::fs::File::open(&path)
                    .map_err(|e| e.to_string())?
                    .take(65537)
                    .read_to_end(&mut bytes)
                    .map_err(|e| e.to_string())?;
                if bytes.len() > 65536 {
                    return Err("Ownership record exceeds 64 KiB".into());
                }
                let record: Record = serde_json::from_slice(&bytes)
                    .map_err(|e| format!("Corrupt lane registry: {e}"))?;
                if record.version != 1
                    || record.id.len() != 32
                    || !record.id.bytes().all(|b| b.is_ascii_hexdigit())
                    || !mcptools_core::lane::valid_head(&record.creation_head)
                    || mcptools_core::lane::ref_input(&record.branch).is_err()
                    || record.worktree.path
                        != mcptools_core::lane::worktree_path(&self.root, &record.branch)?
                    || path.file_name() != Some(std::ffi::OsStr::new(&record.filename()))
                {
                    return Err("Unsupported lane ownership record".into());
                }
                Ok(Some(record))
            })
            .collect::<Result<Vec<_>>>()
            .map(|records| records.into_iter().flatten().collect())
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Identity {
    path: PathBuf,
    device: u64,
    inode: u64,
}

fn identity(path: &Path) -> Result<Identity> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let path = canonical(path)?;
        let metadata = path.metadata().map_err(|e| e.to_string())?;
        Ok(Identity {
            path,
            device: metadata.dev(),
            inode: metadata.ino(),
        })
    }
    #[cfg(not(unix))]
    Err("Lane ownership currently requires Unix filesystem identity".into())
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    version: u32,
    id: String,
    repo: Identity,
    common: Identity,
    worktree: Identity,
    git_dir: Identity,
    branch_log: Identity,
    branch: String,
    creation_head: String,
}

impl Record {
    fn filename(&self) -> String {
        format!(
            "{}.json",
            self.branch
                .bytes()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        )
    }

    fn persist(&self, registry: &Path, common: &Path) -> Result<()> {
        contained(registry, common)?;
        std::fs::create_dir_all(registry).map_err(|e| e.to_string())?;
        contained(registry, common)?;
        let mut file = tempfile::Builder::new()
            .prefix(".tmp")
            .tempfile_in(registry)
            .map_err(|e| e.to_string())?;
        file.write_all(&serde_json::to_vec(self).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        file.as_file().sync_all().map_err(|e| e.to_string())?;
        file.persist_noclobber(registry.join(self.filename()))
            .map_err(|e| e.to_string())?;
        std::fs::File::open(registry)
            .and_then(|file| file.sync_all())
            .map_err(|e| e.to_string())?;
        std::fs::File::open(common)
            .and_then(|file| file.sync_all())
            .map_err(|e| e.to_string())
    }
}

async fn worktree_identity(runner: &Runner, path: &Path, repo: &Repository) -> Result<Identity> {
    let common = runner
        .git(
            path,
            &["rev-parse", "--path-format=absolute", "--git-common-dir"],
        )
        .await?;
    if canonical(Path::new(common.trim()))? != repo.common {
        return Err("Worktree belongs to another Git repository".into());
    }
    let dir = runner
        .git(path, &["rev-parse", "--absolute-git-dir"])
        .await?;
    identity(Path::new(dir.trim()))
}

pub async fn list(runner: &Runner, root: PathBuf) -> Result<ListOutput> {
    let repo = Repository::open(runner, root).await?;
    let records = repo.records()?;
    let porcelain = runner
        .git(&repo.root, &["worktree", "list", "--porcelain", "-z"])
        .await?;
    let mut lanes = mcptools_core::lane::parse_worktrees(&porcelain, text(&repo.root)?)?;
    for lane in &mut lanes {
        let path = Path::new(&lane.path);
        let Ok(git_dir) = worktree_identity(runner, path, &repo).await else {
            continue;
        };
        lane.current = canonical(path).is_ok_and(|path| path == repo.root);
        if runner.executable_config(path).await.is_ok() {
            if let Ok(status) = runner
                .git(
                    path,
                    &[
                        "status",
                        "--porcelain=v1",
                        "-z",
                        "--untracked-files=all",
                        "--ignore-submodules=all",
                    ],
                )
                .await
            {
                lane.state = if !status.is_empty() {
                    State::Dirty
                } else {
                    match runner.git(path, &["ls-files", "--stage", "-z"]).await {
                        Ok(files) if !files.split('\0').any(|file| file.starts_with("160000 ")) => {
                            State::Clean
                        }
                        _ => State::Unknown,
                    }
                };
            }
        }
        for record in &records {
            if lane.branch.as_deref() == Some(&record.branch)
                && identity(&repo.root)? == record.repo
                && identity(&repo.common)? == record.common
                && identity(path).is_ok_and(|id| id == record.worktree)
                && git_dir == record.git_dir
                && identity(&repo.common.join("logs/refs/heads").join(&record.branch))
                    .is_ok_and(|id| id == record.branch_log)
            {
                if let Some(head) = &lane.head {
                    if runner
                        .run(
                            &runner.git,
                            &repo.root,
                            &["merge-base", "--is-ancestor", &record.creation_head, head],
                        )
                        .await?
                        .code
                        == Some(0)
                    {
                        lane.id = Some(record.id.clone());
                        lane.managed = true;
                    }
                }
            }
        }
    }
    Ok(ListOutput { lanes })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SwitchOutput {
    action: String,
    branch: Option<String>,
    path: PathBuf,
    created_branch: Option<bool>,
    base_branch: Option<String>,
    from_remote: Option<String>,
}

pub async fn create(runner: &Runner, root: PathBuf, args: CreateArgs) -> Result<CreateOutput> {
    let repo = Repository::open(runner, root).await?;
    runner.executable_config(&repo.root).await?;
    let branch_ref = mcptools_core::lane::ref_input(&args.branch)?;
    let base_ref = mcptools_core::lane::ref_input(&args.base)?;
    for reference in [&branch_ref, &base_ref] {
        runner
            .git(&repo.root, &["check-ref-format", reference])
            .await?;
    }
    let existing = runner
        .run(
            &runner.git,
            &repo.root,
            &["show-ref", "--verify", "--quiet", &branch_ref],
        )
        .await?;
    if existing.code != Some(1) {
        return Err("New branch already exists or cannot be checked".into());
    }
    let base_head = runner
        .git(
            &repo.root,
            &["rev-parse", "--verify", &format!("{base_ref}^{{commit}}")],
        )
        .await?
        .trim()
        .to_string();
    if !mcptools_core::lane::valid_head(&base_head) {
        return Err("Invalid local base HEAD".into());
    }
    for storage in ["config", "refs", "logs", "worktrees", "wt"] {
        reject_storage_symlinks(&repo.common.join(storage))?;
    }
    if repo
        .records()?
        .iter()
        .any(|record| record.branch == args.branch)
    {
        return Err("Branch has an existing ownership record; refusing reuse".into());
    }
    let id = format!("{:032x}", rand::random::<u128>());
    let path = mcptools_core::lane::worktree_path(&repo.root, &args.branch)?;
    available_lane_path(&path, &repo.root)?;
    let target = text(&path)?;
    if target.contains("{{") || target.contains("{%") {
        return Err("Repository path contains Worktrunk template syntax".into());
    }
    let version = runner
        .run(&runner.wt, &repo.root, &["--version"])
        .await?
        .success()?;
    if version.trim() != "wt v0.77.0" {
        return Err(format!(
            "lane_create requires verified Worktrunk 0.77.0; reported version: {}",
            version.trim()
        ));
    }
    let mut config =
        tempfile::NamedTempFile::new_in(runner.home.path()).map_err(|e| e.to_string())?;
    config.write_all(format!("worktree-path = {}\nskip-shell-integration-prompt = true\n[list]\nfull = false\nsummary = false\njson-schema = 2\n", serde_json::to_string(target).map_err(|e| e.to_string())?).as_bytes()).map_err(|e| e.to_string())?;
    let mutation = async {
        let output = runner
            .run(
                &runner.wt,
                &repo.root,
                &[
                    "-C",
                    text(&repo.root)?,
                    "--config",
                    text(config.path())?,
                    "switch",
                    "--create",
                    "--base",
                    &base_ref,
                    "--no-hooks",
                    "--no-cd",
                    "--format=json",
                    &args.branch,
                ],
            )
            .await?
            .success()?;
        let returned: SwitchOutput = serde_json::from_str(&output).map_err(|e| {
            format!("Unsupported Worktrunk create result: {e}; reported output: {output}")
        })?;
        if returned.action != "created"
            || returned.created_branch != Some(true)
            || returned.branch.as_deref() != Some(&args.branch)
            || returned.from_remote.is_some()
            || returned.path != path
        {
            return Err(format!(
                "Unexpected Worktrunk identity: path={}, branch={:?}",
                returned.path.display(),
                returned.branch
            ));
        }
        if canonical(&returned.path)? != path {
            return Err("Actual lane path differs from requested safe path".into());
        }
        contained(&path, &repo.root)?;
        let git_dir = worktree_identity(runner, &path, &repo).await?;
        let actual_branch = runner.git(&path, &["symbolic-ref", "HEAD"]).await?;
        let actual_head = runner
            .git(&path, &["rev-parse", "--verify", "HEAD"])
            .await?;
        if actual_branch.trim() != branch_ref || actual_head.trim() != base_head {
            return Err("Created lane branch or HEAD differs from preflight identity".into());
        }
        Record {
            version: 1,
            id: id.clone(),
            repo: identity(&repo.root)?,
            common: identity(&repo.common)?,
            worktree: identity(&path)?,
            git_dir,
            branch_log: identity(&repo.common.join("logs/refs/heads").join(&args.branch))?,
            branch: args.branch.clone(),
            creation_head: base_head.clone(),
        }
        .persist(&repo.registry, &repo.common)?;
        Ok(CreateOutput {
            id,
            path: target.into(),
            branch: args.branch.clone(),
            base_head,
        })
    }
    .await;
    mutation.map_err(|error| format!("PARTIAL_MUTATION_POSSIBLE: {error}; requested path={target}; branch={}; path exists={}. No rollback or retry was performed. Inspect Git worktree/branch state before any retry.", args.branch, path.exists()))
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::{symlink, PermissionsExt};

    fn executable(path: &Path, script: &str) {
        std::fs::write(path, script).unwrap();
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)).unwrap();
    }

    async fn repository() -> (tempfile::TempDir, Runner, PathBuf) {
        let directory = tempfile::tempdir().unwrap();
        let runner = Runner::new().unwrap();
        let root = directory.path().join("repo");
        std::fs::create_dir(&root).unwrap();
        runner
            .git(&root, &["init", "--initial-branch=main"])
            .await
            .unwrap();
        runner
            .git(
                &root,
                &[
                    "-c",
                    "user.name=Lane",
                    "-c",
                    "user.email=lane@example.invalid",
                    "commit",
                    "--allow-empty",
                    "-m",
                    "base",
                ],
            )
            .await
            .unwrap();
        (directory, runner, canonical(&root).unwrap())
    }

    fn fake_wt(runner: &mut Runner, after_create: &str) {
        let path = runner.home.path().join("fake-wt");
        executable(
            &path,
            &format!(
                r#"#!/bin/sh
set -eu
if [ "$1" = --version ]; then printf 'wt v0.77.0\n'; exit 0; fi
[ "$#" = 12 ]
[ "$1" = -C ] && [ "$3" = --config ] && [ "$5" = switch ]
[ "$6" = --create ] && [ "$7" = --base ] && [ "$9" = --no-hooks ]
[ "${{10}}" = --no-cd ] && [ "${{11}}" = --format=json ]
[ -z "$WORKTRUNK_PROJECT_CONFIG_PATH" ]
[ "$GIT_CONFIG_NOSYSTEM" = 1 ] && [ "$GIT_OPTIONAL_LOCKS" = 0 ]
[ -z "${{GIT_DIR+x}}" ] && [ -z "${{GIT_WORK_TREE+x}}" ]
[ -z "${{GIT_CONFIG_PARAMETERS+x}}" ] && [ -z "${{WORKTRUNK__LIST__FULL+x}}" ]
grep -q '^full = false$' "$4"
grep -q '^summary = false$' "$4"
grep -q '^json-schema = 2$' "$4"
target=$(sed -n 's/^worktree-path = "\(.*\)"$/\1/p' "$4")
git -C "$2" worktree add -b "${{12}}" -- "$target" "$8" >/dev/null 2>&1
{after_create}
printf '{{"action":"created","branch":"%s","path":"%s","created_branch":true,"base_branch":"%s"}}\n' "${{12}}" "$target" "$8"
"#
            ),
        );
        runner.wt = path.into_os_string();
    }

    fn args(root: &Path, branch: &str) -> CreateArgs {
        CreateArgs {
            repo: text(root).unwrap().into(),
            branch: branch.into(),
            base: "main".into(),
        }
    }

    #[test]
    fn allowlist_denies_unset_empty_outside_unresolved_and_symlink_escape() {
        let directory = tempfile::tempdir().unwrap();
        let root = canonical(directory.path()).unwrap();
        let repo = text(&root).unwrap();
        assert!(permitted_repo(repo, None).is_err());
        assert!(permitted_repo(repo, Some("".into())).is_err());
        assert!(permitted_repo(repo, Some(root.join("missing").into_os_string())).is_err());
        assert!(permitted_repo(repo, Some("/".into())).is_err());
        assert_eq!(
            permitted_repo(repo, Some(root.clone().into_os_string())).unwrap(),
            root
        );
        let outside = tempfile::tempdir().unwrap();
        symlink(outside.path(), root.join("alias")).unwrap();
        assert!(permitted_repo(
            text(&root.join("alias")).unwrap(),
            Some(root.clone().into_os_string())
        )
        .is_err());
        assert!(contained(&root.join("alias/new"), &root).is_err());
        symlink(root.join("missing"), root.join("dangling")).unwrap();
        assert!(contained(&root.join("dangling/new"), &root).is_err());
    }

    #[tokio::test]
    async fn wildcard_admits_distinct_roots_without_bypassing_repository_guards() {
        let repositories = [repository().await, repository().await];
        assert!(permitted_repo(
            text(&repositories[1].2).unwrap(),
            Some(repositories[0].2.clone().into_os_string())
        )
        .is_err());
        for (_directory, mut runner, root) in repositories {
            let permitted =
                permitted_repo(text(&root.join(".")).unwrap(), Some("*".into())).unwrap();
            assert_eq!(permitted, root);
            assert_eq!(
                list(&runner, permitted.clone()).await.unwrap().lanes.len(),
                1
            );
            fake_wt(&mut runner, "");
            let created = create(&runner, permitted, args(&root, "topic"))
                .await
                .unwrap();
            assert!(list(&runner, root.clone())
                .await
                .unwrap()
                .lanes
                .iter()
                .any(|lane| lane.id.as_deref() == Some(&created.id) && lane.managed));
            let child = root.join("child");
            std::fs::create_dir(&child).unwrap();
            for (path, error) in [
                (child, "not a descendant"),
                (PathBuf::from(&created.path), "common directory is outside"),
                (canonical(runner.home.path()).unwrap(), "subprocess failed"),
            ] {
                let permitted = permitted_repo(text(&path).unwrap(), Some("*".into())).unwrap();
                assert!(list(&runner, permitted.clone())
                    .await
                    .unwrap_err()
                    .contains(error));
                assert!(create(&runner, permitted, args(&path, "other"))
                    .await
                    .unwrap_err()
                    .contains(error));
            }
        }
    }

    #[test]
    fn wildcard_requires_exact_operator_value_and_valid_absolute_request() {
        let directory = tempfile::tempdir().unwrap();
        let root = canonical(directory.path()).unwrap();
        let repo = text(&root).unwrap();
        for allowed in [" *", "* ", "*\n", "**", "*:/", "/:*"] {
            assert!(permitted_repo(repo, Some(allowed.into())).is_err());
        }
        for paths in [
            [root.clone(), PathBuf::from("*")],
            [root.clone(), root.join("missing")],
        ] {
            assert!(permitted_repo(repo, Some(std::env::join_paths(paths).unwrap())).is_err());
        }
        for request in ["", ".", "repo", "*", text(&root.join("missing")).unwrap()] {
            assert!(permitted_repo(request, Some("*".into())).is_err());
        }
        assert!(permitted_repo(text(&root.join("*")).unwrap(), Some("*".into())).is_err());
    }

    #[tokio::test]
    async fn list_is_native_read_only_and_reports_dirty_bare_and_invalid_paths() {
        let (_directory, mut runner, root) = repository().await;
        runner.wt = "/does/not/exist".into();
        let initial = list(&runner, root.clone()).await.unwrap();
        assert_eq!(initial.lanes[0].state, State::Clean);
        assert!(initial.lanes[0].main && initial.lanes[0].current);
        assert!(!root.join(".git/wt").exists());
        let index = root.join(".git/index");
        let index_before = std::fs::read(&index).ok();
        std::fs::write(root.join("dirty"), "changed").unwrap();
        assert_eq!(
            list(&runner, root.clone()).await.unwrap().lanes[0].state,
            State::Dirty
        );
        assert_eq!(std::fs::read(&index).ok(), index_before);
        std::fs::create_dir(root.join("child")).unwrap();
        assert!(list(&runner, root.join("child")).await.is_err());
        let bare = runner.home.path().join("bare");
        std::fs::create_dir(&bare).unwrap();
        runner.git(&bare, &["init", "--bare"]).await.unwrap();
        let bare = canonical(&bare).unwrap();
        let lanes = list(&runner, bare.clone()).await.unwrap().lanes;
        assert_eq!(lanes[0].head, None);
        assert_eq!(lanes[0].state, State::Unknown);
        let tree = runner.git(&bare, &["mktree"]).await.unwrap();
        let commit = runner
            .git(
                &bare,
                &[
                    "-c",
                    "user.name=Lane",
                    "-c",
                    "user.email=lane@example.invalid",
                    "commit-tree",
                    tree.trim(),
                    "-m",
                    "base",
                ],
            )
            .await
            .unwrap();
        runner
            .git(&bare, &["update-ref", "refs/heads/main", commit.trim()])
            .await
            .unwrap();
        fake_wt(&mut runner, "");
        let created = create(&runner, bare.clone(), args(&bare, "bare-topic"))
            .await
            .unwrap();
        assert!(list(&runner, bare)
            .await
            .unwrap()
            .lanes
            .iter()
            .any(|lane| lane.id.as_deref() == Some(&created.id) && lane.managed));
    }

    #[tokio::test]
    async fn create_persists_ownership_skips_hooks_and_rejects_reuse() {
        let (_directory, mut runner, root) = repository().await;
        let marker = root.join("hook-ran");
        executable(
            &root.join(".git/hooks/post-checkout"),
            &format!("#!/bin/sh\ntouch '{}'\n", marker.display()),
        );
        fake_wt(&mut runner, "");
        let created = create(&runner, root.clone(), args(&root, "feature/topic"))
            .await
            .unwrap();
        assert!(!marker.exists());
        assert_eq!(
            Path::new(&created.path),
            root.join(".worktrees/feature/topic")
        );
        assert_eq!(created.id.len(), 32);
        let fresh = Runner::new().unwrap();
        let lanes = list(&fresh, root.clone()).await.unwrap().lanes;
        assert!(lanes
            .iter()
            .any(|lane| lane.id.as_deref() == Some(&created.id) && lane.managed));
        fresh
            .git(
                Path::new(&created.path),
                &[
                    "-c",
                    "user.name=Lane",
                    "-c",
                    "user.email=lane@example.invalid",
                    "commit",
                    "--allow-empty",
                    "-m",
                    "next",
                ],
            )
            .await
            .unwrap();
        assert!(list(&fresh, root.clone())
            .await
            .unwrap()
            .lanes
            .iter()
            .any(|lane| lane.id.as_deref() == Some(&created.id) && lane.managed));
        assert!(create(&runner, root.clone(), args(&root, "feature/topic"))
            .await
            .unwrap_err()
            .contains("already exists"));
        fresh
            .git(Path::new(&created.path), &["checkout", "--detach"])
            .await
            .unwrap();
        assert!(!list(&fresh, root.clone())
            .await
            .unwrap()
            .lanes
            .iter()
            .any(|lane| lane.managed));
        for branch in ["--yes", "pr:123", "a..b", "@{-1}"] {
            assert!(create(&runner, root.clone(), args(&root, branch))
                .await
                .is_err());
        }
        let mut remote = args(&root, "another");
        remote.base = "origin/main".into();
        assert!(create(&runner, root.clone(), remote).await.is_err());
    }

    #[tokio::test]
    async fn executable_filters_and_storage_symlinks_fail_before_creation() {
        let (_directory, mut runner, root) = repository().await;
        fake_wt(&mut runner, "");
        runner
            .git(
                &root,
                &["config", "filter.unsafe.smudge", "touch should-not-run"],
            )
            .await
            .unwrap();
        assert!(create(&runner, root.clone(), args(&root, "topic"))
            .await
            .unwrap_err()
            .contains("filters"));
        assert_eq!(
            list(&runner, root.clone()).await.unwrap().lanes[0].state,
            State::Unknown
        );
        assert!(!root.join("should-not-run").exists());
        runner
            .git(&root, &["config", "--unset", "filter.unsafe.smudge"])
            .await
            .unwrap();
        runner
            .git(&root, &["config", "remote.origin.promisor", "true"])
            .await
            .unwrap();
        assert!(create(&runner, root.clone(), args(&root, "topic"))
            .await
            .unwrap_err()
            .contains("promisor"));
        assert_eq!(
            list(&runner, root.clone()).await.unwrap().lanes[0].state,
            State::Unknown
        );
        runner
            .git(&root, &["config", "--unset", "remote.origin.promisor"])
            .await
            .unwrap();
        let outside = tempfile::tempdir().unwrap();
        symlink(outside.path(), root.join(".worktrees")).unwrap();
        assert!(create(&runner, root.clone(), args(&root, "topic"))
            .await
            .unwrap_err()
            .contains("escapes"));
        assert_eq!(std::fs::read_dir(outside.path()).unwrap().count(), 0);
    }

    #[tokio::test]
    async fn nested_branch_ancestors_and_path_collisions_fail_before_mutation() {
        let (_directory, mut runner, root) = repository().await;
        runner.wt = "/must/not/run".into();
        let outside = tempfile::tempdir().unwrap();
        let storage = root.join(".worktrees");
        std::fs::create_dir(&storage).unwrap();
        let parent = storage.join("feature");
        symlink(outside.path(), &parent).unwrap();
        assert!(
            create(&runner, root.clone(), args(&root, "feature/missing/topic"))
                .await
                .unwrap_err()
                .contains("escapes")
        );
        assert_eq!(std::fs::read_dir(outside.path()).unwrap().count(), 0);
        std::fs::remove_file(&parent).unwrap();
        symlink(root.join("missing"), &parent).unwrap();
        assert!(create(&runner, root.clone(), args(&root, "feature/topic"))
            .await
            .unwrap_err()
            .contains("Dangling"));
        std::fs::remove_file(&parent).unwrap();
        symlink(&root, &parent).unwrap();
        assert!(create(&runner, root.clone(), args(&root, "feature/topic"))
            .await
            .unwrap_err()
            .contains("symlinks"));
        std::fs::remove_file(&parent).unwrap();
        std::fs::write(&parent, "occupied").unwrap();
        assert!(create(&runner, root.clone(), args(&root, "feature/topic"))
            .await
            .unwrap_err()
            .contains("directory"));
        std::fs::remove_file(&parent).unwrap();
        std::fs::create_dir_all(parent.join("topic")).unwrap();
        assert!(create(&runner, root.clone(), args(&root, "feature/topic"))
            .await
            .unwrap_err()
            .contains("already exists"));
        assert_eq!(
            runner
                .run(
                    &runner.git,
                    &root,
                    &[
                        "show-ref",
                        "--verify",
                        "--quiet",
                        "refs/heads/feature/topic"
                    ]
                )
                .await
                .unwrap()
                .code,
            Some(1)
        );
        fake_wt(&mut runner, "");
        let created = create(&runner, root.clone(), args(&root, "feature/new/deep"))
            .await
            .unwrap();
        assert_eq!(Path::new(&created.path), storage.join("feature/new/deep"));
        assert!(list(&runner, root)
            .await
            .unwrap()
            .lanes
            .iter()
            .any(|lane| lane.id.as_deref() == Some(&created.id) && lane.managed));
    }

    #[tokio::test]
    async fn ownership_rejects_changed_reflogs_conflicting_records_and_wrong_paths() {
        let (_directory, mut runner, root) = repository().await;
        fake_wt(&mut runner, "");
        let created = create(&runner, root.clone(), args(&root, "feature/topic"))
            .await
            .unwrap();
        let repo = Repository::open(&runner, root.clone()).await.unwrap();
        let mut records = repo.records().unwrap();
        let record = records.pop().unwrap();
        let file = repo.registry.join(record.filename());
        let original = std::fs::read(&file).unwrap();
        assert!(record.persist(&repo.registry, &repo.common).is_err());
        assert_eq!(std::fs::read(&file).unwrap(), original);
        let log = &record.branch_log.path;
        std::fs::rename(log, log.with_extension("old")).unwrap();
        std::fs::copy(log.with_extension("old"), log).unwrap();
        assert!(!list(&runner, root.clone())
            .await
            .unwrap()
            .lanes
            .iter()
            .any(|lane| lane.managed));
        let mut value: serde_json::Value = serde_json::from_slice(&original).unwrap();
        value["worktree"]["path"] = root
            .join(".worktrees")
            .join(&created.id)
            .to_string_lossy()
            .into_owned()
            .into();
        std::fs::write(&file, serde_json::to_vec(&value).unwrap()).unwrap();
        assert!(list(&runner, root.clone())
            .await
            .unwrap_err()
            .contains("ownership record"));
        value["branch"] = "../../outside".into();
        std::fs::write(&file, serde_json::to_vec(&value).unwrap()).unwrap();
        assert!(list(&runner, root).await.is_err());
    }

    #[tokio::test]
    async fn registry_corruption_unknown_worktrees_locks_and_includes_fail_closed() {
        let (_directory, mut runner, root) = repository().await;
        fake_wt(&mut runner, "");
        let created = create(&runner, root.clone(), args(&root, "topic"))
            .await
            .unwrap();
        runner
            .git(&root, &["worktree", "lock", &created.path])
            .await
            .unwrap();
        let listed = list(&runner, root.clone()).await.unwrap();
        assert!(listed
            .lanes
            .iter()
            .any(|lane| lane.id.as_deref() == Some(&created.id) && lane.locked == Some(true)));
        let absent = root.join("missing-lane");
        std::fs::rename(&created.path, &absent).unwrap();
        let listed = list(&runner, root.clone()).await.unwrap();
        let missing = listed
            .lanes
            .iter()
            .find(|lane| lane.path == created.path)
            .unwrap();
        assert_eq!(missing.state, State::Unknown);
        assert!(!missing.managed);
        let common = root.join(".git");
        let registry = common.join("mcptools-lanes");
        let file = std::fs::read_dir(&registry)
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        std::fs::write(&file, "corrupt").unwrap();
        assert!(list(&runner, root.clone())
            .await
            .unwrap_err()
            .contains("Corrupt"));
        assert!(create(&runner, root.clone(), args(&root, "other"))
            .await
            .unwrap_err()
            .contains("Corrupt"));
        let (_directory, mut runner, root) = repository().await;
        fake_wt(&mut runner, "");
        runner
            .git(
                &root,
                &["config", "includeIf.onbranch:other.path", "nonexistent"],
            )
            .await
            .unwrap();
        assert!(create(&runner, root.clone(), args(&root, "other"))
            .await
            .unwrap_err()
            .contains("includes"));
    }

    #[test]
    fn subprocess_configuration_is_explicit_and_has_no_git_selection_overrides() {
        let runner = Runner::new().unwrap();
        let command = runner.command(&runner.git, runner.home.path(), &["status"]);
        let environment: std::collections::BTreeMap<_, _> = command.as_std().get_envs().collect();
        for key in [
            "GIT_DIR",
            "GIT_WORK_TREE",
            "GIT_COMMON_DIR",
            "GIT_CONFIG_PARAMETERS",
            "WORKTRUNK__LIST__FULL",
        ] {
            assert!(!environment.contains_key(std::ffi::OsStr::new(key)));
        }
        for (key, value) in [
            ("GIT_CONFIG_NOSYSTEM", "1"),
            ("GIT_CONFIG_GLOBAL", "/dev/null"),
            ("GIT_OPTIONAL_LOCKS", "0"),
            ("WORKTRUNK_PROJECT_CONFIG_PATH", ""),
            ("GIT_CONFIG_KEY_0", "core.hooksPath"),
            ("GIT_CONFIG_VALUE_0", "/dev/null"),
            ("GIT_CONFIG_KEY_1", "core.fsmonitor"),
            ("GIT_CONFIG_VALUE_1", "false"),
        ] {
            assert_eq!(
                environment[std::ffi::OsStr::new(key)],
                Some(std::ffi::OsStr::new(value))
            );
        }
        assert_eq!(
            command.as_std().get_args().collect::<Vec<_>>(),
            vec![std::ffi::OsStr::new("status")]
        );
    }

    #[tokio::test]
    async fn partial_creation_and_registration_failure_do_not_roll_back() {
        for after in ["exit 1", "printf corrupt > \"$2/.git/mcptools-lanes\""] {
            let (_directory, mut runner, root) = repository().await;
            fake_wt(&mut runner, after);
            let error = create(&runner, root.clone(), args(&root, "topic"))
                .await
                .unwrap_err();
            assert!(error.contains("PARTIAL_MUTATION_POSSIBLE"));
            assert!(error.contains("path exists=true"));
            assert!(error.contains("branch=topic"));
            runner
                .git(&root, &["show-ref", "--verify", "refs/heads/topic"])
                .await
                .unwrap();
            assert_eq!(
                std::fs::read_dir(root.join(".worktrees")).unwrap().count(),
                1
            );
        }
    }

    #[tokio::test]
    async fn concurrent_distinct_creates_register_without_lost_updates() {
        let (_directory, mut runner, root) = repository().await;
        fake_wt(&mut runner, "");
        let (a, b) = tokio::join!(
            create(&runner, root.clone(), args(&root, "a")),
            create(&runner, root.clone(), args(&root, "b"))
        );
        assert!(a.is_ok(), "{a:?}");
        assert!(b.is_ok(), "{b:?}");
        assert_eq!(
            list(&runner, root)
                .await
                .unwrap()
                .lanes
                .iter()
                .filter(|lane| lane.managed)
                .count(),
            2
        );
    }

    #[tokio::test]
    async fn subprocess_timeout_and_combined_output_cap() {
        let mut runner = Runner::new().unwrap();
        let program = runner.home.path().join("child");
        executable(&program, "#!/bin/sh\nsleep 30\n");
        runner.timeout = Duration::from_millis(50);
        assert!(runner
            .run(&program.clone().into_os_string(), runner.home.path(), &[])
            .await
            .unwrap_err()
            .contains("timed out"));
        executable(&program, "#!/bin/sh\nyes output\n");
        runner.timeout = TIMEOUT;
        assert!(runner
            .run(&program.into_os_string(), runner.home.path(), &[])
            .await
            .unwrap_err()
            .contains("output limit"));
    }

    #[tokio::test]
    async fn subprocess_cancellation_and_timeout_kill_descendants() {
        let directory = tempfile::tempdir().unwrap();
        for cancel in [false, true] {
            let mut runner = Runner::new().unwrap();
            runner.timeout = if cancel {
                TIMEOUT
            } else {
                Duration::from_millis(100)
            };
            let program = runner.home.path().join("child");
            let started = directory.path().join(format!("started-{cancel}"));
            let escaped = directory.path().join(format!("escaped-{cancel}"));
            executable(
                &program,
                "#!/bin/sh\n(/bin/sleep 0.5; printf escaped > \"$2\") &\nprintf started > \"$1\"\nwait\n",
            );
            let task_started = started.clone();
            let task_escaped = escaped.clone();
            let mut task = tokio::spawn(async move {
                runner
                    .run(
                        &program.into_os_string(),
                        runner.home.path(),
                        &[text(&task_started).unwrap(), text(&task_escaped).unwrap()],
                    )
                    .await
            });
            if cancel {
                tokio::select! {
                    result = &mut task => panic!("Subprocess stopped before cancellation: {result:?}"),
                    ready = tokio::time::timeout(Duration::from_secs(10), async {
                        while !started.exists() {
                            tokio::time::sleep(Duration::from_millis(5)).await;
                        }
                    }) => ready.unwrap(),
                }
                task.abort();
                assert!(task.await.unwrap_err().is_cancelled());
            } else {
                assert!(task.await.unwrap().unwrap_err().contains("timed out"));
            }
            tokio::time::sleep(Duration::from_millis(600)).await;
            assert!(!escaped.exists());
        }
    }
}
