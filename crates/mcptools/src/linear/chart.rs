use super::{discover, issue};
use crate::linear::client::execute;
use crate::prelude::{eprintln, println, *};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{Html, IntoResponse, Json},
    routing::{get, post},
    Router,
};
pub use mcptools_core::linear::{closure_nodes, FetchedIssue};
use mcptools_core::linear::{ChartClass, ChartNode};
use mcptools_core::linear::{
    ConnectionStop, GraphConnection, IncompleteConnection, IssueClosure, IssueGraphLimits,
    IssueGraphOutput,
};
use std::collections::{HashSet, VecDeque};
use std::sync::Arc;

pub const ISSUE_CAP: usize = 300;

const CHART_ISSUE_QUERY: &str = "query ($id: String!, $after: String) { issue(id: $id) { id identifier title url state { name type } parent { identifier } relations(first: 25) { nodes { type relatedIssue { identifier } } } inverseRelations(first: 25) { nodes { type issue { identifier } } } children(first: 50, after: $after) { nodes { identifier } pageInfo { hasNextPage endCursor } } } }";

const GRAPH_ISSUE_QUERY: &str = "query ($id: String!, $after: String, $blocksAfter: String, $blockedByAfter: String, $children: Boolean!, $blocks: Boolean!, $blockedBy: Boolean!) { issue(id: $id) { identifier title url state { type } parent { identifier } relations(first: 25, after: $blocksAfter) @include(if: $blocks) { nodes { type relatedIssue { identifier } } pageInfo { hasNextPage endCursor } } inverseRelations(first: 25, after: $blockedByAfter) @include(if: $blockedBy) { nodes { type issue { identifier } } pageInfo { hasNextPage endCursor } } children(first: 50, after: $after) @include(if: $children) { nodes { identifier } pageInfo { hasNextPage endCursor } } } }";

#[derive(Debug)]
pub struct ChartClosure {
    pub nodes: Vec<ChartNode>,
    pub cap_hit: bool,
    pub missing_blockers: Vec<(String, String)>,
}

pub fn validate_issue_ids(seeds: &[String]) -> Result<Vec<String>> {
    if seeds.is_empty() {
        return Err(eyre!("Linear chart needs at least one issue id"));
    }
    let mut validated = Vec::with_capacity(seeds.len());
    for seed in seeds {
        let trimmed = seed.trim();
        if trimmed.is_empty() {
            return Err(eyre!("Linear chart issue id must not be empty"));
        }
        validated.push(trimmed.to_string());
    }
    Ok(validated)
}

pub fn validate_chart_args(
    issues: &[String],
    project: Option<&str>,
    team: Option<&str>,
) -> Result<(Vec<String>, Option<String>, Option<String>)> {
    let project = project.map(str::trim).filter(|text| !text.is_empty());
    let team = team.map(str::trim).filter(|text| !text.is_empty());
    if team.is_some() && project.is_none() {
        return Err(eyre!(
            "Linear chart --team needs --project; it only resolves project names"
        ));
    }
    if issues.is_empty() && project.is_none() {
        return Err(eyre!(
            "Linear chart needs at least one issue id (e.g. GUZ-185) or --project <ID_OR_NAME>"
        ));
    }
    let positional = match issues.is_empty() {
        true => Vec::new(),
        false => validate_issue_ids(issues)?,
    };
    Ok((
        positional,
        project.map(str::to_string),
        team.map(str::to_string),
    ))
}

pub async fn chart_data(
    client: &reqwest::Client,
    seeds: &[String],
    exclude_completed: bool,
    limit: usize,
) -> Result<ChartClosure> {
    let seeds = validate_issue_ids(seeds)?;
    if limit == 0 {
        return Err(eyre!("Linear chart limit must be at least 1"));
    }
    let closure = fetch_closure(&seeds, limit, None, |selector| async move {
        let (issue, incomplete) = fetch_issue_with(&selector, None, |query, variables| {
            execute(client, query, variables)
        })
        .await?;
        let issue = issue.ok_or_else(|| eyre!("Linear issue not found: {}", selector))?;
        Ok((Some(issue), incomplete))
    })
    .await?;
    let (nodes, missing_blockers) = closure_nodes(&closure.fetched, exclude_completed);
    Ok(ChartClosure {
        nodes,
        cap_hit: !closure.pending.is_empty(),
        missing_blockers,
    })
}

pub async fn issue_graph_data(
    client: &reqwest::Client,
    request: IssueGraphRequest,
) -> Result<IssueGraphOutput> {
    let IssueGraphRequest {
        seeds,
        limit,
        max_pages,
    } = request;
    let closure = fetch_closure(&seeds, limit, Some(limit), |selector| async move {
        fetch_issue_with(&selector, Some(max_pages), |query, variables| {
            execute(client, query, variables)
        })
        .await
    })
    .await?;
    Ok(mcptools_core::linear::issue_graph_output(
        &seeds,
        closure,
        IssueGraphLimits {
            issues: limit,
            pages_per_issue: max_pages,
            children_per_page: 50,
            relations_per_page: 25,
        },
    ))
}

#[derive(Debug)]
pub struct IssueGraphRequest {
    seeds: Vec<String>,
    limit: usize,
    max_pages: usize,
}

impl IssueGraphRequest {
    pub fn new(ids: &[String], limit: usize, max_pages: usize) -> Result<Self> {
        if !(1..=ISSUE_CAP).contains(&limit) {
            return Err(eyre!(
                "Linear issue graph limit must be between 1 and {}",
                ISSUE_CAP
            ));
        }
        if !(1..=10).contains(&max_pages) {
            return Err(eyre!(
                "Linear issue graph maxPages must be between 1 and 10"
            ));
        }
        if ids.len() > ISSUE_CAP {
            return Err(eyre!(
                "Linear issue graph accepts at most {} ids",
                ISSUE_CAP
            ));
        }
        let mut seeds = validate_issue_ids(ids)?;
        if seeds.iter().any(|id| id.len() > 128) {
            return Err(eyre!("Linear issue graph ids must be at most 128 bytes"));
        }
        seeds.sort();
        seeds.dedup();
        if seeds.len() > limit {
            return Err(eyre!(
                "Linear issue graph limit must cover all unique root ids"
            ));
        }
        Ok(Self {
            seeds,
            limit,
            max_pages,
        })
    }
}

async fn fetch_closure<F, Fut>(
    seeds: &[String],
    limit: usize,
    max_fetches: Option<usize>,
    mut fetch: F,
) -> Result<IssueClosure>
where
    F: FnMut(String) -> Fut,
    Fut: std::future::Future<Output = Result<(Option<FetchedIssue>, Vec<IncompleteConnection>)>>,
{
    let mut closure = IssueClosure::default();
    let mut queue: VecDeque<String> = seeds.iter().cloned().collect();
    let mut seen: HashSet<String> = seeds.iter().cloned().collect();
    while let Some(selector) = queue.pop_front() {
        if closure.fetched.contains_key(&selector) || closure.resolved.contains_key(&selector) {
            continue;
        }
        if closure.fetched.len() >= limit || max_fetches.is_some_and(|cap| closure.attempted >= cap)
        {
            queue.push_front(selector);
            break;
        }
        closure.attempted += 1;
        let (issue, incomplete) = fetch(selector.clone()).await?;
        let Some(issue) = issue else {
            closure.missing.push(selector);
            continue;
        };
        closure.resolved.insert(selector, issue.identifier.clone());
        seen.insert(issue.identifier.clone());
        closure.incomplete_connections.extend(incomplete);
        for id in issue
            .children
            .iter()
            .chain(issue.blocked_by.iter())
            .chain(issue.blocks.iter())
        {
            if seen.insert(id.clone()) {
                queue.push_back(id.clone());
            }
        }
        if let Some(parent) = issue.parent.clone() {
            if seen.insert(parent.clone()) {
                queue.push_back(parent);
            }
        }
        closure.fetched.insert(issue.identifier.clone(), issue);
    }
    closure.pending = queue
        .into_iter()
        .filter(|id| !closure.fetched.contains_key(id) && !closure.resolved.contains_key(id))
        .collect();
    closure.pending.sort();
    closure.pending.dedup();
    closure.missing.sort();
    closure.missing.dedup();
    closure
        .incomplete_connections
        .sort_by(|a, b| a.identifier.cmp(&b.identifier));
    Ok(closure)
}

pub fn slugify(input: &str) -> String {
    let mut slug = String::with_capacity(input.len());
    for c in input.chars() {
        let c = match c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') {
            true => c,
            false => '-',
        };
        if c == '-' && (slug.is_empty() || slug.ends_with('-')) {
            continue;
        }
        slug.push(c);
    }
    let slug = slug.trim_matches('-');
    match slug.is_empty() {
        true => "chart".to_string(),
        false => slug.to_string(),
    }
}

pub fn default_out_path(slug: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("mcptools-linear-chart-{}.html", slug))
}

pub fn render_html(title: &str, chart: &str) -> String {
    let heading = escape_html(&format!("{title} — Linear chart"));
    let chart = escape_html(chart);
    format!(
        r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{heading}</title>
<style>
html, body {{
  margin: 0;
  width: 100%;
  min-height: 100dvh;
  overflow: auto;
  background-color: #0a0e1a;
  background-image: radial-gradient(#ffffff1c 1px, transparent 1px);
  background-size: 26px 26px;
  color: #e2e8f0;
  font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
}}
pre.mermaid {{
  margin: 0;
  padding: 0;
  background: transparent;
  font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
}}
svg {{
  max-width: none;
  height: auto;
  display: block;
}}
</style>
</head>
<body>
<pre class="mermaid">
{chart}
</pre>
<script type="module">
import mermaid from "https://cdn.jsdelivr.net/npm/mermaid@11/dist/mermaid.esm.min.mjs";
import elkLayouts from "https://cdn.jsdelivr.net/npm/@mermaid-js/layout-elk@0/dist/mermaid-layout-elk.esm.min.mjs";
mermaid.registerLayoutLoaders(elkLayouts);
mermaid.initialize({{
  startOnLoad: true,
  layout: 'elk',
  straightenEdges: true,
  theme: 'base',
  themeVariables: {{
    darkMode: true,
    background: '#0a0e1a',
    textColor: '#e2e8f0',
    titleColor: '#e2e8f0',
    primaryTextColor: '#e2e8f0',
    lineColor: '#94a3b8',
    clusterBkg: 'rgba(18,20,29,0.85)',
    clusterBorder: '#94a3b8',
    fontFamily: 'ui-monospace, SFMono-Regular, Menlo, monospace',
    fontSize: '14px'
  }},
  flowchart: {{ htmlLabels: true, useMaxWidth: false, curve: 'rounded' }}
}});
</script>
</body>
</html>
"#
    )
}

fn escape_html(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

pub fn validate_serve_out(serve: bool, out: Option<&std::path::Path>) -> Result<()> {
    if serve && out.is_some() {
        return Err(eyre!("Linear chart --out conflicts with --serve"));
    }
    Ok(())
}

pub fn chart_class_name(class: &ChartClass) -> &'static str {
    match class {
        ChartClass::Complete => "complete",
        ChartClass::InProgress => "inprogress",
        ChartClass::Frontier => "frontier",
        ChartClass::Fog => "fog",
    }
}

#[derive(Debug, Clone)]
pub struct ChartServeConfig {
    pub client: reqwest::Client,
    pub seeds: Vec<String>,
    pub project: Option<String>,
    pub team: Option<String>,
    pub exclude_completed: bool,
    pub limit: usize,
    pub title: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ChartNodePayload {
    pub identifier: String,
    pub title: String,
    pub state_type: String,
    pub parent: Option<String>,
    pub blocked_by: Vec<String>,
    pub class: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ChartPayload {
    pub title: String,
    pub stats_line: String,
    pub total: usize,
    pub complete: usize,
    pub inprogress: usize,
    pub frontier: usize,
    pub fog: usize,
    pub mermaid: String,
    pub cap_hit: bool,
    pub missing_blockers: Vec<(String, String)>,
    pub nodes: Vec<ChartNodePayload>,
}

#[derive(Debug, serde::Deserialize)]
struct StatesQuery {
    team: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
struct StateTransition {
    state: Option<String>,
    team: Option<String>,
}

pub async fn resolve_seeds(
    client: &reqwest::Client,
    positional: &[String],
    project: Option<&str>,
    team: Option<&str>,
    limit: usize,
) -> Result<Vec<String>> {
    let mut seeds = positional.to_vec();
    if let Some(project) = project {
        let filter = issue::resolve_issue_filter(
            client,
            team,
            Some(project),
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .await?;
        let mut project_seeds = Vec::new();
        let mut cursor: Option<String> = None;
        loop {
            let page = issue::issues_list_data(client, &filter, None, 50, cursor.clone()).await?;
            let has_next = page.page_info.has_next;
            cursor = page.page_info.end_cursor;
            project_seeds.extend(page.nodes.into_iter().map(|node| node.identifier));
            if !has_next || project_seeds.len() >= limit {
                break;
            }
        }
        if project_seeds.is_empty() {
            return Err(eyre!("Linear chart --project '{}' has no issues", project));
        }
        project_seeds.truncate(limit);
        seeds.extend(project_seeds);
    }
    Ok(seeds)
}

pub async fn build_chart_payload(config: &ChartServeConfig) -> Result<ChartPayload> {
    let seeds = resolve_seeds(
        &config.client,
        &config.seeds,
        config.project.as_deref(),
        config.team.as_deref(),
        config.limit,
    )
    .await?;
    let closure = chart_data(
        &config.client,
        &seeds,
        config.exclude_completed,
        config.limit,
    )
    .await?;
    let classes = mcptools_core::linear::classify_nodes(&closure.nodes);
    let stats = mcptools_core::linear::chart_stats(&closure.nodes);
    let mermaid = mcptools_core::linear::build_mermaid(&closure.nodes);
    let mut nodes: Vec<ChartNodePayload> = closure
        .nodes
        .iter()
        .map(|node| ChartNodePayload {
            identifier: node.identifier.clone(),
            title: node.title.clone(),
            state_type: node.state_type.clone(),
            parent: node.parent.clone(),
            blocked_by: node.blocked_by.clone(),
            class: classes
                .get(node.identifier.as_str())
                .map(chart_class_name)
                .unwrap_or("fog")
                .to_string(),
        })
        .collect();
    nodes.sort_by(|a, b| a.identifier.cmp(&b.identifier));
    Ok(ChartPayload {
        title: config.title.clone(),
        stats_line: mcptools_core::linear::stats_line(&stats),
        total: stats.total,
        complete: stats.complete,
        inprogress: stats.inprogress,
        frontier: stats.frontier,
        fog: stats.fog,
        mermaid,
        cap_hit: closure.cap_hit,
        missing_blockers: closure.missing_blockers,
        nodes,
    })
}

fn api_error(status: StatusCode, message: String) -> (StatusCode, Json<serde_json::Value>) {
    if status.is_server_error() {
        eprintln!("chart server backend error ({status}): {message}");
    }
    (status, Json(serde_json::json!({"error": message})))
}

async fn index_handler(State(config): State<Arc<ChartServeConfig>>) -> Html<String> {
    Html(render_app_html(&config.title))
}

async fn api_chart_handler(
    State(config): State<Arc<ChartServeConfig>>,
) -> Result<Json<ChartPayload>, (StatusCode, Json<serde_json::Value>)> {
    build_chart_payload(&config)
        .await
        .map(Json)
        .map_err(|e| api_error(StatusCode::BAD_GATEWAY, e.to_string()))
}

async fn api_issue_handler(
    State(config): State<Arc<ChartServeConfig>>,
    Path(id): Path<String>,
) -> Result<Json<mcptools_core::linear::IssueGetOutput>, (StatusCode, Json<serde_json::Value>)> {
    let id = id.trim();
    if id.is_empty() {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "issue id must not be empty".to_string(),
        ));
    }
    issue::issue_get_output(&config.client, id)
        .await
        .map(Json)
        .map_err(|e| match e.to_string().contains("not found") {
            true => api_error(StatusCode::NOT_FOUND, e.to_string()),
            false => api_error(StatusCode::BAD_GATEWAY, e.to_string()),
        })
}

async fn api_states_handler(
    State(config): State<Arc<ChartServeConfig>>,
    Query(query): Query<StatesQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let team = query
        .team
        .as_deref()
        .map(str::trim)
        .filter(|t| !t.is_empty());
    let team = match team {
        Some(team) => team,
        None => {
            return Err(api_error(
                StatusCode::BAD_REQUEST,
                "query ?team= is required".to_string(),
            ));
        }
    };
    discover::states_list_data(&config.client, team)
        .await
        .map(|data| Json(serde_json::json!({"nodes": data.nodes})))
        .map_err(|e| api_error(StatusCode::BAD_GATEWAY, e.to_string()))
}

async fn api_transition_handler(
    State(config): State<Arc<ChartServeConfig>>,
    Path(id): Path<String>,
    Json(body): Json<StateTransition>,
) -> Result<Json<mcptools_core::linear::IssueMini>, (StatusCode, Json<serde_json::Value>)> {
    let id = id.trim();
    if id.is_empty() {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "issue id must not be empty".to_string(),
        ));
    }
    let state = body
        .state
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let state = match state {
        Some(state) => state,
        None => {
            return Err(api_error(
                StatusCode::BAD_REQUEST,
                "body.state must not be empty".to_string(),
            ));
        }
    };
    let team = body
        .team
        .as_deref()
        .map(str::trim)
        .filter(|t| !t.is_empty());
    issue::issue_update_data(
        &config.client,
        id,
        None,
        None,
        Some(state),
        team,
        None,
        None,
        None,
        false,
    )
    .await
    .map(Json)
    .map_err(|e| match e.to_string().contains("not found") {
        true => api_error(StatusCode::NOT_FOUND, e.to_string()),
        false => api_error(StatusCode::BAD_GATEWAY, e.to_string()),
    })
}

pub async fn serve_chart(config: ChartServeConfig, port: u16, no_open: bool) -> Result<()> {
    let state = Arc::new(config);
    let app = Router::new()
        .route("/", get(index_handler))
        .route("/api/chart", get(api_chart_handler))
        .route("/api/issues/{id}", get(api_issue_handler))
        .route("/api/states", get(api_states_handler))
        .route("/api/issues/{id}/state", post(api_transition_handler))
        .with_state(state);
    let listener = tokio::net::TcpListener::bind(format!("127.0.0.1:{port}"))
        .await
        .map_err(|e| eyre!("Failed to bind chart server to port {}: {}", port, e))?;
    let addr = listener
        .local_addr()
        .map_err(|e| eyre!("Failed to read chart server address: {}", e))?;
    let url = format!("http://{addr}/");
    println!("{url}");
    crate::open::maybe_open(&[url], !no_open);
    axum::serve(listener, app)
        .await
        .map_err(|e| eyre!("Chart server error: {e}"))?;
    Ok(())
}

pub fn render_app_html(title: &str) -> String {
    let heading = escape_html(&format!("{title} — Linear chart"));
    format!(
        r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{heading}</title>
<style>
html, body {{
  margin: 0;
  width: 100%;
  min-height: 100dvh;
  background-color: #0a0e1a;
  background-image: radial-gradient(#ffffff1c 1px, transparent 1px);
  background-size: 26px 26px;
  color: #e2e8f0;
  font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
}}
body {{
  height: 100dvh;
  display: flex;
  flex-direction: column;
  overflow: hidden;
}}
header {{
  position: sticky;
  top: 0;
  z-index: 5;
  flex-shrink: 0;
  display: flex;
  gap: 12px;
  align-items: center;
  padding: 10px 16px;
  background: rgba(10,14,26,0.92);
  border-bottom: 1px solid #263049;
}}
header h1 {{
  font-size: 14px;
  margin: 0;
  font-weight: 600;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}}
#stats {{
  font-size: 12px;
  color: #94a3b8;
  flex: 1;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}}
button, select {{
  font: inherit;
  font-size: 12px;
  color: #e2e8f0;
  background: #16213a;
  border: 1px solid #334155;
  border-radius: 6px;
  padding: 6px 10px;
}}
button {{
  cursor: pointer;
}}
button:hover {{
  border-color: #60a5fa;
}}
#ticket-list button.complete {{
  background: #dcfce7;
  border-color: #15803d;
  color: #14532d;
}}
#ticket-list button.inprogress {{
  background: #fef9c3;
  border-color: #ca8a04;
  color: #713f12;
}}
#ticket-list button.frontier {{
  background: #dbeafe;
  border-color: #2563eb;
  color: #1e3a8a;
}}
#ticket-list button.fog {{
  background: #f1f5f9;
  border-color: #64748b;
  color: #334155;
}}
#ticket-list button.complete:hover {{
  border-color: #15803d;
}}
#ticket-list button.inprogress:hover {{
  border-color: #ca8a04;
}}
#ticket-list button.frontier:hover {{
  border-color: #2563eb;
}}
#ticket-list button.fog:hover {{
  border-color: #64748b;
}}
#status {{
  font-size: 12px;
  color: #94a3b8;
  min-width: 120px;
  text-align: right;
}}
#chart {{
  flex: 1 1 auto;
  min-height: 0;
  padding: 16px;
  overflow: auto;
  cursor: grab;
}}
#chart.panning {{
  cursor: grabbing;
  user-select: none;
}}
#ticket-list {{
  flex-shrink: 0;
  max-height: 30vh;
  overflow: auto;
  display: none;
  flex-wrap: wrap;
  gap: 8px;
  padding: 0 16px 16px 16px;
}}
#ticket-list.open {{
  display: flex;
}}
#chart svg {{
  max-width: none;
  height: auto;
  display: block;
}}
#chart g.node {{
  cursor: pointer;
}}
#chart g.cluster-label text {{
  fill: #e2e8f0;
}}
#chart .cluster-label {{
  color: #e2e8f0;
}}
#modal-backdrop {{
  position: fixed;
  inset: 0;
  background: rgba(2,6,23,0.7);
  display: none;
  align-items: center;
  justify-content: center;
  z-index: 20;
  padding: 24px;
}}
#modal-backdrop.open {{
  display: flex;
}}
#modal {{
  width: min(720px, 100%);
  max-height: 85dvh;
  overflow: auto;
  background: #0f172a;
  border: 1px solid #334155;
  border-radius: 12px;
  padding: 20px;
}}
#modal h2 {{
  margin: 0 0 4px 0;
  font-size: 16px;
}}
#modal .meta {{
  font-size: 12px;
  color: #94a3b8;
  margin-bottom: 12px;
}}
#modal .row {{
  display: flex;
  gap: 8px;
  margin: 12px 0;
  flex-wrap: wrap;
}}
.md {{
  font-size: 13px;
  font-family: ui-sans-serif, system-ui, sans-serif;
  line-height: 1.5;
  background: #0a0e1a;
  border: 1px solid #263049;
  border-radius: 8px;
  padding: 12px;
  overflow-wrap: break-word;
}}
.md > :first-child {{
  margin-top: 0;
}}
.md > :last-child {{
  margin-bottom: 0;
}}
.md h1, .md h2, .md h3, .md h4 {{
  color: #e2e8f0;
  margin: 12px 0 6px 0;
}}
.md p {{
  margin: 8px 0;
}}
.md a {{
  color: #60a5fa;
}}
.md code {{
  font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
  font-size: 12px;
  background: #16213a;
  border-radius: 4px;
  padding: 1px 4px;
}}
.md pre {{
  background: #020617;
  border: 1px solid #263049;
  border-radius: 8px;
  padding: 10px;
  overflow: auto;
}}
.md pre code {{
  background: transparent;
  padding: 0;
}}
.md blockquote {{
  border-left: 3px solid #334155;
  margin: 8px 0;
  padding: 4px 12px;
  color: #94a3b8;
}}
.md table {{
  border-collapse: collapse;
  margin: 8px 0;
}}
.md th, .md td {{
  border: 1px solid #334155;
  padding: 4px 8px;
}}
.md img {{
  max-width: 100%;
}}
.md ul, .md ol {{
  padding-left: 20px;
}}
.comment-author {{
  font-size: 11px;
  color: #94a3b8;
  margin-bottom: 4px;
}}
#modal h3 {{
  font-size: 12px;
  text-transform: uppercase;
  letter-spacing: 0.06em;
  color: #94a3b8;
  margin: 16px 0 8px 0;
}}
#modal ul {{
  margin: 0;
  padding-left: 18px;
  font-size: 12px;
}}
</style>
</head>
<body>
<header>
<h1>{heading}</h1>
<span id="stats">Loading</span>
<button id="refresh" type="button">Refresh</button>
<button id="tickets" type="button" aria-expanded="false" aria-controls="ticket-list">Show issues</button>
<span id="status" role="status"></span>
</header>
<div id="chart"></div>
<div id="ticket-list"></div>
<div id="modal-backdrop">
<div id="modal" role="dialog" aria-modal="true">
<h2 id="m-title"></h2>
<div class="meta" id="m-meta"></div>
<div class="row">
<select id="m-state"></select>
<button id="m-refresh" type="button">Refresh issue</button>
<button id="m-open" type="button">Open in Linear</button>
<button id="m-close" type="button">Close</button>
</div>
<div class="row"><span id="m-msg" role="status"></span></div>
<h3>Description</h3>
<div id="m-desc" class="md"></div>
<h3>Comments</h3>
<ul id="m-comments"></ul>
<h3>Activity</h3>
<ul id="m-activity"></ul>
</div>
</div>
<script type="module">
import mermaid from "https://cdn.jsdelivr.net/npm/mermaid@11/dist/mermaid.esm.min.mjs";
import elkLayouts from "https://cdn.jsdelivr.net/npm/@mermaid-js/layout-elk@0/dist/mermaid-layout-elk.esm.min.mjs";
import {{ marked }} from "https://cdn.jsdelivr.net/npm/marked@12/lib/marked.esm.js";
import DOMPurify from "https://cdn.jsdelivr.net/npm/dompurify@3/dist/purify.es.js";
mermaid.registerLayoutLoaders(elkLayouts);
mermaid.initialize({{
  startOnLoad: false,
  layout: 'elk',
  theme: 'base',
  themeVariables: {{
    darkMode: true,
    background: '#0a0e1a',
    textColor: '#e2e8f0',
    titleColor: '#e2e8f0',
    primaryTextColor: '#e2e8f0',
    lineColor: '#94a3b8',
    clusterBkg: 'rgba(18,20,29,0.85)',
    clusterBorder: '#94a3b8',
    fontFamily: 'ui-monospace, SFMono-Regular, Menlo, monospace',
    fontSize: '14px'
  }},
  flowchart: {{ htmlLabels: true, useMaxWidth: false, curve: 'rounded' }}
}});
const chartEl = document.getElementById('chart');
const statsEl = document.getElementById('stats');
const statusEl = document.getElementById('status');
const refreshBtn = document.getElementById('refresh');
const ticketsBtn = document.getElementById('tickets');
const ticketList = document.getElementById('ticket-list');
const backdrop = document.getElementById('modal-backdrop');
const mTitle = document.getElementById('m-title');
const mMeta = document.getElementById('m-meta');
const mState = document.getElementById('m-state');
const mRefresh = document.getElementById('m-refresh');
const mOpen = document.getElementById('m-open');
const mClose = document.getElementById('m-close');
const mMsg = document.getElementById('m-msg');
const mDesc = document.getElementById('m-desc');
const mComments = document.getElementById('m-comments');
const mActivity = document.getElementById('m-activity');
function renderMarkdown(text) {{
  const source = text && text.trim() ? text : '(none)';
  return DOMPurify.sanitize(marked.parse(source, {{ breaks: true }}));
}}
let renderSeq = 0;
let currentId = '';
let currentUrl = '';
let issueSeq = 0;
let lastState = '';
let currentProject = '';
function teamOf(identifier) {{
  const dash = identifier.indexOf('-');
  return dash > 0 ? identifier.slice(0, dash) : '';
}}
function setStatus(text) {{
  statusEl.textContent = text;
}}
async function loadChart() {{
  setStatus('Refreshing');
  refreshBtn.disabled = true;
  try {{
    const res = await fetch('/api/chart');
    if (!res.ok) {{
      const payload = await res.json().catch(() => ({{}}));
      throw new Error(payload.error || ('chart request failed: ' + res.status));
    }}
    const data = await res.json();
    statsEl.textContent = data.stats_line;
    renderSeq += 1;
    const renderId = 'chartSvg' + renderSeq;
    const rendered = await mermaid.render(renderId, data.mermaid);
    chartEl.innerHTML = rendered.svg;
    const svg = chartEl.querySelector('svg');
    if (svg) {{
      const box = svg.getBoundingClientRect();
      baseW = box.width;
      baseH = box.height;
      zoom = 1;
      svg.style.width = baseW + 'px';
      svg.style.height = baseH + 'px';
    }}
    bindNodes(data.nodes || []);
    const warnings = [];
    if (data.cap_hit) {{
      warnings.push('capped');
    }}
    for (const pair of (data.missing_blockers || [])) {{
      warnings.push(pair[0] + ' blocked by outside ' + pair[1]);
    }}
    setStatus(warnings.join('; '));
  }} catch (err) {{
    setStatus(String(err && err.message ? err.message : err));
  }} finally {{
    refreshBtn.disabled = false;
  }}
}}
function centerOn(identifier) {{
  let target = null;
  const nodeGroups = chartEl.querySelectorAll('g.node');
  for (const group of nodeGroups) {{
    const text = group.textContent || '';
    const match = text.match(/[A-Z][A-Z0-9]*-\d+/);
    if (match && match[0] === identifier) {{
      target = group;
      break;
    }}
  }}
  if (!target) {{
    const clusters = chartEl.querySelectorAll('g.cluster');
    for (const cl of clusters) {{
      const label = cl.querySelector(':scope > g.cluster-label, :scope > .cluster-label');
      if (label && (label.textContent || '').includes(identifier)) {{
        target = cl;
        break;
      }}
    }}
  }}
  if (!target) {{
    return;
  }}
  const targetRect = target.getBoundingClientRect();
  const chartRect = chartEl.getBoundingClientRect();
  const targetCenterX = targetRect.left + targetRect.width / 2;
  const targetCenterY = targetRect.top + targetRect.height / 2;
  const viewCenterX = chartRect.left + chartRect.width / 2;
  const viewCenterY = chartRect.top + chartRect.height / 2;
  const deltaX = targetCenterX - viewCenterX;
  const deltaY = targetCenterY - viewCenterY;
  chartEl.scrollLeft += deltaX;
  chartEl.scrollTop += deltaY;
}}
function bindNodes(nodes) {{
  const byId = new Map(nodes.map((node) => [node.identifier, node]));
  const groups = chartEl.querySelectorAll('g.node');
  const seen = new Set();
  groups.forEach((group) => {{
    const text = group.textContent || '';
    const match = text.match(/[A-Z][A-Z0-9]*-\d+/);
    if (!match) {{
      return;
    }}
    const identifier = match[0];
    if (!byId.has(identifier) || seen.has(group)) {{
      return;
    }}
    seen.add(group);
    group.addEventListener('click', () => openIssue(identifier));
  }});
  const list = ticketList;
  list.innerHTML = '';
  nodes.forEach((node) => {{
    const item = document.createElement('button');
    item.type = 'button';
    item.className = node.class;
    item.textContent = node.identifier + ' ' + node.class;
    item.addEventListener('click', () => {{ centerOn(node.identifier); openIssue(node.identifier); }});
    list.appendChild(item);
  }});
  const count = nodes.length;
  const isOpen = list.classList.contains('open');
  ticketsBtn.textContent = (isOpen ? 'Hide issues' : 'Show issues') + ' (' + count + ')';
  ticketsBtn.setAttribute('aria-expanded', isOpen ? 'true' : 'false');
}}
async function openIssue(identifier) {{
  currentId = identifier;
  const mySeq = ++issueSeq;
  mMsg.textContent = 'Loading';
  backdrop.classList.add('open');
  try {{
    const res = await fetch('/api/issues/' + encodeURIComponent(identifier));
    if (!res.ok) {{
      const payload = await res.json().catch(() => ({{}}));
      throw new Error(payload.error || ('issue request failed: ' + res.status));
    }}
    const data = await res.json();
    if (mySeq !== issueSeq) {{
      return;
    }}
    currentUrl = data.url || '';
    mTitle.textContent = data.identifier + ': ' + data.title;
    currentProject = data.project ? data.project.name : '';
    lastState = data.state;
    mMeta.textContent = 'State ' + lastState + ' Project ' + currentProject;
    mDesc.innerHTML = renderMarkdown(data.description);
    mComments.innerHTML = '';
    (data.comments || []).forEach((comment) => {{
      const item = document.createElement('li');
      const author = document.createElement('div');
      author.className = 'comment-author';
      author.textContent = (comment.author || 'unknown') + ' (' + (comment.created_at || '') + ')';
      const body = document.createElement('div');
      body.className = 'md';
      body.innerHTML = renderMarkdown(comment.body);
      item.appendChild(author);
      item.appendChild(body);
      mComments.appendChild(item);
    }});
    mActivity.innerHTML = '';
    (data.activity || []).forEach((event) => {{
      const item = document.createElement('li');
      item.textContent = event.summary;
      mActivity.appendChild(item);
    }});
    await loadStates(teamOf(identifier), data.state, mySeq);
    if (mySeq !== issueSeq) {{
      return;
    }}
    lastState = mState.value;
    mMsg.textContent = '';
  }} catch (err) {{
    if (mySeq !== issueSeq) {{
      return;
    }}
    mMsg.textContent = String(err && err.message ? err.message : err);
  }}
}}
async function loadStates(team, current, mySeq) {{
  mState.innerHTML = '';
  if (!team) {{
    return;
  }}
  const res = await fetch('/api/states?team=' + encodeURIComponent(team));
  if (!res.ok) {{
    const payload = await res.json().catch(() => ({{}}));
    throw new Error(payload.error || ('states request failed: ' + res.status));
  }}
  if (mySeq !== issueSeq) {{
    return;
  }}
  const data = await res.json();
  (data.nodes || []).forEach((state) => {{
    const option = document.createElement('option');
    option.value = state.name;
    option.textContent = state.name;
    if (state.name === current) {{
      option.selected = true;
    }}
    mState.appendChild(option);
  }});
}}
function closeModal() {{
  issueSeq += 1;
  backdrop.classList.remove('open');
  currentId = '';
}}
mState.addEventListener('change', async () => {{
  if (!currentId) {{
    return;
  }}
  const savedId = currentId;
  const next = mState.value;
  const previous = lastState;
  lastState = next;
  mMeta.textContent = 'State ' + next + ' Project ' + currentProject;
  mMsg.textContent = 'Saving';
  try {{
    const res = await fetch('/api/issues/' + encodeURIComponent(savedId) + '/state', {{
      method: 'POST',
      headers: {{ 'content-type': 'application/json' }},
      body: JSON.stringify({{ state: next }})
    }});
    if (!res.ok) {{
      const payload = await res.json().catch(() => ({{}}));
      throw new Error(payload.error || ('transition failed: ' + res.status));
    }}
  }} catch (err) {{
    await loadChart();
    if (currentId !== savedId || lastState !== next) {{
      return;
    }}
    lastState = previous;
    mState.value = previous;
    mMeta.textContent = 'State ' + previous + ' Project ' + currentProject;
    mMsg.textContent = String(err && err.message ? err.message : err);
    return;
  }}
  await loadChart();
  if (currentId !== savedId) {{
    return;
  }}
  mMsg.textContent = 'Saved';
}});
mRefresh.addEventListener('click', () => {{
  if (currentId) {{
    openIssue(currentId);
  }}
}});
mOpen.addEventListener('click', () => {{
  if (currentUrl) {{
    window.open(currentUrl, '_blank', 'noopener');
  }}
}});
mClose.addEventListener('click', closeModal);
backdrop.addEventListener('click', (event) => {{
  if (event.target === backdrop) {{
    closeModal();
  }}
}});
document.addEventListener('keydown', (event) => {{
  if (event.key === 'Escape' && backdrop.classList.contains('open')) {{
    closeModal();
    return;
  }}
  const tag = (event.target && event.target.tagName ? event.target.tagName : '').toLowerCase();
  if (tag === 'input' || tag === 'textarea' || tag === 'select') {{
    return;
  }}
  if (event.key === 'r' || event.key === 'R') {{
    loadChart();
  }}
}});
refreshBtn.addEventListener('click', loadChart);
ticketsBtn.addEventListener('click', () => {{
  const isOpen = ticketList.classList.toggle('open');
  ticketsBtn.setAttribute('aria-expanded', isOpen ? 'true' : 'false');
  const match = ticketsBtn.textContent.match(/\((\d+)\)/);
  const n = match ? match[1] : '0';
  ticketsBtn.textContent = (isOpen ? 'Hide issues' : 'Show issues') + ' (' + n + ')';
}});
let panX = 0;
let panY = 0;
let panLeft = 0;
let panTop = 0;
let panning = false;
let panMoved = false;
let zoom = 1;
let baseW = 0;
let baseH = 0;
function applyZoom(next, clientX, clientY) {{
  const svg = chartEl.querySelector('svg');
  if (!svg || !baseW || !baseH) {{
    return;
  }}
  const clamped = Math.min(3, Math.max(0.25, next));
  const rect = chartEl.getBoundingClientRect();
  const px = clientX === undefined ? rect.left + rect.width / 2 : clientX;
  const py = clientY === undefined ? rect.top + rect.height / 2 : clientY;
  const cx = px - rect.left + chartEl.scrollLeft;
  const cy = py - rect.top + chartEl.scrollTop;
  const ratio = clamped / zoom;
  zoom = clamped;
  svg.style.width = baseW * zoom + 'px';
  svg.style.height = baseH * zoom + 'px';
  chartEl.scrollLeft = cx * ratio - (px - rect.left);
  chartEl.scrollTop = cy * ratio - (py - rect.top);
}}
chartEl.addEventListener('mousedown', (event) => {{
  if (event.button !== 0) {{
    return;
  }}
  panning = true;
  panMoved = false;
  panX = event.clientX;
  panY = event.clientY;
  panLeft = chartEl.scrollLeft;
  panTop = chartEl.scrollTop;
  chartEl.classList.add('panning');
  event.preventDefault();
}});
document.addEventListener('mousemove', (event) => {{
  if (!panning) {{
    return;
  }}
  if (event.buttons === 0) {{
    panning = false;
    chartEl.classList.remove('panning');
    return;
  }}
  const dx = event.clientX - panX;
  const dy = event.clientY - panY;
  if (!panMoved && Math.abs(dx) + Math.abs(dy) > 4) {{
    panMoved = true;
  }}
  if (panMoved) {{
    chartEl.scrollLeft = panLeft - dx;
    chartEl.scrollTop = panTop - dy;
  }}
}});
document.addEventListener('mouseup', () => {{
  panning = false;
  chartEl.classList.remove('panning');
}});
chartEl.addEventListener('click', (event) => {{
  if (panMoved) {{
    event.stopPropagation();
    event.preventDefault();
    panMoved = false;
  }}
}}, true);
chartEl.addEventListener('wheel', (event) => {{
  if (event.ctrlKey || event.metaKey) {{
    return;
  }}
  event.preventDefault();
  const unit = event.deltaMode === 1 ? 16 : event.deltaMode === 2 ? 800 : 1;
  applyZoom(zoom * Math.exp(-event.deltaY * unit * 0.0015), event.clientX, event.clientY);
}}, {{ passive: false }});
chartEl.addEventListener('dblclick', (event) => {{
  if (event.target && event.target.closest && event.target.closest('g.node')) {{
    return;
  }}
  applyZoom(1);
}});
loadChart();
</script>
</body>
</html>
"#
    )
}

async fn fetch_issue_with<F, Fut>(
    selector: &str,
    max_pages: Option<usize>,
    mut query: F,
) -> Result<(Option<FetchedIssue>, Vec<IncompleteConnection>)>
where
    F: FnMut(&'static str, serde_json::Value) -> Fut,
    Fut: std::future::Future<Output = Result<serde_json::Value>>,
{
    let mut root: Option<RawIssue> = None;
    let mut links: [Vec<String>; 3] = Default::default();
    let mut cursors: [Option<String>; 3] = Default::default();
    let mut seen_cursors: [HashSet<String>; 3] = Default::default();
    let mut active = [true; 3];
    let mut incomplete = Vec::new();
    let mut pages = 0;
    loop {
        let data = query(
            if max_pages.is_some() { GRAPH_ISSUE_QUERY } else { CHART_ISSUE_QUERY },
            serde_json::json!({"id": selector, "after": cursors[0], "blocksAfter": cursors[1], "blockedByAfter": cursors[2], "children": active[0], "blocks": active[1], "blockedBy": active[2]}),
        )
        .await?;
        let Some(issue) = parse_issue(data, selector)? else {
            if root.is_some() {
                return Err(eyre!("Linear issue disappeared while paging: {}", selector));
            }
            return Ok((None, Vec::new()));
        };
        if root
            .as_ref()
            .is_some_and(|root| root.identifier != issue.identifier)
        {
            return Err(eyre!(
                "Linear issue identifier changed while paging: {}",
                selector
            ));
        }
        if max_pages.is_some()
            && (issue
                .relations
                .iter()
                .flat_map(|rels| &rels.nodes)
                .any(|node| {
                    node.rel_type == "blocks"
                        && node.related_issue.is_none()
                        && node.issue.is_none()
                })
                || issue
                    .inverse_relations
                    .iter()
                    .flat_map(|rels| &rels.nodes)
                    .any(|node| node.rel_type == "blocks" && node.issue.is_none()))
        {
            return Err(eyre!(
                "Linear graph response missing blocker endpoint: {}",
                selector
            ));
        }
        pages += 1;
        if active[0] {
            links[0].extend(
                issue
                    .children
                    .iter()
                    .flat_map(|kids| kids.nodes.iter().map(|node| node.identifier.clone())),
            );
        }
        if active[1] {
            links[1].extend(
                issue
                    .relations
                    .iter()
                    .flat_map(|rels| rels.nodes.iter())
                    .filter(|node| node.rel_type == "blocks")
                    .filter_map(|node| {
                        node.related_issue
                            .as_ref()
                            .or(node.issue.as_ref())
                            .map(|issue| issue.identifier.clone())
                    }),
            );
        }
        if active[2] {
            links[2].extend(
                issue
                    .inverse_relations
                    .iter()
                    .flat_map(|rels| rels.nodes.iter())
                    .filter(|node| node.rel_type == "blocks")
                    .filter_map(|node| node.issue.as_ref().map(|issue| issue.identifier.clone())),
            );
        }
        let page_info = [
            issue.children.as_ref().map(|kids| &kids.page_info),
            issue
                .relations
                .as_ref()
                .and_then(|rels| rels.page_info.as_ref()),
            issue
                .inverse_relations
                .as_ref()
                .and_then(|rels| rels.page_info.as_ref()),
        ];
        for index in 0..3 {
            if !active[index] {
                continue;
            }
            if max_pages.is_none() && index > 0 {
                active[index] = false;
                continue;
            }
            let Some(page) = page_info[index] else {
                if max_pages.is_some() {
                    return Err(eyre!(
                        "Linear graph response missing connection pageInfo: {}",
                        selector
                    ));
                }
                active[index] = false;
                continue;
            };
            if !page.has_next {
                active[index] = false;
                continue;
            }
            let valid_cursor = page
                .end_cursor
                .as_ref()
                .filter(|cursor| !cursor.is_empty())
                .is_some_and(|cursor| seen_cursors[index].insert(cursor.clone()));
            let reason = if !valid_cursor {
                Some(ConnectionStop::InvalidCursor)
            } else if max_pages.is_some_and(|cap| pages >= cap) {
                Some(ConnectionStop::PageLimit)
            } else {
                None
            };
            if let Some(reason) = reason {
                if max_pages.is_some() {
                    incomplete.push(IncompleteConnection {
                        identifier: issue.identifier.clone(),
                        connection: [
                            GraphConnection::Children,
                            GraphConnection::Blocks,
                            GraphConnection::BlockedBy,
                        ][index]
                            .clone(),
                        reason,
                        cursor: page.end_cursor.clone(),
                    });
                }
                active[index] = false;
            } else {
                cursors[index] = page.end_cursor.clone();
            }
        }
        if root.is_none() {
            root = Some(issue);
        }
        if active.iter().all(|active| !active) {
            break;
        }
    }
    let issue = root.ok_or_else(|| eyre!("Linear issue not found: {}", selector))?;
    if max_pages.is_some() {
        for links in &mut links {
            links.sort();
            links.dedup();
        }
    }
    let [children, blocks, blocked_by] = links;
    Ok((
        Some(FetchedIssue {
            identifier: issue.identifier,
            title: issue.title,
            url: issue.url,
            state_type: issue.state.state_type,
            parent: issue.parent.map(|parent| parent.identifier),
            blocked_by,
            blocks,
            children,
        }),
        incomplete,
    ))
}

fn parse_issue(data: serde_json::Value, selector: &str) -> Result<Option<RawIssue>> {
    let issue = data
        .get("issue")
        .ok_or_else(|| eyre!("Linear response missing issue field: {}", selector))?;
    serde_json::from_value(issue.clone())
        .map_err(|e| eyre!("Failed to parse Linear issue {}: {}", selector, e))
}

#[derive(Clone, serde::Deserialize)]
pub struct RawIssue {
    identifier: String,
    title: String,
    url: String,
    state: RawState,
    parent: Option<RawParent>,
    #[serde(default)]
    relations: Option<RawRelations>,
    #[serde(rename = "inverseRelations")]
    inverse_relations: Option<RawRelations>,
    children: Option<RawChildren>,
}

#[derive(Clone, serde::Deserialize)]
struct RawState {
    #[serde(rename = "type")]
    state_type: String,
}

#[derive(Clone, serde::Deserialize)]
struct RawParent {
    identifier: String,
}

#[derive(Clone, Default, serde::Deserialize)]
struct RawRelations {
    #[serde(default)]
    nodes: Vec<RawRelationNode>,
    #[serde(default, rename = "pageInfo")]
    page_info: Option<RawPageInfo>,
}

#[derive(Clone, serde::Deserialize)]
struct RawRelationNode {
    #[serde(rename = "type")]
    rel_type: String,
    #[serde(default)]
    issue: Option<RawIdentifier>,
    #[serde(default, rename = "relatedIssue")]
    related_issue: Option<RawIdentifier>,
}

#[derive(Clone, serde::Deserialize)]
struct RawIdentifier {
    identifier: String,
}

#[derive(Clone, serde::Deserialize)]
struct RawChildren {
    #[serde(default)]
    nodes: Vec<RawIdentifier>,
    #[serde(rename = "pageInfo")]
    page_info: RawPageInfo,
}

#[derive(Clone, serde::Deserialize)]
struct RawPageInfo {
    #[serde(rename = "hasNextPage")]
    has_next: bool,
    #[serde(rename = "endCursor")]
    end_cursor: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::linear::config::LinearConfig;
    use std::collections::BTreeMap;

    #[test]
    fn graph_request_validates_and_normalizes_before_io() {
        for (ids, limit, pages) in [
            (vec![], 100, 2),
            (vec![" ".to_string()], 100, 2),
            (vec!["a".repeat(129)], 100, 2),
            (vec!["GUZ-1".to_string()], 0, 2),
            (vec!["GUZ-1".to_string()], 301, 2),
            (vec!["GUZ-1".to_string()], 100, 0),
            (vec!["GUZ-1".to_string()], 100, 11),
            (vec!["GUZ-1".to_string(); 301], 100, 2),
            (vec!["GUZ-1".to_string(), "GUZ-2".to_string()], 1, 2),
        ] {
            assert!(IssueGraphRequest::new(&ids, limit, pages).is_err());
        }
        let request = IssueGraphRequest::new(
            &[
                " GUZ-2 ".to_string(),
                "GUZ-1".to_string(),
                "GUZ-1".to_string(),
            ],
            2,
            1,
        )
        .unwrap();
        assert_eq!(request.seeds, ["GUZ-1", "GUZ-2"]);
    }

    #[tokio::test]
    async fn graph_http_reads_only_the_root_and_honors_both_caps() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/graphql"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"data": {"issue": {
                "identifier": "GUZ-1", "title": "One", "url": "https://example.test/1", "state": {"type": "backlog"}, "parent": null,
                "children": {"nodes": [{"identifier": "GUZ-2"}], "pageInfo": {"hasNextPage": false, "endCursor": null}},
                "relations": {"nodes": [], "pageInfo": {"hasNextPage": false, "endCursor": null}},
                "inverseRelations": {"nodes": [], "pageInfo": {"hasNextPage": true, "endCursor": "more"}}
            }}})))
            .expect(1)
            .mount(&server)
            .await;
        let client = crate::linear::client::build_client(&LinearConfig {
            api_key: "test-key".to_string(),
        })
        .unwrap();
        let url = format!("{}/graphql", server.uri());
        let seeds = vec!["GUZ-1".to_string()];
        let closure = fetch_closure(&seeds, 1, Some(1), |selector| {
            let client = &client;
            let url = &url;
            async move {
                fetch_issue_with(&selector, Some(1), |query, variables| {
                    crate::linear::client::execute_with_url(client, url, query, variables)
                })
                .await
            }
        })
        .await
        .unwrap();
        let graph = mcptools_core::linear::issue_graph_output(
            &seeds,
            closure,
            IssueGraphLimits {
                issues: 1,
                pages_per_issue: 1,
                children_per_page: 50,
                relations_per_page: 25,
            },
        );
        assert_eq!(graph.nodes.len(), 1);
        assert!(graph.frontier.is_empty());
        assert!(graph.cap_hit && graph.truncated);
        assert_eq!(graph.pending, ["GUZ-2"]);
        assert_eq!(
            graph.incomplete_connections[0].connection,
            GraphConnection::BlockedBy
        );
        let requests = server.received_requests().await.unwrap();
        assert_eq!(requests.len(), 1);
        let body: serde_json::Value = serde_json::from_slice(&requests[0].body).unwrap();
        assert_eq!(body["query"], GRAPH_ISSUE_QUERY);
        assert_eq!(body["variables"]["id"], "GUZ-1");
        assert!(!mcptools_core::linear::is_mutation(
            body["query"].as_str().unwrap()
        ));
    }

    #[tokio::test]
    async fn graph_closure_handles_cycles_aliases_missing_and_exact_bounds() {
        let mut first = fetched("GUZ-1", "unstarted", Some("GUZ-2"), &["GUZ-3"]);
        first.children = vec!["GUZ-2".to_string()];
        first.blocks = vec!["GUZ-4".to_string()];
        let mut calls = Vec::new();
        let seeds = vec!["alias".to_string()];
        let full = fetch_closure(&seeds, 4, Some(4), |id| {
            calls.push(id.clone());
            let issue = match id.as_str() {
                "alias" => Some(first.clone()),
                "GUZ-2" => Some(fetched("GUZ-2", "started", Some("GUZ-1"), &[])),
                "GUZ-3" => None,
                "GUZ-4" => Some(fetched("GUZ-4", "unstarted", None, &["GUZ-1"])),
                other => panic!("unexpected fetch: {other}"),
            };
            std::future::ready(Ok((issue, Vec::new())))
        })
        .await
        .unwrap();
        assert_eq!(calls, ["alias", "GUZ-2", "GUZ-3", "GUZ-4"]);
        assert!(full.pending.is_empty());
        assert_eq!(full.missing, ["GUZ-3"]);
        let graph = mcptools_core::linear::issue_graph_output(
            &seeds,
            full,
            IssueGraphLimits {
                issues: 4,
                pages_per_issue: 2,
                children_per_page: 50,
                relations_per_page: 25,
            },
        );
        assert_eq!(graph.roots[0].identifier.as_deref(), Some("GUZ-1"));
        assert_eq!(graph.nodes.len(), 3);
        assert_eq!(graph.inprogress, ["GUZ-2"]);
        assert!(graph.frontier.is_empty());
        assert_eq!(graph.missing_blockers[0].blocker, "GUZ-3");
        assert_eq!(graph.unresolved_blockers.len(), 2);
        assert!(!graph.truncated);

        let capped = fetch_closure(&seeds, 1, Some(1), |_| {
            std::future::ready(Ok((Some(first.clone()), Vec::new())))
        })
        .await
        .unwrap();
        assert_eq!(capped.attempted, 1);
        assert_eq!(capped.pending, ["GUZ-2", "GUZ-3", "GUZ-4"]);
    }

    #[tokio::test]
    async fn graph_pages_connections_independently_and_reports_page_cap() {
        let mut calls = Vec::new();
        let (issue, incomplete) = fetch_issue_with("GUZ-1", Some(2), |query, variables| {
            assert_eq!(query, GRAPH_ISSUE_QUERY);
            let second = !calls.is_empty();
            calls.push(variables);
            std::future::ready(Ok(serde_json::json!({"issue": {
                "identifier": "GUZ-1", "title": "One", "url": "https://example.test/1", "state": {"type": "backlog"}, "parent": null,
                "children": {"nodes": [{"identifier": if second { "GUZ-3" } else { "GUZ-2" }}], "pageInfo": {"hasNextPage": !second, "endCursor": "kids1"}},
                "relations": {"nodes": [{"type": "blocks", "relatedIssue": {"identifier": if second { "GUZ-5" } else { "GUZ-4" }}}, {"type": "related", "relatedIssue": {"identifier": "GUZ-99"}}], "pageInfo": {"hasNextPage": true, "endCursor": if second { "blocks2" } else { "blocks1" }}},
                "inverseRelations": {"nodes": [{"type": "blocks", "issue": {"identifier": "GUZ-6"}}], "pageInfo": {"hasNextPage": false, "endCursor": null}}
            }})))
        }).await.unwrap();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[1]["after"], "kids1");
        assert_eq!(calls[1]["blocksAfter"], "blocks1");
        assert_eq!(calls[1]["blockedBy"], false);
        let issue = issue.unwrap();
        assert_eq!(issue.children, ["GUZ-2", "GUZ-3"]);
        assert_eq!(issue.blocks, ["GUZ-4", "GUZ-5"]);
        assert_eq!(issue.blocked_by, ["GUZ-6"]);
        assert_eq!(incomplete.len(), 1);
        assert_eq!(incomplete[0].connection, GraphConnection::Blocks);
        assert!(matches!(incomplete[0].reason, ConnectionStop::PageLimit));
        assert_eq!(incomplete[0].cursor.as_deref(), Some("blocks2"));
    }

    #[tokio::test]
    async fn graph_stalled_cursor_is_explicit_and_cli_query_is_preserved() {
        for cursor in [serde_json::Value::Null, serde_json::json!("stalled")] {
            let mut calls = 0;
            let (_, incomplete) = fetch_issue_with("GUZ-1", Some(10), |_, _| {
                calls += 1;
                std::future::ready(Ok(serde_json::json!({"issue": {
                    "identifier": "GUZ-1", "title": "One", "url": "https://example.test/1", "state": {"type": "backlog"}, "parent": null,
                    "children": {"nodes": [], "pageInfo": {"hasNextPage": true, "endCursor": cursor}},
                    "relations": {"nodes": [], "pageInfo": {"hasNextPage": false, "endCursor": null}},
                    "inverseRelations": {"nodes": [], "pageInfo": {"hasNextPage": false, "endCursor": null}}
                }})))
            }).await.unwrap();
            assert!(calls <= 2);
            assert!(matches!(
                incomplete[0].reason,
                ConnectionStop::InvalidCursor
            ));
        }
        let mut pages = 0;
        let (issue, incomplete) = fetch_issue_with("GUZ-1", None, |query, variables| {
            assert_eq!(query, CHART_ISSUE_QUERY);
            let second = pages > 0;
            pages += 1;
            assert_eq!(variables["after"], if second { serde_json::json!("kids1") } else { serde_json::Value::Null });
            std::future::ready(Ok(serde_json::json!({"issue": {
                "identifier": "GUZ-1", "title": "One", "url": "https://example.test/1", "state": {"type": "backlog"}, "parent": null,
                "children": {"nodes": [{"identifier": if second { "GUZ-3" } else { "GUZ-2" }}], "pageInfo": {"hasNextPage": !second, "endCursor": "kids1"}},
                "relations": {"nodes": [{"type": "blocks", "relatedIssue": {"identifier": if second { "GUZ-5" } else { "GUZ-4" }}}]},
                "inverseRelations": {"nodes": [{"type": "blocks", "issue": {"identifier": "GUZ-6"}}]}
            }})))
        })
        .await
        .unwrap();
        let issue = issue.unwrap();
        assert_eq!(pages, 2);
        assert_eq!(issue.children, ["GUZ-2", "GUZ-3"]);
        assert_eq!(issue.blocks, ["GUZ-4"]);
        assert_eq!(issue.blocked_by, ["GUZ-6"]);
        assert!(incomplete.is_empty());
        assert!(parse_issue(serde_json::json!({}), "GUZ-1").is_err());
        assert!(parse_issue(serde_json::json!({"issue": null}), "GUZ-1")
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn rejects_empty_issue_ids_before_io() {
        let cfg = LinearConfig {
            api_key: "test-key".to_string(),
        };
        let client = crate::linear::client::build_client(&cfg).unwrap();
        for seed in ["", "   "] {
            let err = chart_data(&client, &[seed.to_string()], false, ISSUE_CAP)
                .await
                .unwrap_err();
            assert!(err.to_string().contains("must not be empty"));
        }
        let err = chart_data(&client, &[], false, ISSUE_CAP)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("at least one issue id"));
    }

    #[tokio::test]
    async fn rejects_zero_limit_before_io() {
        let cfg = LinearConfig {
            api_key: "test-key".to_string(),
        };
        let client = crate::linear::client::build_client(&cfg).unwrap();
        let err = chart_data(&client, &["GUZ-1".to_string()], false, 0)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("at least 1"));
    }

    fn fetched(id: &str, state: &str, parent: Option<&str>, blocked_by: &[&str]) -> FetchedIssue {
        FetchedIssue {
            identifier: id.to_string(),
            title: format!("Title {id}"),
            url: format!("https://example.test/{id}"),
            state_type: state.to_string(),
            parent: parent.map(str::to_string),
            blocked_by: blocked_by.iter().map(|item| item.to_string()).collect(),
            blocks: Vec::new(),
            children: Vec::new(),
        }
    }

    #[test]
    fn exclude_completed_drops_nodes_and_unblocks_dependents() {
        let mut map = BTreeMap::new();
        for item in [
            fetched("GUZ-1", "completed", None, &[]),
            fetched("GUZ-2", "unstarted", None, &["GUZ-1"]),
            fetched("GUZ-3", "unstarted", Some("GUZ-1"), &[]),
        ] {
            map.insert(item.identifier.clone(), item);
        }
        let (nodes, missing) = closure_nodes(&map, true);
        assert_eq!(nodes.len(), 2);
        assert!(nodes.iter().all(|node| node.state_type != "completed"));
        let guz2 = nodes
            .iter()
            .find(|node| node.identifier == "GUZ-2")
            .unwrap();
        assert!(guz2.blocked_by.is_empty());
        let guz3 = nodes
            .iter()
            .find(|node| node.identifier == "GUZ-3")
            .unwrap();
        assert!(guz3.parent.is_none());
        assert!(missing.is_empty());
        let classes = mcptools_core::linear::classify_nodes(&nodes);
        assert_eq!(
            classes["GUZ-2"],
            mcptools_core::linear::ChartClass::Frontier
        );
    }

    #[test]
    fn keep_completed_without_flag_and_report_outside_blockers() {
        let mut map = BTreeMap::new();
        for item in [
            fetched("GUZ-2", "unstarted", None, &["GUZ-1"]),
            fetched("GUZ-4", "unstarted", None, &["GUZ-99"]),
        ] {
            map.insert(item.identifier.clone(), item);
        }
        let (nodes, missing) = closure_nodes(&map, false);
        assert_eq!(nodes.len(), 2);
        assert_eq!(
            missing,
            vec![
                ("GUZ-2".to_string(), "GUZ-1".to_string()),
                ("GUZ-4".to_string(), "GUZ-99".to_string()),
            ]
        );
    }

    #[test]
    fn chart_args_require_an_input_source() {
        let err = validate_chart_args(&[], None, None).unwrap_err();
        assert!(err.to_string().contains("issue id"));
        assert!(err.to_string().contains("--project"));
        let err = validate_chart_args(&[], None, Some("GUZ")).unwrap_err();
        assert!(err.to_string().contains("--team"));
        let err = validate_chart_args(&["   ".to_string()], None, None).unwrap_err();
        assert!(err.to_string().contains("must not be empty"));
        let (positional, project, team) =
            validate_chart_args(&[], Some("  proj  "), Some(" GUZ ")).unwrap();
        assert!(positional.is_empty());
        assert_eq!(project.as_deref(), Some("proj"));
        assert_eq!(team.as_deref(), Some("GUZ"));
        let (positional, project, team) =
            validate_chart_args(&["GUZ-1".to_string()], None, None).unwrap();
        assert_eq!(positional, vec!["GUZ-1"]);
        assert!(project.is_none());
        assert!(team.is_none());
    }

    #[test]
    fn slugify_slugs_input_for_default_out_path() {
        assert_eq!(slugify("modelops-cycles"), "modelops-cycles");
        assert_eq!(slugify("GUZ-185"), "GUZ-185");
        assert_eq!(slugify("My Project Name"), "My-Project-Name");
        assert_eq!(slugify("a  /  b"), "a-b");
        assert_eq!(slugify("foo bar--baz"), "foo-bar-baz");
        assert_eq!(slugify("-lead- and trail-"), "lead-and-trail");
        assert_eq!(slugify("   "), "chart");
        assert_eq!(slugify(""), "chart");
        assert_eq!(slugify("///"), "chart");
        assert_eq!(
            default_out_path(&slugify("My Project"))
                .file_name()
                .unwrap(),
            "mcptools-linear-chart-My-Project.html"
        );
    }

    #[test]
    fn serve_rejects_out_path() {
        let err = validate_serve_out(true, Some(std::path::Path::new("/tmp/x.html"))).unwrap_err();
        assert!(err.to_string().contains("--out"));
        validate_serve_out(true, None).unwrap();
        validate_serve_out(false, Some(std::path::Path::new("/tmp/x.html"))).unwrap();
    }

    #[test]
    fn class_names_cover_every_variant() {
        assert_eq!(chart_class_name(&ChartClass::Complete), "complete");
        assert_eq!(chart_class_name(&ChartClass::InProgress), "inprogress");
        assert_eq!(chart_class_name(&ChartClass::Frontier), "frontier");
        assert_eq!(chart_class_name(&ChartClass::Fog), "fog");
    }

    #[test]
    fn app_html_exposes_interactive_contract() {
        let html = render_app_html("GUZ-185");
        for marker in [
            "id=\"refresh\"",
            "/api/chart",
            "keydown",
            "/api/issues/",
            "/api/states",
            "id=\"modal-backdrop\"",
            "id=\"m-state\"",
            "id=\"m-refresh\"",
            "Refresh issue",
            "marked@12",
            "dompurify@3",
            "renderMarkdown",
            "comment-author",
            "panning",
            "cluster-label",
            "min-height: 0",
            "applyZoom",
            "dblclick",
            "payload.error || ('chart request failed",
            "payload.error || ('issue request failed",
            "payload.error || ('states request failed",
            "id=\"tickets\"",
            "id=\"ticket-list\"",
            "max-height: 30vh",
            "aria-expanded=\"false\"",
            "aria-controls=\"ticket-list\"",
            "#ticket-list button.complete",
            "#ticket-list button.inprogress",
            "#ticket-list button.frontier",
            "#ticket-list button.fog",
            "function centerOn",
        ] {
            assert!(html.contains(marker), "missing {marker}");
        }
        assert!(html.contains("GUZ-185"));
    }

    #[test]
    fn chart_payload_serializes_nodes_with_class() {
        let payload = ChartPayload {
            title: "GUZ-1".to_string(),
            stats_line: "1 issues: 0 complete, 0 in progress, 1 frontier, 0 fog".to_string(),
            total: 1,
            complete: 0,
            inprogress: 0,
            frontier: 1,
            fog: 0,
            mermaid: "flowchart TB\n".to_string(),
            cap_hit: false,
            missing_blockers: Vec::new(),
            nodes: vec![ChartNodePayload {
                identifier: "GUZ-1".to_string(),
                title: "Title GUZ-1".to_string(),
                state_type: "backlog".to_string(),
                parent: None,
                blocked_by: Vec::new(),
                class: "frontier".to_string(),
            }],
        };
        let value = serde_json::to_value(&payload).unwrap();
        assert_eq!(value["nodes"][0]["class"], "frontier");
        assert_eq!(value["total"], 1);
    }
}
