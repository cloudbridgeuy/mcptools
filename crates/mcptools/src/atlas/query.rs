use std::path::{Component, Path, PathBuf};

use color_eyre::eyre::{bail, Result};
use mcptools_core::atlas::{Symbol, TreeEntry};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::db::{Database, PeekResult};

pub const DEFAULT_LIMIT: usize = 100;
pub const MAX_LIMIT: usize = 1000;
pub const DEFAULT_CONTEXT_LINES: usize = 20;
pub const MAX_CONTEXT_LINES: usize = 200;
pub const DEFAULT_CONTEXT_CHARS: usize = 8192;
pub const MAX_CONTEXT_CHARS: usize = 32768;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Bounds {
    pub limit: usize,
    pub total: usize,
    pub returned: usize,
    pub truncated: bool,
    pub context_chars: usize,
    pub context_truncated: bool,
}

pub fn bound<T>(values: &mut Vec<T>, limit: usize) -> Bounds {
    let total = values.len();
    values.truncate(limit);
    Bounds {
        limit,
        total,
        returned: values.len(),
        truncated: total > limit,
        context_chars: DEFAULT_CONTEXT_CHARS,
        context_truncated: false,
    }
}

pub fn validate_limit(limit: usize) -> Result<()> {
    if !(1..=MAX_LIMIT).contains(&limit) {
        bail!("limit must be between 1 and {MAX_LIMIT}");
    }
    Ok(())
}

pub fn validate_context(context_chars: usize) -> Result<()> {
    if context_chars > MAX_CONTEXT_CHARS {
        bail!("context_chars must be between 0 and {MAX_CONTEXT_CHARS}");
    }
    Ok(())
}

fn bound_context<'a>(
    values: impl IntoIterator<Item = &'a mut Option<String>>,
    limit: usize,
    bounds: &mut Bounds,
) {
    bounds.context_chars = limit;
    let mut remaining = limit;
    for value in values.into_iter().flatten() {
        let chars = value.chars().count();
        if chars > remaining {
            *value = value.chars().take(remaining).collect();
            bounds.context_truncated = true;
        }
        remaining = remaining.saturating_sub(chars);
    }
}

pub fn normalize_path(path: &Path) -> Result<PathBuf> {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => normalized.push(part),
            Component::CurDir => {}
            _ => {
                bail!("Atlas paths must be repository-relative, without '..'");
            }
        }
    }
    Ok(normalized)
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, JsonSchema, clap::ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum QueryKind {
    #[default]
    All,
    Files,
    Symbols,
}

#[derive(Debug, Clone, Serialize)]
pub struct QueryHit {
    pub path: PathBuf,
    pub score: u32,
    pub matched_on: String,
    pub symbol: Option<Symbol>,
    pub short_description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub excerpt: Option<Excerpt>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Excerpt {
    pub source: &'static str,
    pub start_line: u32,
    pub end_line: u32,
    pub text: String,
    pub truncated: bool,
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct QueryResult {
    pub query: String,
    pub kind: QueryKind,
    pub hits: Vec<QueryHit>,
    pub bounds: Bounds,
    pub context_lines: usize,
    pub freshness: String,
}

pub struct QueryRequest<'a> {
    pub query: &'a str,
    pub kind: QueryKind,
    pub limit: usize,
    pub context_lines: usize,
    pub context_chars: usize,
}

fn match_score(value: &str, query: &str, base: u32) -> Option<u32> {
    let value = value.to_lowercase();
    if value == query {
        Some(base + 3)
    } else if value.starts_with(query) {
        Some(base + 2)
    } else if value.contains(query) {
        Some(base + 1)
    } else {
        None
    }
}

pub fn rank_hits(
    files: Vec<(PathBuf, Option<String>, Vec<Symbol>)>,
    query: &str,
    kind: QueryKind,
) -> Vec<QueryHit> {
    let query = query.to_lowercase();
    let mut hits = Vec::new();
    for (path, description, symbols) in files {
        if !matches!(kind, QueryKind::Symbols) {
            let score = match_score(&path.to_string_lossy(), &query, 60)
                .map(|score| (score, "path"))
                .or_else(|| {
                    description
                        .as_deref()
                        .and_then(|value| match_score(value, &query, 10))
                        .map(|score| (score, "description"))
                });
            if let Some((score, matched_on)) = score {
                hits.push(QueryHit {
                    path: path.clone(),
                    score,
                    matched_on: matched_on.into(),
                    symbol: None,
                    short_description: description.clone(),
                    excerpt: None,
                });
            }
        }
        if !matches!(kind, QueryKind::Files) {
            for symbol in symbols {
                let score = match_score(&symbol.name, &query, 100)
                    .map(|score| (score, "symbol_name"))
                    .or_else(|| {
                        symbol
                            .signature
                            .as_deref()
                            .and_then(|value| match_score(value, &query, 30))
                            .map(|score| (score, "signature"))
                    });
                if let Some((score, matched_on)) = score {
                    hits.push(QueryHit {
                        path: path.clone(),
                        score,
                        matched_on: matched_on.into(),
                        symbol: Some(symbol),
                        short_description: description.clone(),
                        excerpt: None,
                    });
                }
            }
        }
    }
    hits.sort_by(|a, b| {
        b.score
            .cmp(&a.score)
            .then_with(|| a.path.cmp(&b.path))
            .then_with(|| {
                let key = |hit: &QueryHit| {
                    hit.symbol.as_ref().map(|s| {
                        (
                            s.start_line,
                            s.end_line,
                            s.name.clone(),
                            s.kind.to_string(),
                            s.signature.clone(),
                            s.visibility.to_string(),
                        )
                    })
                };
                key(a).cmp(&key(b))
            })
    });
    hits
}

pub fn validate_query(query: &str, limit: usize, context_lines: usize) -> Result<()> {
    validate_limit(limit)?;
    if query.trim().is_empty() || query.len() > 1024 {
        bail!("query must contain 1 to 1024 bytes of non-blank text");
    }
    if context_lines > MAX_CONTEXT_LINES {
        bail!("context_lines must be between 0 and {MAX_CONTEXT_LINES}");
    }
    Ok(())
}

pub fn query_data(
    db: &Database,
    root: &Path,
    request: QueryRequest<'_>,
    freshness: String,
) -> Result<QueryResult> {
    let QueryRequest {
        query,
        kind,
        limit,
        context_lines,
        context_chars,
    } = request;
    validate_query(query, limit, context_lines)?;
    validate_context(context_chars)?;
    let files = db
        .all_file_paths()?
        .into_iter()
        .map(|path| {
            let peek = db.peek_file(&path)?.ok_or_else(|| {
                color_eyre::eyre::eyre!("indexed file missing: {}", path.display())
            })?;
            Ok((path, peek.short_description, peek.symbols))
        })
        .collect::<Result<Vec<_>>>()?;
    let mut hits = rank_hits(files, query.trim(), kind);
    let mut bounds = bound(&mut hits, limit);
    bound_context(
        hits.iter_mut().flat_map(|hit| {
            std::iter::once(&mut hit.short_description)
                .chain(hit.symbol.iter_mut().map(|symbol| &mut symbol.signature))
        }),
        context_chars,
        &mut bounds,
    );
    if context_lines > 0 && context_chars > 0 {
        for hit in &mut hits {
            hit.excerpt = Some(read_excerpt(root, hit, context_lines, context_chars));
        }
    }
    Ok(QueryResult {
        query: query.trim().into(),
        kind,
        hits,
        bounds,
        context_lines,
        freshness,
    })
}

fn read_excerpt(root: &Path, hit: &QueryHit, limit: usize, context_chars: usize) -> Excerpt {
    let start_line = hit.symbol.as_ref().map_or(1, |s| s.start_line.max(1));
    let result = (|| -> Result<String> {
        let path = root.canonicalize()?.join(normalize_path(&hit.path)?);
        let canonical = path.canonicalize()?;
        if canonical != path || !std::fs::symlink_metadata(&path)?.file_type().is_file() {
            bail!("source path is not a regular repository file");
        }
        use std::io::Read;
        let mut bytes = Vec::new();
        std::fs::File::open(path)?
            .take(1024 * 1024 + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() > 1024 * 1024 {
            bail!("source exceeds 1 MiB excerpt read budget");
        }
        Ok(String::from_utf8(bytes)?)
    })();
    match result {
        Ok(source) => excerpt_from_source(
            &source,
            start_line,
            hit.symbol.as_ref().map(|s| s.end_line),
            limit,
            context_chars,
        ),
        Err(error) => Excerpt {
            source: "current_worktree",
            start_line,
            end_line: start_line,
            text: String::new(),
            truncated: false,
            error: Some(error.to_string()),
        },
    }
}

fn excerpt_from_source(
    source: &str,
    start_line: u32,
    end_line: Option<u32>,
    limit: usize,
    context_chars: usize,
) -> Excerpt {
    let lines: Vec<_> = source.lines().collect();
    let start = (start_line as usize - 1).min(lines.len());
    let end = end_line
        .map_or(lines.len(), |end| end as usize)
        .min(lines.len())
        .max(start);
    let selected = &lines[start..end.min(start + limit)];
    let selected_text = selected.join("\n");
    let text: String = selected_text.chars().take(context_chars).collect();
    let truncated = end - start > limit || selected_text.chars().count() > context_chars;
    let returned_lines = selected
        .len()
        .min(text.bytes().filter(|byte| *byte == b'\n').count() + 1);
    Excerpt {
        source: "current_worktree",
        start_line,
        end_line: start_line + returned_lines.saturating_sub(1) as u32,
        text,
        truncated,
        error: (start >= lines.len())
            .then(|| "indexed range is outside current source; refresh the index".into()),
    }
}

pub fn validate_tree(
    path: &Path,
    depth: u32,
    limit: usize,
    context_chars: usize,
) -> Result<PathBuf> {
    validate_limit(limit)?;
    validate_context(context_chars)?;
    if depth > 100 {
        bail!("depth must be between 0 and 100");
    }
    normalize_path(path)
}

pub fn tree_data(
    db: &Database,
    path: &Path,
    depth: u32,
    limit: usize,
    context_chars: usize,
) -> Result<(Vec<TreeEntry>, Bounds)> {
    let path = validate_tree(path, depth, limit, context_chars)?;
    let mut entries = db.tree_entries(&path, depth)?;
    let mut bounds = bound(&mut entries, limit);
    bound_context(
        entries.iter_mut().map(|entry| &mut entry.short_description),
        context_chars,
        &mut bounds,
    );
    Ok((entries, bounds))
}

pub fn peek_data(
    db: &Database,
    path: &Path,
    limit: usize,
    context_chars: usize,
) -> Result<(PeekResult, Bounds, Option<Bounds>)> {
    validate_limit(limit)?;
    validate_context(context_chars)?;
    let mut peek = db.peek_file_or_dir(&normalize_path(path)?)?;
    let (symbols, children) = match &mut peek {
        PeekResult::File(view) => {
            let mut bounds = bound(&mut view.symbols, limit);
            bound_context(
                [&mut view.short_description, &mut view.long_description]
                    .into_iter()
                    .chain(view.symbols.iter_mut().map(|symbol| &mut symbol.signature)),
                context_chars,
                &mut bounds,
            );
            (bounds, None)
        }
        PeekResult::Directory(view) => {
            let mut bounds = bound(&mut view.symbols, limit);
            let mut children = bound(&mut view.children, limit);
            bound_context(
                [&mut view.short_description, &mut view.long_description]
                    .into_iter()
                    .chain(view.symbols.iter_mut().map(|symbol| &mut symbol.signature)),
                context_chars,
                &mut bounds,
            );
            bound_context(
                view.children
                    .iter_mut()
                    .map(|child| &mut child.short_description),
                context_chars,
                &mut children,
            );
            (bounds, Some(children))
        }
    };
    Ok((peek, symbols, children))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranking_and_context_are_stable_and_unicode_safe() {
        let files = vec![
            (
                PathBuf::from("z.rs"),
                Some("provider implementation".into()),
                Vec::new(),
            ),
            (PathBuf::from("b/provider.rs"), None, Vec::new()),
            (PathBuf::from("a/provider.rs"), None, Vec::new()),
        ];
        let hits = rank_hits(files, "PROVIDER", QueryKind::All);
        assert_eq!(
            hits.iter()
                .map(|hit| hit.path.to_str().unwrap())
                .collect::<Vec<_>>(),
            ["a/provider.rs", "b/provider.rs", "z.rs"]
        );
        assert_eq!(hits[2].matched_on, "description");
        let mut values = vec![Some("世界é".into()), None, Some("remaining".into())];
        let mut bounds = bound(&mut values, 3);
        bound_context(values.iter_mut(), 2, &mut bounds);
        assert_eq!(values, [Some("世界".into()), None, Some(String::new())]);
        assert!(bounds.context_truncated);
        let excerpt = excerpt_from_source("世界é\nsecond\n", 1, None, 2, 2);
        assert_eq!(excerpt.text, "世界");
        assert_eq!(excerpt.end_line, 1);
        assert!(excerpt.truncated);
    }

    #[test]
    fn excerpts_preserve_blank_line_spans() {
        let blank = excerpt_from_source("\nsecond\n", 1, None, 1, 100);
        assert_eq!(blank.start_line, 1);
        assert_eq!(blank.end_line, 1);
        assert!(blank.error.is_none());
        let trailing_blank = excerpt_from_source("first\n\nthird\n", 1, None, 2, 100);
        assert_eq!(trailing_blank.end_line, 2);
        let missing = excerpt_from_source("first\n", 5, Some(6), 2, 100);
        assert_eq!(missing.end_line, 5);
        assert!(missing.error.is_some());
    }

    #[test]
    fn ranking_breaks_symbol_ties_by_visibility() {
        use mcptools_core::atlas::{SymbolKind, Visibility};
        let public = Symbol {
            file_path: PathBuf::from("provider.rs"),
            name: "provider".into(),
            kind: SymbolKind::Function,
            signature: None,
            visibility: Visibility::Public,
            start_line: 1,
            end_line: 1,
        };
        let private = Symbol {
            visibility: Visibility::Private,
            ..public.clone()
        };
        let hits = rank_hits(
            vec![(public.file_path.clone(), None, vec![public, private])],
            "provider",
            QueryKind::Symbols,
        );
        assert_eq!(
            hits[0].symbol.as_ref().unwrap().visibility,
            Visibility::Private
        );
    }

    #[test]
    fn navigation_validates_bounds_before_io() {
        assert!(validate_query(" ", 1, 0).is_err());
        assert!(validate_query(&"x".repeat(1025), 1, 0).is_err());
        assert!(validate_query("provider", 1001, 0).is_err());
        assert!(validate_query("provider", 1, 201).is_err());
        assert!(validate_context(32769).is_err());
        assert!(validate_tree(Path::new("src"), 101, 1, 0).is_err());
        assert!(normalize_path(Path::new("/src")).is_err());
        assert!(normalize_path(Path::new("src/../other")).is_err());
        assert_eq!(
            normalize_path(Path::new("./src/")).unwrap(),
            Path::new("src")
        );
        assert!(validate_tree(Path::new(""), 0, 1000, 32768).is_ok());
    }

    #[cfg(unix)]
    #[test]
    fn excerpts_reject_symlinked_parent_directories() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join("real")).unwrap();
        std::fs::write(root.path().join("real/source.rs"), "private source").unwrap();
        std::os::unix::fs::symlink("real", root.path().join("linked")).unwrap();
        let hit = QueryHit {
            path: PathBuf::from("linked/source.rs"),
            score: 0,
            matched_on: "path".into(),
            symbol: None,
            short_description: None,
            excerpt: None,
        };
        let excerpt = read_excerpt(root.path(), &hit, 1, 100);
        assert!(excerpt.error.is_some());
        assert!(excerpt.text.is_empty());
    }
}
