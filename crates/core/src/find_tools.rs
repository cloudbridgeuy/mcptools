use regex::Regex;
use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

use crate::catalog::CatalogEntry;

static ISSUE_KEY_RE: OnceLock<Regex> = OnceLock::new();
static TOKEN_RE: OnceLock<Regex> = OnceLock::new();

fn get_issue_key_re() -> &'static Regex {
    ISSUE_KEY_RE.get_or_init(|| Regex::new(r"\b[A-Za-z][A-Za-z0-9]*-\d+\b").expect("valid regex"))
}

fn get_token_re() -> &'static Regex {
    TOKEN_RE.get_or_init(|| Regex::new(r"[a-z0-9]+").expect("valid regex"))
}

fn stem(token: &str) -> String {
    if token.ends_with("ing") && token.len() > 5 {
        token[..token.len() - 3].to_string()
    } else if (token.ends_with("ed") || token.ends_with("es")) && token.len() > 4 {
        token[..token.len() - 2].to_string()
    } else if token.ends_with("s") && token.len() > 3 && !token.ends_with("ss") {
        token[..token.len() - 1].to_string()
    } else {
        token.to_string()
    }
}

fn synonym(stem: &str) -> &str {
    match stem {
        "mark" => "update",
        "done" | "resolv" | "resolve" | "finish" => "close",
        "ticket" => "issue",
        _ => stem,
    }
}

fn tokenize(text: &str) -> Vec<String> {
    let lower = text.to_lowercase().replace('_', " ");
    get_token_re()
        .find_iter(&lower)
        .map(|m| synonym(&stem(m.as_str())).to_string())
        .collect()
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct RankedTool {
    pub name: String,
    pub domain: String,
    pub score: f64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Ranking {
    pub none: f64,
    pub tools: Vec<RankedTool>,
}

#[derive(Debug, thiserror::Error)]
pub enum FindToolsError {
    #[error("task is empty")]
    EmptyTask,
    #[error("k must be greater than zero")]
    ZeroK,
}

pub const DEFAULT_K: usize = 5;

pub const USAGE: &str = "Each declaration is the call signature: the input interface is the tools/call arguments object, the Promise type is the result.";

pub const CODE_MODE_USAGE: &str =
    "Call a single tool with tools.mcptools.call_tool, e.g. await tools.mcptools.call_tool({ name: \"linear_issue_list\", input: { team: \"GUZ\" } }). To chain many calls in one round trip, pass code as a string to tools.mcptools.execute, where each declared function is an async global (one fresh sandbox per call). The outer Code Mode scope has only find_tools, execute and call_tool; a declared name called there is a ReferenceError, not a TTL or expiry.";

pub fn rank_tools(
    task: &str,
    catalog: &[CatalogEntry],
    k: usize,
) -> Result<Ranking, FindToolsError> {
    let trimmed = task.trim();
    if trimmed.is_empty() {
        return Err(FindToolsError::EmptyTask);
    }
    if k == 0 {
        return Err(FindToolsError::ZeroK);
    }
    let rewritten = get_issue_key_re()
        .replace_all(trimmed, "issue ticket")
        .to_string();
    let task_tokens = tokenize(&rewritten);
    let unique_task: HashSet<String> = task_tokens.into_iter().collect();
    if catalog.is_empty() {
        return Ok(Ranking {
            none: 1.0,
            tools: vec![],
        });
    }
    let n = catalog.len();
    let entry_data: Vec<_> = catalog
        .iter()
        .map(|e| {
            let nt: HashSet<String> = tokenize(&e.name).into_iter().collect();
            let dt: HashSet<String> = tokenize(&e.domain).into_iter().collect();
            let st: HashSet<String> = tokenize(&e.summary).into_iter().collect();
            (e, nt, dt, st)
        })
        .collect();
    let mut df: HashMap<String, usize> = HashMap::new();
    for (_, nt, dt, st) in &entry_data {
        let mut all: HashSet<String> = HashSet::new();
        all.extend(nt.iter().cloned());
        all.extend(dt.iter().cloned());
        all.extend(st.iter().cloned());
        for t in all {
            *df.entry(t).or_insert(0) += 1;
        }
    }
    let idf = |t: &str| -> f64 {
        if let Some(&d) = df.get(t) {
            (n as f64 / d as f64).ln()
        } else {
            (n as f64 / 1.0).ln()
        }
    };
    let sum_idf: f64 = unique_task.iter().map(|t| idf(t.as_str())).sum();
    let mut scored: Vec<(String, String, f64)> = entry_data
        .into_iter()
        .map(|(e, nt, dt, st)| {
            let score = if sum_idf <= 0.0 {
                0.0
            } else {
                let number: f64 = unique_task
                    .iter()
                    .map(|t| {
                        let w = if nt.contains(t) {
                            2.0
                        } else if dt.contains(t) {
                            1.5
                        } else if st.contains(t) {
                            1.0
                        } else {
                            0.0
                        };
                        idf(t.as_str()) * w
                    })
                    .sum();
                number / (2.0 * sum_idf)
            };
            (e.name.clone(), e.domain.clone(), score)
        })
        .collect();
    scored.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));
    let take = k.min(catalog.len());
    let top: Vec<RankedTool> = scored
        .into_iter()
        .take(take)
        .map(|(name, domain, score)| RankedTool {
            name,
            domain,
            score,
        })
        .collect();
    let top1 = top.first().map(|t| t.score).unwrap_or(1.0);
    let none = 1.0 - top1;
    Ok(Ranking { none, tools: top })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::build_catalog;

    #[test]
    fn tokenize_maps_synonyms() {
        assert_eq!(
            tokenize("mark the ticket done"),
            vec!["update", "the", "issue", "close"]
        );
        assert_eq!(tokenize("resolved"), vec!["close"]);
        assert_eq!(tokenize("resolve"), vec!["close"]);
    }

    #[test]
    fn rank_tools_orders_by_score_descending() {
        let catalog = build_catalog([
            ("other", "nothing relevant"),
            (
                "linear_issue_update",
                "Update a Linear issue by id or identifier",
            ),
            ("jira_search", "Search Jira issues"),
        ]);
        let r = rank_tools("close GUZ-22", &catalog, 5).unwrap();
        assert_eq!(r.tools[0].name, "linear_issue_update");
        assert!(r.tools[0].score > r.tools[1].score);
    }

    #[test]
    fn rank_tools_rejects_empty_task() {
        let catalog = build_catalog([("x", "y")]);
        let e = rank_tools("   ", &catalog, 5);
        assert!(matches!(e, Err(FindToolsError::EmptyTask)));
    }

    #[test]
    fn rank_tools_rejects_zero_k() {
        let catalog = build_catalog([("x", "y")]);
        let e = rank_tools("close it", &catalog, 0);
        assert!(matches!(e, Err(FindToolsError::ZeroK)));
    }

    #[test]
    fn rank_tools_rewrites_issue_keys() {
        let catalog = build_catalog([("linear_issue_update", "Update Linear issue")]);
        let r1 = rank_tools("close GUZ-22", &catalog, 5).unwrap();
        let r2 = rank_tools("close issue ticket", &catalog, 5).unwrap();
        assert_eq!(r1.tools.len(), r2.tools.len());
        assert_eq!(r1.tools[0].score, r2.tools[0].score);
        assert_eq!(r1.none, r2.none);
    }

    #[test]
    fn rank_tools_scores_stay_within_unit_interval() {
        let pairs = [
            (
                "close GUZ-22",
                [("linear_issue_update", "update linear"), ("other", "foo")],
            ),
            (
                "list stories",
                [("hn_list_items", "list hacker news"), ("jira", "search")],
            ),
            ("!!!", [("a", "b"), ("c", "d")]),
        ];
        for (task, entries) in pairs {
            let cat = build_catalog(entries);
            let r = rank_tools(task, &cat, 5).unwrap();
            for t in &r.tools {
                assert!(t.score >= 0.0 && t.score <= 1.0);
            }
            assert!(r.none >= 0.0 && r.none <= 1.0);
        }
    }
}
