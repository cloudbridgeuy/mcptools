use std::collections::{BTreeMap, BTreeSet};

use base64::{engine::general_purpose::STANDARD, Engine};
use serde_json::Value;

type Point = (f64, f64);
type Paths = Vec<Vec<Point>>;

#[derive(Clone, Copy, Debug)]
struct Bounds {
    min_x: f64,
    min_y: f64,
    max_x: f64,
    max_y: f64,
}

impl Bounds {
    fn point(x: f64, y: f64) -> Self {
        Self {
            min_x: x,
            min_y: y,
            max_x: x,
            max_y: y,
        }
    }

    fn include(&mut self, other: Self) {
        self.min_x = self.min_x.min(other.min_x);
        self.min_y = self.min_y.min(other.min_y);
        self.max_x = self.max_x.max(other.max_x);
        self.max_y = self.max_y.max(other.max_y);
    }

    fn distance(self, other: Self) -> f64 {
        let dx = (self.min_x - other.max_x)
            .max(other.min_x - self.max_x)
            .max(0.0);
        let dy = (self.min_y - other.max_y)
            .max(other.min_y - self.max_y)
            .max(0.0);
        dx.hypot(dy)
    }

    fn relation(self, other: Self) -> &'static str {
        if self.min_x >= other.min_x
            && self.max_x <= other.max_x
            && self.min_y >= other.min_y
            && self.max_y <= other.max_y
        {
            "inside"
        } else if self.distance(other) == 0.0 {
            "crossing"
        } else {
            let dx = (self.min_x + self.max_x - other.min_x - other.max_x) / 2.0;
            let dy = (self.min_y + self.max_y - other.min_y - other.max_y) / 2.0;
            if (self.min_x - other.max_x).max(other.min_x - self.max_x)
                > (self.min_y - other.max_y).max(other.min_y - self.max_y)
            {
                if dx < 0.0 {
                    "left of"
                } else {
                    "right of"
                }
            } else if dy < 0.0 {
                "above"
            } else {
                "below"
            }
        }
    }

    fn display(self) -> String {
        format!(
            "bbox [{:.1}, {:.1}, {:.1}, {:.1}]",
            self.min_x,
            self.min_y,
            self.max_x - self.min_x,
            self.max_y - self.min_y
        )
    }
}

struct Shape<'a> {
    id: &'a str,
    kind: &'a str,
    text: String,
    x: f64,
    y: f64,
    bounds: Bounds,
    paths: Paths,
    endpoints: Option<(&'a str, &'a str)>,
}

impl Shape<'_> {
    fn name(&self) -> String {
        if self.text.is_empty() {
            self.id.to_string()
        } else {
            Value::String(self.text.clone()).to_string()
        }
    }

    fn description(&self, shapes: &[Shape<'_>]) -> String {
        if let Some((start, end)) = self.endpoints {
            let name = |id: &str| {
                shapes
                    .iter()
                    .find(|shape| shape.id == id)
                    .map_or_else(|| id.to_string(), Shape::name)
            };
            let arrow = format!("{} → {}", name(start), name(end));
            if self.text.is_empty() {
                format!("arrow {arrow}")
            } else {
                format!("arrow {} ({arrow})", self.name())
            }
        } else {
            format!("{} {}", self.kind, self.name())
        }
    }
}

fn intersects(a: (f64, f64), b: (f64, f64), c: (f64, f64), d: (f64, f64)) -> bool {
    let cross = |p: (f64, f64), q: (f64, f64), r: (f64, f64)| {
        (q.0 - p.0) * (r.1 - p.1) - (q.1 - p.1) * (r.0 - p.0)
    };
    let overlap = |a: f64, b: f64, c: f64, d: f64| a.min(b) <= c.max(d) && c.min(d) <= a.max(b);
    overlap(a.0, b.0, c.0, d.0)
        && overlap(a.1, b.1, c.1, d.1)
        && cross(a, b, c) * cross(a, b, d) <= 0.0
        && cross(c, d, a) * cross(c, d, b) <= 0.0
}

fn anchor(bounds: Bounds, shapes: &[Shape<'_>], exclude: &str, ink: &[&Shape<'_>]) -> String {
    let mut excluded = BTreeSet::from([exclude]);
    if let Some((start, end)) = shapes
        .iter()
        .find(|shape| shape.id == exclude)
        .and_then(|shape| shape.endpoints)
    {
        excluded.extend([start, end]);
    }
    let labels: BTreeSet<_> = shapes
        .iter()
        .filter(|shape| !shape.text.trim().is_empty())
        .map(|shape| shape.id)
        .collect();
    let ink_edges: Vec<_> = ink.iter().flat_map(|stroke| edges(&stroke.paths)).collect();
    let crossing = shapes
        .iter()
        .filter(|shape| shape.kind == "arrow" && !excluded.contains(shape.id))
        .filter(|arrow| {
            labels.contains(arrow.id)
                || arrow
                    .endpoints
                    .is_some_and(|(start, end)| labels.contains(start) && labels.contains(end))
        })
        .filter(|arrow| {
            edges(&arrow.paths).any(|edge| {
                ink_edges
                    .iter()
                    .any(|segment| intersects(edge[0], edge[1], segment[0], segment[1]))
            })
        })
        .min_by_key(|arrow| arrow.id);
    if let Some(arrow) = crossing {
        return format!(" crossing {}", arrow.description(shapes));
    }
    let closest = shapes
        .iter()
        .filter(|shape| {
            !excluded.contains(shape.id)
                && matches!(shape.kind, "geo" | "text")
                && labels.contains(shape.id)
        })
        .min_by(|a, b| {
            bounds
                .distance(a.bounds)
                .total_cmp(&bounds.distance(b.bounds))
                .then_with(|| a.id.cmp(b.id))
        });
    closest.map_or_else(String::new, |shape| {
        format!(" {} {}", bounds.relation(shape.bounds), shape.name())
    })
}

fn edges(paths: &Paths) -> impl Iterator<Item = &[Point]> {
    paths.iter().flat_map(|path| path.windows(2))
}

fn root(parent: &mut [usize], mut index: usize) -> usize {
    while parent[index] != index {
        parent[index] = parent[parent[index]];
        index = parent[index];
    }
    index
}

fn ink_changes(sign: char, draws: &[&Shape<'_>], shapes: &[Shape<'_>]) -> Vec<String> {
    if draws.is_empty() {
        return Vec::new();
    }
    let mut heights: Vec<_> = draws
        .iter()
        .map(|s| s.bounds.max_y - s.bounds.min_y)
        .filter(|height| *height > 0.0)
        .collect();
    heights.sort_by(f64::total_cmp);
    let line_height = if heights.is_empty() {
        1.0
    } else {
        (heights[(heights.len() - 1) / 2] + heights[heights.len() / 2]) / 2.0
    };
    let mut parent: Vec<_> = (0..draws.len()).collect();
    for i in 0..draws.len() {
        for j in 0..i {
            let a = draws[i].bounds;
            let b = draws[j].bounds;
            let height = (a.max_y - a.min_y).max(b.max_y - b.min_y).max(line_height);
            let word_gap = (a.max_y - a.min_y).min(b.max_y - b.min_y).max(line_height) * 3.0;
            if a.max_y > a.min_y
                && b.max_y > b.min_y
                && a.min_x <= b.max_x + word_gap
                && b.min_x <= a.max_x + word_gap
                && (a.min_y + a.max_y - b.min_y - b.max_y).abs() / 2.0 <= height * 0.75
            {
                let left = root(&mut parent, i);
                let right = root(&mut parent, j);
                parent[left] = right;
            }
        }
    }
    for (i, dot) in draws
        .iter()
        .enumerate()
        .filter(|(_, shape)| shape.bounds.max_y == shape.bounds.min_y)
    {
        let closest = draws
            .iter()
            .enumerate()
            .filter(|(_, shape)| {
                shape.bounds.max_y > shape.bounds.min_y && shape.bounds.min_y >= dot.bounds.min_y
            })
            .min_by(|(_, a), (_, b)| {
                dot.bounds
                    .distance(a.bounds)
                    .total_cmp(&dot.bounds.distance(b.bounds))
                    .then_with(|| a.id.cmp(b.id))
            });
        if let Some((j, _)) = closest.filter(|(_, stroke)| {
            dot.bounds.distance(stroke.bounds)
                <= (stroke.bounds.max_y - stroke.bounds.min_y).max(line_height) * 2.0
        }) {
            let target = root(&mut parent, j);
            parent[i] = target;
        }
    }
    let mut groups = BTreeMap::<usize, (Bounds, Vec<&Shape<'_>>)>::new();
    for (i, shape) in draws.iter().enumerate() {
        let entry = groups
            .entry(root(&mut parent, i))
            .or_insert((shape.bounds, Vec::new()));
        entry.0.include(shape.bounds);
        entry.1.push(shape);
    }
    let mut groups: Vec<_> = groups.into_values().collect();
    groups.sort_by(|a, b| {
        a.0.min_y
            .total_cmp(&b.0.min_y)
            .then_with(|| a.0.min_x.total_cmp(&b.0.min_x))
            .then_with(|| a.0.max_y.total_cmp(&b.0.max_y))
            .then_with(|| a.0.max_x.total_cmp(&b.0.max_x))
    });
    groups
        .into_iter()
        .enumerate()
        .map(|(i, (bounds, strokes))| {
            format!(
                "{sign} ink #{} ({} stroke{}){} {}",
                i + 1,
                strokes.len(),
                if strokes.len() == 1 { "" } else { "s" },
                anchor(bounds, shapes, "", &strokes),
                bounds.display()
            )
        })
        .collect()
}

pub fn diff_snapshots(baseline: &Value, current: &Value) -> Result<Vec<String>, String> {
    let baseline = shapes(baseline)?;
    let current = shapes(current)?;
    let old: BTreeMap<_, _> = baseline.iter().map(|shape| (shape.id, shape)).collect();
    let new: BTreeMap<_, _> = current.iter().map(|shape| (shape.id, shape)).collect();
    let mut changes = Vec::new();
    for (sign, source, other) in [('+', &current, &old), ('-', &baseline, &new)] {
        let mut changed: Vec<_> = source
            .iter()
            .filter(|shape| !other.contains_key(shape.id))
            .collect();
        changed.sort_by(|a, b| {
            a.description(source)
                .cmp(&b.description(source))
                .then_with(|| a.id.cmp(b.id))
        });
        changes.extend(
            changed
                .iter()
                .filter(|shape| shape.kind != "draw")
                .map(|shape| {
                    format!(
                        "{sign} {}{}",
                        shape.description(source),
                        anchor(shape.bounds, source, shape.id, &[])
                    )
                }),
        );
        let draws: Vec<_> = changed
            .into_iter()
            .filter(|shape| shape.kind == "draw")
            .collect();
        changes.extend(ink_changes(sign, &draws, source));
    }
    for before in old.values() {
        let Some(after) = new.get(before.id) else {
            continue;
        };
        let name = before.description(&baseline);
        let ink = if after.kind == "draw" {
            std::slice::from_ref(after)
        } else {
            &[]
        };
        if before.text != after.text {
            changes.push(format!(
                "~ {name} text: {} → {}{}",
                Value::String(before.text.clone()),
                Value::String(after.text.clone()),
                anchor(after.bounds, &current, after.id, ink)
            ));
        }
        if before.endpoints != after.endpoints {
            changes.push(format!(
                "~ {name} endpoints: {} → {}{}",
                before.description(&baseline),
                after.description(&current),
                anchor(after.bounds, &current, after.id, &[])
            ));
        }
        if before.x != after.x || before.y != after.y {
            changes.push(format!(
                "~ {name} position: ({}, {}) → ({}, {}){}",
                before.x,
                before.y,
                after.x,
                after.y,
                anchor(after.bounds, &current, after.id, ink)
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

fn half(bytes: &[u8]) -> f64 {
    let bits = u16::from_le_bytes([bytes[0], bytes[1]]);
    let sign = if bits & 0x8000 == 0 { 1.0 } else { -1.0 };
    let exponent = (bits >> 10) & 31;
    let fraction = bits & 1023;
    match exponent {
        0 => sign * f64::from(fraction) * 2_f64.powi(-24),
        31 => f64::NAN,
        _ => sign * (1.0 + f64::from(fraction) / 1024.0) * 2_f64.powi(i32::from(exponent) - 15),
    }
}

fn decode_path(segment: &Value) -> Result<Vec<Point>, String> {
    if let Some(points) = segment["points"].as_array() {
        return points
            .iter()
            .map(|point| {
                Ok((
                    point["x"].as_f64().ok_or("invalid draw point")?,
                    point["y"].as_f64().ok_or("invalid draw point")?,
                ))
            })
            .collect();
    }
    let path = segment["path"].as_str().ok_or("missing draw path")?;
    let bytes = STANDARD.decode(path).map_err(|_| "invalid draw path")?;
    let dim = match segment["dim"].as_u64() {
        None | Some(3) => 3,
        Some(2) => 2,
        _ => return Err("invalid draw dimension".into()),
    };
    if bytes.is_empty() {
        return Ok(Vec::new());
    }
    let first = dim * 4;
    let stride = dim * 2;
    if bytes.len() < first || (bytes.len() - first) % stride != 0 {
        return Err("invalid draw path length".into());
    }
    let mut x = f64::from(f32::from_le_bytes(
        bytes[0..4].try_into().map_err(|_| "invalid draw path")?,
    ));
    let mut y = f64::from(f32::from_le_bytes(
        bytes[4..8].try_into().map_err(|_| "invalid draw path")?,
    ));
    let mut points = vec![(x, y)];
    for delta in bytes[first..].chunks_exact(stride) {
        x += half(&delta[0..2]);
        y += half(&delta[2..4]);
        points.push((x, y));
    }
    Ok(points)
}

fn geometry(record: &Value, kind: &str, x: f64, y: f64) -> Result<(Bounds, Paths), String> {
    let props = &record["props"];
    let rotation = record["rotation"].as_f64().unwrap_or(0.0);
    let sx = props["scaleX"].as_f64().unwrap_or(1.0);
    let sy = props["scaleY"].as_f64().unwrap_or(1.0);
    if !rotation.is_finite() || !sx.is_finite() || !sy.is_finite() {
        return Err("invalid shape transform".into());
    }
    let paths = if kind == "draw" {
        props["segments"].as_array().map_or_else(
            || Ok(Vec::new()),
            |segments| {
                segments
                    .iter()
                    .map(decode_path)
                    .collect::<Result<Vec<_>, _>>()
            },
        )?
    } else if kind == "arrow" {
        vec![["start", "end"]
            .iter()
            .filter_map(|terminal| {
                Some((
                    props[terminal]["x"].as_f64()?,
                    props[terminal]["y"].as_f64()?,
                ))
            })
            .collect()]
    } else {
        let width = props["w"].as_f64().unwrap_or(0.0);
        let height = props["h"].as_f64().unwrap_or(0.0) + props["growY"].as_f64().unwrap_or(0.0);
        vec![vec![
            (0.0, 0.0),
            (width, 0.0),
            (0.0, height),
            (width, height),
        ]]
    };
    let (sin, cos) = rotation.sin_cos();
    let paths = paths
        .into_iter()
        .map(|path| {
            path.into_iter()
                .map(|(px, py)| {
                    let (px, py) = if kind == "draw" {
                        (px * sx, py * sy)
                    } else {
                        (px, py)
                    };
                    let point = (x + px * cos - py * sin, y + px * sin + py * cos);
                    if !point.0.is_finite() || !point.1.is_finite() {
                        return Err("invalid shape geometry".into());
                    }
                    Ok(point)
                })
                .collect::<Result<Vec<_>, String>>()
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut bounds = None::<Bounds>;
    for &(px, py) in paths.iter().flatten() {
        let point = Bounds::point(px, py);
        if let Some(bounds) = &mut bounds {
            bounds.include(point);
        } else {
            bounds = Some(point);
        }
    }
    let paths = if kind == "arrow" && props["bend"].as_f64().unwrap_or(0.0) != 0.0 {
        Vec::new()
    } else {
        paths
    };
    Ok((bounds.unwrap_or(Bounds::point(x, y)), paths))
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
            let rich_text = props.get("richText").map(plain_text).unwrap_or_default();
            let text = if rich_text.trim().is_empty() {
                props
                    .get("text")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .into()
            } else {
                rich_text
            };
            let endpoints = (kind == "arrow").then(|| {
                (
                    bindings.get(&(id, "start")).copied().unwrap_or("?"),
                    bindings.get(&(id, "end")).copied().unwrap_or("?"),
                )
            });
            let x = coordinate("x")?;
            let y = coordinate("y")?;
            let (bounds, paths) = geometry(record, kind, x, y)?;
            Ok(Shape {
                id,
                kind,
                text,
                x,
                y,
                bounds,
                paths,
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
    fn ink_fixture_clusters_and_labels() {
        let mut current: Value = serde_json::from_str(include_str!(
            "../../../tests/fixtures/plan/ink-cluster/plan.tldr"
        ))
        .unwrap();
        let mut baseline: Value = serde_json::from_str(include_str!(
            "../../../tests/fixtures/plan/ink-cluster/baseline.tldr"
        ))
        .unwrap();
        let records = baseline["records"].as_array().unwrap();
        let added_box = records
            .iter()
            .find(|record| plain_text(&record["props"]["richText"]) == "This is a new box")
            .unwrap()["id"]
            .as_str()
            .unwrap();
        let added_arrow = records
            .iter()
            .find(|record| record["typeName"] == "binding" && record["toId"] == added_box)
            .unwrap()["fromId"]
            .as_str()
            .unwrap();
        let added_box = added_box.to_owned();
        let added_arrow = added_arrow.to_owned();
        baseline["records"]
            .as_array_mut()
            .unwrap()
            .retain(|record| {
                record["id"] != added_box
                    && record["id"] != added_arrow
                    && record["type"] != "draw"
                    && record["fromId"] != added_arrow
                    && plain_text(&record["props"]["richText"]) != "But connecting stuff is easy."
            });
        let output = render_diff(&diff_snapshots(&baseline, &current).unwrap());
        assert_eq!(
            output,
            [
                "+ arrow \"This is a new box\" → \"Diff in chat\" left of \"But connecting stuff is easy.\"",
                "+ geo \"This is a new box\" below \"But connecting stuff is easy.\"",
                "+ text \"But connecting stuff is easy.\" below \"Diff in chat\"",
                "+ ink #1 (13 strokes) above \"Phone edit\" bbox [-89.0, -913.5, 1083.5, 232.1]",
                "+ ink #2 (11 strokes) above \"Phone edit\" bbox [-55.6, -640.1, 1144.9, 205.3]",
                "+ ink #3 (10 strokes) above \"Phone edit\" bbox [-72.0, -453.7, 309.0, 113.5]",
                "+ ink #4 (7 strokes) above \"Phone edit\" bbox [-36.1, -315.4, 291.1, 87.9]",
                "+ ink #5 (9 strokes) above \"Phone edit\" bbox [8.7, -201.4, 292.4, 84.1]",
                "+ ink #6 (7 strokes) below \"Phone edit\" bbox [-2.9, 142.2, 279.6, 146.4]",
            ]
            .join("\n")
        );
        current["records"].as_array_mut().unwrap().reverse();
        baseline["records"].as_array_mut().unwrap().reverse();
        assert_eq!(
            output,
            render_diff(&diff_snapshots(&baseline, &current).unwrap())
        );
    }

    #[test]
    fn ink_clusters_order_and_anchor() {
        let baseline = json!({"records":[{"id":"shape:box","typeName":"shape","type":"geo","x":0,"y":0,"props":{"w":100,"h":100,"text":"Box"}}]});
        let mut current = baseline.clone();
        for (id, x, y) in [("c", 300, 300), ("a", 10, 10), ("b", 12, 12)] {
            current["records"].as_array_mut().unwrap().push(json!({"id":format!("shape:{id}"),"typeName":"shape","type":"draw","x":x,"y":y,"props":{"segments":[{"points":[{"x":0,"y":0},{"x":5,"y":10}]}]}}));
        }
        let output = render_diff(&diff_snapshots(&baseline, &current).unwrap());
        assert!(output.contains("ink #1 (2 strokes) inside"), "{output}");
        assert!(output.contains("ink #2 (1 stroke)"), "{output}");
        current["records"].as_array_mut().unwrap().reverse();
        assert_eq!(
            output,
            render_diff(&diff_snapshots(&baseline, &current).unwrap())
        );
        let removed = render_diff(&diff_snapshots(&current, &baseline).unwrap());
        assert!(removed.contains("- ink #1 (2 strokes) inside"), "{removed}");
        assert!(removed.contains("- ink #2 (1 stroke)"), "{removed}");
    }

    #[test]
    fn official_compressed_paths_decode_expected_points() {
        let expected = vec![(12.25, -3.5), (14.75, 0.5), (13.75, -1.5)];
        assert_eq!(
            decode_path(&json!({"dim":2,"path":"AABEQQAAYMAAQQBEALwAwA=="})).unwrap(),
            expected
        );
        assert_eq!(
            decode_path(&json!({"path":"AABEQQAAYMAAAAA/AEEARAA0ALwAwAC4"})).unwrap(),
            expected
        );
        assert!(decode_path(&json!({"dim":2,"path":""})).unwrap().is_empty());
        assert!(decode_path(&json!({"dim":2,"path":"AAAA"})).is_err());
    }

    #[test]
    fn transitive_clusters_do_not_merge_separate_lines() {
        let empty = json!({"records":[]});
        let mut current = empty.clone();
        for (id, x, y) in [("a", 0, 0), ("b", 20, 0), ("c", 40, 0), ("d", 0, 40)] {
            current["records"].as_array_mut().unwrap().push(json!({"id":format!("shape:{id}"),"typeName":"shape","type":"draw","x":x,"y":y,"props":{"segments":[{"points":[{"x":0,"y":0},{"x":1,"y":10}]}]}}));
        }
        let expected = "+ ink #1 (3 strokes) bbox [0.0, 0.0, 41.0, 10.0]\n+ ink #2 (1 stroke) bbox [0.0, 40.0, 1.0, 10.0]";
        assert_eq!(
            render_diff(&diff_snapshots(&empty, &current).unwrap()),
            expected
        );
        current["records"].as_array_mut().unwrap().reverse();
        assert_eq!(
            render_diff(&diff_snapshots(&empty, &current).unwrap()),
            expected
        );
    }

    #[test]
    fn anchors_exclude_unlabelled_shapes_and_use_legacy_text_fallback() {
        let baseline = json!({"records":[
            {"id":"shape:blank","typeName":"shape","type":"geo","x":0,"y":0,"props":{"w":20,"h":20,"text":" "}},
            {"id":"shape:label","typeName":"shape","type":"geo","x":100,"y":0,"props":{"w":20,"h":20,"text":"Fallback","richText":{"type":"doc","content":[]}}}
        ]});
        let mut current = baseline.clone();
        current["records"].as_array_mut().unwrap().push(json!({"id":"shape:ink","typeName":"shape","type":"draw","x":0,"y":0,"props":{"segments":[{"points":[{"x":0,"y":0},{"x":2,"y":2}]}]}}));
        assert_eq!(
            render_diff(&diff_snapshots(&baseline, &current).unwrap()),
            "+ ink #1 (1 stroke) left of \"Fallback\" bbox [0.0, 0.0, 2.0, 2.0]"
        );
    }

    #[test]
    fn ink_crossings_require_segment_intersection_and_follow_moves_and_deletes() {
        let baseline = json!({"records":[
            {"id":"shape:label","typeName":"shape","type":"geo","x":200,"y":200,"props":{"w":20,"h":20,"text":"Label"}},
            {"id":"shape:arrow","typeName":"shape","type":"arrow","x":0,"y":0,"props":{"text":"Link","start":{"x":0,"y":0},"end":{"x":100,"y":100},"bend":0}}
        ]});
        let mut current = baseline.clone();
        current["records"].as_array_mut().unwrap().push(json!({"id":"shape:ink","typeName":"shape","type":"draw","x":0,"y":0,"props":{"segments":[{"points":[{"x":0,"y":90},{"x":10,"y":100}]}]}}));
        let miss = render_diff(&diff_snapshots(&baseline, &current).unwrap());
        assert!(!miss.contains("crossing"), "{miss}");
        record(&mut current, "shape:ink")["props"]["segments"][0]["points"] =
            json!([{"x":0,"y":100},{"x":100,"y":0}]);
        let hit = render_diff(&diff_snapshots(&baseline, &current).unwrap());
        assert!(hit.contains("crossing arrow \"Link\""), "{hit}");
        assert!(render_diff(&diff_snapshots(&current, &baseline).unwrap())
            .contains("- ink #1 (1 stroke) crossing arrow \"Link\""));
        let before_move = current.clone();
        record(&mut current, "shape:ink")["x"] = json!(500);
        assert!(
            !render_diff(&diff_snapshots(&before_move, &current).unwrap()).contains("crossing")
        );
        let moved_back = render_diff(&diff_snapshots(&current, &before_move).unwrap());
        assert!(
            moved_back.contains("crossing arrow \"Link\""),
            "{moved_back}"
        );
        record(&mut current, "shape:ink")["x"] = json!(0);
        record(&mut current, "shape:arrow")["props"]["bend"] = json!(50);
        assert!(!render_diff(&diff_snapshots(&baseline, &current).unwrap()).contains("crossing"));
    }

    fn stroke(id: &str, x: f64, y: f64, w: f64, h: f64) -> Value {
        json!({"id":format!("shape:{id}"),"typeName":"shape","type":"draw","x":x,"y":y,
            "props":{"segments":[{"points":[{"x":0,"y":0},{"x":w,"y":h}]}]}})
    }

    #[test]
    fn ink_merges_words_on_a_line_and_splits_lines() {
        let baseline = json!({"records":[]});
        let mut current = baseline.clone();
        for (id, x, y) in [("w1", 0.0, 0.0), ("w2", 200.0, 0.0), ("l2", 0.0, 70.0)] {
            current["records"]
                .as_array_mut()
                .unwrap()
                .push(stroke(id, x, y, 80.0, 40.0));
        }
        assert_eq!(
            render_diff(&diff_snapshots(&baseline, &current).unwrap()),
            "+ ink #1 (2 strokes) bbox [0.0, 0.0, 280.0, 40.0]\n+ ink #2 (1 stroke) bbox [0.0, 70.0, 80.0, 40.0]"
        );
    }

    #[test]
    fn anchors_skip_bound_shapes_and_bound_arrows() {
        let snapshot = json!({"records":[
            {"id":"shape:a","typeName":"shape","type":"geo","x":0,"y":0,"props":{"w":100,"h":50,"text":"A"}},
            {"id":"shape:b","typeName":"shape","type":"geo","x":300,"y":0,"props":{"w":100,"h":50,"text":"B"}},
            {"id":"shape:n","typeName":"shape","type":"geo","x":0,"y":200,"props":{"w":100,"h":50,"text":"N"}},
            {"id":"shape:ab","typeName":"shape","type":"arrow","x":100,"y":25,"props":{"start":{"x":0,"y":0},"end":{"x":200,"y":0}}},
            {"id":"binding:ab-start","typeName":"binding","type":"arrow","fromId":"shape:ab","toId":"shape:a","props":{"terminal":"start"}},
            {"id":"binding:ab-end","typeName":"binding","type":"arrow","fromId":"shape:ab","toId":"shape:b","props":{"terminal":"end"}}
        ]});
        let empty = json!({"records":[]});
        let output = render_diff(&diff_snapshots(&empty, &snapshot).unwrap());
        assert_eq!(
            output,
            "+ arrow \"A\" → \"B\" above \"N\"\n+ geo \"A\" above \"N\"\n+ geo \"B\" right of \"A\"\n+ geo \"N\" below \"A\""
        );
    }

    #[test]
    fn geometry_relations_and_rotation() {
        let reference = Bounds {
            min_x: 0.0,
            min_y: 0.0,
            max_x: 10.0,
            max_y: 10.0,
        };
        assert_eq!(Bounds::point(5.0, -1.0).relation(reference), "above");
        assert_eq!(Bounds::point(5.0, 11.0).relation(reference), "below");
        assert_eq!(Bounds::point(-1.0, 5.0).relation(reference), "left of");
        assert_eq!(Bounds::point(11.0, 5.0).relation(reference), "right of");
        assert_eq!(Bounds::point(5.0, 5.0).relation(reference), "inside");
        assert_eq!(
            Bounds {
                min_x: -1.0,
                min_y: 5.0,
                max_x: 1.0,
                max_y: 6.0
            }
            .relation(reference),
            "crossing"
        );
        let shape = json!({"rotation":std::f64::consts::FRAC_PI_2,"props":{"segments":[{"points":[{"x":0,"y":0},{"x":10,"y":0}]}]}});
        let actual = geometry(&shape, "draw", 20.0, 30.0).unwrap().0;
        assert!((actual.max_y - 40.0).abs() < 1e-8);
        assert!((actual.min_x - 20.0).abs() < 1e-8);
    }

    #[test]
    fn relabel_golden() {
        let mut current = snapshot();
        relabel(&mut current);
        assert_eq!(diff(&current), "~ geo \"Fix new clippy warnings in core\" text: \"Fix new clippy warnings in core\" → \"Fix clippy warnings in core (allow pedantic)\" above \"Fix new clippy warnings in cli\"");
    }

    #[test]
    fn add_golden() {
        let mut current = snapshot();
        add(&mut current);
        assert_eq!(
            diff(&current),
            "+ arrow \"Add [lints] workspace = true to each crate\" → \"NEW: run cargo fmt check\" crossing \"Add clippy -D warnings to CI\"\n+ geo \"NEW: run cargo fmt check\" below \"Document lint policy in CLAUDE.md\""
        );
    }

    #[test]
    fn delete_golden() {
        let mut current = snapshot();
        delete(&mut current);
        assert_eq!(
            diff(&current),
            "- arrow \"Add clippy -D warnings to CI\" → \"Document lint policy in CLAUDE.md\" right of \"Fix new clippy warnings in cli\"\n- geo \"Document lint policy in CLAUDE.md\" below \"Add clippy -D warnings to CI\""
        );
    }

    #[test]
    fn repoint_arrow_golden() {
        let mut current = snapshot();
        record(&mut current, "binding:s5-s6-end")["toId"] = json!("shape:s4");
        assert_eq!(
            diff(&current),
            "~ arrow \"Add clippy -D warnings to CI\" → \"Document lint policy in CLAUDE.md\" endpoints: arrow \"Add clippy -D warnings to CI\" → \"Document lint policy in CLAUDE.md\" → arrow \"Add clippy -D warnings to CI\" → \"Fix new clippy warnings in cli\" crossing \"Document lint policy in CLAUDE.md\""
        );
    }

    #[test]
    fn move_golden() {
        let mut current = snapshot();
        let node = record(&mut current, "shape:s1");
        node["x"] = json!(42);
        node["y"] = json!(-17);
        assert_eq!(diff(&current), "~ geo \"Add [workspace.lints] to root Cargo.toml\" position: (0, 0) → (42, -17) left of \"Fix new clippy warnings in core\"");
    }

    #[test]
    fn freehand_golden() {
        let mut current = snapshot();
        current["records"].as_array_mut().unwrap().push(json!({
            "id":"shape:ink1", "typeName":"shape", "type":"draw", "parentId":"page:page",
            "index":"aD", "isLocked":false, "opacity":1, "meta":{}, "x":20, "y":30,
            "rotation":0, "props":{"color":"black", "fill":"none", "dash":"draw",
                "size":"m", "segments":[{"type":"free", "dim":2, "path":"AAAAAAAAgD8AAABAAACAPw=="}],
                "isComplete":true, "isClosed":false, "isPen":false, "scale":1, "scaleX":1, "scaleY":1}
        }));
        assert_eq!(diff(&current), "+ ink #1 (1 stroke) inside \"Add [workspace.lints] to root Cargo.toml\" bbox [20.0, 31.0, 0.0, 3.9]");
    }

    #[test]
    fn combined_spike_golden_and_change_list() {
        let mut current = snapshot();
        relabel(&mut current);
        add(&mut current);
        delete(&mut current);
        let changes = diff_snapshots(&snapshot(), &current).unwrap();
        assert_eq!(changes.len(), 5);
        assert_eq!(render_diff(&changes), "+ arrow \"Add [lints] workspace = true to each crate\" → \"NEW: run cargo fmt check\" crossing \"Add clippy -D warnings to CI\"\n+ geo \"NEW: run cargo fmt check\" below \"Fix new clippy warnings in cli\"\n- arrow \"Add clippy -D warnings to CI\" → \"Document lint policy in CLAUDE.md\" right of \"Fix new clippy warnings in cli\"\n- geo \"Document lint policy in CLAUDE.md\" below \"Add clippy -D warnings to CI\"\n~ geo \"Fix new clippy warnings in core\" text: \"Fix new clippy warnings in core\" → \"Fix clippy warnings in core (allow pedantic)\" above \"Fix new clippy warnings in cli\"");
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
        assert_eq!(diff(&current), "~ geo \"Fix new clippy warnings in core\" text: \"Fix new clippy warnings in core\" → \"Use \\\"core\\\" first\\nthen \\\\shell\\nFinish\" above \"Fix new clippy warnings in cli\"");
    }

    #[test]
    fn unbinding_an_arrow_reports_unknown_endpoint_without_guessing_from_its_id() {
        let mut current = snapshot();
        current["records"]
            .as_array_mut()
            .unwrap()
            .retain(|record| record["id"] != "binding:s5-s6-end");
        assert_eq!(diff(&current), "~ arrow \"Add clippy -D warnings to CI\" → \"Document lint policy in CLAUDE.md\" endpoints: arrow \"Add clippy -D warnings to CI\" → \"Document lint policy in CLAUDE.md\" → arrow \"Add clippy -D warnings to CI\" → ? crossing \"Document lint policy in CLAUDE.md\"");
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
        assert!(changes[0].starts_with("~ geo \"Fix new clippy warnings in core\" text:"));
        assert_eq!(changes[1], "~ geo \"Fix new clippy warnings in core\" position: (340, 70) → (42.5, -17.25) crossing \"Add [workspace.lints] to root Cargo.toml\"");
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
