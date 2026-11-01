use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, Clone, Copy, PartialEq, Deserialize, Serialize)]
pub struct Bounds {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

#[derive(Deserialize)]
struct ShortPlan {
    id: String,
    nodes: Vec<Node>,
    edges: Vec<Edge>,
}

#[derive(Deserialize)]
struct Node {
    id: String,
    text: String,
}

#[derive(Deserialize)]
struct Edge {
    from: String,
    to: String,
}

fn parse_plan(plan: &Value) -> Result<ShortPlan, String> {
    let plan: ShortPlan = serde_json::from_value(plan.clone())
        .map_err(|error| format!("invalid short-form plan: {error}"))?;
    if plan.id.trim().is_empty() {
        return Err("plan id must not be empty".into());
    }
    let mut ids = BTreeSet::new();
    for node in &plan.nodes {
        if node.id.trim().is_empty() {
            return Err("node id must not be empty".into());
        }
        if !ids.insert(node.id.as_str()) {
            return Err(format!("duplicate node id: {}", node.id));
        }
        if node.text.trim().is_empty() {
            return Err(format!("empty text for node: {}", node.id));
        }
    }
    for edge in &plan.edges {
        for endpoint in [&edge.from, &edge.to] {
            if !ids.contains(endpoint.as_str()) {
                return Err(format!("unknown edge endpoint: {endpoint}"));
            }
        }
    }
    Ok(plan)
}

pub fn validate_plan(plan: &Value) -> Result<(), String> {
    parse_plan(plan).map(|_| ())
}

pub fn expand_plan(plan: &Value, positions: &BTreeMap<String, Bounds>) -> Result<Value, String> {
    let plan = parse_plan(plan)?;
    for node in &plan.nodes {
        let bounds = positions
            .get(&node.id)
            .ok_or_else(|| format!("missing position for node: {}", node.id))?;
        if ![bounds.x, bounds.y, bounds.w, bounds.h]
            .into_iter()
            .all(f64::is_finite)
            || bounds.w <= 0.0
            || bounds.h <= 0.0
        {
            return Err(format!("invalid bounds for node: {}", node.id));
        }
    }
    let mut records = vec![
        json!({"id":"document:document","typeName":"document","gridSize":10,"name":"","meta":{}}),
        json!({"id":"page:page","typeName":"page","name":"Plan","index":"a1","meta":{}}),
    ];
    for (i, node) in plan.nodes.iter().enumerate() {
        let bounds = positions[&node.id];
        let props = json!({
            "w":number(bounds.w)?,"h":number(bounds.h)?,"geo":"rectangle",
            "dash":"draw","growY":0,"flipX":false,"flipY":false,"url":"",
            "scale":1,"color":"blue","labelColor":"black","fill":"semi",
            "size":"m","font":"draw","align":"middle","verticalAlign":"middle",
            "richText":rich_text(&node.text)
        });
        records.push(shape(&node.id, "geo", i + 1, bounds.x, bounds.y, props)?);
    }
    let mut ids: BTreeSet<_> = plan.nodes.iter().map(|node| node.id.clone()).collect();
    for (i, edge) in plan.edges.iter().enumerate() {
        let mut id = format!("{}-{}", edge.from, edge.to);
        while !ids.insert(id.clone()) {
            id.push('~');
        }
        let from = positions[&edge.from];
        let to = positions[&edge.to];
        let x = from.x + from.w;
        let y = from.y + from.h / 2.0;
        let props = json!({
            "kind":"arc","labelColor":"black","color":"black","fill":"none",
            "dash":"draw","size":"m","arrowheadStart":"none","arrowheadEnd":"arrow",
            "font":"draw","start":{"x":0,"y":0},
            "end":{"x":number(to.x - x)?,"y":number(to.y + to.h / 2.0 - y)?},
            "bend":0,"richText":rich_text(""),"labelPosition":0.5,"scale":1,"elbowMidPoint":0.5
        });
        records.push(shape(&id, "arrow", plan.nodes.len() + i + 1, x, y, props)?);
        for (terminal, target) in [("start", &edge.from), ("end", &edge.to)] {
            records.push(json!({
                "id":format!("binding:{id}-{terminal}"),"typeName":"binding","type":"arrow",
                "fromId":format!("shape:{id}"),"toId":format!("shape:{target}"),"meta":{},
                "props":{"terminal":terminal,"normalizedAnchor":{"x":0.5,"y":0.5},
                    "isExact":false,"isPrecise":false,"snap":"none"}
            }));
        }
    }
    Ok(json!({"tldrawFileFormatVersion":1,"schema":schema(),"records":records}))
}

fn number(value: f64) -> Result<Value, String> {
    serde_json::from_str(&value.to_string())
        .map_err(|_| "non-finite coordinate in expanded plan".into())
}

fn rich_text(text: &str) -> Value {
    let content = if text.is_empty() {
        vec![]
    } else {
        vec![json!({"type":"text","text":text})]
    };
    json!({"type":"doc","content":[{"type":"paragraph","content":content}]})
}

fn shape(
    id: &str,
    kind: &str,
    index: usize,
    x: f64,
    y: f64,
    props: Value,
) -> Result<Value, String> {
    Ok(json!({
        "id":format!("shape:{id}"),"typeName":"shape","type":kind,"parentId":"page:page",
        "index":fractional_index(index),"isLocked":false,"opacity":1,"meta":{},
        "x":number(x)?,"y":number(y)?,"rotation":0,"props":props
    }))
}

fn fractional_index(mut ordinal: usize) -> String {
    let digits = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";
    let mut suffix = Vec::new();
    loop {
        suffix.push(digits[ordinal % digits.len()]);
        ordinal /= digits.len();
        if ordinal == 0 {
            break;
        }
    }
    let mut index = String::from(char::from(b'a' + (suffix.len() - 1) as u8));
    index.extend(suffix.into_iter().rev().map(char::from));
    index
}

fn schema() -> Value {
    json!({"schemaVersion":2,"sequences":{
        "com.tldraw.store":5,"com.tldraw.asset":1,"com.tldraw.camera":1,
        "com.tldraw.document":2,"com.tldraw.instance":26,"com.tldraw.instance_page_state":5,
        "com.tldraw.page":1,"com.tldraw.instance_presence":6,"com.tldraw.pointer":1,
        "com.tldraw.shape":4,"com.tldraw.user":1,"com.tldraw.asset.image":6,
        "com.tldraw.asset.video":5,"com.tldraw.asset.bookmark":2,"com.tldraw.shape.arrow":8,
        "com.tldraw.shape.bookmark":2,"com.tldraw.shape.draw":5,"com.tldraw.shape.embed":4,
        "com.tldraw.shape.frame":1,"com.tldraw.shape.geo":12,"com.tldraw.shape.group":0,
        "com.tldraw.shape.highlight":4,"com.tldraw.shape.image":5,"com.tldraw.shape.line":5,
        "com.tldraw.shape.note":13,"com.tldraw.shape.text":4,"com.tldraw.shape.video":4,
        "com.tldraw.binding.arrow":1
    }})
}

pub fn layout_plan(
    plan: &Value,
    existing: &BTreeMap<String, Bounds>,
) -> Result<BTreeMap<String, Bounds>, String> {
    let plan = parse_plan(plan)?;
    let layers = dependency_layers(&plan);
    let mut positions = BTreeMap::new();
    for id in layers.keys() {
        if let Some(&bounds) = existing.get(*id) {
            if ![
                bounds.x,
                bounds.y,
                bounds.w,
                bounds.h,
                bounds.x + bounds.w,
                bounds.y + bounds.h,
            ]
            .into_iter()
            .all(f64::is_finite)
                || bounds.w <= 0.0
                || bounds.h <= 0.0
                || bounds.x + bounds.w <= bounds.x
                || bounds.y + bounds.h <= bounds.y
            {
                return Err(format!("invalid bounds for node: {id}"));
            }
            for (other_id, other) in &positions {
                if bounds_overlap(&bounds, other) {
                    return Err(format!("existing nodes overlap: {other_id} and {id}"));
                }
            }
            positions.insert((*id).to_owned(), bounds);
        }
    }
    let mut ordered: Vec<_> = layers.into_iter().map(|(id, layer)| (layer, id)).collect();
    ordered.sort_unstable();
    for (layer, id) in ordered {
        if positions.contains_key(id) {
            continue;
        }
        let mut bounds = Bounds {
            x: layer as f64 * 340.0,
            y: 0.0,
            w: 260.0,
            h: 80.0,
        };
        loop {
            let next_y = positions
                .values()
                .filter(|other| bounds_overlap(&bounds, other))
                .map(|other| other.y + other.h + 60.0)
                .reduce(f64::max);
            let Some(next_y) = next_y else {
                break;
            };
            if !next_y.is_finite() || next_y <= bounds.y || next_y + bounds.h <= next_y {
                return Err(format!("layout coordinate overflow for node: {id}"));
            }
            bounds.y = next_y;
        }
        positions.insert(id.to_owned(), bounds);
    }
    Ok(positions)
}

fn bounds_overlap(a: &Bounds, b: &Bounds) -> bool {
    a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h
}

fn dependency_layers(plan: &ShortPlan) -> BTreeMap<&str, usize> {
    let mut outgoing: BTreeMap<_, BTreeSet<_>> = plan
        .nodes
        .iter()
        .map(|node| (node.id.as_str(), BTreeSet::new()))
        .collect();
    for edge in &plan.edges {
        outgoing
            .get_mut(edge.from.as_str())
            .unwrap()
            .insert(edge.to.as_str());
    }
    let mut visited = BTreeSet::new();
    let mut finished = Vec::new();
    for &root in outgoing.keys() {
        let mut stack = vec![(root, false)];
        while let Some((id, exiting)) = stack.pop() {
            if exiting {
                finished.push(id);
            } else if visited.insert(id) {
                stack.push((id, true));
                stack.extend(outgoing[id].iter().rev().map(|&next| (next, false)));
            }
        }
    }
    let mut layers = BTreeMap::new();
    let mut placed = BTreeSet::new();
    for id in finished.into_iter().rev() {
        let layer = *layers.entry(id).or_insert(0);
        placed.insert(id);
        for &next in &outgoing[id] {
            if !placed.contains(next) {
                let next_layer = layers.entry(next).or_insert(0);
                *next_layer = (*next_layer).max(layer + 1);
            }
        }
    }
    layers
}

#[cfg(test)]
mod tests {
    use super::*;

    fn single_node() -> Value {
        json!({"id":"plan","nodes":[{"id":"a","text":"  café\n次  "}],"edges":[]})
    }

    fn positions() -> BTreeMap<String, Bounds> {
        BTreeMap::from([(
            "a".into(),
            Bounds {
                x: -1.25,
                y: 2.5,
                w: 10.0,
                h: 5.0,
            },
        )])
    }

    fn assert_no_layout_overlap(positions: &BTreeMap<String, Bounds>) {
        let bounds: Vec<_> = positions.values().collect();
        for (index, a) in bounds.iter().enumerate() {
            assert!(a.x.is_finite() && a.y.is_finite());
            assert!(a.w.is_finite() && a.h.is_finite() && a.w > 0.0 && a.h > 0.0);
            for b in &bounds[index + 1..] {
                assert!(
                    a.x + a.w <= b.x || b.x + b.w <= a.x || a.y + a.h <= b.y || b.y + b.h <= a.y,
                    "overlap: {a:?} and {b:?}"
                );
            }
        }
    }

    #[test]
    fn layout_uses_longest_dependency_layers_independent_of_input_order() {
        let mut input = json!({"id":"layers","nodes":[
            {"id":"d","text":"D"},{"id":"c","text":"C"},
            {"id":"b","text":"B"},{"id":"a","text":"A"}
        ],"edges":[{"from":"a","to":"d"},{"from":"a","to":"b"},
            {"from":"b","to":"c"},{"from":"c","to":"d"},
            {"from":"a","to":"b"}]});
        let positions = layout_plan(&input, &BTreeMap::new()).unwrap();
        for (from, to) in [("a", "b"), ("b", "c"), ("c", "d")] {
            assert!(positions[from].x + positions[from].w < positions[to].x);
        }
        input["nodes"].as_array_mut().unwrap().reverse();
        input["edges"].as_array_mut().unwrap().reverse();
        assert_eq!(positions, layout_plan(&input, &BTreeMap::new()).unwrap());
        assert_no_layout_overlap(&positions);
    }

    #[test]
    fn cyclic_layout_is_deterministic_and_non_overlapping() {
        let mut input = json!({"id":"cycles","nodes":[
            {"id":"a","text":"A"},{"id":"b","text":"B"},
            {"id":"0","text":"Downstream"},{"id":"self","text":"Self"}
        ],"edges":[{"from":"a","to":"b"},{"from":"b","to":"a"},
            {"from":"b","to":"0"},{"from":"self","to":"self"}]});
        let positions = layout_plan(&input, &BTreeMap::new()).unwrap();
        assert_eq!(positions.len(), 4);
        assert!(positions["b"].x + positions["b"].w < positions["0"].x);
        assert_no_layout_overlap(&positions);
        input["nodes"].as_array_mut().unwrap().reverse();
        input["edges"].as_array_mut().unwrap().reverse();
        assert_eq!(
            serde_json::to_vec(&positions).unwrap(),
            serde_json::to_vec(&layout_plan(&input, &BTreeMap::new()).unwrap()).unwrap()
        );
    }

    #[test]
    fn disconnected_nodes_share_a_layer_without_overlapping() {
        let input = json!({"id":"disconnected","nodes":[
            {"id":"c","text":"C"},{"id":"a","text":"A"},{"id":"b","text":"B"}
        ],"edges":[]});
        let positions = layout_plan(&input, &BTreeMap::new()).unwrap();
        assert_eq!(positions.len(), 3);
        assert_eq!(positions["a"].x, positions["b"].x);
        assert_eq!(positions["b"].x, positions["c"].x);
        assert!(positions["a"].y < positions["b"].y);
        assert!(positions["b"].y < positions["c"].y);
        assert_no_layout_overlap(&positions);
    }

    #[test]
    fn empty_layout_discards_previous_positions() {
        let input = json!({"id":"empty","nodes":[],"edges":[]});
        assert!(layout_plan(&input, &positions()).unwrap().is_empty());
    }

    #[test]
    fn new_nodes_avoid_all_preserved_bounds_and_removed_nodes_are_dropped() {
        let input = json!({"id":"update","nodes":[
            {"id":"a","text":"Changed"},{"id":"b","text":"B"},{"id":"new","text":"New"}
        ],"edges":[{"from":"a","to":"new"}]});
        let existing = BTreeMap::from([
            (
                "a".into(),
                Bounds {
                    x: -10.5,
                    y: -20.25,
                    w: 1000.0,
                    h: 300.0,
                },
            ),
            (
                "b".into(),
                Bounds {
                    x: 300.0,
                    y: 300.0,
                    w: 900.0,
                    h: 500.0,
                },
            ),
            (
                "removed".into(),
                Bounds {
                    x: 0.0,
                    y: 0.0,
                    w: 10.0,
                    h: 10.0,
                },
            ),
        ]);
        let positions = layout_plan(&input, &existing).unwrap();
        assert_eq!(positions.len(), 3);
        for id in ["a", "b"] {
            assert_eq!(positions[id], existing[id]);
        }
        assert!(!positions.contains_key("removed"));
        assert_no_layout_overlap(&positions);
        assert_eq!(positions, layout_plan(&input, &positions).unwrap());
    }

    #[test]
    fn layout_reuses_short_form_validation() {
        for input in [
            Value::Null,
            json!({"id":"plan","nodes":[{"id":"a","text":""}],"edges":[]}),
            json!({"id":"plan","nodes":[],"edges":[{"from":"a","to":"b"}]}),
        ] {
            assert_eq!(
                layout_plan(&input, &BTreeMap::new()).unwrap_err(),
                validate_plan(&input).unwrap_err()
            );
        }
    }

    #[test]
    fn layout_rejects_invalid_or_unrepresentable_preserved_bounds() {
        for bounds in [
            Bounds {
                x: f64::NAN,
                y: 0.0,
                w: 1.0,
                h: 1.0,
            },
            Bounds {
                x: 0.0,
                y: 0.0,
                w: 0.0,
                h: 1.0,
            },
            Bounds {
                x: 0.0,
                y: 0.0,
                w: 1.0,
                h: -1.0,
            },
            Bounds {
                x: f64::MAX,
                y: 0.0,
                w: f64::MAX,
                h: 1.0,
            },
            Bounds {
                x: 0.0,
                y: f64::MAX,
                w: 1.0,
                h: 1.0,
            },
        ] {
            let existing = BTreeMap::from([("a".into(), bounds)]);
            assert!(layout_plan(&single_node(), &existing)
                .unwrap_err()
                .contains("invalid bounds for node: a"));
        }
    }

    #[test]
    fn overlapping_preserved_nodes_are_rejected_instead_of_moved() {
        let input = json!({"id":"overlap","nodes":[
            {"id":"a","text":"A"},{"id":"b","text":"B"}
        ],"edges":[]});
        let bounds = positions()["a"];
        let existing = BTreeMap::from([("a".into(), bounds), ("b".into(), bounds)]);
        assert!(layout_plan(&input, &existing)
            .unwrap_err()
            .contains("overlap"));
    }

    #[test]
    fn malformed_short_forms_are_rejected() {
        for input in [
            Value::Null,
            json!({}),
            json!({"id":1,"nodes":[],"edges":[]}),
            json!({"id":"plan","nodes":{},"edges":[]}),
            json!({"id":"plan","nodes":[{"id":"a"}],"edges":[]}),
            json!({"id":"plan","nodes":[{"id":"a","text":1}],"edges":[]}),
            json!({"id":"plan","nodes":[],"edges":[{"from":"a"}]}),
        ] {
            assert!(validate_plan(&input)
                .unwrap_err()
                .contains("invalid short-form"));
        }
    }

    #[test]
    fn blank_ids_and_unicode_whitespace_text_are_rejected() {
        for id in ["", " \t\n", "\u{2003}"] {
            let mut input = single_node();
            input["id"] = json!(id);
            assert!(validate_plan(&input).unwrap_err().contains("plan id"));
            let mut input = single_node();
            input["nodes"][0]["id"] = json!(id);
            assert!(validate_plan(&input).unwrap_err().contains("node id"));
        }
        let mut input = single_node();
        input["nodes"][0]["text"] = json!("\u{2003}\u{a0}");
        assert!(validate_plan(&input).unwrap_err().contains("empty text"));
    }

    #[test]
    fn expansion_validates_before_using_positions() {
        let mut input = single_node();
        input["nodes"][0]["text"] = json!("");
        assert!(expand_plan(&input, &BTreeMap::new())
            .unwrap_err()
            .contains("empty text"));
        assert!(expand_plan(&single_node(), &BTreeMap::new())
            .unwrap_err()
            .contains("missing position for node: a"));
    }

    #[test]
    fn invalid_bounds_are_rejected() {
        for field in ["x", "y", "w", "h"] {
            for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
                let mut bounds = positions();
                let bounds_a = bounds.get_mut("a").unwrap();
                match field {
                    "x" => bounds_a.x = value,
                    "y" => bounds_a.y = value,
                    "w" => bounds_a.w = value,
                    _ => bounds_a.h = value,
                }
                assert!(expand_plan(&single_node(), &bounds)
                    .unwrap_err()
                    .contains("invalid bounds for node: a"));
            }
        }
        for value in [0.0, -1.0] {
            let mut bounds = positions();
            bounds.get_mut("a").unwrap().w = value;
            assert!(expand_plan(&single_node(), &bounds).is_err());
            bounds = positions();
            bounds.get_mut("a").unwrap().h = value;
            assert!(expand_plan(&single_node(), &bounds).is_err());
        }
    }

    #[test]
    fn rich_text_and_fractional_positions_are_preserved() {
        let snapshot = expand_plan(&single_node(), &positions()).unwrap();
        let node = &snapshot["records"][2];
        assert_eq!(node["x"], json!(-1.25));
        assert_eq!(node["y"], json!(2.5));
        assert_eq!(node["props"]["w"], json!(10));
        assert_eq!(node["props"]["flipX"], false);
        assert_eq!(node["props"]["flipY"], false);
        assert_eq!(
            node["props"]["richText"]["content"][0]["content"][0]["text"],
            single_node()["nodes"][0]["text"]
        );
    }

    #[test]
    fn empty_plan_has_only_document_and_page() {
        let input = json!({"id":"empty","nodes":[],"edges":[]});
        validate_plan(&input).unwrap();
        let snapshot = expand_plan(&input, &BTreeMap::new()).unwrap();
        assert_eq!(snapshot["records"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn duplicate_edges_and_colliding_ids_keep_all_records_and_bindings() {
        let input = json!({"id":"collisions","nodes":[
            {"id":"a","text":"A"},{"id":"b","text":"B"},
            {"id":"a-b","text":"C"},{"id":"a-b~","text":"D"}
        ],"edges":[{"from":"a","to":"b"},{"from":"a","to":"b"},
            {"from":"a","to":"a"},{"from":"b","to":"a"}]});
        let bounds = positions()["a"];
        let positions = ["a", "b", "a-b", "a-b~"]
            .into_iter()
            .map(|id| (id.to_owned(), bounds))
            .collect();
        let snapshot = expand_plan(&input, &positions).unwrap();
        let records = snapshot["records"].as_array().unwrap();
        assert_eq!(records.len(), 18);
        let ids: BTreeSet<_> = records
            .iter()
            .map(|record| record["id"].as_str().unwrap())
            .collect();
        assert_eq!(ids.len(), records.len());
        for binding in records
            .iter()
            .filter(|record| record["typeName"] == "binding")
        {
            assert!(ids.contains(binding["fromId"].as_str().unwrap()));
            assert!(ids.contains(binding["toId"].as_str().unwrap()));
        }
        assert_eq!(records[6]["id"], "shape:a-b~~");
        assert_eq!(records[9]["id"], "shape:a-b~~~");
        assert_eq!(records[6]["x"], json!(8.75));
        assert_eq!(records[6]["props"]["end"], json!({"x":-10,"y":0}));
        assert_eq!(
            records[6]["props"]["richText"],
            json!({"type":"doc","content":[{"type":"paragraph","content":[]}]})
        );
        assert_eq!(snapshot, expand_plan(&input, &positions).unwrap());
    }

    #[test]
    fn overflowing_arrow_coordinates_are_rejected() {
        let mut input = single_node();
        input["edges"] = json!([{"from":"a","to":"a"}]);
        let mut bounds = positions();
        bounds.get_mut("a").unwrap().x = f64::MAX;
        bounds.get_mut("a").unwrap().w = f64::MAX;
        assert!(expand_plan(&input, &bounds)
            .unwrap_err()
            .contains("non-finite coordinate"));
    }

    #[test]
    fn fractional_indexes_are_unique_ordered_and_have_valid_integer_lengths() {
        let mut previous = String::new();
        for ordinal in 1..10_000 {
            let index = fractional_index(ordinal);
            assert!(index > previous);
            assert_eq!(index.len(), usize::from(index.as_bytes()[0] - b'a') + 2);
            assert!(index[1..].bytes().all(|byte| byte.is_ascii_alphanumeric()));
            previous = index;
        }
        assert_eq!(fractional_index(1), "a1");
        assert_eq!(fractional_index(12), "aC");
        assert_eq!(fractional_index(61), "az");
        assert_eq!(fractional_index(62), "b10");
    }
}
