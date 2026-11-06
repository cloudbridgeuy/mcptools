use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};

use mcptools_core::plan_inbox;
use mcptools_core::plan_share::{self, ShareWindow};
use mcptools_core::plan_store::{self, PlanMeta, UrlOutput, WriteArgs, WriteOutput};
use rand::RngCore;
use serde::de::DeserializeOwned;

use crate::prelude::*;

#[derive(serde::Deserialize, serde::Serialize)]
struct Notify {
    inbox: PathBuf,
}

fn notify_target(inbox: PathBuf) -> Result<Notify> {
    if !inbox.is_absolute() {
        return Err(eyre!("RALPH_INBOX must be an absolute directory"));
    }
    Ok(Notify { inbox })
}

pub fn data_dir() -> Result<PathBuf> {
    if let Some(path) = std::env::var_os("XDG_DATA_HOME").filter(|path| !path.is_empty()) {
        return Ok(PathBuf::from(path).join("mcptools"));
    }
    #[cfg(target_os = "windows")]
    let base =
        PathBuf::from(std::env::var_os("APPDATA").ok_or_else(|| eyre!("APPDATA is not set"))?);
    #[cfg(not(target_os = "windows"))]
    let base = PathBuf::from(std::env::var_os("HOME").ok_or_else(|| eyre!("HOME is not set"))?);
    #[cfg(target_os = "macos")]
    let base = base.join("Library/Application Support");
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let base = base.join(".local/share");
    Ok(base.join("mcptools"))
}

fn read_optional<T: DeserializeOwned>(path: &Path) -> Result<Option<T>> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(Some(
            serde_json::from_slice(&bytes).wrap_err_with(|| format!("read {}", path.display()))?,
        )),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error).wrap_err_with(|| format!("read {}", path.display())),
    }
}

fn stage(directory: &Path, value: &impl serde::Serialize) -> Result<tempfile::NamedTempFile> {
    let mut file = tempfile::NamedTempFile::new_in(directory)?;
    serde_json::to_writer(&mut file, value)?;
    file.flush()?;
    file.as_file().sync_all()?;
    Ok(file)
}

fn now() -> std::io::Result<u64> {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .map_err(std::io::Error::other)
}

fn share_window(seconds: Option<u64>) -> Result<ShareWindow> {
    let seconds = match seconds {
        Some(seconds) => Some(seconds),
        None => match std::env::var("MCPTOOLS_DIAGRAM_SHARE_SECONDS") {
            Ok(value) => mcptools_core::config::Diagram::seconds(Some(&value), None)
                .wrap_err("MCPTOOLS_DIAGRAM_SHARE_SECONDS must be unsigned seconds")?,
            Err(_) => {
                crate::config::optional()?
                    .unwrap_or_default()
                    .diagram
                    .share_seconds
            }
        },
    };
    Ok(ShareWindow {
        expires_at_unix_secs: now()?.saturating_add(plan_share::clamp_ttl(seconds)),
    })
}

pub fn share_is_open(directory: &Path) -> std::io::Result<bool> {
    let window = match std::fs::read(directory.join("share.json")) {
        Ok(bytes) => serde_json::from_slice::<ShareWindow>(&bytes).ok(),
        Err(error) if error.kind() == ErrorKind::NotFound => None,
        Err(error) => return Err(error),
    };
    Ok(plan_share::is_open(window.as_ref(), now()?))
}

pub fn touch_access(directory: &Path) {
    if let Err(error) = tempfile::NamedTempFile::new_in(directory).and_then(|file| {
        file.persist(directory.join("last-access"))
            .map(|_| ())
            .map_err(|error| error.error)
    }) {
        std::eprintln!("diagram last access: {error}");
    }
}

pub fn save(
    directory: &Path,
    body: &plan_store::SaveBody,
    png: &[u8],
) -> Result<std::result::Result<PlanMeta, plan_store::SaveError>> {
    let meta: PlanMeta = read_optional(&directory.join("meta.json"))?
        .ok_or_else(|| eyre!("missing plan metadata"))?;
    let meta = match plan_store::prepare_save(body, png, &meta) {
        Ok(meta) => meta,
        Err(error) => return Ok(Err(error)),
    };
    let notify: Option<Notify> = read_optional(&directory.join("notify.json"))?;
    let report = if let Some(notify) = notify {
        let notify = notify_target(notify.inbox)?;
        let baseline = read_optional(&directory.join("baseline.tldr"))?;
        let stored = read_optional(&directory.join("plan.tldr"))?;
        let diff =
            plan_store::prepare_save_diff(baseline.as_ref(), stored.as_ref(), &body.snapshot)
                .map_err(|error| eyre!(error))?;
        let png = std::path::absolute(directory.join("latest.png"))?;
        let elapsed = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?;
        let at = plan_inbox::report_at(elapsed.as_secs(), elapsed.subsec_nanos())
            .ok_or_else(|| eyre!("invalid report timestamp"))?;
        let report = plan_inbox::canvas_report(&meta.id, meta.rev, &diff.diff, &png, &at)
            .map_err(|error| eyre!(error))?;
        let name = plan_inbox::inbox_file_name(&report.at, meta.rev, &report.worker);
        std::fs::create_dir_all(&notify.inbox)?;
        let mut file = tempfile::Builder::new()
            .prefix(".")
            .tempfile_in(&notify.inbox)?;
        serde_json::to_writer(&mut file, &report)?;
        file.flush()?;
        file.as_file().sync_all()?;
        Some((file, notify.inbox.join(name)))
    } else {
        std::eprintln!("diagram save: no notify target");
        None
    };
    let snapshot = stage(directory, &body.snapshot)?;
    let baseline = stage(directory, &body.snapshot)?;
    let mut image = tempfile::NamedTempFile::new_in(directory)?;
    image.write_all(png)?;
    image.flush()?;
    image.as_file().sync_all()?;
    let metadata = stage(directory, &meta)?;
    let share = stage(directory, &plan_share::closed())?;
    snapshot.persist(directory.join("plan.tldr"))?;
    image.persist(directory.join("latest.png"))?;
    metadata.persist(directory.join("meta.json"))?;
    if let Some((file, target)) = report {
        file.persist(target)?;
    }
    baseline.persist(directory.join("baseline.tldr"))?;
    share.persist(directory.join("share.json"))?;
    Ok(Ok(meta))
}

pub fn endpoint() -> Result<(String, String)> {
    let mut host = std::env::var("MCPTOOLS_DIAGRAM_HOST").ok();
    let mut port = std::env::var("MCPTOOLS_DIAGRAM_PORT").ok();
    if host.is_none() || port.is_none() {
        let file = crate::config::optional()?.unwrap_or_default().diagram;
        host = host.or(file.host);
        port = port.or(file.port.map(|value| value.to_string()));
    }
    let record = std::fs::read_to_string(data_dir()?.join("diagrams/.daemon-address!")).ok();
    Ok(mcptools_core::diagram_endpoint::endpoint(
        host.as_deref(),
        port.as_deref(),
        record.as_deref(),
    ))
}

pub fn current_url(id: &str) -> Result<String> {
    let directory = data_dir()?.join("diagrams").join(id);
    let meta: PlanMeta =
        read_optional(&directory.join("meta.json"))?.ok_or_else(|| eyre!("unknown plan: {id}"))?;
    let (host, port) = endpoint()?;
    plan_store::plan_url(&meta, Some(&host), Some(&port)).map_err(|error| eyre!(error))
}

pub fn write(args: &WriteArgs) -> Result<WriteOutput> {
    let plan = plan_store::validate_write(args).map_err(|error| eyre!(error))?;
    let notify = std::env::var_os("RALPH_INBOX")
        .map(PathBuf::from)
        .map(notify_target)
        .transpose()?;
    let window = share_window(args.share_seconds)?;
    let directory = data_dir()?.join("diagrams").join(&args.id);
    let meta: Option<PlanMeta> = read_optional(&directory.join("meta.json"))?;
    let previous = read_optional(&directory.join("plan.tldr"))?;
    let (host, port) = endpoint()?;
    let mut token = [0; 16];
    rand::rngs::OsRng
        .try_fill_bytes(&mut token)
        .map_err(|error| eyre!("generate plan token: {error}"))?;
    let write = plan_store::prepare_write(
        &plan,
        previous.as_ref(),
        meta.as_ref(),
        token,
        Some(&host),
        Some(&port),
    )
    .map_err(|error| eyre!(error))?;
    std::fs::create_dir_all(&directory)?;
    let snapshot = stage(&directory, &write.snapshot)?;
    let meta = stage(&directory, &write.meta)?;
    let share = stage(&directory, &window)?;
    snapshot.persist(directory.join("plan.tldr"))?;
    meta.persist(directory.join("meta.json"))?;
    share.persist(directory.join("share.json"))?;
    if let Some(notify) = notify {
        stage(&directory, &notify)?.persist(directory.join("notify.json"))?;
    } else {
        match std::fs::remove_file(directory.join("notify.json")) {
            Ok(()) => {}
            Err(error) if error.kind() == ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(write.output)
}

pub fn url(id: &str, seconds: Option<u64>) -> Result<UrlOutput> {
    plan_store::validate_id(id).map_err(|error| eyre!(error))?;
    let directory = data_dir()?.join("diagrams").join(id);
    let meta: PlanMeta =
        read_optional(&directory.join("meta.json"))?.ok_or_else(|| eyre!("unknown plan: {id}"))?;
    plan_store::validate_meta(id, &meta).map_err(|error| eyre!(error))?;
    let (host, port) = endpoint()?;
    let output = UrlOutput {
        url: plan_store::plan_url(&meta, Some(&host), Some(&port)).map_err(|error| eyre!(error))?,
    };
    stage(&directory, &share_window(seconds)?)?.persist(directory.join("share.json"))?;
    touch_access(&directory);
    Ok(output)
}

pub fn close(id: &str) -> Result<plan_store::CloseOutput> {
    plan_store::validate_id(id).map_err(|error| eyre!(error))?;
    let directory = data_dir()?.join("diagrams").join(id);
    let meta: PlanMeta =
        read_optional(&directory.join("meta.json"))?.ok_or_else(|| eyre!("unknown plan: {id}"))?;
    plan_store::validate_meta(id, &meta).map_err(|error| eyre!(error))?;
    stage(
        &directory,
        &ShareWindow {
            expires_at_unix_secs: 0,
        },
    )?
    .persist(directory.join("share.json"))?;
    Ok(plan_store::CloseOutput { closed: true })
}

pub fn diff(id: &str) -> Result<plan_store::DiffOutput> {
    plan_store::validate_id(id).map_err(|error| eyre!(error))?;
    let directory = data_dir()?.join("diagrams").join(id);
    let current: serde_json::Value =
        read_optional(&directory.join("plan.tldr"))?.ok_or_else(|| eyre!("unknown plan: {id}"))?;
    let baseline = read_optional(&directory.join("baseline.tldr"))?;
    let output =
        plan_store::prepare_diff(baseline.as_ref(), &current).map_err(|error| eyre!(error))?;
    stage(&directory, &current)?.persist(directory.join("baseline.tldr"))?;
    Ok(output)
}

pub fn png(id: &str) -> Result<plan_store::PngOutput> {
    let data_dir = std::path::absolute(data_dir()?)?;
    let path = plan_store::latest_png_path(&data_dir, id).map_err(|error| eyre!(error))?;
    let metadata = std::fs::metadata(&path)?;
    let output = plan_store::png_output(path, metadata.is_file()).map_err(|error| eyre!(error))?;
    touch_access(output.path.parent().expect("PNG directory"));
    Ok(output)
}
