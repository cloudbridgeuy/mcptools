use super::{atlassian, github, linear, JsonRpcError};
use mcptools_core::github as core_github;
use mcptools_core::work_status::pr_keys_from_search;
use mcptools_core::work_status::NaiveDate;
use mcptools_core::work_status::{
    as_of_from_unix_secs, author_search_args, build_report, jira_ticket, linear_ticket,
    parse_as_of, snapshot_of, view_requests, GithubPr, PrInput, PrKey, Report, ReportInput,
    Snapshot, SnapshotName, SourceError, TicketInput,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

const DEFAULT_STALE_DAYS: i64 = 7;
const MAX_SEARCH_PAGES: u32 = 100;

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GithubSource {
    #[serde(default)]
    repos: Vec<String>,
    author: Option<String>,
}

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct JiraSource {
    #[serde(default)]
    projects: Vec<String>,
}

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LinearSource {
    #[serde(default)]
    teams: Vec<String>,
}

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Sources {
    #[serde(default)]
    github: GithubSource,
    #[serde(default)]
    jira: JiraSource,
    #[serde(default)]
    linear: LinearSource,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Query {
    person: Option<String>,
    #[serde(default)]
    sources: Sources,
    stale_days: Option<u32>,
    as_of: Option<String>,
    include_closed: Option<bool>,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, schemars::JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum Format {
    #[default]
    Json,
    Markdown,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Args {
    #[serde(flatten)]
    query: Query,
    snapshot: Option<String>,
    format: Option<Format>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotSaveArgs {
    name: String,
    #[serde(flatten)]
    query: Query,
}

#[derive(Debug, serde::Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotSaveOutput {
    name: String,
    path: String,
    items: usize,
    source_errors: Vec<SourceError>,
}

fn invalid(message: impl ToString) -> JsonRpcError {
    JsonRpcError {
        code: -32602,
        message: message.to_string(),
        data: None,
    }
}

fn internal(message: impl ToString) -> JsonRpcError {
    JsonRpcError {
        code: -32603,
        message: message.to_string(),
        data: None,
    }
}

fn parse<T: for<'de> Deserialize<'de>>(arguments: Option<Value>) -> Result<T, JsonRpcError> {
    serde_json::from_value(arguments.unwrap_or_else(|| json!({})))
        .map_err(|e| invalid(format!("Invalid arguments: {e}")))
}

pub fn snapshot_dir() -> Result<PathBuf, JsonRpcError> {
    Ok(crate::plan_store::data_dir()
        .map_err(internal)?
        .join("work-status"))
}

pub fn load_snapshot(dir: &Path, raw_name: &str) -> Result<Snapshot, JsonRpcError> {
    let name = SnapshotName::parse(raw_name).map_err(invalid)?;
    let path = dir.join(name.file_name());
    let bytes = std::fs::read(&path).map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => invalid(format!("Snapshot not found: {}", name.as_str())),
        _ => internal(format!("Cannot read snapshot {}: {e}", name.as_str())),
    })?;
    serde_json::from_slice(&bytes)
        .map_err(|e| internal(format!("Invalid snapshot {}: {e}", name.as_str())))
}

pub fn save_snapshot(
    dir: &Path,
    raw_name: &str,
    snapshot: &Snapshot,
) -> Result<PathBuf, JsonRpcError> {
    let name = SnapshotName::parse(raw_name).map_err(invalid)?;
    std::fs::create_dir_all(dir).map_err(internal)?;
    let target = dir.join(name.file_name());
    let mut file = tempfile::NamedTempFile::new_in(dir).map_err(internal)?;
    serde_json::to_writer_pretty(&mut file, snapshot).map_err(internal)?;
    std::io::Write::flush(&mut file).map_err(internal)?;
    file.persist(&target).map_err(|e| internal(e.error))?;
    Ok(target)
}

fn structured(result: Value) -> Result<Value, JsonRpcError> {
    result
        .get("structuredContent")
        .cloned()
        .ok_or_else(|| internal("Upstream tool returned no structured content"))
}

fn source_error(source: &str, scope: &str, error: &JsonRpcError) -> SourceError {
    SourceError {
        source: source.to_owned(),
        scope: Some(scope.to_owned()),
        message: error.message.clone(),
    }
}

struct Collected {
    tickets: Vec<TicketInput>,
    prs: Vec<PrInput>,
    errors: Vec<SourceError>,
}

async fn fetch_linear(
    args: Value,
    global: &crate::Global,
) -> Result<Vec<TicketInput>, JsonRpcError> {
    let output: mcptools_core::linear::StatusListOutput = serde_json::from_value(structured(
        linear::handle_linear_status_list(Some(args), global).await?,
    )?)
    .map_err(internal)?;
    Ok(output.nodes.iter().map(linear_ticket).collect())
}

async fn fetch_jira(args: Value, global: &crate::Global) -> Result<Vec<TicketInput>, JsonRpcError> {
    let output: mcptools_core::atlassian::jira::status::StatusListOutput = serde_json::from_value(
        structured(atlassian::handle_jira_status_list(Some(args), global).await?)?,
    )
    .map_err(internal)?;
    Ok(output.nodes.iter().map(jira_ticket).collect())
}

async fn fetch_prs(request: Value) -> Result<Vec<GithubPr>, JsonRpcError> {
    let output = structured(github::view(Some(request)).await?)?;
    serde_json::from_value(output["pullRequests"].clone()).map_err(internal)
}

async fn search_prs(args: Value) -> Result<Value, JsonRpcError> {
    github::search(args).await
}

async fn collect(query: &Query, global: &crate::Global) -> Collected {
    let person = query.person.as_deref().unwrap_or("me");
    let open = !query.include_closed.unwrap_or(false);
    let mut collected = Collected {
        tickets: Vec::new(),
        prs: Vec::new(),
        errors: Vec::new(),
    };
    for team in &query.sources.linear.teams {
        let args = json!({"assignee":person,"team":team,"open":open});
        match fetch_linear(args, global).await {
            Ok(tickets) => collected.tickets.extend(tickets),
            Err(error) => collected.errors.push(source_error("linear", team, &error)),
        }
    }
    for project in &query.sources.jira.projects {
        let args = json!({"assignee":person,"project":project,"open":open});
        match fetch_jira(args, global).await {
            Ok(tickets) => collected.tickets.extend(tickets),
            Err(error) => collected.errors.push(source_error("jira", project, &error)),
        }
    }
    for repo in &query.sources.github.repos {
        let linked: BTreeSet<u64> = collected
            .tickets
            .iter()
            .flat_map(|t| t.pr_refs.iter())
            .filter(|r| &r.repo == repo)
            .map(|r| r.number)
            .collect();
        if linked.is_empty() {
            continue;
        }
        match fetch_prs(json!({"repo":repo,"numbers":linked})).await {
            Ok(records) => collected
                .prs
                .extend(records.into_iter().map(|record| PrInput {
                    repo: repo.clone(),
                    record,
                })),
            Err(error) => collected.errors.push(source_error("github", repo, &error)),
        }
    }
    if let Some(author) = &query.sources.github.author {
        let (prs, errors) = collect_author_prs(author, search_prs, fetch_prs).await;
        for pr in prs {
            let known = collected
                .prs
                .iter()
                .any(|p| p.repo == pr.repo && p.record.number == pr.record.number);
            if !known {
                collected.prs.push(pr);
            }
        }
        collected.errors.extend(errors);
    }
    collected
}

async fn search_author_keys<S>(author: &str, search: &S) -> Result<Vec<PrKey>, JsonRpcError>
where
    S: AsyncFn(Value) -> Result<Value, JsonRpcError>,
{
    let mut items: Vec<Value> = Vec::new();
    for page in 1..=MAX_SEARCH_PAGES {
        let response = search(author_search_args(author, page).map_err(invalid)?).await?;
        let batch = core_github::page_items("search_pull_requests", &response).map_err(internal)?;
        let count = batch.len();
        items.extend(batch.iter().cloned());
        if core_github::page_complete(&response, count, items.len()) {
            return pr_keys_from_search(&items).map_err(internal);
        }
    }
    Err(internal("GitHub author search exceeded 100 pages"))
}

async fn collect_author_prs<S, V>(
    author: &str,
    search: S,
    view: V,
) -> (Vec<PrInput>, Vec<SourceError>)
where
    S: AsyncFn(Value) -> Result<Value, JsonRpcError>,
    V: AsyncFn(Value) -> Result<Vec<GithubPr>, JsonRpcError>,
{
    let scope = format!("author:{author}");
    let mut prs = Vec::new();
    let mut errors = Vec::new();
    match search_author_keys(author, &search).await {
        Err(error) => errors.push(source_error("github", &scope, &error)),
        Ok(keys) => {
            for (repo, request) in view_requests(&keys) {
                match view(request).await {
                    Ok(records) => prs.extend(records.into_iter().map(|record| PrInput {
                        repo: repo.clone(),
                        record,
                    })),
                    Err(error) => errors.push(source_error("github", &scope, &error)),
                }
            }
        }
    }
    (prs, errors)
}

fn as_of(query: &Query) -> Result<NaiveDate, JsonRpcError> {
    match &query.as_of {
        Some(raw) => parse_as_of(raw).map_err(invalid),
        None => {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(internal)?;
            let secs = i64::try_from(now.as_secs()).map_err(internal)?;
            as_of_from_unix_secs(secs).map_err(internal)
        }
    }
}

async fn report(
    query: &Query,
    snapshot: Option<Snapshot>,
    global: &crate::Global,
) -> Result<Report, JsonRpcError> {
    let as_of = as_of(query)?;
    let collected = collect(query, global).await;
    Ok(build_report(ReportInput {
        tickets: collected.tickets,
        prs: collected.prs,
        source_errors: collected.errors,
        as_of,
        stale_days: query.stale_days.map_or(DEFAULT_STALE_DAYS, i64::from),
        snapshot,
    }))
}

pub async fn handle_work_status(
    arguments: Option<Value>,
    global: &crate::Global,
) -> Result<Value, JsonRpcError> {
    let args: Args = parse(arguments)?;
    let snapshot = args
        .snapshot
        .as_deref()
        .map(|name| load_snapshot(&snapshot_dir()?, name))
        .transpose()?;
    let report = report(&args.query, snapshot, global).await?;
    let mut result = super::to_dual_result(&report)?;
    if args.format == Some(Format::Markdown) {
        result["content"][0]["text"] = json!(report.markdown);
    }
    Ok(result)
}

pub async fn handle_work_status_snapshot_save(
    arguments: Option<Value>,
    global: &crate::Global,
) -> Result<Value, JsonRpcError> {
    let args: SnapshotSaveArgs = parse(arguments)?;
    SnapshotName::parse(&args.name).map_err(invalid)?;
    let report = report(&args.query, None, global).await?;
    let snapshot = snapshot_of(&report);
    let path = save_snapshot(&snapshot_dir()?, &args.name, &snapshot)?;
    super::to_dual_result(SnapshotSaveOutput {
        name: args.name,
        path: path.display().to_string(),
        items: snapshot.items.len(),
        source_errors: report.source_errors,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use mcptools_core::work_status::{SnapshotItem, Source, Stage};

    fn snapshot() -> Snapshot {
        Snapshot {
            version: 1,
            items: vec![SnapshotItem {
                source: Source::Linear,
                key: "GUZ-1".into(),
                stage: Stage::Draft,
                flags: vec![],
            }],
        }
    }

    fn entries(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    fn fake_pr(number: u64) -> GithubPr {
        serde_json::from_value(json!({"number":number,"title":format!("T-{number}"),"url":null,"state":"open","isDraft":false}))
            .unwrap()
    }

    fn search_page(items: &[(&str, u64)]) -> Value {
        let items: Vec<Value> = items
            .iter()
            .map(|(repo, n)| json!({"number":n,"repository_url":format!("https://api.github.com/repos/{repo}")}))
            .collect();
        json!({"total_count":items.len(),"items":items})
    }

    #[tokio::test]
    async fn author_without_repos_gets_prs_from_several_repos() {
        let search = async |args: Value| -> Result<Value, JsonRpcError> {
            assert_eq!(args["query"], "is:pr is:open author:ada");
            Ok(search_page(&[
                ("acme/a", 1),
                ("acme/b", 2),
                ("acme/b", 3),
                ("zed/c", 4),
            ]))
        };
        let view = async |args: Value| -> Result<Vec<GithubPr>, JsonRpcError> {
            let numbers = args["numbers"].as_array().unwrap();
            Ok(numbers
                .iter()
                .map(|n| fake_pr(n.as_u64().unwrap()))
                .collect())
        };
        let (prs, errors) = collect_author_prs("ada", search, view).await;
        let got: Vec<(String, u64)> = prs
            .iter()
            .map(|p| (p.repo.clone(), p.record.number))
            .collect();
        assert_eq!(
            got,
            [
                ("acme/a".to_owned(), 1),
                ("acme/b".to_owned(), 2),
                ("acme/b".to_owned(), 3),
                ("zed/c".to_owned(), 4)
            ]
        );
        assert!(errors.is_empty());
    }

    #[tokio::test]
    async fn author_search_failure_becomes_a_source_error_naming_the_author() {
        let search = async |_: Value| -> Result<Value, JsonRpcError> {
            Err(internal("tool search_pull_requests not found"))
        };
        let view = async |_: Value| -> Result<Vec<GithubPr>, JsonRpcError> { Ok(Vec::new()) };
        let (prs, errors) = collect_author_prs("ada", search, view).await;
        assert!(prs.is_empty());
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].source, "github");
        assert_eq!(errors[0].scope.as_deref(), Some("author:ada"));
        assert!(errors[0].message.contains("search_pull_requests"));
    }

    #[test]
    fn bad_names_are_rejected_and_nothing_is_written_outside_state_dir() {
        let root = tempfile::tempdir().unwrap();
        let state = root.path().join("state").join("work-status");
        let outside = root.path().join("outside");
        std::fs::create_dir(&outside).unwrap();
        for bad in [
            outside.join("abs").to_str().unwrap().to_owned(),
            "../x".to_owned(),
            "a/b".to_owned(),
            "..".to_owned(),
            "../../outside/x".to_owned(),
        ] {
            assert_eq!(
                save_snapshot(&state, &bad, &snapshot()).unwrap_err().code,
                -32602,
                "{bad}"
            );
            assert_eq!(load_snapshot(&state, &bad).unwrap_err().code, -32602);
        }
        assert!(!state.exists());
        assert!(entries(&outside).is_empty());
        assert_eq!(entries(root.path()), ["outside"]);
    }

    #[test]
    fn saved_snapshot_lands_in_state_dir_and_loads_back() {
        let root = tempfile::tempdir().unwrap();
        let state = root.path().join("work-status");
        let path = save_snapshot(&state, "daily-1", &snapshot()).unwrap();
        assert_eq!(path, state.join("daily-1.json"));
        assert_eq!(entries(&state), ["daily-1.json"]);
        assert_eq!(load_snapshot(&state, "daily-1").unwrap().items.len(), 1);
        assert_eq!(load_snapshot(&state, "missing").unwrap_err().code, -32602);
    }

    #[tokio::test]
    async fn direct_calls_cannot_write_without_allow_writes() {
        let global = crate::Global {
            verbose: false,
            execute_timeout_secs: 30,
            execute_memory_mb: 64,
            execute_output_kb: 256,
        };
        let flags = super::super::ServeFlags {
            discovery: false,
            code_mode: false,
        };
        let denied = super::super::handle_tools_call(
            Some(json!({"name":"work_status_snapshot_save","arguments":{"name":"x","save":true}})),
            &global,
            flags,
        )
        .await
        .unwrap();
        assert_eq!(denied["isError"], true);
        assert!(denied["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("allowWrites"));
    }

    #[test]
    fn work_status_tool_is_read_and_snapshot_save_is_write() {
        let tools = super::super::registered_tools();
        let kind = |name: &str| tools.iter().find(|t| t.name == name).unwrap().kind;
        assert_eq!(kind("work_status"), super::super::ToolKind::Read);
        assert_eq!(
            kind("work_status_snapshot_save"),
            super::super::ToolKind::Write
        );
    }
}
