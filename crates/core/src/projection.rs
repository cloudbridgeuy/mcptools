#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectionError {
    pub path: String,
}

pub fn project(
    value: serde_json::Value,
    paths: &[String],
) -> Result<serde_json::Value, ProjectionError> {
    if paths.is_empty() {
        return Ok(value);
    }
    let mut out = serde_json::Value::Null;
    for path in paths {
        let segments: Vec<&str> = path.split('.').collect();
        let mut leaves = 0usize;
        insert(&mut out, &value, &segments, &mut leaves);
        if leaves == 0 {
            return Err(ProjectionError { path: path.clone() });
        }
    }
    Ok(out)
}

fn resolves(value: &serde_json::Value, segments: &[&str]) -> bool {
    match (value, segments.split_first()) {
        (_, None) => true,
        (serde_json::Value::Object(map), Some((head, rest))) => {
            map.get(*head).is_some_and(|child| resolves(child, rest))
        }
        (serde_json::Value::Array(items), Some(_)) => {
            items.is_empty() || items.iter().any(|item| resolves(item, segments))
        }
        _ => false,
    }
}

fn insert(
    out: &mut serde_json::Value,
    value: &serde_json::Value,
    segments: &[&str],
    leaves: &mut usize,
) {
    let Some((head, rest)) = segments.split_first() else {
        *out = value.clone();
        *leaves += 1;
        return;
    };
    match value {
        serde_json::Value::Object(map) => {
            let Some(child) = map.get(*head) else {
                return;
            };
            if !resolves(child, rest) {
                return;
            }
            if !out.is_object() {
                *out = serde_json::Value::Object(serde_json::Map::new());
            }
            let Some(sinks) = out.as_object_mut() else {
                return;
            };
            let sink = sinks
                .entry((*head).to_string())
                .or_insert(serde_json::Value::Null);
            insert(sink, child, rest, leaves);
        }
        serde_json::Value::Array(items) => {
            if items.is_empty() {
                if !out.is_array() {
                    *out = serde_json::Value::Array(Vec::new());
                }
                *leaves += 1;
                return;
            }
            if !out.is_array() {
                *out = serde_json::Value::Array(Vec::new());
            }
            let Some(sinks) = out.as_array_mut() else {
                return;
            };
            sinks.resize(
                items.len(),
                serde_json::Value::Object(serde_json::Map::new()),
            );
            for (sink, item) in sinks.iter_mut().zip(items) {
                insert(sink, item, segments, leaves);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn projected(
        value: serde_json::Value,
        paths: &[&str],
    ) -> Result<serde_json::Value, ProjectionError> {
        let paths: Vec<String> = paths.iter().map(|p| p.to_string()).collect();
        project(value, &paths)
    }

    #[test]
    fn flat_select_keeps_only_named_keys() {
        assert_eq!(
            projected(json!({"a": 1, "b": 2}), &["a"]),
            Ok(json!({"a": 1}))
        );
    }

    #[test]
    fn union_keeps_every_named_key() {
        assert_eq!(
            projected(json!({"a": 1, "b": 2}), &["a", "b"]),
            Ok(json!({"a": 1, "b": 2}))
        );
    }

    #[test]
    fn ancestors_kept_and_siblings_dropped() {
        let value = json!({
            "nodes": [{"title": "t", "body": "x"}],
            "pageInfo": {"endCursor": "e", "hasNextPage": false}
        });
        assert_eq!(
            projected(value, &["nodes.title"]),
            Ok(json!({"nodes": [{"title": "t"}]}))
        );
    }

    #[test]
    fn array_element_missing_the_key_contributes_empty_object() {
        let value = json!({"nodes": [{"title": "t"}, {"body": "x"}]});
        assert_eq!(
            projected(value, &["nodes.title"]),
            Ok(json!({"nodes": [{"title": "t"}, {}]}))
        );
    }

    #[test]
    fn wider_path_wins() {
        let value = json!({"nodes": [{"title": "t", "body": "x"}]});
        assert_eq!(
            projected(value, &["nodes", "nodes.title"]),
            Ok(json!({"nodes": [{"title": "t", "body": "x"}]}))
        );
    }

    #[test]
    fn top_level_path_resolving_nowhere_errors() {
        assert_eq!(
            projected(json!({"a": 1}), &["bogus"]),
            Err(ProjectionError {
                path: "bogus".to_string()
            })
        );
    }

    #[test]
    fn path_under_array_resolving_nowhere_errors() {
        let value = json!({"nodes": [{"title": "t"}]});
        assert_eq!(
            projected(value, &["nodes.nope"]),
            Err(ProjectionError {
                path: "nodes.nope".to_string()
            })
        );
    }

    #[test]
    fn empty_segment_errors() {
        assert_eq!(
            projected(json!({"a": {"b": 1}}), &["a..b"]),
            Err(ProjectionError {
                path: "a..b".to_string()
            })
        );
    }

    #[test]
    fn empty_array_projects_to_empty_array() {
        let value = json!({
            "nodes": [],
            "pageInfo": {"endCursor": "e", "hasNextPage": false}
        });
        assert_eq!(
            projected(value, &["nodes.identifier"]),
            Ok(json!({"nodes": []}))
        );
    }

    #[test]
    fn no_paths_returns_value_unchanged() {
        let value = json!({"a": 1, "nested": {"b": [1, 2]}});
        assert_eq!(project(value.clone(), &[]), Ok(value));
    }
}
