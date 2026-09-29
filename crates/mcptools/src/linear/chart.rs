use crate::linear::client::execute;
use crate::prelude::*;
use mcptools_core::linear::ChartNode;
use std::collections::{BTreeMap, HashSet, VecDeque};

pub const ISSUE_CAP: usize = 300;

const CHART_ISSUE_QUERY: &str = "query ($id: String!, $after: String) { issue(id: $id) { id identifier title url state { name type } parent { identifier } relations(first: 25) { nodes { type relatedIssue { identifier } } } inverseRelations(first: 25) { nodes { type issue { identifier } } } children(first: 50, after: $after) { nodes { identifier } pageInfo { hasNextPage endCursor } } } }";

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
    let mut fetched: BTreeMap<String, FetchedIssue> = BTreeMap::new();
    let mut queue: VecDeque<String> = seeds.iter().cloned().collect();
    let mut seen: HashSet<String> = seeds.iter().cloned().collect();
    let mut cap_hit = false;
    while let Some(selector) = queue.pop_front() {
        if fetched.contains_key(&selector) {
            continue;
        }
        if fetched.len() >= limit {
            cap_hit = true;
            break;
        }
        let issue = fetch_issue(client, &selector).await?;
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
        fetched.insert(issue.identifier.clone(), issue);
    }
    let (nodes, missing_blockers) = closure_nodes(&fetched, exclude_completed);
    Ok(ChartClosure {
        nodes,
        cap_hit,
        missing_blockers,
    })
}

pub fn closure_nodes(
    fetched: &BTreeMap<String, FetchedIssue>,
    exclude_completed: bool,
) -> (Vec<ChartNode>, Vec<(String, String)>) {
    let mut excluded: HashSet<&str> = HashSet::new();
    if exclude_completed {
        for issue in fetched.values() {
            if issue.state_type == "completed" {
                excluded.insert(issue.identifier.as_str());
            }
        }
    }
    let mut missing_blockers = Vec::new();
    for issue in fetched.values() {
        if excluded.contains(issue.identifier.as_str()) {
            continue;
        }
        for blocker in &issue.blocked_by {
            if excluded.contains(blocker.as_str()) {
                continue;
            }
            match fetched.get(blocker) {
                None => missing_blockers.push((issue.identifier.clone(), blocker.clone())),
                Some(other) if excluded.contains(other.identifier.as_str()) => continue,
                _ => {}
            }
        }
    }
    let nodes = fetched
        .values()
        .filter(|issue| !excluded.contains(issue.identifier.as_str()))
        .map(|issue| ChartNode {
            identifier: issue.identifier.clone(),
            title: issue.title.clone(),
            state_type: issue.state_type.clone(),
            parent: issue
                .parent
                .clone()
                .filter(|p| !excluded.contains(p.as_str())),
            blocked_by: issue
                .blocked_by
                .iter()
                .filter(|blocker| !excluded.contains(blocker.as_str()))
                .cloned()
                .collect(),
        })
        .collect();
    (nodes, missing_blockers)
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

pub struct FetchedIssue {
    pub identifier: String,
    pub title: String,
    pub url: String,
    pub state_type: String,
    pub parent: Option<String>,
    pub blocked_by: Vec<String>,
    pub blocks: Vec<String>,
    pub children: Vec<String>,
}

async fn fetch_issue(client: &reqwest::Client, selector: &str) -> Result<FetchedIssue> {
    let mut root: Option<RawIssue> = None;
    let mut children: Vec<String> = Vec::new();
    let mut after: Option<String> = None;
    let mut last_cursor: Option<String> = None;
    loop {
        let data = execute(
            client,
            CHART_ISSUE_QUERY,
            serde_json::json!({"id": selector, "after": after}),
        )
        .await?;
        let issue = parse_issue(data, selector)?;
        let page = issue.children.as_ref().map(|kids| kids.page_info.clone());
        children.extend(
            issue
                .children
                .iter()
                .flat_map(|kids| kids.nodes.iter().map(|node| node.identifier.clone())),
        );
        if root.is_none() {
            root = Some(issue);
        }
        let (has_next, cursor) = match page {
            Some(info) => (info.has_next, info.end_cursor),
            None => (false, None),
        };
        if !has_next {
            break;
        }
        let cursor = match cursor {
            Some(cursor) if last_cursor.as_deref() != Some(cursor.as_str()) => cursor,
            _ => break,
        };
        last_cursor = Some(cursor.clone());
        after = Some(cursor);
    }
    let issue = root.ok_or_else(|| eyre!("Linear issue not found: {}", selector))?;
    Ok(FetchedIssue {
        identifier: issue.identifier,
        title: issue.title,
        url: issue.url,
        state_type: issue.state.state_type,
        parent: issue.parent.map(|parent| parent.identifier),
        blocked_by: issue
            .inverse_relations
            .unwrap_or_default()
            .nodes
            .into_iter()
            .filter(|node| node.rel_type == "blocks")
            .filter_map(|node| node.issue.map(|issue| issue.identifier))
            .collect(),
        blocks: issue
            .relations
            .unwrap_or_default()
            .nodes
            .into_iter()
            .filter(|node| node.rel_type == "blocks")
            .filter_map(|node| {
                node.related_issue
                    .map(|issue| issue.identifier)
                    .or(node.issue.map(|issue| issue.identifier))
            })
            .collect(),
        children,
    })
}

fn parse_issue(data: serde_json::Value, selector: &str) -> Result<RawIssue> {
    #[derive(serde::Deserialize)]
    struct Root {
        issue: Option<RawIssue>,
    }
    let root: Root = serde_json::from_value(data)
        .map_err(|e| eyre!("Failed to parse Linear issue {}: {}", selector, e))?;
    root.issue
        .ok_or_else(|| eyre!("Linear issue not found: {}", selector))
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
}
