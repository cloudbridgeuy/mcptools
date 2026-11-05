use crate::prelude::*;
use mcptools_core::chrome::{
    orphaned_and_ours, recorded_chrome_matches, ChromeRecord, ProcessIdentity,
};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[derive(Debug, clap::Args)]
pub struct App {
    #[arg(long)]
    record: PathBuf,
}

pub(crate) struct LaunchGuard {
    record: PathBuf,
    pub(crate) profile: PathBuf,
    armed: bool,
}

fn directory() -> Result<PathBuf> {
    Ok(std::path::absolute(
        crate::plan_store::data_dir()?.join("chrome"),
    )?)
}

pub(crate) fn inspect(pid: u32) -> Result<Option<ProcessIdentity>> {
    Ok(inspect_with_parent(pid)?.map(|(process, _)| process))
}

fn inspect_with_parent(pid: u32) -> Result<Option<(ProcessIdentity, u32)>> {
    if pid <= 1 {
        return Ok(None);
    }
    let output = Command::new("ps")
        .env("LC_ALL", "C")
        .args([
            "-ww",
            "-p",
            &pid.to_string(),
            "-o",
            "ppid=",
            "-o",
            "lstart=",
            "-o",
            "stat=",
            "-o",
            "command=",
        ])
        .output()?;
    if !output.status.success() {
        if output.status.code() == Some(1) && output.stdout.is_empty() {
            return Ok(None);
        }
        return Err(eyre!("Cannot inspect process {pid}"));
    }
    let output = String::from_utf8(output.stdout)?;
    let (parent, line) = output
        .trim_start()
        .split_once(char::is_whitespace)
        .ok_or_else(|| eyre!("Missing process parent"))?;
    let parent = parent.parse()?;
    let line = line.trim_start();
    let started = line
        .get(..24)
        .ok_or_else(|| eyre!("Missing process start time"))?
        .trim();
    let rest = line
        .get(24..)
        .ok_or_else(|| eyre!("Missing process command"))?
        .trim_start();
    let (state, command) = rest
        .split_once(char::is_whitespace)
        .ok_or_else(|| eyre!("Missing process state"))?;
    if state.starts_with('Z') {
        return Ok(None);
    }
    Ok(Some((
        ProcessIdentity {
            pid,
            started: started.into(),
            command: command.trim().into(),
        },
        parent,
    )))
}

fn chrome_process(record: &ChromeRecord) -> Result<Option<ProcessIdentity>> {
    if let Some(chrome) = &record.chrome {
        return inspect(chrome.pid);
    }
    let output = Command::new("ps")
        .args(["-axww", "-o", "pid=", "-o", "command="])
        .output()?;
    if !output.status.success() {
        return Err(eyre!("Cannot find pending Chrome"));
    }
    for line in String::from_utf8(output.stdout)?.lines() {
        let line = line.trim_start();
        let Some((pid, command)) = line.split_once(char::is_whitespace) else {
            continue;
        };
        if !command.contains(&record.profile) {
            continue;
        }
        if let Some(process) = inspect(pid.parse()?)? {
            if recorded_chrome_matches(record, Some(&process)) {
                return Ok(Some(process));
            }
        }
    }
    Ok(None)
}

fn write_record(path: &Path, record: &ChromeRecord) -> Result<()> {
    let mut staged = tempfile::NamedTempFile::new_in(
        path.parent()
            .ok_or_else(|| eyre!("Missing record directory"))?,
    )?;
    serde_json::to_writer(&mut staged, record)?;
    staged.as_file_mut().sync_all()?;
    staged.persist(path).map_err(|error| eyre!(error))?;
    Ok(())
}

impl LaunchGuard {
    pub(crate) fn prepare() -> Result<Self> {
        sweep()?;
        let directory = directory()?;
        std::fs::create_dir_all(&directory)?;
        let profile = tempfile::Builder::new()
            .prefix("profile-")
            .tempdir_in(&directory)?
            .keep();
        let record = directory.join(format!(
            "{}.json",
            profile
                .file_name()
                .ok_or_else(|| eyre!("Missing profile name"))?
                .to_string_lossy()
        ));
        let guard = Self {
            record,
            profile,
            armed: true,
        };
        let owner =
            inspect(std::process::id())?.ok_or_else(|| eyre!("Cannot identify Chrome owner"))?;
        write_record(
            &guard.record,
            &ChromeRecord {
                owner,
                chrome: None,
                profile: guard
                    .profile
                    .to_str()
                    .ok_or_else(|| eyre!("Chrome profile is not UTF-8"))?
                    .into(),
            },
        )?;
        let mut child = Command::new(std::env::current_exe()?)
            .arg("chrome-reaper")
            .arg("--record")
            .arg(&guard.record)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;
        std::thread::spawn(move || {
            let _ = child.wait();
        });
        Ok(guard)
    }

    pub(crate) fn launched(mut self, pid: u32) -> Result<()> {
        let mut record: ChromeRecord = serde_json::from_slice(&std::fs::read(&self.record)?)?;
        let chrome = inspect(pid)?.ok_or_else(|| eyre!("Launched Chrome disappeared"))?;
        if !recorded_chrome_matches(&record, Some(&chrome)) {
            return Err(eyre!("Launched Chrome identity mismatch"));
        }
        record.chrome = Some(chrome);
        write_record(&self.record, &record)?;
        self.armed = false;
        Ok(())
    }
}

impl Drop for LaunchGuard {
    fn drop(&mut self) {
        if self.armed {
            let _ = std::fs::remove_file(&self.record);
            let _ = std::fs::remove_dir_all(&self.profile);
        }
    }
}

pub(crate) fn signal(process: &ProcessIdentity, name: &str) -> Result<()> {
    if inspect(process.pid)?.as_ref() == Some(process) {
        let output = Command::new("kill")
            .args([name, &process.pid.to_string()])
            .output()?;
        if !output.status.success() && inspect(process.pid)?.is_some() {
            return Err(eyre!("Cannot signal Chrome process {}", process.pid));
        }
    }
    Ok(())
}

pub(crate) fn process_tree(chrome: &ProcessIdentity) -> Result<Vec<ProcessIdentity>> {
    let output = Command::new("ps")
        .args(["-ax", "-o", "pid=", "-o", "ppid="])
        .output()?;
    if !output.status.success() {
        return Err(eyre!("Cannot inspect Chrome descendants"));
    }
    let edges: Vec<(u32, u32)> = String::from_utf8(output.stdout)?
        .lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            Some((fields.next()?.parse().ok()?, fields.next()?.parse().ok()?))
        })
        .collect();
    let mut processes = vec![chrome.clone()];
    let mut index = 0;
    while index < processes.len() {
        let parent = processes[index].clone();
        if inspect(parent.pid)?.as_ref() == Some(&parent) {
            for (pid, _) in edges.iter().filter(|(_, ppid)| *ppid == parent.pid) {
                if let Some((child, parent_pid)) = inspect_with_parent(*pid)? {
                    if parent_pid == parent.pid
                        && inspect(parent.pid)?.as_ref() == Some(&parent)
                        && !processes.iter().any(|process| process.pid == child.pid)
                    {
                        processes.push(child);
                    }
                }
            }
        }
        index += 1;
    }
    Ok(processes)
}

fn terminate_tree(record: &ChromeRecord, chrome: &ProcessIdentity) -> Result<()> {
    let processes = process_tree(chrome)?;
    if !orphaned_and_ours(
        record,
        inspect(record.owner.pid)?.as_ref(),
        inspect(chrome.pid)?.as_ref(),
    ) {
        return Ok(());
    }
    signal(chrome, "-TERM")?;
    let deadline = Instant::now() + Duration::from_millis(500);
    while Instant::now() < deadline && inspect(chrome.pid)?.as_ref() == Some(chrome) {
        std::thread::sleep(Duration::from_millis(25));
    }
    for process in processes.iter().rev() {
        signal(process, "-TERM")?;
    }
    std::thread::sleep(Duration::from_millis(100));
    for process in processes.iter().rev() {
        signal(process, "-KILL")?;
    }
    Ok(())
}

fn remove_record(path: &Path, record: &ChromeRecord) -> Result<()> {
    match std::fs::remove_file(path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    let profile = Path::new(&record.profile);
    if profile.parent() == Some(directory()?.as_path())
        && profile
            .file_name()
            .is_some_and(|name| name.to_string_lossy().starts_with("profile-"))
        && std::fs::symlink_metadata(profile)
            .is_ok_and(|metadata| metadata.is_dir() && !metadata.is_symlink())
    {
        std::fs::remove_dir_all(profile)?;
    }
    Ok(())
}

fn check(path: &Path) -> Result<bool> {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(true),
        Err(error) => return Err(error.into()),
    };
    let record: ChromeRecord = serde_json::from_slice(&bytes)?;
    let owner = inspect(record.owner.pid)?;
    let chrome = chrome_process(&record)?;
    if orphaned_and_ours(&record, owner.as_ref(), chrome.as_ref()) {
        terminate_tree(
            &record,
            chrome.as_ref().ok_or_else(|| eyre!("Missing orphan"))?,
        )?;
        remove_record(path, &record)?;
        return Ok(true);
    }
    if chrome.is_none() && (record.chrome.is_some() || owner.as_ref() != Some(&record.owner)) {
        remove_record(path, &record)?;
        return Ok(true);
    }
    Ok(chrome.is_some() && !recorded_chrome_matches(&record, chrome.as_ref()))
}

pub(crate) fn sweep() -> Result<()> {
    let entries = match std::fs::read_dir(directory()?) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    for entry in entries {
        let entry = entry?;
        if entry.file_type()?.is_file() && entry.path().extension().is_some_and(|ext| ext == "json")
        {
            if let Err(error) = check(&entry.path()) {
                std::eprintln!("Chrome startup sweep: {error}");
            }
        }
    }
    Ok(())
}

pub(crate) fn run(app: App) -> Result<()> {
    loop {
        if check(&app.record)? {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(500));
    }
}
