use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

struct Shape<'a> {
    id: &'a str,
    kind: &'a str,
    text: String,
    x: f64,
    y: f64,
    endpoints: Option<String>,
}

impl Shape<'_> {
    fn description(&self) -> String {
        let mut description = format!("{} {}", self.kind, self.id);
        if !self.text.is_empty() {
            description.push_str(&format!(" {}", Value::String(self.text.clone())));
        }
        if let Some(endpoints) = &self.endpoints {
            description.push_str(&format!(" {endpoints}"));
        }
        description
    }
}

pub fn diff_snapshots(baseline: &Value, current: &Value) -> Result<Vec<String>, String> {
    let baseline = shapes(baseline)?;
    let current = shapes(current)?;
    let old: BTreeMap<_, _> = baseline.iter().map(|shape| (shape.id, shape)).collect();
    let new: BTreeMap<_, _> = current.iter().map(|shape| (shape.id, shape)).collect();
    let mut changes: Vec<_> = current
        .iter()
        .filter(|shape| !old.contains_key(shape.id))
        .map(|shape| format!("+ {}", shape.description()))
        .collect();
    changes.sort();
    changes.extend(
        baseline
            .iter()
            .filter(|shape| !new.contains_key(shape.id))
            .map(|shape| format!("- {}", shape.description())),
    );
    for before in &baseline {
        let Some(after) = new.get(before.id) else {
            continue;
        };
        let id = before.id;
        if before.text != after.text {
            changes.push(format!(
                "~ {id} text: {} -> {}",
                Value::String(before.text.clone()),
                Value::String(after.text.clone())
            ));
        }
        if let (Some(from), Some(to)) = (&before.endpoints, &after.endpoints) {
            if from != to {
                changes.push(format!("~ {id} endpoints: {from} -> {to}"));
            }
        }
        if before.x != after.x || before.y != after.y {
            changes.push(format!(
                "~ {id} position: ({}, {}) -> ({}, {})",
                before.x, before.y, after.x, after.y
            ));
        }
    }
    Ok(changes)
}

pub fn render_diff(changes: &[String]) -> String {
    changes.join("\n")
}

fn string<'a>(record: &'a Value, field: &str) -> Result<&'a str, String> {
    record[field]
        .as_str()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("snapshot record requires nonempty {field}"))
}

fn shape_id(id: &str) -> Result<&str, String> {
    id.strip_prefix("shape:")
        .filter(|id| !id.is_empty())
        .ok_or_else(|| format!("invalid shape id: {id}"))
}

fn shapes(snapshot: &Value) -> Result<Vec<Shape<'_>>, String> {
    let records = snapshot["records"]
        .as_array()
        .ok_or("snapshot requires a records array")?;
    let mut ids = BTreeSet::new();
    let mut bindings = BTreeMap::new();
    for record in records {
        let id = string(record, "id")?;
        if !ids.insert(id) {
            return Err(format!("duplicate snapshot record: {id}"));
        }
        let type_name = string(record, "typeName")?;
        if type_name == "binding" && record["type"] == "arrow" {
            let from = shape_id(string(record, "fromId")?)?;
            let to = shape_id(string(record, "toId")?)?;
            let terminal = string(&record["props"], "terminal")?;
            if !matches!(terminal, "start" | "end") {
                return Err(format!("invalid arrow terminal: {terminal}"));
            }
            if bindings.insert((from, terminal), to).is_some() {
                return Err(format!("duplicate arrow terminal: {from} {terminal}"));
            }
        }
    }
    records
        .iter()
        .filter(|record| record["typeName"] == "shape")
        .map(|record| {
            let id = shape_id(string(record, "id")?)?;
            let kind = string(record, "type")?;
            let coordinate = |field: &str| {
                record[field]
                    .as_f64()
                    .filter(|value| value.is_finite())
                    .ok_or_else(|| format!("invalid {field} coordinate for shape: {id}"))
            };
            let props = record["props"]
                .as_object()
                .ok_or_else(|| format!("missing props for shape: {id}"))?;
            let text = props.get("richText").map_or_else(
                || {
                    props
                        .get("text")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .into()
                },
                plain_text,
            );
            let endpoints = (kind == "arrow").then(|| {
                let start = bindings.get(&(id, "start")).copied().unwrap_or("?");
                let end = bindings.get(&(id, "end")).copied().unwrap_or("?");
                format!("({start} -> {end})")
            });
            Ok(Shape {
                id,
                kind,
                text,
                x: coordinate("x")?,
                y: coordinate("y")?,
                endpoints,
            })
        })
        .collect()
}

fn plain_text(node: &Value) -> String {
    if let Some(text) = node["text"].as_str() {
        return text.into();
    }
    if node["type"] == "hardBreak" {
        return "\n".into();
    }
    node["content"]
        .as_array()
        .map_or_else(String::new, |nodes| {
            let separator = if node["type"] == "doc" { "\n" } else { "" };
            nodes
                .iter()
                .map(plain_text)
                .collect::<Vec<_>>()
                .join(separator)
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn snapshot() -> Value {
        serde_json::from_str(include_str!("../../../tests/fixtures/plan/spike.tldr")).unwrap()
    }

    fn record<'a>(snapshot: &'a mut Value, id: &str) -> &'a mut Value {
        snapshot["records"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|record| record["id"] == id)
            .unwrap()
    }

    fn relabel(current: &mut Value) {
        record(current, "shape:s3")["props"]["richText"]["content"][0]["content"][0]["text"] =
            json!("Fix clippy warnings in core (allow pedantic)");
    }

    fn add(current: &mut Value) {
        let mut node = record(current, "shape:s6").clone();
        node["id"] = json!("shape:s7");
        node["index"] = json!("aD");
        node["y"] = json!(400);
        node["props"]["richText"]["content"][0]["content"][0]["text"] =
            json!("NEW: run cargo fmt check");
        let mut arrow = record(current, "shape:s5-s6").clone();
        arrow["id"] = json!("shape:s2-s7");
        arrow["index"] = json!("aE");
        let mut bindings = Vec::new();
        for (terminal, target) in [("start", "s2"), ("end", "s7")] {
            let mut binding = record(current, &format!("binding:s5-s6-{terminal}")).clone();
            binding["id"] = json!(format!("binding:s2-s7-{terminal}"));
            binding["fromId"] = json!("shape:s2-s7");
            binding["toId"] = json!(format!("shape:{target}"));
            bindings.push(binding);
        }
        current["records"]
            .as_array_mut()
            .unwrap()
            .extend([node, arrow]);
        current["records"].as_array_mut().unwrap().extend(bindings);
    }

    fn delete(current: &mut Value) {
        current["records"].as_array_mut().unwrap().retain(|record| {
            ![
                "shape:s6",
                "shape:s5-s6",
                "binding:s5-s6-start",
                "binding:s5-s6-end",
            ]
            .contains(&record["id"].as_str().unwrap())
        });
    }

    fn diff(current: &Value) -> String {
        render_diff(&diff_snapshots(&snapshot(), current).unwrap())
    }

    #[test]
    fn relabel_golden() {
        let mut current = snapshot();
        relabel(&mut current);
        assert_eq!(diff(&current), "~ s3 text: \"Fix new clippy warnings in core\" -> \"Fix clippy warnings in core (allow pedantic)\"");
    }

    #[test]
    fn add_golden() {
        let mut current = snapshot();
        add(&mut current);
        assert_eq!(
            diff(&current),
            "+ arrow s2-s7 (s2 -> s7)\n+ geo s7 \"NEW: run cargo fmt check\""
        );
    }

    #[test]
    fn delete_golden() {
        let mut current = snapshot();
        delete(&mut current);
        assert_eq!(
            diff(&current),
            "- geo s6 \"Document lint policy in CLAUDE.md\"\n- arrow s5-s6 (s5 -> s6)"
        );
    }

    #[test]
    fn repoint_arrow_golden() {
        let mut current = snapshot();
        record(&mut current, "binding:s5-s6-end")["toId"] = json!("shape:s4");
        assert_eq!(
            diff(&current),
            "~ s5-s6 endpoints: (s5 -> s6) -> (s5 -> s4)"
        );
    }

    #[test]
    fn move_golden() {
        let mut current = snapshot();
        let node = record(&mut current, "shape:s1");
        node["x"] = json!(42);
        node["y"] = json!(-17);
        assert_eq!(diff(&current), "~ s1 position: (0, 0) -> (42, -17)");
    }

    #[test]
    fn freehand_golden() {
        let mut current = snapshot();
        current["records"].as_array_mut().unwrap().push(json!({
            "id":"shape:ink1", "typeName":"shape", "type":"draw", "parentId":"page:page",
            "index":"aD", "isLocked":false, "opacity":1, "meta":{}, "x":20, "y":30,
            "rotation":0, "props":{"color":"black", "fill":"none", "dash":"draw",
                "size":"m", "segments":[{"type":"free", "path":"AAAAAAAAgD8AAABAAACAPw=="}],
                "isComplete":true, "isClosed":false, "isPen":false, "scale":1, "scaleX":1, "scaleY":1}
        }));
        assert_eq!(diff(&current), "+ draw ink1");
    }

    #[test]
    fn combined_spike_golden_and_change_list() {
        let mut current = snapshot();
        relabel(&mut current);
        add(&mut current);
        delete(&mut current);
        let changes = diff_snapshots(&snapshot(), &current).unwrap();
        assert_eq!(changes.len(), 5);
        assert_eq!(render_diff(&changes), "+ arrow s2-s7 (s2 -> s7)\n+ geo s7 \"NEW: run cargo fmt check\"\n- geo s6 \"Document lint policy in CLAUDE.md\"\n- arrow s5-s6 (s5 -> s6)\n~ s3 text: \"Fix new clippy warnings in core\" -> \"Fix clippy warnings in core (allow pedantic)\"");
    }

    #[test]
    fn identical_and_empty_snapshots_have_no_changes() {
        assert_eq!(diff(&snapshot()), "");
        let empty = json!({"records":[]});
        assert!(diff_snapshots(&empty, &empty).unwrap().is_empty());
        assert_eq!(render_diff(&[]), "");
    }

    #[test]
    fn record_order_binding_ids_and_non_shape_metadata_do_not_change_the_diff() {
        let mut current = snapshot();
        record(&mut current, "binding:s5-s6-end")["id"] = json!("binding:phone-generated");
        record(&mut current, "document:document")["name"] = json!("Phone plan");
        current["records"].as_array_mut().unwrap().reverse();
        assert_eq!(diff(&current), "");
    }

    #[test]
    fn rich_text_preserves_inline_runs_paragraphs_breaks_and_escaped_text() {
        let mut current = snapshot();
        record(&mut current, "shape:s3")["props"]["richText"] = json!({
            "type":"doc", "content":[
                {"type":"paragraph", "content":[
                    {"type":"text", "text":"Use \"core\"", "marks":[{"type":"bold"}]},
                    {"type":"text", "text":" first"}, {"type":"hardBreak"},
                    {"type":"text", "text":"then \\shell"}
                ]},
                {"type":"paragraph", "content":[{"type":"text", "text":"Finish"}]}
            ]
        });
        assert_eq!(diff(&current), "~ s3 text: \"Fix new clippy warnings in core\" -> \"Use \\\"core\\\" first\\nthen \\\\shell\\nFinish\"");
    }

    #[test]
    fn unbinding_an_arrow_reports_unknown_endpoint_without_guessing_from_its_id() {
        let mut current = snapshot();
        current["records"]
            .as_array_mut()
            .unwrap()
            .retain(|record| record["id"] != "binding:s5-s6-end");
        assert_eq!(diff(&current), "~ s5-s6 endpoints: (s5 -> s6) -> (s5 -> ?)");
    }

    #[test]
    fn simultaneous_changes_are_ordered_and_inputs_are_unchanged() {
        let baseline = snapshot();
        let mut current = baseline.clone();
        relabel(&mut current);
        let node = record(&mut current, "shape:s3");
        node["x"] = json!(42.5);
        node["y"] = json!(-17.25);
        let original = current.clone();
        let changes = diff_snapshots(&baseline, &current).unwrap();
        assert_eq!(changes.len(), 2);
        assert!(changes[0].starts_with("~ s3 text:"));
        assert_eq!(changes[1], "~ s3 position: (340, 70) -> (42.5, -17.25)");
        assert_eq!(current, original);
        assert_eq!(baseline, snapshot());
    }

    #[test]
    fn malformed_snapshots_fail_instead_of_reporting_false_deletions() {
        let mut duplicate = snapshot();
        let node = record(&mut duplicate, "shape:s1").clone();
        duplicate["records"].as_array_mut().unwrap().push(node);
        let mut bad_coordinate = snapshot();
        record(&mut bad_coordinate, "shape:s1")["x"] = json!("0");
        let mut bad_binding = snapshot();
        record(&mut bad_binding, "binding:s5-s6-end")["props"]["terminal"] = json!("middle");
        let mut duplicate_terminal = snapshot();
        let mut binding = record(&mut duplicate_terminal, "binding:s5-s6-end").clone();
        binding["id"] = json!("binding:duplicate");
        duplicate_terminal["records"]
            .as_array_mut()
            .unwrap()
            .push(binding);
        for invalid in [
            Value::Null,
            json!({"records":{}}),
            json!({"records":[{}]}),
            duplicate,
            bad_coordinate,
            bad_binding,
            duplicate_terminal,
        ] {
            assert!(diff_snapshots(&snapshot(), &invalid).is_err());
            assert!(diff_snapshots(&invalid, &snapshot()).is_err());
        }
    }
}
