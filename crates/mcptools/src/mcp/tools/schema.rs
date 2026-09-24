pub fn input_schema_for<T: schemars::JsonSchema>() -> serde_json::Value {
    schema_value::<T>(true)
}

pub fn output_schema_for<T: schemars::JsonSchema>() -> serde_json::Value {
    schema_value::<T>(false)
}

pub fn projected_output_schema_for<T: schemars::JsonSchema>() -> serde_json::Value {
    let mut value = output_schema_for::<T>();
    strip_required(&mut value);
    value
}

fn strip_required(value: &mut serde_json::Value) {
    let Some(map) = value.as_object_mut() else {
        if let Some(items) = value.as_array_mut() {
            for item in items {
                strip_required(item);
            }
        }
        return;
    };
    map.remove("required");
    if let Some(props) = map.get_mut("properties").and_then(|v| v.as_object_mut()) {
        for child in props.values_mut() {
            strip_required(child);
        }
    }
    if let Some(items) = map.get_mut("items") {
        strip_required(items);
    }
    for key in ["anyOf", "oneOf", "allOf", "prefixItems"] {
        if let Some(variants) = map.get_mut(key).and_then(|v| v.as_array_mut()) {
            for variant in variants {
                strip_required(variant);
            }
        }
    }
    for key in ["$defs", "definitions"] {
        if let Some(defs) = map.get_mut(key).and_then(|v| v.as_object_mut()) {
            for child in defs.values_mut() {
                strip_required(child);
            }
        }
    }
}

fn schema_value<T: schemars::JsonSchema>(normalize: bool) -> serde_json::Value {
    let schema = schemars::schema_for!(T);
    let mut value = serde_json::to_value(&schema).unwrap_or(serde_json::json!({
        "type": "object",
    }));
    strip_envelope(&mut value, normalize);
    value
}

fn strip_envelope(value: &mut serde_json::Value, normalize: bool) {
    let Some(root) = value.as_object_mut() else {
        return;
    };
    root.remove("$schema");
    root.remove("title");
    let Some(props) = root.get_mut("properties").and_then(|v| v.as_object_mut()) else {
        return;
    };
    if normalize {
        for (_, prop) in props.iter_mut() {
            normalize_prop(prop);
        }
    }
}

fn normalize_prop(prop: &mut serde_json::Value) {
    let Some(obj) = prop.as_object_mut() else {
        return;
    };
    if let Some(serde_json::Value::Array(types)) = obj.get("type").cloned() {
        let single = types.iter().find_map(|t| match t.as_str() {
            Some("null") => None,
            Some(other) => Some(other.to_string()),
            None => None,
        });
        match single {
            Some(t) => {
                obj.insert("type".to_string(), serde_json::Value::String(t));
            }
            None => {
                obj.remove("type");
            }
        }
    }
    obj.remove("format");
    obj.remove("minimum");
    obj.remove("maximum");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::linear::args::*;

    fn prop_names(schema: &serde_json::Value) -> Vec<String> {
        let mut names: Vec<String> = schema
            .get("properties")
            .and_then(|v| v.as_object())
            .map(|o| o.keys().cloned().collect())
            .unwrap_or_default();
        names.sort();
        names
    }

    fn required(schema: &serde_json::Value) -> Vec<String> {
        let mut req: Vec<String> = schema
            .get("required")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        req.sort();
        req
    }

    fn prop_type(schema: &serde_json::Value, field: &str) -> Option<String> {
        schema
            .get("properties")?
            .get(field)?
            .get("type")?
            .as_str()
            .map(str::to_string)
    }

    #[test]
    fn strips_envelope_and_null_unions() {
        let schema = input_schema_for::<IssueListArgs>();
        assert!(schema.get("$schema").is_none());
        assert!(schema.get("title").is_none());
        assert_eq!(schema.get("type").and_then(|v| v.as_str()), Some("object"));
        assert_eq!(prop_type(&schema, "limit"), Some("integer".to_string()));
        assert_eq!(
            prop_type(&schema, "updatedAfter"),
            Some("string".to_string())
        );
        assert!(schema
            .get("properties")
            .and_then(|p| p.get("limit"))
            .and_then(|l| l.get("format"))
            .is_none());
    }

    #[test]
    fn required_sets_match_previous_contract() {
        let cases: Vec<(serde_json::Value, Vec<&str>)> = vec![
            (input_schema_for::<AuthStatusArgs>(), vec![]),
            (input_schema_for::<IssueGetArgs>(), vec!["id"]),
            (input_schema_for::<IssueListArgs>(), vec![]),
            (input_schema_for::<CommentListArgs>(), vec!["id"]),
            (input_schema_for::<RelationListArgs>(), vec!["id"]),
            (input_schema_for::<TeamListArgs>(), vec![]),
            (input_schema_for::<TeamGetArgs>(), vec!["selector"]),
            (input_schema_for::<ProjectListArgs>(), vec!["team"]),
            (input_schema_for::<ProjectGetArgs>(), vec!["id", "team"]),
            (input_schema_for::<UserListArgs>(), vec!["query"]),
            (input_schema_for::<StateListArgs>(), vec!["team"]),
            (input_schema_for::<LabelListArgs>(), vec!["team"]),
            (input_schema_for::<CycleListArgs>(), vec!["team"]),
            (input_schema_for::<IssueCreateArgs>(), vec!["team", "title"]),
            (input_schema_for::<IssueUpdateArgs>(), vec!["id"]),
            (input_schema_for::<CommentCreateArgs>(), vec!["id", "body"]),
            (
                input_schema_for::<RelationAddArgs>(),
                vec!["related", "source", "type"],
            ),
            (
                input_schema_for::<RelationRemoveArgs>(),
                vec!["related", "source", "type"],
            ),
        ];
        for (schema, expected) in cases {
            let mut want: Vec<String> = expected.iter().map(|s| s.to_string()).collect();
            want.sort();
            assert_eq!(required(&schema), want);
        }
    }

    #[test]
    fn property_names_match_previous_contract() {
        assert_eq!(
            prop_names(&input_schema_for::<IssueListArgs>()),
            [
                "all",
                "assignee",
                "cursor",
                "cycle",
                "fields",
                "label",
                "limit",
                "project",
                "query",
                "state",
                "team",
                "updatedAfter"
            ]
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>()
        );
        assert_eq!(
            prop_names(&input_schema_for::<IssueUpdateArgs>()),
            [
                "assignee",
                "clearParent",
                "description",
                "id",
                "parent",
                "state",
                "team",
                "title"
            ]
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>()
        );
        assert_eq!(
            prop_names(&input_schema_for::<TeamGetArgs>()),
            vec!["selector".to_string()]
        );
    }
}
