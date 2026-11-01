use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Deserialize, Serialize)]
pub struct Bounds {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

pub fn validate_plan(_plan: &Value) -> Result<(), String> {
    Err("GUZ-1129: short-form validation is not implemented".into())
}

pub fn expand_plan(_plan: &Value, _positions: &BTreeMap<String, Bounds>) -> Result<Value, String> {
    Err("GUZ-1129: tldraw snapshot expansion is not implemented".into())
}

pub fn layout_plan(
    _plan: &Value,
    _existing: &BTreeMap<String, Bounds>,
) -> Result<BTreeMap<String, Bounds>, String> {
    Err("GUZ-1130: deterministic incremental layout is not implemented".into())
}
