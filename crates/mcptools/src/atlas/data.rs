use std::path::Path;

use color_eyre::eyre::Result;
use mcptools_core::atlas::{
    build_ignore_matcher, compute_change_set, content_hash, ActiveEnrichment, AtlasConfig,
    EnrichmentConfig, EnrichmentCounts, IndexStatus, RoleIdentity, StructuralState,
};

use crate::atlas::db::Database;

pub fn open_navigation() -> Result<(std::path::PathBuf, AtlasConfig, Database)> {
    let root = crate::atlas::cli::index::find_git_root()?;
    let config = crate::atlas::config::load_config(&root)?;
    let path = config.db_path.resolve(&root);
    if !path.is_file() {
        color_eyre::eyre::bail!("Atlas is uninitialized; run `mcptools atlas init`");
    }
    let db = Database::open_repository(&path, &root, false)?;
    Ok((root, config, db))
}

pub fn query_freshness(db: &Database, config: &AtlasConfig, root: &Path) -> Result<String> {
    let ignore = build_ignore_matcher(&config.skip_patterns)
        .map_err(|error| color_eyre::eyre::eyre!(error))?;
    let primer = config.primer_path.resolve(root);
    let path = config.db_path.resolve(root);
    let snapshot = crate::atlas::fs::walk_repo_bounded(root, &ignore, &primer, &path)
        .map(|entry| entry.map(|(path, bytes)| (path, content_hash(&bytes))))
        .collect::<Result<std::collections::HashMap<_, _>>>();
    let structural = if db.get_metadata("last_full_sync")?.is_none()
        && db.get_metadata("last_update")?.is_none()
    {
        StructuralState::NotYetSynced
    } else {
        match snapshot {
            Ok(current) if compute_change_set(&db.file_hashes()?, &current).is_empty() => {
                StructuralState::Ready
            }
            Ok(_) => StructuralState::Stale,
            Err(_) => StructuralState::Unknown,
        }
    };
    Ok(format!("structural={}; semantic=cached_unverified {}; model-free, no automatic refresh; run `mcptools atlas index` to refresh; use source reads for current content", serde_json::to_value(structural)?.as_str().unwrap_or("unknown"), db.semantic_summary()?))
}

pub fn atlas_status_data(db: &Database, config: &AtlasConfig, root: &Path) -> Result<IndexStatus> {
    let total_files = db.count_files()?;
    let total_directories = db.count_directories()?;
    let total_symbols = db.count_symbols()?;
    let files_with_descriptions = db.count_files_with_descriptions()?;
    let directories_with_descriptions = db.count_directories_with_descriptions()?;
    let last_sync = db.get_metadata("last_full_sync")?;
    let primer_hash = db.get_metadata("primer_hash")?;

    let primer_present = config.primer_path.resolve(root).is_file();
    let ignore = build_ignore_matcher(&config.skip_patterns)
        .map_err(|error| StatusConfigurationFailure(error.to_string()))?;
    let synced = last_sync.is_some() || db.get_metadata("last_update")?.is_some();
    let snapshot = if synced {
        let primer_path = config.primer_path.resolve(root);
        let db_path = config.db_path.resolve(root);
        crate::atlas::fs::walk_repo_bounded(root, &ignore, &primer_path, &db_path)
            .map(|entry| entry.map(|(path, bytes)| (path, content_hash(&bytes))))
            .collect::<Result<std::collections::HashMap<_, _>>>()
            .ok()
    } else {
        None
    };
    let changes = if let Some(current_hashes) = &snapshot {
        compute_change_set(&db.file_hashes()?, current_hashes)
    } else {
        mcptools_core::atlas::ChangeSet {
            added: Vec::new(),
            modified: Vec::new(),
            deleted: Vec::new(),
        }
    };
    let changed_paths: std::collections::BTreeSet<_> = changes
        .added
        .iter()
        .chain(&changes.modified)
        .chain(&changes.deleted)
        .collect();
    let structural = if !synced {
        StructuralState::NotYetSynced
    } else if snapshot.is_none() {
        StructuralState::Unknown
    } else if changes.is_empty() {
        StructuralState::Ready
    } else {
        StructuralState::Stale
    };
    let active_enrichment = active_enrichment(root);
    let mut enrichment = EnrichmentCounts::default();
    let targets = db
        .all_file_paths()?
        .into_iter()
        .map(|path| crate::atlas::enrichment::EnrichmentTarget {
            kind: crate::atlas::enrichment::EntryKind::File,
            path,
        })
        .chain(db.all_directories()?.into_iter().map(|entry| {
            crate::atlas::enrichment::EnrichmentTarget {
                kind: crate::atlas::enrichment::EntryKind::Directory,
                path: entry.path,
            }
        }));
    for target in targets {
        use crate::atlas::enrichment::EnrichmentState;
        match db.enrichment_state(&target)? {
            EnrichmentState::Missing => enrichment.missing += 1,
            EnrichmentState::Stale => enrichment.stale += 1,
            EnrichmentState::Failed(_) => enrichment.failed += 1,
            EnrichmentState::Deferred(_) => enrichment.deferred += 1,
            EnrichmentState::InFlight { .. } => enrichment.in_flight += 1,
            EnrichmentState::Current(_) if structural == StructuralState::Unknown => {
                enrichment.unknown += 1
            }
            EnrichmentState::Current(_)
                if changed_paths.contains(&target.path)
                    || (target.kind == crate::atlas::enrichment::EntryKind::Directory
                        && changed_paths
                            .iter()
                            .any(|path| path.starts_with(&target.path))) =>
            {
                enrichment.stale += 1
            }
            EnrichmentState::Current(saved) => {
                match crate::atlas::cli::enrich::current_input(db, root, &target) {
                    Ok((candidate, _)) if candidate.identity == saved => enrichment.current += 1,
                    Ok(_) => enrichment.stale += 1,
                    Err(_)
                        if matches!(active_enrichment, ActiveEnrichment::Enabled { .. })
                            && !root.join(&target.path).exists() =>
                    {
                        enrichment.stale += 1
                    }
                    Err(_) => enrichment.unknown += 1,
                }
            }
        }
    }
    let enrichment_state = enrichment.state().to_string();
    let last_execution = db
        .get_metadata("last_enrichment_execution")?
        .map(|value| serde_json::from_str(&value))
        .transpose()?;

    Ok(IndexStatus {
        last_sync,
        total_files,
        total_directories,
        total_symbols,
        files_with_descriptions,
        directories_with_descriptions,
        primer_hash,
        primer_present,
        structural,
        guidance: match structural {
            StructuralState::Stale | StructuralState::NotYetSynced | StructuralState::Unknown => {
                Some("Run `mcptools atlas index` to refresh the structural index".into())
            }
            StructuralState::Ready | StructuralState::Uninitialized => None,
        },
        enrichment,
        enrichment_state,
        active_enrichment,
        retries: retry_statuses(db, root)?,
        last_execution,
        ..IndexStatus::default()
    })
}

pub fn retry_statuses(
    db: &Database,
    root: &Path,
) -> Result<Vec<mcptools_core::atlas::RetryStatus>> {
    let cooldown = match crate::atlas::config::load_enrichment_config(root) {
        Ok(EnrichmentConfig::Enabled(runner)) => match mcptools_core::llm_stream::resolve_paths() {
            Ok(paths) => {
                let scope = crate::atlas::retry::RateScope::new(
                    runner.provider.as_str(),
                    &paths.config_dir.to_string_lossy(),
                )?;
                db.cooldown(&scope)?
            }
            Err(_) => 0,
        },
        Ok(EnrichmentConfig::Disabled) | Err(_) => 0,
    };
    db.retry_statuses(cooldown)
}

fn active_enrichment(root: &Path) -> ActiveEnrichment {
    match crate::atlas::config::load_enrichment_config(root) {
        Ok(EnrichmentConfig::Enabled(runner)) => ActiveEnrichment::Enabled {
            provider: runner.provider.to_string(),
            file: RoleIdentity {
                model: runner.file.model.to_string(),
                template: runner.file.template.to_string(),
            },
            directory: RoleIdentity {
                model: runner.directory.model.to_string(),
                template: runner.directory.template.to_string(),
            },
        },
        Ok(EnrichmentConfig::Disabled) => ActiveEnrichment::Disabled,
        Err(_) => ActiveEnrichment::ConfigFailure,
    }
}

pub fn repository_status(root: &Path) -> Result<IndexStatus> {
    let config = crate::atlas::config::load_config(root)
        .map_err(|error| StatusConfigurationFailure(error.to_string()))?;
    build_ignore_matcher(&config.skip_patterns)
        .map_err(|error| StatusConfigurationFailure(error.to_string()))?;
    let path = config.db_path.resolve(root);
    if !path.exists() {
        return Ok(IndexStatus {
            active_enrichment: active_enrichment(root),
            primer_present: config.primer_path.resolve(root).is_file(),
            ..IndexStatus::default()
        });
    }
    atlas_status_data(
        &Database::open_repository(&path, root, false)?,
        &config,
        root,
    )
}

#[derive(Debug, thiserror::Error)]
#[error("Atlas configuration failure: {0}")]
pub(crate) struct StatusConfigurationFailure(String);
