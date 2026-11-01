use std::collections::BTreeMap;

use mcptools_core::diagram::{expand_plan, layout_plan, validate_plan, Bounds};
use serde_json::{json, Value};

fn plan() -> Value {
    serde_json::from_str(include_str!(
        "../../../tests/fixtures/plan/spike.short.json"
    ))
    .expect("short-form fixture")
}

fn spike_positions() -> BTreeMap<String, Bounds> {
    serde_json::from_str(include_str!(
        "../../../tests/fixtures/plan/spike.positions.json"
    ))
    .expect("spike positions fixture")
}

#[test]
#[ignore = "GUZ-1129: short-form expansion into a valid tldraw 5.x snapshot"]
fn short_form_matches_spike_snapshot() {
    let expected: Value =
        serde_json::from_str(include_str!("../../../tests/fixtures/plan/spike.tldr"))
            .expect("spike snapshot");
    let actual = expand_plan(&plan(), &spike_positions()).expect("expand valid plan");
    assert_eq!(actual, expected);
}

#[test]
#[ignore = "GUZ-1129: accept valid short-form plans"]
fn valid_short_form_is_accepted() {
    validate_plan(&plan()).expect("valid spike plan");
}

#[test]
#[ignore = "GUZ-1129: reject duplicate node ids with a clear error"]
fn duplicate_ids_are_rejected() {
    let input =
        json!({"id":"bad", "nodes":[{"id":"a","text":"One"},{"id":"a","text":"Two"}],"edges":[]});
    let error = validate_plan(&input).expect_err("duplicate ids must fail");
    assert!(
        error.contains("duplicate") && error.contains('a'),
        "{error}"
    );
}

#[test]
#[ignore = "GUZ-1129: reject edges with unknown endpoints with a clear error"]
fn unknown_edge_endpoints_are_rejected() {
    for edge in [
        json!({"from":"a","to":"missing"}),
        json!({"from":"missing","to":"a"}),
    ] {
        let input = json!({"id":"bad","nodes":[{"id":"a","text":"One"}],"edges":[edge]});
        let error = validate_plan(&input).expect_err("unknown endpoint must fail");
        assert!(
            error.contains("unknown") && error.contains("missing"),
            "{error}"
        );
    }
}

#[test]
#[ignore = "GUZ-1129: reject empty node text with a clear error"]
fn empty_text_is_rejected() {
    for text in ["", " \t\n"] {
        let input = json!({"id":"bad","nodes":[{"id":"a","text":text}],"edges":[]});
        let error = validate_plan(&input).expect_err("empty text must fail");
        assert!(error.contains("text") && error.contains('a'), "{error}");
    }
}

fn laid_out_snapshot(input: &Value) -> Vec<u8> {
    let positions = layout_plan(input, &BTreeMap::new()).expect("layout valid plan");
    serde_json::to_vec(&expand_plan(input, &positions).expect("expand laid-out plan"))
        .expect("serialize snapshot")
}

#[test]
#[ignore = "GUZ-1130: same short form produces byte-identical laid-out snapshots"]
fn layout_is_byte_deterministic() {
    assert_eq!(laid_out_snapshot(&plan()), laid_out_snapshot(&plan()));
}

#[test]
#[ignore = "GUZ-1130: laid-out nodes have positive bounds and never overlap"]
fn layout_nodes_do_not_overlap() {
    let input = plan();
    let positions = layout_plan(&input, &BTreeMap::new()).expect("layout valid plan");
    assert_eq!(positions.len(), input["nodes"].as_array().unwrap().len());
    for node in input["nodes"].as_array().unwrap() {
        assert!(positions.contains_key(node["id"].as_str().unwrap()));
    }
    let nodes: Vec<_> = positions.values().collect();
    for (i, a) in nodes.iter().enumerate() {
        assert!(a.x.is_finite() && a.y.is_finite() && a.w > 0.0 && a.h > 0.0);
        for b in &nodes[i + 1..] {
            assert!(
                a.x + a.w <= b.x || b.x + b.w <= a.x || a.y + a.h <= b.y || b.y + b.h <= a.y,
                "overlap: {a:?} and {b:?}"
            );
        }
    }
}

#[test]
#[ignore = "GUZ-1130: updates preserve existing node positions"]
fn updates_keep_existing_positions() {
    let existing = spike_positions();
    let mut updated = plan();
    updated["nodes"][0]["text"] = json!("Changed text");
    updated["nodes"].as_array_mut().unwrap().reverse();
    updated["nodes"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"s7","text":"New step"}));
    updated["edges"]
        .as_array_mut()
        .unwrap()
        .push(json!({"from":"s6","to":"s7"}));
    let positions = layout_plan(&updated, &existing).expect("layout update");
    assert_eq!(positions.len(), 7);
    for (id, before) in existing {
        let after = positions.get(&id).expect("existing node retained");
        assert_eq!((after.x, after.y), (before.x, before.y), "moved {id}");
    }
    assert!(positions.contains_key("s7"));
}
