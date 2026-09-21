#[derive(Debug, Clone, serde::Serialize, schemars::JsonSchema)]
pub struct FoundTool {
    pub name: String,
    pub domain: String,
    pub score: f64,
    #[serde(rename = "inputSchema")]
    pub input_schema: serde_json::Value,
}

#[derive(Debug, Clone, serde::Serialize, schemars::JsonSchema)]
pub struct FoundTools {
    pub none: f64,
    pub tools: Vec<FoundTool>,
}

pub fn find_tools(
    task: &str,
    k: usize,
) -> Result<FoundTools, mcptools_core::find_tools::FindToolsError> {
    let catalog = super::tool_catalog();
    let ranking = mcptools_core::find_tools::rank_tools(task, &catalog, k)?;
    let registered = super::registered_tools();
    let tools: Vec<FoundTool> = ranking
        .tools
        .into_iter()
        .filter_map(|r| {
            registered
                .iter()
                .find(|t| t.name == r.name)
                .map(|t| FoundTool {
                    name: r.name,
                    domain: r.domain,
                    score: r.score,
                    input_schema: t.input_schema.clone(),
                })
        })
        .collect();
    Ok(FoundTools {
        none: ranking.none,
        tools,
    })
}

#[cfg(test)]
mod find_tools_tests {
    use super::*;

    fn parse_golden() -> Vec<(String, Option<Vec<String>>, String)> {
        let content = include_str!("fixtures/golden-queries.tsv");
        let mut rows = vec![];
        for (i, line) in content.lines().enumerate() {
            if i == 0 || line.trim().is_empty() {
                continue;
            }
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() != 3 {
                continue;
            }
            let task = parts[0].to_string();
            let expected = if parts[1] == "NONE" {
                None
            } else {
                Some(parts[1].split(',').map(|s| s.trim().to_string()).collect())
            };
            let kind = parts[2].to_string();
            rows.push((task, expected, kind));
        }
        rows
    }

    #[test]
    fn find_tools_attaches_real_input_schema() {
        let result = find_tools("close GUZ-22", 5).unwrap();
        let registered = super::super::registered_tools();
        for found in &result.tools {
            let reg = registered.iter().find(|t| t.name == found.name).unwrap();
            assert_eq!(found.input_schema, reg.input_schema);
        }
    }

    #[test]
    fn golden_recall_at_5_meets_threshold() {
        let rows = parse_golden();
        let in_scope: Vec<_> = rows.iter().filter(|(_, _, k)| k == "in_scope").collect();
        let mut hits = 0;
        for (task, expected, _) in &in_scope {
            if let Some(exps) = expected {
                let res = find_tools(task, 5).unwrap();
                let names: Vec<_> = res.tools.iter().map(|t| t.name.as_str()).collect();
                if exps.iter().any(|e| names.contains(&e.as_str())) {
                    hits += 1;
                }
            }
        }
        let recall = hits as f64 / in_scope.len() as f64;
        assert!(recall >= 0.85, "recall@5 = {}", recall);
    }

    #[test]
    fn close_guz_22_ranks_linear_issue_update_in_top_5() {
        let res = find_tools("close GUZ-22", 5).unwrap();
        assert!(res.tools.iter().any(|t| t.name == "linear_issue_update"));
    }

    #[test]
    fn unrelated_rows_score_higher_none_than_close_guz_22() {
        let rows = parse_golden();
        let unrelated: Vec<_> = rows.iter().filter(|(_, _, k)| k == "unrelated").collect();
        let close_none = find_tools("close GUZ-22", 5).unwrap().none;
        for (task, _, _) in &unrelated {
            let n = find_tools(task, 5).unwrap().none;
            assert!(
                n > close_none,
                "task={} none={} close_none={}",
                task,
                n,
                close_none
            );
        }
    }
}
