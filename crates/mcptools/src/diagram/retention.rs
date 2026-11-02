#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use mcptools_core::plan_retention::{expired_plan_ids, Plan};
use mcptools_core::plan_store::{validate_id, validate_meta, PlanMeta};

use crate::prelude::*;

fn ttl(value: Option<&str>) -> Result<Duration> {
    let seconds = value.unwrap_or("2592000").parse::<u64>()?;
    Ok(Duration::from_secs(seconds))
}

fn inspect(path: &Path, id: &str) -> Result<Option<(PlanMeta, [SystemTime; 3])>> {
    let inspect = || -> Result<Option<(PlanMeta, [SystemTime; 3])>> {
        let directory = std::fs::symlink_metadata(path)?;
        let snapshot = std::fs::symlink_metadata(path.join("plan.tldr"))?;
        let metadata = std::fs::symlink_metadata(path.join("meta.json"))?;
        if !directory.is_dir() || !snapshot.is_file() || !metadata.is_file() {
            return Ok(None);
        }
        for entry in std::fs::read_dir(path)? {
            if !entry?.file_type()?.is_file() {
                return Ok(None);
            }
        }
        let meta = match serde_json::from_slice::<PlanMeta>(&std::fs::read(path.join("meta.json"))?)
        {
            Ok(meta) if validate_meta(id, &meta).is_ok() => meta,
            _ => return Ok(None),
        };
        Ok(Some((
            meta,
            [
                directory.modified()?,
                snapshot.modified()?,
                metadata.modified()?,
            ],
        )))
    };
    match inspect() {
        Err(error)
            if error
                .downcast_ref::<std::io::Error>()
                .is_some_and(|error| error.kind() == std::io::ErrorKind::NotFound) =>
        {
            Ok(None)
        }
        result => result,
    }
}

pub fn sweep(directory: &Path, active_ids: &[&str], now: SystemTime, ttl: Duration) -> Result<()> {
    let root = match std::fs::symlink_metadata(directory) {
        Ok(root) if root.is_dir() => root,
        Ok(_) => return Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    let _ = root;
    for entry in std::fs::read_dir(directory)? {
        let entry = entry?;
        let name = entry.file_name();
        let Some(id) = name.to_str().filter(|id| validate_id(id).is_ok()) else {
            continue;
        };
        let path = entry.path();
        let Some((meta, times)) = inspect(&path, id)? else {
            continue;
        };
        let select = |times: &[SystemTime; 3]| {
            expired_plan_ids(
                &[Plan {
                    id,
                    modified: *times.iter().max().expect("three timestamps"),
                    active: active_ids.contains(&id),
                }],
                now,
                ttl,
            )
            .map_err(|error| eyre!(error))
            .map(|ids| !ids.is_empty())
        };
        if !select(&times)? {
            continue;
        }
        if let Some((current_meta, current_times)) = inspect(&path, id)? {
            if meta == current_meta && times == current_times && select(&current_times)? {
                std::fs::remove_dir_all(&path)?;
            }
        }
    }
    Ok(())
}

pub async fn run(directory: PathBuf) -> Result<()> {
    let value = match std::env::var("MCPTOOLS_DIAGRAM_TTL_SECONDS") {
        Ok(value) => Some(value),
        Err(std::env::VarError::NotPresent) => None,
        Err(error) => return Err(error.into()),
    };
    let ttl = ttl(value.as_deref())?;
    let mut interval = tokio::time::interval(Duration::from_secs(3600));
    loop {
        interval.tick().await;
        let directory = directory.clone();
        match tokio::task::spawn_blocking(move || sweep(&directory, &[], SystemTime::now(), ttl))
            .await
        {
            Ok(Ok(())) => {}
            Ok(Err(error)) => std::eprintln!("diagram retention: {error}"),
            Err(error) => return Err(error.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seed(root: &Path, id: &str, modified: SystemTime) {
        let directory = root.join(id);
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(directory.join("plan.tldr"), "{}").unwrap();
        std::fs::write(
            directory.join("meta.json"),
            format!(
                "{{\"id\":\"{id}\",\"token\":\"{}\",\"rev\":1}}",
                "0".repeat(32)
            ),
        )
        .unwrap();
        for path in [
            directory.join("plan.tldr"),
            directory.join("meta.json"),
            directory,
        ] {
            std::fs::File::open(path)
                .unwrap()
                .set_times(std::fs::FileTimes::new().set_modified(modified))
                .unwrap();
        }
    }

    #[test]
    fn sweep_deletes_only_expired_inactive_plans() {
        let root = tempfile::tempdir().unwrap();
        let now = SystemTime::now();
        let old = now - Duration::from_secs(1000);
        seed(root.path(), "old", old);
        seed(root.path(), "active", old);
        seed(root.path(), "fresh", now);
        std::fs::create_dir(root.path().join("unrelated")).unwrap();
        std::fs::write(root.path().join("unrelated/file"), "keep").unwrap();
        sweep(root.path(), &["active"], now, Duration::from_secs(100)).unwrap();
        assert!(!root.path().join("old").exists());
        assert!(root.path().join("active/plan.tldr").exists());
        assert!(root.path().join("fresh/plan.tldr").exists());
        assert!(root.path().join("unrelated/file").exists());
    }

    #[test]
    fn missing_root_is_empty() {
        let root = tempfile::tempdir().unwrap();
        sweep(
            &root.path().join("missing"),
            &[],
            SystemTime::now(),
            Duration::ZERO,
        )
        .unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn sweep_skips_symlinked_plans_and_files() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let now = SystemTime::now();
        let old = now - Duration::from_secs(1000);
        seed(outside.path(), "external", old);
        std::os::unix::fs::symlink(outside.path().join("external"), root.path().join("link"))
            .unwrap();
        seed(root.path(), "filelink", old);
        std::fs::remove_file(root.path().join("filelink/plan.tldr")).unwrap();
        std::os::unix::fs::symlink(
            outside.path().join("external/plan.tldr"),
            root.path().join("filelink/plan.tldr"),
        )
        .unwrap();
        sweep(root.path(), &[], now, Duration::from_secs(100)).unwrap();
        assert!(outside.path().join("external/plan.tldr").exists());
        assert!(root.path().join("filelink").exists());
    }

    #[test]
    fn ttl_defaults_and_rejects_invalid_values() {
        assert_eq!(ttl(None).unwrap(), Duration::from_secs(30 * 24 * 60 * 60));
        assert_eq!(ttl(Some("42")).unwrap(), Duration::from_secs(42));
        assert!(ttl(Some("invalid")).is_err());
        assert!(ttl(Some("-1")).is_err());
    }
}
