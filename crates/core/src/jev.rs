use serde_json::json;

pub struct Secret(String);

impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[redacted]")
    }
}

impl Clone for Secret {
    fn clone(&self) -> Self {
        Secret(self.0.clone())
    }
}

impl Secret {
    pub fn new(value: impl Into<String>) -> Self {
        Secret(value.into())
    }
    pub fn expose(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Provider {
    OpenCode,
    OpenRouter,
    Vercel,
    TypeSafe,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Preset {
    pub endpoint: &'static str,
    pub model: &'static str,
    pub key_var: &'static str,
}

impl Provider {
    pub fn preset(&self) -> Preset {
        match self {
            Provider::OpenCode => Preset {
                endpoint: "https://opencode.ai/zen/v1/systemone",
                model: "jev-1.13",
                key_var: "OPENCODE_API_KEY",
            },
            Provider::OpenRouter => Preset {
                endpoint: "https://openrouter.ai/api/v1/systemone",
                model: "typesafe/jev-1.13",
                key_var: "OPENROUTER_API_KEY",
            },
            Provider::Vercel => Preset {
                endpoint: "https://ai-gateway.vercel.sh/typesafe/v1/systemone",
                model: "typesafe-ai/jev",
                key_var: "AI_GATEWAY_API_KEY",
            },
            Provider::TypeSafe => Preset {
                endpoint: "https://api.typesafe.ai/v1/systemone",
                model: "jev-1.13.0",
                key_var: "TYPESAFE_API_KEY",
            },
        }
    }
}

#[derive(Debug, Clone)]
pub struct GatewayConfig {
    pub endpoint: String,
    pub model: String,
    pub api_key: Secret,
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("unknown JEV_PROVIDER")]
    UnknownProvider,
    #[error("missing API key")]
    MissingKey,
}

pub fn gateway_config(
    var: impl Fn(&str) -> Option<String>,
) -> Result<Option<GatewayConfig>, ConfigError> {
    match var("JEV_PROVIDER") {
        None => Ok(None),
        Some(name) => {
            let provider = match name.as_str() {
                "opencode" => Provider::OpenCode,
                "openrouter" => Provider::OpenRouter,
                "vercel" => Provider::Vercel,
                "typesafe" => Provider::TypeSafe,
                _ => return Err(ConfigError::UnknownProvider),
            };
            let preset = provider.preset();
            let endpoint = var("JEV_ENDPOINT").unwrap_or_else(|| preset.endpoint.to_string());
            let model = var("JEV_MODEL").unwrap_or_else(|| preset.model.to_string());
            let key = var("JEV_API_KEY").or_else(|| var(preset.key_var));
            match key {
                Some(k) => Ok(Some(GatewayConfig {
                    endpoint,
                    model,
                    api_key: Secret::new(k),
                })),
                None => Err(ConfigError::MissingKey),
            }
        }
    }
}

pub fn build_request(
    task: &str,
    catalog: &[crate::catalog::CatalogEntry],
    model: &str,
) -> serde_json::Value {
    let mut criteria = serde_json::Map::new();
    for entry in catalog {
        criteria.insert(entry.name.clone(), json!(entry.summary.clone()));
    }
    criteria.insert("none".to_string(), json!("No listed tool can do this task"));
    json!({
        "model": model,
        "state": task,
        "questions": {
            "tool": {
                "type": "choice",
                "instructions": "Which tool does this task?",
                "criteria": criteria
            }
        }
    })
}

#[derive(Debug, thiserror::Error)]
pub enum ClassifyError {
    #[error("malformed response body")]
    MalformedBody,
    #[error("missing answer")]
    MissingAnswer,
}

pub fn parse_ranking(
    body: &str,
    catalog: &[crate::catalog::CatalogEntry],
) -> Result<crate::find_tools::Ranking, ClassifyError> {
    let v: serde_json::Value =
        serde_json::from_str(body).map_err(|_| ClassifyError::MalformedBody)?;
    let probs = v
        .get("answers")
        .and_then(|a| a.get("tool"))
        .and_then(|t| t.get("probabilities"))
        .and_then(|p| p.as_object())
        .ok_or(ClassifyError::MalformedBody)?;
    if !probs.contains_key("none") {
        return Err(ClassifyError::MissingAnswer);
    }
    let none = probs
        .get("none")
        .and_then(|n| n.as_f64())
        .ok_or(ClassifyError::MalformedBody)?;
    if !(0.0..=1.0).contains(&none) {
        return Err(ClassifyError::MalformedBody);
    }
    let mut tools = vec![];
    for (k, val) in probs {
        if k == "none" {
            continue;
        }
        let score = val.as_f64().ok_or(ClassifyError::MalformedBody)?;
        if !(0.0..=1.0).contains(&score) {
            return Err(ClassifyError::MalformedBody);
        }
        if let Some(entry) = catalog.iter().find(|c| c.name == *k) {
            tools.push(crate::find_tools::RankedTool {
                name: k.clone(),
                domain: entry.domain.clone(),
                score,
            });
        }
    }
    tools.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    Ok(crate::find_tools::Ranking { none, tools })
}

pub const MIN_PROBABILITY: f64 = 0.05;
pub const MAX_JEV_TOOLS: usize = 3;

pub fn select(ranking: crate::find_tools::Ranking, k: usize) -> crate::find_tools::Ranking {
    let mut tools: Vec<_> = ranking
        .tools
        .into_iter()
        .filter(|t| t.score >= MIN_PROBABILITY)
        .collect();
    let cap = k.min(MAX_JEV_TOOLS);
    if tools.len() > cap {
        tools.truncate(cap);
    }
    crate::find_tools::Ranking {
        none: ranking.none,
        tools,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::build_catalog;

    #[test]
    fn provider_preset_matches_the_gateway_table() {
        assert_eq!(
            Provider::OpenCode.preset(),
            Preset {
                endpoint: "https://opencode.ai/zen/v1/systemone",
                model: "jev-1.13",
                key_var: "OPENCODE_API_KEY"
            }
        );
        assert_eq!(
            Provider::OpenRouter.preset(),
            Preset {
                endpoint: "https://openrouter.ai/api/v1/systemone",
                model: "typesafe/jev-1.13",
                key_var: "OPENROUTER_API_KEY"
            }
        );
        assert_eq!(
            Provider::Vercel.preset(),
            Preset {
                endpoint: "https://ai-gateway.vercel.sh/typesafe/v1/systemone",
                model: "typesafe-ai/jev",
                key_var: "AI_GATEWAY_API_KEY"
            }
        );
        assert_eq!(
            Provider::TypeSafe.preset(),
            Preset {
                endpoint: "https://api.typesafe.ai/v1/systemone",
                model: "jev-1.13.0",
                key_var: "TYPESAFE_API_KEY"
            }
        );
    }

    #[test]
    fn gateway_config_is_none_when_provider_unset() {
        let res = gateway_config(|k| {
            if k == "OPENCODE_API_KEY" {
                Some("x".into())
            } else {
                None
            }
        });
        assert!(matches!(res, Ok(None)));
    }

    #[test]
    fn gateway_config_uses_preset_for_known_provider() {
        let res = gateway_config(|k| match k {
            "JEV_PROVIDER" => Some("opencode".into()),
            "OPENCODE_API_KEY" => Some("k1".into()),
            _ => None,
        })
        .unwrap()
        .unwrap();
        assert_eq!(res.endpoint, "https://opencode.ai/zen/v1/systemone");
        assert_eq!(res.model, "jev-1.13");
    }

    #[test]
    fn gateway_config_overrides_endpoint_and_model() {
        let res = gateway_config(|k| match k {
            "JEV_PROVIDER" => Some("opencode".into()),
            "JEV_ENDPOINT" => Some("https://ex".into()),
            "JEV_MODEL" => Some("m2".into()),
            "OPENCODE_API_KEY" => Some("k".into()),
            _ => None,
        })
        .unwrap()
        .unwrap();
        assert_eq!(res.endpoint, "https://ex");
        assert_eq!(res.model, "m2");
    }

    #[test]
    fn gateway_config_prefers_jev_api_key_over_conventional_var() {
        let res = gateway_config(|k| match k {
            "JEV_PROVIDER" => Some("opencode".into()),
            "JEV_API_KEY" => Some("jevkey".into()),
            "OPENCODE_API_KEY" => Some("conv".into()),
            _ => None,
        })
        .unwrap()
        .unwrap();
        assert_eq!(res.api_key.expose(), "jevkey");
    }

    #[test]
    fn gateway_config_rejects_unknown_provider() {
        let res = gateway_config(|k| {
            if k == "JEV_PROVIDER" {
                Some("bogus".into())
            } else {
                None
            }
        });
        assert!(matches!(res, Err(ConfigError::UnknownProvider)));
    }

    #[test]
    fn gateway_config_rejects_missing_key() {
        let res = gateway_config(|k| {
            if k == "JEV_PROVIDER" {
                Some("opencode".into())
            } else {
                None
            }
        });
        assert!(matches!(res, Err(ConfigError::MissingKey)));
    }

    #[test]
    fn build_request_criteria_equals_catalog_plus_none() {
        let cat = build_catalog([("jira_update", "update jira"), ("foo_bar", "foo")]);
        let v = build_request("do", &cat, "m");
        let crit = &v["questions"]["tool"]["criteria"];
        let obj = crit.as_object().unwrap();
        assert_eq!(obj.len(), 3);
        assert_eq!(obj.get("jira_update").unwrap(), "update jira");
        assert_eq!(obj.get("foo_bar").unwrap(), "foo");
        assert_eq!(obj.get("none").unwrap(), "No listed tool can do this task");
    }

    #[test]
    fn parse_ranking_drops_non_catalog_key() {
        let cat = build_catalog([("jira_update", "u")]);
        let body =
            r#"{"answers":{"tool":{"probabilities":{"jira_update":0.8,"none":0.1,"bogus":0.1}}}}"#;
        let r = parse_ranking(body, &cat).unwrap();
        assert_eq!(r.tools.len(), 1);
        assert_eq!(r.tools[0].name, "jira_update");
        assert_eq!(r.none, 0.1);
    }

    #[test]
    fn parse_ranking_errors_on_missing_none() {
        let cat = build_catalog([("jira_update", "u")]);
        let body = r#"{"answers":{"tool":{"probabilities":{"jira_update":0.9}}}}"#;
        let e = parse_ranking(body, &cat);
        assert!(matches!(e, Err(ClassifyError::MissingAnswer)));
    }

    #[test]
    fn parse_ranking_errors_on_out_of_range_probability() {
        let cat = build_catalog([("jira_update", "u")]);
        let body = r#"{"answers":{"tool":{"probabilities":{"jira_update":1.4,"none":0.0}}}}"#;
        let e = parse_ranking(body, &cat);
        assert!(matches!(e, Err(ClassifyError::MalformedBody)));
    }

    #[test]
    fn parse_ranking_errors_on_non_numeric_probability() {
        let cat = build_catalog([("jira_update", "u")]);
        let body = r#"{"answers":{"tool":{"probabilities":{"jira_update":"x","none":0.1}}}}"#;
        let e = parse_ranking(body, &cat);
        assert!(matches!(e, Err(ClassifyError::MalformedBody)));
    }

    #[test]
    fn select_drops_below_floor_and_caps_at_max() {
        let cat = build_catalog([("a", "s"), ("b", "s"), ("c", "s"), ("d", "s"), ("e", "s")]);
        let full = parse_ranking(
            r#"{"answers":{"tool":{"probabilities":{"a":0.9,"b":0.06,"c":0.04,"d":0.03,"e":0.01,"none":0.0}}}}"#,
            &cat,
        )
        .unwrap();
        let sel = select(full, 5);
        assert_eq!(sel.tools.len(), 2);
        assert_eq!(sel.tools[0].name, "a");
        assert_eq!(sel.tools[1].name, "b");
    }

    #[test]
    fn select_returns_empty_list_when_all_below_floor() {
        let cat = build_catalog([("a", "s"), ("b", "s")]);
        let r = parse_ranking(
            r#"{"answers":{"tool":{"probabilities":{"a":0.01,"b":0.01,"none":0.98}}}}"#,
            &cat,
        )
        .unwrap();
        let sel = select(r, 5);
        assert!(sel.tools.is_empty());
        assert_eq!(sel.none, 0.98);
    }

    #[test]
    fn secret_debug_is_redacted() {
        let s = Secret::new("sentinel");
        let d = format!("{:?}", s);
        assert!(!d.contains("sentinel"));
        assert!(d.contains("[redacted]"));
    }
}
