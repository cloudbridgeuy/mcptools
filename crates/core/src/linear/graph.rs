use super::{chart_stats, classify_nodes, ChartClass, ChartNode, ChartStats};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};

#[derive(Debug, Clone)]
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

#[derive(Debug, Default)]
pub struct IssueClosure {
    pub fetched: BTreeMap<String, FetchedIssue>,
    pub resolved: BTreeMap<String, String>,
    pub pending: Vec<String>,
    pub missing: Vec<String>,
    pub attempted: usize,
    pub incomplete_connections: Vec<IncompleteConnection>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum GraphConnection {
    Children,
    Blocks,
    BlockedBy,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionStop {
    PageLimit,
    InvalidCursor,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct IncompleteConnection {
    pub identifier: String,
    pub connection: GraphConnection,
    pub reason: ConnectionStop,
    pub cursor: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct GraphRoot {
    pub selector: String,
    pub identifier: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct GraphBlocker {
    pub issue: String,
    pub blocker: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct IssueGraphNode {
    pub identifier: String,
    pub title: String,
    pub url: String,
    pub state_type: String,
    pub parent: Option<String>,
    pub children: Vec<String>,
    pub blocked_by: Vec<String>,
    pub blocks: Vec<String>,
    pub class: Option<ChartClass>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct IssueGraphLimits {
    pub issues: usize,
    pub pages_per_issue: usize,
    pub children_per_page: usize,
    pub relations_per_page: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct IssueGraphOutput {
    pub roots: Vec<GraphRoot>,
    pub nodes: Vec<IssueGraphNode>,
    pub frontier: Vec<String>,
    pub inprogress: Vec<String>,
    pub stats: ChartStats,
    pub missing_blockers: Vec<GraphBlocker>,
    pub unresolved_blockers: Vec<GraphBlocker>,
    pub limits: IssueGraphLimits,
    pub fetched_count: usize,
    pub attempted_count: usize,
    pub cap_hit: bool,
    pub truncated: bool,
    pub pending: Vec<String>,
    pub missing_issues: Vec<String>,
    pub incomplete_connections: Vec<IncompleteConnection>,
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

pub fn issue_graph_output(
    seeds: &[String],
    closure: IssueClosure,
    limits: IssueGraphLimits,
) -> IssueGraphOutput {
    let (chart_nodes, missing) = closure_nodes(&closure.fetched, false);
    let classes = classify_nodes(&chart_nodes);
    let roots = seeds
        .iter()
        .map(|selector| GraphRoot {
            selector: selector.clone(),
            identifier: closure.resolved.get(selector).cloned().or_else(|| {
                closure
                    .fetched
                    .contains_key(selector)
                    .then(|| selector.clone())
            }),
        })
        .collect();
    let nodes = closure
        .fetched
        .values()
        .map(|issue| IssueGraphNode {
            identifier: issue.identifier.clone(),
            title: issue.title.clone(),
            url: issue.url.clone(),
            state_type: issue.state_type.clone(),
            parent: issue.parent.clone(),
            children: issue.children.clone(),
            blocked_by: issue.blocked_by.clone(),
            blocks: issue.blocks.clone(),
            class: classes.get(&issue.identifier).copied(),
        })
        .collect();
    let unresolved_blockers = closure
        .fetched
        .values()
        .flat_map(|issue| {
            issue
                .blocked_by
                .iter()
                .filter(|blocker| {
                    closure.fetched.get(*blocker).is_none_or(|other| {
                        !matches!(other.state_type.as_str(), "completed" | "canceled")
                    })
                })
                .map(|blocker| GraphBlocker {
                    issue: issue.identifier.clone(),
                    blocker: blocker.clone(),
                })
        })
        .collect();
    IssueGraphOutput {
        roots,
        nodes,
        frontier: classes
            .iter()
            .filter(|(id, class)| {
                **class == ChartClass::Frontier
                    && !closure.incomplete_connections.iter().any(|connection| {
                        connection.identifier == **id
                            && connection.connection == GraphConnection::BlockedBy
                    })
            })
            .map(|(id, _)| id.clone())
            .collect(),
        inprogress: classes
            .iter()
            .filter(|(_, class)| **class == ChartClass::InProgress)
            .map(|(id, _)| id.clone())
            .collect(),
        stats: chart_stats(&chart_nodes),
        missing_blockers: missing
            .into_iter()
            .map(|(issue, blocker)| GraphBlocker { issue, blocker })
            .collect(),
        unresolved_blockers,
        limits,
        fetched_count: closure.fetched.len(),
        attempted_count: closure.attempted,
        cap_hit: !closure.pending.is_empty(),
        truncated: !closure.pending.is_empty() || !closure.incomplete_connections.is_empty(),
        pending: closure.pending,
        missing_issues: closure.missing,
        incomplete_connections: closure.incomplete_connections,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canceled_and_completed_blockers_satisfy_but_incomplete_paging_is_not_actionable() {
        let mut closure = IssueClosure::default();
        for (id, state, blockers) in [
            ("A-1", "backlog", vec!["A-2".to_string(), "A-3".to_string()]),
            ("A-2", "completed", vec![]),
            ("A-3", "canceled", vec![]),
            ("A-4", "unstarted", vec![]),
        ] {
            closure.fetched.insert(
                id.to_string(),
                FetchedIssue {
                    identifier: id.to_string(),
                    title: id.to_string(),
                    url: String::new(),
                    state_type: state.to_string(),
                    parent: None,
                    children: Vec::new(),
                    blocks: Vec::new(),
                    blocked_by: blockers,
                },
            );
        }
        closure.incomplete_connections.push(IncompleteConnection {
            identifier: "A-4".to_string(),
            connection: GraphConnection::BlockedBy,
            reason: ConnectionStop::PageLimit,
            cursor: Some("next".to_string()),
        });
        let graph = issue_graph_output(
            &["A-1".to_string()],
            closure,
            IssueGraphLimits {
                issues: 100,
                pages_per_issue: 2,
                children_per_page: 50,
                relations_per_page: 25,
            },
        );
        assert_eq!(graph.frontier, ["A-1"]);
        assert_eq!(graph.nodes[3].class, Some(ChartClass::Frontier));
        assert_eq!(graph.nodes[2].class, None);
        assert_eq!(graph.stats.total, 3);
        assert_eq!(graph.stats.frontier, 2);
        assert!(graph.unresolved_blockers.is_empty());
        assert!(graph.truncated);
        assert!(!graph.cap_hit);
    }
}
