use std::collections::{BTreeMap, BTreeSet};
use std::num::NonZeroUsize;
use std::path::{Component, Path, PathBuf};

use mcptools_core::atlas::{
    estimate_tokens, format_symbol, load, render, render_system, resolve_concurrency,
    truncate_to_tokens, EnrichmentConfig, LlmRoleConfig, LoadedTemplate, RunnerConfig,
};

use crate::atlas::config::{load_config, load_enrichment_config};
use crate::atlas::db::Database;
use crate::atlas::enrichment::{
    plan_enrichment, EnrichmentCandidate, EnrichmentInput, EnrichmentPlan, EnrichmentTarget,
    EntryKind, SourceInput,
};
use crate::prelude::*;

use super::index::find_git_root;

#[derive(Debug, clap::Args)]
pub struct EnrichOptions {
    pub path: Option<PathBuf>,
    #[arg(long)]
    pub dry_run: bool,
    #[arg(long)]
    pub refresh: bool,
    #[arg(long)]
    pub limit: Option<NonZeroUsize>,
    #[arg(long)]
    pub concurrency: Option<usize>,
}

pub async fn run(opts: EnrichOptions, _global: crate::Global) -> Result<()> {
    if !opts.dry_run {
        return Err(eyre!(
            "Atlas enrichment execution is not available yet; use `mcptools atlas enrich --dry-run`"
        ));
    }
    let root = find_git_root()?;
    let structural = load_config(&root)?;
    let runner = match load_enrichment_config(&root)? {
        EnrichmentConfig::Enabled(runner) => runner,
        EnrichmentConfig::Disabled => {
            return Err(eyre!(
                "Atlas enrichment is not configured; set [atlas].api and model"
            ))
        }
    };
    let concurrency = resolve_concurrency(opts.concurrency, runner.concurrency)
        .map_err(|error| eyre!("config error: {error}"))?;
    let paths = mcptools_core::llm_stream::resolve_paths()?;
    let runner_binary = resolve_runner_binary(&paths.binary)?;
    let templates_dir = paths.config_dir.join("templates");
    let file_template = load(&templates_dir, runner.file.template.as_str())?;
    let directory_template = load(&templates_dir, runner.directory.template.as_str())?;
    let db = Database::open_read_only(&structural.db_path.resolve(&root))?;
    let all_files = db.all_file_paths()?;
    let all_directories: Vec<PathBuf> = db
        .all_directories()?
        .into_iter()
        .map(|entry| entry.path)
        .collect();
    let scope = resolve_scope(&root, opts.path.as_deref(), &all_files, &all_directories)?;
    let ignore = mcptools_core::atlas::build_ignore_matcher(&structural.skip_patterns)
        .map_err(|error| eyre!("invalid skip pattern: {error}"))?;
    validate_scope(
        &root,
        &scope,
        &ignore,
        &structural.primer_path.resolve(&root),
        &structural.db_path.resolve(&root),
    )?;
    let primer = crate::atlas::fs::read_optional_primer(&structural.primer_path.resolve(&root))?;
    let preparation = InputPreparation {
        db: &db,
        root: &root,
        structural: &structural,
        runner: &runner,
        primer: primer.as_deref().unwrap_or(""),
        context: primer.as_deref().map(str::as_bytes),
    };
    let mut candidates = Vec::with_capacity(scope.files.len() + scope.directories.len());
    for path in &scope.files {
        candidates.push(file_candidate(&preparation, path, &file_template)?);
    }
    for path in &scope.directories {
        candidates.push(directory_candidate(
            &preparation,
            path,
            &directory_template,
        )?);
    }
    let plan = plan_enrichment(candidates, opts.refresh, opts.limit.map(NonZeroUsize::get));
    crate::prelude::println!(
        "{}",
        format_plan(
            &plan,
            &scope,
            &runner,
            &runner_binary,
            concurrency,
            opts.limit,
            opts.refresh,
        )
    );
    Ok(())
}

struct Scope {
    label: String,
    files: Vec<PathBuf>,
    directories: Vec<PathBuf>,
    external_ancestors: Vec<PathBuf>,
}

struct InputPreparation<'a> {
    db: &'a Database,
    root: &'a Path,
    structural: &'a mcptools_core::atlas::AtlasConfig,
    runner: &'a RunnerConfig,
    primer: &'a str,
    context: Option<&'a [u8]>,
}

struct PreparedTarget<'a> {
    target: EnrichmentTarget,
    source: SourceInput,
    template: &'a LoadedTemplate,
    variables: BTreeMap<String, String>,
    role: &'a LlmRoleConfig,
    settings: BTreeMap<String, String>,
}

fn resolve_scope(
    root: &Path,
    requested: Option<&Path>,
    all_files: &[PathBuf],
    all_directories: &[PathBuf],
) -> Result<Scope> {
    let Some(requested) = requested else {
        return Ok(Scope {
            label: ".".into(),
            files: all_files.to_vec(),
            directories: all_directories.to_vec(),
            external_ancestors: Vec::new(),
        });
    };
    let current = std::env::current_dir().wrap_err("getting current directory")?;
    let unresolved = if requested.is_absolute() {
        requested.to_path_buf()
    } else {
        current.join(requested)
    };
    let normalized_root = normalize_lexically(root)?;
    let normalized = normalize_lexically(&unresolved)?;
    let canonical_root = std::fs::canonicalize(root).wrap_err("resolving repository root")?;
    let canonical = std::fs::canonicalize(&unresolved)
        .wrap_err_with(|| format!("enrichment path does not exist: {}", requested.display()))?;
    if !canonical.starts_with(&canonical_root) {
        return Err(eyre!(
            "enrichment path is outside the repository: {}",
            requested.display()
        ));
    }
    let lexical_relative = normalized.strip_prefix(&normalized_root).map_err(|_| {
        eyre!(
            "enrichment path is outside the repository: {}",
            requested.display()
        )
    })?;
    if canonical_root.join(lexical_relative) != canonical {
        return Err(eyre!(
            "enrichment path must not traverse a symlink: {}",
            requested.display()
        ));
    }
    let relative = canonical
        .strip_prefix(&canonical_root)
        .map(Path::to_path_buf)
        .wrap_err("resolving repository-relative enrichment path")?;
    if canonical.is_file() {
        if !all_files.contains(&relative) {
            return Err(not_indexed(requested));
        }
        let external_ancestors =
            external_ancestors(std::slice::from_ref(&relative), &[], all_directories);
        return Ok(Scope {
            label: display_scope(&relative),
            files: vec![relative],
            directories: Vec::new(),
            external_ancestors,
        });
    }
    if !canonical.is_dir() {
        return Err(eyre!(
            "enrichment path is not a file or directory: {}",
            requested.display()
        ));
    }
    let files: Vec<PathBuf> = all_files
        .iter()
        .filter(|path| relative.as_os_str().is_empty() || path.starts_with(&relative))
        .cloned()
        .collect();
    let directories: Vec<PathBuf> = all_directories
        .iter()
        .filter(|path| {
            relative.as_os_str().is_empty() || *path == &relative || path.starts_with(&relative)
        })
        .cloned()
        .collect();
    if files.is_empty() && directories.is_empty() {
        return Err(not_indexed(requested));
    }
    Ok(Scope {
        label: display_scope(&relative),
        external_ancestors: external_ancestors(&files, &directories, all_directories),
        files,
        directories,
    })
}

fn external_ancestors(
    files: &[PathBuf],
    directories: &[PathBuf],
    all_directories: &[PathBuf],
) -> Vec<PathBuf> {
    let selected: BTreeSet<&Path> = directories.iter().map(PathBuf::as_path).collect();
    let indexed: BTreeSet<&Path> = all_directories.iter().map(PathBuf::as_path).collect();
    let mut external = BTreeSet::new();
    for path in files.iter().chain(directories) {
        let mut parent = path.parent();
        while let Some(path) = parent {
            if !path.as_os_str().is_empty() && indexed.contains(path) && !selected.contains(path) {
                external.insert(path.to_path_buf());
            }
            parent = path.parent();
        }
    }
    external.into_iter().collect()
}

fn normalize_lexically(path: &Path) -> Result<PathBuf> {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(_) | Component::RootDir | Component::Normal(_) => {
                normalized.push(component.as_os_str())
            }
            Component::CurDir => {}
            Component::ParentDir => {
                if !normalized.pop() {
                    return Err(eyre!(
                        "path escapes its filesystem root: {}",
                        path.display()
                    ));
                }
            }
        }
    }
    Ok(normalized)
}

fn not_indexed(path: &Path) -> color_eyre::Report {
    eyre!(
        "enrichment path is not in the structural index (it may be ignored or stale): {}",
        path.display()
    )
}

fn validate_scope(
    root: &Path,
    scope: &Scope,
    ignore: &mcptools_core::atlas::IgnoreMatcher,
    primer_path: &Path,
    db_path: &Path,
) -> Result<()> {
    if scope.files.is_empty() && scope.directories.is_empty() {
        return Err(eyre!(
            "Atlas structural index is empty; run `mcptools atlas index`"
        ));
    }
    for path in &scope.files {
        if ignore.is_match(path) {
            return Err(eyre!(
                "indexed enrichment path is now ignored; run `mcptools atlas index`: {}",
                path.display()
            ));
        }
        let absolute = root.join(path);
        if crate::atlas::fs::same_path(&absolute, primer_path)
            || crate::atlas::fs::same_path(&absolute, db_path)
        {
            return Err(eyre!(
                "indexed enrichment path is reserved by Atlas; run `mcptools atlas index`: {}",
                path.display()
            ));
        }
        validate_indexed_path(root, path, false)?;
    }
    for path in &scope.directories {
        validate_indexed_path(root, path, true)?;
    }
    Ok(())
}

fn validate_indexed_path(root: &Path, path: &Path, directory: bool) -> Result<()> {
    if path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(eyre!(
            "structural index contains a non-normalized path: {}",
            path.display()
        ));
    }
    let canonical_root = std::fs::canonicalize(root).wrap_err("resolving repository root")?;
    let absolute = root.join(path);
    let canonical = std::fs::canonicalize(&absolute).wrap_err_with(|| {
        format!(
            "indexed path is missing; run `mcptools atlas index`: {}",
            path.display()
        )
    })?;
    if !canonical.starts_with(&canonical_root) || canonical != canonical_root.join(path) {
        return Err(eyre!(
            "indexed path traverses a symlink or repository boundary: {}",
            path.display()
        ));
    }
    if directory != canonical.is_dir() || (!directory && !canonical.is_file()) {
        return Err(eyre!(
            "indexed path changed kind; run `mcptools atlas index`: {}",
            path.display()
        ));
    }
    Ok(())
}

fn display_scope(path: &Path) -> String {
    if path.as_os_str().is_empty() {
        ".".into()
    } else {
        path.display().to_string()
    }
}

fn file_candidate(
    preparation: &InputPreparation<'_>,
    path: &Path,
    template: &LoadedTemplate,
) -> Result<EnrichmentCandidate> {
    let bytes = std::fs::read(preparation.root.join(path))
        .wrap_err_with(|| format!("reading enrichment source: {}", path.display()))?;
    let text = String::from_utf8_lossy(&bytes);
    let tree_path = preparation.db.tree_path_to(path)?;
    let location = if tree_path.is_empty() {
        "(no path context available)".to_string()
    } else {
        tree_path
            .iter()
            .map(|(directory, description)| match description {
                Some(description) => format!("- {} — {description}", directory.display()),
                None => format!("- {} — [missing current description]", directory.display()),
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    let symbols = symbols_value(&preparation.db.symbols_for(path)?);
    let truncated = truncate_to_tokens(&text, preparation.structural.max_file_tokens);
    let content = if truncated.len() < text.len() {
        format!("{truncated}\n... (truncated)")
    } else {
        truncated.to_string()
    };
    let variables = BTreeMap::from([
        ("content".into(), content),
        ("location".into(), location),
        ("primer".into(), preparation.primer.to_string()),
        ("symbols".into(), symbols),
    ]);
    candidate(
        preparation,
        PreparedTarget {
            target: EnrichmentTarget {
                kind: EntryKind::File,
                path: path.to_path_buf(),
            },
            source: SourceInput::File(bytes),
            template,
            variables,
            role: &preparation.runner.file,
            settings: BTreeMap::from([(
                "max_file_tokens".into(),
                preparation.structural.max_file_tokens.to_string(),
            )]),
        },
    )
}

fn directory_candidate(
    preparation: &InputPreparation<'_>,
    path: &Path,
    template: &LoadedTemplate,
) -> Result<EnrichmentCandidate> {
    let children = preparation.db.directory_children(path)?;
    let mut source = BTreeMap::new();
    let children_text = if children.is_empty() {
        "(empty directory)".to_string()
    } else {
        let mut lines = Vec::with_capacity(children.len());
        for child in &children {
            let kind = if child.is_dir { "dir" } else { "file" };
            let description = child
                .short_description
                .as_deref()
                .unwrap_or("[missing current description]");
            let symbols = if child.is_dir {
                String::new()
            } else {
                symbols_value(&preparation.db.symbols_for(&child.path)?)
            };
            source.insert(
                format!("{kind}:{}", child.path.display()),
                serde_json::to_vec(&(description, symbols))?,
            );
            lines.push(format!(
                "- [{kind}] {} — {description}",
                child.path.display()
            ));
        }
        lines.join("\n")
    };
    let variables = BTreeMap::from([
        ("children".into(), children_text),
        ("dir_path".into(), display_scope(path)),
        ("primer".into(), preparation.primer.to_string()),
        (
            "symbols".into(),
            symbols_value(&preparation.db.aggregated_symbols_for(path)?),
        ),
    ]);
    candidate(
        preparation,
        PreparedTarget {
            target: EnrichmentTarget {
                kind: EntryKind::Directory,
                path: path.to_path_buf(),
            },
            source: SourceInput::Directory(source),
            template,
            variables,
            role: &preparation.runner.directory,
            settings: BTreeMap::new(),
        },
    )
}

fn candidate(
    preparation: &InputPreparation<'_>,
    prepared: PreparedTarget<'_>,
) -> Result<EnrichmentCandidate> {
    let values = serde_json::to_value(&prepared.variables)?;
    let system = render_system(prepared.template, &values)?;
    let prompt = render(prepared.template, &values)?;
    let input = EnrichmentInput {
        source: prepared.source,
        template: prepared.template.raw.as_bytes().to_vec(),
        system,
        prompt,
        variables: prepared.variables,
        context: preparation.context.map(<[u8]>::to_vec),
        provider: preparation.runner.provider.to_string(),
        model: prepared.role.model.to_string(),
        model_revision: None,
        settings: prepared.settings,
    };
    let context_tokens = estimate_tokens(&input.system) + estimate_tokens(&input.prompt);
    let identity = input.identity()?;
    let state = preparation.db.enrichment_state(&prepared.target)?;
    Ok(EnrichmentCandidate {
        target: prepared.target,
        identity,
        state,
        context_tokens,
    })
}

fn symbols_value(symbols: &[mcptools_core::atlas::Symbol]) -> String {
    if symbols.is_empty() {
        return "(no symbols extracted)".into();
    }
    symbols
        .iter()
        .map(|symbol| format_symbol(symbol).trim_end().to_string())
        .collect::<Vec<_>>()
        .join("\n")
}

fn resolve_runner_binary(binary: &Path) -> Result<PathBuf> {
    if binary.components().count() > 1 || binary.is_absolute() {
        if is_executable(binary) {
            return Ok(binary.to_path_buf());
        }
        return Err(eyre!(
            "llm-stream runner is not an executable file: {}",
            binary.display()
        ));
    }
    let path = std::env::var_os("PATH").unwrap_or_default();
    std::env::split_paths(&path)
        .map(|directory| directory.join(binary))
        .find(|candidate| is_executable(candidate))
        .ok_or_else(|| {
            eyre!(
                "llm-stream runner '{}' was not found on PATH",
                binary.display()
            )
        })
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;

    std::fs::metadata(path)
        .is_ok_and(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
fn is_executable(path: &Path) -> bool {
    path.is_file()
}

fn format_plan(
    plan: &EnrichmentPlan,
    scope: &Scope,
    runner: &RunnerConfig,
    runner_binary: &Path,
    concurrency: usize,
    limit: Option<NonZeroUsize>,
    refresh: bool,
) -> String {
    let files = plan
        .entries
        .iter()
        .filter(|entry| entry.target.kind == EntryKind::File)
        .count();
    let directories = plan.entries.len() - files;
    let bounded_out = plan.pending().saturating_sub(plan.expected_calls());
    let mut output = format!(
        "Atlas enrichment plan (read-only)\n  scope: {}\n  runner: {}\n  provider: {}\n  file: model={}, template={}\n  directory: model={}, template={}\n  concurrency: {}\n  refresh: {}\n  selected: {} files, {} directories\n  cache: {} reusable, {} stale, {} missing, {} failed, {} deferred, {} in-flight\n  expected model calls: {}\n  scheduled context estimate: {} tokens",
        scope.label,
        runner_binary.display(),
        runner.provider,
        runner.file.model,
        runner.file.template,
        runner.directory.model,
        runner.directory.template,
        concurrency,
        refresh,
        files,
        directories,
        plan.count(crate::atlas::enrichment::PlanStatus::Reusable),
        plan.count(crate::atlas::enrichment::PlanStatus::Stale),
        plan.count(crate::atlas::enrichment::PlanStatus::Missing),
        plan.count(crate::atlas::enrichment::PlanStatus::Failed),
        plan.count(crate::atlas::enrichment::PlanStatus::Deferred),
        plan.count(crate::atlas::enrichment::PlanStatus::InFlight),
        plan.expected_calls(),
        plan.scheduled_context_tokens(),
    );
    if let Some(limit) = limit {
        output.push_str(&format!(
            "\n  invocation limit: {} ({} pending beyond limit)",
            limit, bounded_out
        ));
    }
    if scope.external_ancestors.is_empty() {
        output.push_str("\n  outside-scope invalidation: none");
    } else {
        output.push_str(&format!(
            "\n  outside-scope invalidation: {} may become stale",
            scope
                .external_ancestors
                .iter()
                .map(|path| path.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    output.push_str("\n  entries:");
    for entry in &plan.entries {
        let kind = match entry.target.kind {
            EntryKind::File => "file",
            EntryKind::Directory => "directory",
        };
        let scheduling = if entry.scheduled {
            "scheduled"
        } else if entry.status == crate::atlas::enrichment::PlanStatus::Reusable {
            "cache hit"
        } else if entry.status == crate::atlas::enrichment::PlanStatus::InFlight {
            "active lease"
        } else {
            "beyond limit"
        };
        output.push_str(&format!(
            "\n    {} {}  {}  {} tokens  {}",
            entry.status.as_str(),
            kind,
            display_scope(&entry.target.path),
            entry.context_tokens,
            scheduling,
        ));
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lexical_normalization_removes_dot_segments_without_crossing_root() {
        assert_eq!(
            normalize_lexically(Path::new("/repo/src/../src/lib.rs")).unwrap(),
            PathBuf::from("/repo/src/lib.rs")
        );
        assert!(normalize_lexically(Path::new("../../repo")).is_err());
    }

    #[test]
    fn scope_selection_uses_path_components_not_prefixes() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join("src")).unwrap();
        let scope = resolve_scope(
            root.path(),
            Some(&root.path().join("src")),
            &["src/a.rs".into(), "src_extra/b.rs".into()],
            &["src".into(), "src_extra".into()],
        )
        .unwrap();
        assert_eq!(scope.files, [PathBuf::from("src/a.rs")]);
        assert_eq!(scope.directories, [PathBuf::from("src")]);
    }

    #[test]
    fn absent_index_is_not_created_by_read_only_open() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("index.db");
        assert!(Database::open_read_only(&path).is_err());
        assert!(!path.exists());
    }
}
