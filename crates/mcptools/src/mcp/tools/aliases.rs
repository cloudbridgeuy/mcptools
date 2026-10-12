use super::{is_meta_tool, JsonRpcError, Tool};

pub const HIDDEN_ALIASES: &[(&str, &str)] = &[("linear_issue_view", "linear_issue_get")];

fn validate(tools: &[Tool], aliases: &[(&str, &str)]) -> Result<(), JsonRpcError> {
    for (index, (alias, canonical)) in aliases.iter().enumerate() {
        let message = if tools.iter().any(|tool| tool.name == *alias) {
            Some(format!("Tool alias collides with registered tool: {alias}"))
        } else if aliases[..index].iter().any(|(name, _)| name == alias) {
            Some(format!("Duplicate tool alias: {alias}"))
        } else if is_meta_tool(canonical) || !tools.iter().any(|tool| tool.name == *canonical) {
            Some(format!(
                "Invalid canonical tool for alias {alias}: {canonical}"
            ))
        } else {
            None
        };
        if let Some(message) = message {
            return Err(JsonRpcError {
                code: -32603,
                message,
                data: None,
            });
        }
    }
    Ok(())
}

pub fn resolve<'a>(name: &'a str, tools: &[Tool]) -> Result<&'a str, JsonRpcError> {
    validate(tools, HIDDEN_ALIASES)?;
    Ok(HIDDEN_ALIASES
        .iter()
        .find(|(alias, _)| *alias == name)
        .map_or(name, |(_, canonical)| *canonical))
}

pub fn extend_bindings(
    mut names: Vec<String>,
    tools: &[Tool],
) -> Result<Vec<String>, JsonRpcError> {
    validate(tools, HIDDEN_ALIASES)?;
    let aliases = HIDDEN_ALIASES
        .iter()
        .filter(|(_, canonical)| names.iter().any(|name| name == canonical))
        .map(|(alias, _)| (*alias).to_string())
        .collect::<Vec<_>>();
    names.extend(aliases);
    Ok(names)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aliases_are_explicit_and_reject_registry_collisions() {
        let tools = super::super::registered_tools();
        assert_eq!(HIDDEN_ALIASES, &[("linear_issue_view", "linear_issue_get")]);
        assert_eq!(
            resolve("linear_issue_view", &tools).unwrap(),
            "linear_issue_get"
        );
        assert_eq!(
            resolve("linear_issue_get", &tools).unwrap(),
            "linear_issue_get"
        );
        assert_eq!(resolve("unknown_view", &tools).unwrap(), "unknown_view");
        let collision = validate(&tools, &[("linear_issue_get", "linear_issue_list")]).unwrap_err();
        assert_eq!(collision.code, -32603);
        assert_eq!(
            collision.message,
            "Tool alias collides with registered tool: linear_issue_get"
        );
        assert!(validate(&tools, &[("hidden", "unknown")]).is_err());
        assert!(validate(&tools, &[("hidden", "execute")]).is_err());
        assert!(validate(
            &tools,
            &[
                ("hidden", "linear_issue_get"),
                ("hidden", "linear_issue_list")
            ]
        )
        .is_err());
    }

    #[test]
    fn alias_binding_inherits_canonical_permission() {
        let tools = super::super::registered_tools();
        assert!(extend_bindings(Vec::new(), &tools).unwrap().is_empty());
        assert_eq!(
            extend_bindings(vec!["linear_issue_get".to_string()], &tools).unwrap(),
            ["linear_issue_get", "linear_issue_view"]
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn alias_dispatch_matches_canonical_in_all_modes() {
        use super::super::{handle_tools_call, ServeFlags};
        use serde_json::json;

        let global = crate::Global {
            verbose: false,
            execute_timeout_secs: 30,
            execute_memory_mb: 64,
            execute_output_kb: 256,
        };
        for flags in [
            ServeFlags {
                discovery: false,
                code_mode: false,
            },
            ServeFlags {
                discovery: true,
                code_mode: false,
            },
            ServeFlags {
                discovery: false,
                code_mode: true,
            },
        ] {
            let canonical = handle_tools_call(
                Some(json!({"name": "linear_issue_get", "arguments": {}})),
                &global,
                flags,
            )
            .await
            .unwrap_err();
            assert!(canonical.message.starts_with("Invalid arguments"));
            for name in ["linear_issue_get", "linear_issue_view"] {
                for request in [
                    json!({"name": name, "arguments": {}}),
                    json!({"name": "call_tool", "arguments": {"name": name, "input": {}}}),
                ] {
                    let error = handle_tools_call(Some(request), &global, flags)
                        .await
                        .unwrap_err();
                    assert_eq!(error.code, canonical.code);
                    assert_eq!(error.message, canonical.message);
                }
                let executed = handle_tools_call(
                    Some(json!({"name": "execute", "arguments": {"code": format!("try {{ await {name}({{}}); return null }} catch (e) {{ return [e.code, e.message] }}")}})),
                    &global,
                    flags,
                ).await.unwrap();
                assert_eq!(
                    executed["structuredContent"]["result"],
                    json!([canonical.code, canonical.message])
                );
                assert!(executed["structuredContent"]["error"].is_null());
            }
        }
    }

    #[tokio::test]
    async fn alias_stays_hidden_from_every_discovery_surface() {
        use super::super::{declarations, handle_tools_list, tool_catalog, ServeFlags};

        let alias = "linear_issue_view";
        for flags in [
            ServeFlags {
                discovery: false,
                code_mode: false,
            },
            ServeFlags {
                discovery: true,
                code_mode: false,
            },
            ServeFlags {
                discovery: false,
                code_mode: true,
            },
        ] {
            assert!(!handle_tools_list(flags)
                .unwrap()
                .to_string()
                .contains(alias));
        }
        assert!(!mcptools_core::catalog::render_catalog(&tool_catalog()).contains(alias));
        assert!(!declarations(&[]).unwrap().contains(alias));
        assert!(declarations(&[alias.to_string()]).is_err());
        let ranked = super::super::find_tools::find_tools_with(Ok(None), alias, 100)
            .await
            .unwrap();
        assert!(ranked
            .tools
            .iter()
            .all(|tool| tool.name != alias && !tool.declaration.contains(alias)));
    }
}
