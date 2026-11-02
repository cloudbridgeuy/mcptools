use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};

use mcptools_core::plan_store::{self, PlanMeta, UrlOutput, WriteArgs, WriteOutput};
use rand::RngCore;
use serde::de::DeserializeOwned;

use crate::prelude::*;

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

pub fn write(args: &WriteArgs) -> Result<WriteOutput> {
    let plan = plan_store::validate_write(args).map_err(|error| eyre!(error))?;
    let directory = data_dir()?.join("diagrams").join(&args.id);
    let meta: Option<PlanMeta> = read_optional(&directory.join("meta.json"))?;
    let previous = read_optional(&directory.join("plan.tldr"))?;
    let host = std::env::var("MCPTOOLS_DIAGRAM_HOST").ok();
    let port = std::env::var("MCPTOOLS_DIAGRAM_PORT").ok();
    let mut token = [0; 16];
    rand::rngs::OsRng
        .try_fill_bytes(&mut token)
        .map_err(|error| eyre!("generate plan token: {error}"))?;
    let write = plan_store::prepare_write(
        &plan,
        previous.as_ref(),
        meta.as_ref(),
        token,
        host.as_deref(),
        port.as_deref(),
    )
    .map_err(|error| eyre!(error))?;
    std::fs::create_dir_all(&directory)?;
    let snapshot = stage(&directory, &write.snapshot)?;
    let meta = stage(&directory, &write.meta)?;
    snapshot.persist(directory.join("plan.tldr"))?;
    meta.persist(directory.join("meta.json"))?;
    Ok(write.output)
}

pub fn url(id: &str) -> Result<UrlOutput> {
    plan_store::validate_id(id).map_err(|error| eyre!(error))?;
    let directory = data_dir()?.join("diagrams").join(id);
    let meta: PlanMeta =
        read_optional(&directory.join("meta.json"))?.ok_or_else(|| eyre!("unknown plan: {id}"))?;
    plan_store::validate_meta(id, &meta).map_err(|error| eyre!(error))?;
    let host = std::env::var("MCPTOOLS_DIAGRAM_HOST").ok();
    let port = std::env::var("MCPTOOLS_DIAGRAM_PORT").ok();
    Ok(UrlOutput {
        url: plan_store::plan_url(&meta, host.as_deref(), port.as_deref())
            .map_err(|error| eyre!(error))?,
    })
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
    plan_store::png_output(path, metadata.is_file()).map_err(|error| eyre!(error))
}
