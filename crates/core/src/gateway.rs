use crate::catalog::{build_catalog, CatalogEntry, SUMMARY_MAX_CHARS};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default)]
    pub upstreams: Vec<Upstream>,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Upstream {
    pub prefix: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[serde(default, rename = "readOnly")]
    pub read_only: BTreeSet<String>,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct UpstreamTool {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(rename = "inputSchema")]
    pub input_schema: Value,
}

#[derive(Clone)]
pub struct GatewayTool {
    pub name: String,
    pub upstream: usize,
    pub original_name: String,
    pub description: String,
    pub input_schema: Value,
    pub requires_writes: bool,
}

#[derive(Deserialize, Serialize)]
pub struct Cache {
    pub version: u32,
    pub fingerprint: String,
    pub tools: Vec<Vec<UpstreamTool>>,
}

#[derive(Debug, thiserror::Error)]
#[error("Invalid upstream configuration or catalog: {0}")]
pub struct Error(pub String);

pub fn validate_config(config: &Config) -> Result<(), Error> {
    let mut prefixes = BTreeSet::new();
    for (index, upstream) in config.upstreams.iter().enumerate() {
        if upstream.prefix.is_empty()
            || !upstream.prefix.as_bytes()[0].is_ascii_alphabetic()
            || !upstream
                .prefix
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric())
            || !prefixes.insert(&upstream.prefix)
        {
            return Err(Error(format!("gateway.upstreams[{index}].prefix must be unique ASCII letters/digits starting with a letter")));
        }
        if upstream.command.trim().is_empty() || upstream.command.contains('\0') {
            return Err(Error(format!(
                "gateway.upstreams[{index}].command must be nonempty and contain no NUL"
            )));
        }
        if upstream.args.iter().any(|arg| arg.contains('\0')) {
            return Err(Error(format!(
                "gateway.upstreams[{index}].args must contain no NUL"
            )));
        }
        if upstream
            .env
            .iter()
            .any(|(key, value)| key.is_empty() || key.contains(['=', '\0']) || value.contains('\0'))
        {
            return Err(Error(format!("gateway.upstreams[{index}].env needs nonempty keys without '=' or NUL, and values without NUL")));
        }
    }
    Ok(())
}

pub fn fingerprint(config: &Config) -> String {
    format!(
        "{:x}",
        md5::compute(serde_json::to_vec(config).expect("serializable config"))
    )
}

pub fn reusable_cache(config: &Config, cache: &Cache) -> bool {
    cache.version == 1
        && cache.fingerprint == fingerprint(config)
        && cache.tools.len() == config.upstreams.len()
}

pub fn merge_tools(
    config: &Config,
    discovered: &[Vec<UpstreamTool>],
    existing_names: impl IntoIterator<Item = String>,
) -> Result<Vec<GatewayTool>, Error> {
    if discovered.len() != config.upstreams.len() {
        return Err(Error("missing upstream catalog".into()));
    }
    let mut names: BTreeSet<String> = existing_names.into_iter().collect();
    let mut tools = Vec::new();
    for (index, (upstream, discovered)) in config.upstreams.iter().zip(discovered).enumerate() {
        for tool in discovered {
            if tool.name.is_empty()
                || !tool.input_schema.is_object()
                || tool.input_schema.get("type").and_then(Value::as_str) != Some("object")
            {
                return Err(Error(
                    "tool names must be nonempty and inputSchema must be an object schema".into(),
                ));
            }
            let normalized: String = tool
                .name
                .chars()
                .map(|character| {
                    if character.is_ascii_alphanumeric() || character == '_' {
                        character
                    } else {
                        '_'
                    }
                })
                .collect();
            let name = format!("{}_{}", upstream.prefix, normalized);
            if !names.insert(name.clone()) {
                return Err(Error(format!("duplicate tool name: {name}")));
            }
            tools.push(GatewayTool {
                name,
                upstream: index,
                original_name: tool.name.clone(),
                description: tool.description.clone(),
                input_schema: tool.input_schema.clone(),
                requires_writes: !upstream.read_only.contains(&tool.name),
            });
        }
    }
    Ok(tools)
}

pub fn merge_catalog(mut existing: Vec<CatalogEntry>, tools: &[GatewayTool]) -> Vec<CatalogEntry> {
    let summaries: Vec<String> = tools
        .iter()
        .map(|tool| tool.description.chars().take(SUMMARY_MAX_CHARS).collect())
        .collect();
    existing.extend(build_catalog(
        tools
            .iter()
            .zip(&summaries)
            .map(|(tool, summary)| (tool.name.as_str(), summary.as_str())),
    ));
    existing
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn config(value: serde_json::Value) -> Result<Config, Error> {
        let config = serde_json::from_value(value).map_err(|error| Error(error.to_string()))?;
        validate_config(&config)?;
        Ok(config)
    }

    #[test]
    fn config_rejects_ambiguous_prefixes_unknown_fields_and_invalid_process_arguments() {
        for value in [
            json!({"upstreams": [{"prefix": "has_under", "command": "x"}]}),
            json!({"upstreams": [{"prefix": "a", "command": ""}]}),
            json!({"upstreams": [{"prefix": "a", "command": "x", "url": "https://example.com"}]}),
            json!({"upstreams": [{"prefix": "a", "command": "x"}, {"prefix": "a", "command": "y"}]}),
            json!({"upstreams": [{"prefix": "a", "command": "x", "env": {"BAD=KEY": "secret"}}]}),
        ] {
            assert!(config(value).is_err());
        }
    }

    #[test]
    fn cache_key_covers_all_gateway_configuration_and_ignores_toml_formatting() {
        let value = json!({"upstreams": [{"prefix": "a", "command": "x"}]});
        let config = self::config(value.clone()).unwrap();
        assert_eq!(
            fingerprint(&config),
            fingerprint(
                &crate::config::parse("[[gateway.upstreams]]\nprefix = 'a'\ncommand = 'x'\n")
                    .unwrap()
                    .gateway
            )
        );
        let cache = Cache {
            version: 1,
            fingerprint: fingerprint(&config),
            tools: vec![vec![]],
        };
        assert!(reusable_cache(&config, &cache));
        for field in ["prefix", "command", "args", "env", "readOnly"] {
            let mut changed = value.clone();
            changed["upstreams"][0][field] = match field {
                "args" | "readOnly" => json!(["echo"]),
                "env" => json!({"TOKEN": "changed"}),
                _ => json!("changed"),
            };
            assert!(!reusable_cache(&self::config(changed).unwrap(), &cache));
        }
    }

    #[test]
    fn merge_rejects_collisions_and_ignores_upstream_safety_annotations() {
        let config =
            config(json!({"upstreams":[{"prefix":"fake","command":"x","readOnly":["read"]}]}))
                .unwrap();
        let tools: Vec<UpstreamTool> = serde_json::from_value(json!([
            {"name":"echo","inputSchema":{"type":"object"},"annotations":{"readOnlyHint":true}},
            {"name":"read","inputSchema":{"type":"object"}}
        ]))
        .unwrap();
        let merged = merge_tools(&config, std::slice::from_ref(&tools), []).unwrap();
        assert!(merged[0].requires_writes);
        assert!(!merged[1].requires_writes);
        assert_eq!(merge_catalog(vec![], &merged)[0].domain, "fake");
        assert!(merge_tools(&config, &[tools], ["fake_echo".into()]).is_err());
    }

    #[test]
    fn merge_normalizes_each_non_identifier_character_and_preserves_original_policy() {
        let config = config(json!({"upstreams":[{"prefix":"fake","command":"x","readOnly":["resolve-library-id", "query_docs"]}]})).unwrap();
        let tools: Vec<UpstreamTool> = serde_json::from_value(json!([
            {"name":"resolve-library-id","inputSchema":{"type":"object"}},
            {"name":"query.docs","inputSchema":{"type":"object"}},
            {"name":"9_Az /é😀-$","inputSchema":{"type":"object"}}
        ]))
        .unwrap();
        let merged = merge_tools(&config, &[tools], []).unwrap();
        assert_eq!(merged[0].name, "fake_resolve_library_id");
        assert_eq!(merged[0].original_name, "resolve-library-id");
        assert!(!merged[0].requires_writes);
        assert_eq!(merged[1].name, "fake_query_docs");
        assert!(merged[1].requires_writes);
        assert_eq!(merged[2].name, "fake_9_Az______");
        assert_eq!(merged[2].original_name, "9_Az /é😀-$");
    }

    #[test]
    fn merge_rejects_normalized_collisions_with_upstream_or_reserved_names() {
        let config = config(json!({"upstreams":[{"prefix":"fake","command":"x"}]})).unwrap();
        let tools: Vec<UpstreamTool> = serde_json::from_value(json!([
            {"name":"a-b","inputSchema":{"type":"object"}},
            {"name":"a_b","inputSchema":{"type":"object"}}
        ]))
        .unwrap();
        assert!(merge_tools(&config, std::slice::from_ref(&tools), [])
            .err()
            .unwrap()
            .to_string()
            .contains("duplicate tool name: fake_a_b"));
        assert!(merge_tools(&config, &[tools[..1].to_vec()], ["fake_a_b".into()]).is_err());
    }
}
