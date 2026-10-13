use serde_json::{Map, Value};

const DIRECT_SCHEMA_KEYS: [&str; 11] = [
    "additionalItems",
    "contains",
    "contentSchema",
    "else",
    "if",
    "items",
    "not",
    "propertyNames",
    "then",
    "unevaluatedItems",
    "unevaluatedProperties",
];

const SCHEMA_ARRAY_KEYS: [&str; 4] = ["allOf", "anyOf", "oneOf", "prefixItems"];
const SCHEMA_MAP_KEYS: [&str; 6] = [
    "$defs",
    "dependencies",
    "definitions",
    "dependentSchemas",
    "patternProperties",
    "properties",
];

pub fn objectify_boolean_subschemas(mut schema: Value) -> Value {
    transform_schema(&mut schema);
    schema
}

pub fn boolean_subschema_paths(schema: &Value) -> Vec<String> {
    let mut paths = Vec::new();
    collect_paths(schema, "", &mut paths);
    paths
}

fn transform_schema(schema: &mut Value) {
    let Some(object) = schema.as_object_mut() else {
        return;
    };
    for key in DIRECT_SCHEMA_KEYS {
        if let Some(value) = object.get_mut(key) {
            transform_child(value);
        }
    }
    for key in SCHEMA_ARRAY_KEYS {
        if let Some(values) = object.get_mut(key).and_then(Value::as_array_mut) {
            for value in values {
                transform_child(value);
            }
        }
    }
    for key in SCHEMA_MAP_KEYS {
        if let Some(values) = object.get_mut(key).and_then(Value::as_object_mut) {
            for value in values.values_mut() {
                transform_child(value);
            }
        }
    }
}

fn transform_child(schema: &mut Value) {
    if let Some(value) = schema.as_bool() {
        *schema = if value {
            Value::Object(Map::new())
        } else {
            serde_json::json!({"not": {}})
        };
    } else if let Some(values) = schema.as_array_mut() {
        for value in values {
            transform_child(value);
        }
    } else {
        transform_schema(schema);
    }
}

fn collect_paths(schema: &Value, path: &str, paths: &mut Vec<String>) {
    let Some(object) = schema.as_object() else {
        return;
    };
    for key in DIRECT_SCHEMA_KEYS {
        if let Some(value) = object.get(key) {
            collect_child(value, &format!("{path}/{key}"), paths);
        }
    }
    for key in SCHEMA_ARRAY_KEYS {
        if let Some(values) = object.get(key).and_then(Value::as_array) {
            for (index, value) in values.iter().enumerate() {
                collect_child(value, &format!("{path}/{key}/{index}"), paths);
            }
        }
    }
    for key in SCHEMA_MAP_KEYS {
        if let Some(values) = object.get(key).and_then(Value::as_object) {
            for (name, value) in values {
                collect_child(value, &format!("{path}/{key}/{name}"), paths);
            }
        }
    }
}

fn collect_child(schema: &Value, path: &str, paths: &mut Vec<String>) {
    if schema.is_boolean() {
        paths.push(path.to_string());
    } else if let Some(values) = schema.as_array() {
        for (index, value) in values.iter().enumerate() {
            collect_child(value, &format!("{path}/{index}"), paths);
        }
    } else {
        collect_paths(schema, path, paths);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn objectifies_boolean_subschemas_without_changing_additional_properties() {
        let schema = serde_json::json!({
            "properties": {
                "anything": true,
                "nothing": false,
                "nested": {"items": true}
            },
            "anyOf": [true, {"not": false}],
            "additionalProperties": true
        });
        let transformed = objectify_boolean_subschemas(schema);
        assert_eq!(transformed["properties"]["anything"], serde_json::json!({}));
        assert_eq!(
            transformed["properties"]["nothing"],
            serde_json::json!({"not": {}})
        );
        assert_eq!(
            transformed["properties"]["nested"]["items"],
            serde_json::json!({})
        );
        assert_eq!(transformed["anyOf"][0], serde_json::json!({}));
        assert_eq!(
            transformed["anyOf"][1]["not"],
            serde_json::json!({"not": {}})
        );
        assert_eq!(transformed["additionalProperties"], true);
        assert!(boolean_subschema_paths(&transformed).is_empty());
    }

    #[test]
    fn reports_nested_boolean_subschema_paths() {
        let schema = serde_json::json!({
            "properties": {"result": true},
            "$defs": {"entry": {"prefixItems": [false]}}
        });
        assert_eq!(
            boolean_subschema_paths(&schema),
            ["/$defs/entry/prefixItems/0", "/properties/result"]
        );
    }
}
