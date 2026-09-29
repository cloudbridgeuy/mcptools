use std::collections::{BTreeMap, BTreeSet, HashSet};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChartNode {
    pub identifier: String,
    pub title: String,
    pub state_type: String,
    pub parent: Option<String>,
    pub blocked_by: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChartClass {
    Complete,
    InProgress,
    Frontier,
    Fog,
}

impl ChartClass {
    fn name(self) -> &'static str {
        match self {
            ChartClass::Complete => "complete",
            ChartClass::InProgress => "inprogress",
            ChartClass::Frontier => "frontier",
            ChartClass::Fog => "fog",
        }
    }

    fn legend_id(self) -> &'static str {
        match self {
            ChartClass::Complete => "LComplete",
            ChartClass::InProgress => "LProgress",
            ChartClass::Frontier => "LFrontier",
            ChartClass::Fog => "LFog",
        }
    }
}

const CLASS_ORDER: [ChartClass; 4] = [
    ChartClass::Frontier,
    ChartClass::Complete,
    ChartClass::InProgress,
    ChartClass::Fog,
];

const SUBGRAPH_HUES: [&str; 8] = [
    "#8b7bb8", "#2dd4bf", "#f59e0b", "#4ade80", "#60a5fa", "#f472b6", "#a78bfa", "#38bdf8",
];

const CLUSTER_WRAP: usize = 22;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChartStats {
    pub total: usize,
    pub complete: usize,
    pub inprogress: usize,
    pub frontier: usize,
    pub fog: usize,
}

pub fn classify_nodes(nodes: &[ChartNode]) -> BTreeMap<String, ChartClass> {
    let by_id: BTreeMap<&str, &ChartNode> = nodes
        .iter()
        .map(|node| (node.identifier.as_str(), node))
        .collect();
    let mut classes = BTreeMap::new();
    for node in nodes {
        if node.state_type == "canceled" {
            continue;
        }
        let class = match node.state_type.as_str() {
            "completed" => ChartClass::Complete,
            "started" => ChartClass::InProgress,
            _ => {
                let blocked =
                    node.blocked_by
                        .iter()
                        .any(|blocker| match by_id.get(blocker.as_str()) {
                            None => true,
                            Some(other) if other.state_type == "canceled" => false,
                            Some(other) => other.state_type != "completed",
                        });
                if blocked {
                    ChartClass::Fog
                } else {
                    ChartClass::Frontier
                }
            }
        };
        classes.insert(node.identifier.clone(), class);
    }
    classes
}

pub fn chart_stats(nodes: &[ChartNode]) -> ChartStats {
    let classes = classify_nodes(nodes);
    let mut stats = ChartStats {
        total: classes.len(),
        complete: 0,
        inprogress: 0,
        frontier: 0,
        fog: 0,
    };
    for class in classes.values() {
        match class {
            ChartClass::Complete => stats.complete += 1,
            ChartClass::InProgress => stats.inprogress += 1,
            ChartClass::Frontier => stats.frontier += 1,
            ChartClass::Fog => stats.fog += 1,
        }
    }
    stats
}

pub fn stats_line(stats: &ChartStats) -> String {
    format!(
        "{} issues: {} complete, {} in progress, {} frontier, {} fog",
        stats.total, stats.complete, stats.inprogress, stats.frontier, stats.fog
    )
}

pub fn build_mermaid(nodes: &[ChartNode]) -> String {
    let classes = classify_nodes(nodes);
    let rendered: Vec<&ChartNode> = nodes
        .iter()
        .filter(|node| node.state_type != "canceled")
        .collect();
    let rendered_ids: HashSet<&str> = rendered
        .iter()
        .map(|node| node.identifier.as_str())
        .collect();

    let mut children: BTreeMap<&str, Vec<&ChartNode>> = BTreeMap::new();
    for node in &rendered {
        if let Some(parent) = node.parent.as_deref() {
            if rendered_ids.contains(parent) {
                children.entry(parent).or_default().push(node);
            }
        }
    }
    let containers: HashSet<&str> = children.keys().copied().collect();
    let top_level: Vec<&ChartNode> = rendered
        .iter()
        .copied()
        .filter(|node| {
            !node
                .parent
                .as_deref()
                .is_some_and(|parent| rendered_ids.contains(parent))
        })
        .collect();

    let mut sorted: Vec<&ChartNode> = rendered.clone();
    sorted.sort_by_key(|node| (issue_number(&node.identifier), node.identifier.as_str()));

    let mut out = String::from("flowchart TB\n");
    let (solo, rest): (Vec<&ChartNode>, Vec<&ChartNode>) = top_level
        .into_iter()
        .partition(|node| !containers.contains(node.identifier.as_str()));
    emit_scope(&rest, &children, &containers, 1, &mut out);
    if !solo.is_empty() {
        out.push_str("  subgraph Ungrouped[\"Standalone issues\"]\n");
        emit_scope(&solo, &children, &containers, 2, &mut out);
        out.push_str("  end\n");
    }

    let mut edges: BTreeSet<(String, String)> = BTreeSet::new();
    for node in &rendered {
        for blocker in &node.blocked_by {
            if !rendered_ids.contains(blocker.as_str()) {
                continue;
            }
            edges.insert((
                target_ref(blocker, &containers),
                target_ref(&node.identifier, &containers),
            ));
        }
    }
    for (from, to) in &edges {
        out.push_str(&format!("  {from} --> {to}\n"));
    }

    let mut styled: Vec<&str> = containers.iter().copied().collect();
    styled.sort_by_key(|identifier| (issue_number(identifier), *identifier));
    for (index, identifier) in styled.iter().enumerate() {
        let hue = SUBGRAPH_HUES[index % SUBGRAPH_HUES.len()];
        out.push_str(&format!(
            "  style {} fill:#12141d,stroke:{},stroke-width:1.5px\n",
            subgraph_id(identifier),
            hue
        ));
    }
    if !solo.is_empty() {
        out.push_str("  style Ungrouped fill:#12141d,stroke:#64748b,stroke-width:1.5px\n");
    }

    out.push_str("  subgraph Legend[\"State legend\"]\n");
    out.push_str("    LFrontier[\"Frontier: ready to start\"]\n");
    out.push_str("    LComplete[\"Complete\"]\n");
    out.push_str("    LProgress[\"In progress\"]\n");
    out.push_str("    LFog[\"Fog: prerequisite incomplete\"]\n");
    out.push_str("  end\n");
    out.push_str("  style Legend fill:#12141d,stroke:#f59e0b,stroke-width:1.5px\n");
    out.push_str(
        "  classDef complete fill:#dcfce7,stroke:#15803d,color:#14532d,stroke-width:2px\n",
    );
    out.push_str(
        "  classDef inprogress fill:#fef9c3,stroke:#ca8a04,color:#713f12,stroke-width:4px\n",
    );
    out.push_str(
        "  classDef frontier fill:#dbeafe,stroke:#2563eb,color:#1e3a8a,stroke-width:3px\n",
    );
    out.push_str("  classDef fog fill:#f1f5f9,stroke:#64748b,color:#334155,stroke-dasharray:5 5\n");

    for class in CLASS_ORDER {
        let members: Vec<String> = sorted
            .iter()
            .filter(|node| {
                !containers.contains(node.identifier.as_str())
                    && classes.get(node.identifier.as_str()) == Some(&class)
            })
            .map(|node| node_id(&node.identifier))
            .collect();
        if members.is_empty() {
            continue;
        }
        out.push_str(&format!(
            "  class {},{} {}\n",
            class.legend_id(),
            members.join(","),
            class.name()
        ));
    }
    out
}

fn emit_scope<'a>(
    items: &[&'a ChartNode],
    children: &BTreeMap<&'a str, Vec<&'a ChartNode>>,
    containers: &HashSet<&'a str>,
    depth: usize,
    out: &mut String,
) {
    let mut sorted: Vec<&&ChartNode> = items.iter().collect();
    sorted.sort_by_key(|node| (issue_number(&node.identifier), node.identifier.as_str()));
    for node in sorted {
        let indent = "  ".repeat(depth);
        let identifier = node.identifier.as_str();
        if containers.contains(identifier) {
            out.push_str(&format!(
                "{}subgraph {}[\"{}\"]\n",
                indent,
                subgraph_id(identifier),
                cluster_label(identifier, &node.title)
            ));
            let kids: Vec<&ChartNode> = children.get(identifier).cloned().unwrap_or_default();
            emit_scope(&kids, children, containers, depth + 1, out);
            out.push_str(&format!("{}end\n", indent));
        } else {
            out.push_str(&format!(
                "{}{}[\"{}\"]\n",
                indent,
                node_id(identifier),
                label(identifier, &node.title)
            ));
        }
    }
}

fn target_ref(identifier: &str, containers: &HashSet<&str>) -> String {
    match containers.contains(identifier) {
        true => subgraph_id(identifier),
        false => node_id(identifier),
    }
}

fn node_id(identifier: &str) -> String {
    identifier
        .chars()
        .map(|c| match c.is_ascii_alphanumeric() {
            true => c,
            false => '_',
        })
        .collect()
}

fn subgraph_id(identifier: &str) -> String {
    let tail = identifier.rsplit('-').next().unwrap_or("");
    match !tail.is_empty() && tail.chars().all(|c| c.is_ascii_digit()) {
        true => format!("E{tail}"),
        false => format!("E{}", node_id(identifier)),
    }
}

fn issue_number(identifier: &str) -> u32 {
    identifier
        .rsplit('-')
        .next()
        .and_then(|tail| tail.parse::<u32>().ok())
        .unwrap_or(0)
}

fn label(identifier: &str, title: &str) -> String {
    format!("{identifier}: {title}")
        .replace('"', "#quot;")
        .replace(['[', ']'], "")
}

fn cluster_label(identifier: &str, title: &str) -> String {
    wrap_label(&label(identifier, title), CLUSTER_WRAP).join("<br/>")
}

fn wrap_label(text: &str, max: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    let mut width = 0usize;
    for word in text.split(' ') {
        let mut rest = word;
        if width > 0 && width + 1 + rest.chars().count() <= max {
            line.push(' ');
            line.push_str(rest);
            width += 1 + rest.chars().count();
            continue;
        }
        if width > 0 {
            lines.push(std::mem::take(&mut line));
        }
        while rest.chars().count() > max {
            let cut: usize = rest.chars().take(max).map(char::len_utf8).sum();
            lines.push(rest[..cut].to_string());
            rest = &rest[cut..];
        }
        line.push_str(rest);
        width = rest.chars().count();
    }
    if width > 0 || lines.is_empty() {
        lines.push(line);
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(id: &str, state: &str, parent: Option<&str>, blocked_by: &[&str]) -> ChartNode {
        ChartNode {
            identifier: id.to_string(),
            title: format!("Title {id}"),
            state_type: state.to_string(),
            parent: parent.map(str::to_string),
            blocked_by: blocked_by.iter().map(|item| item.to_string()).collect(),
        }
    }

    #[test]
    fn classification_covers_every_state() {
        let nodes = vec![
            node("GUZ-1", "completed", None, &[]),
            node("GUZ-2", "started", None, &[]),
            node("GUZ-3", "backlog", None, &[]),
            node("GUZ-4", "unstarted", None, &["GUZ-3"]),
            node("GUZ-5", "unstarted", None, &["GUZ-6"]),
            node("GUZ-6", "completed", None, &[]),
            node("GUZ-7", "backlog", None, &["GUZ-99"]),
        ];
        let classes = classify_nodes(&nodes);
        assert_eq!(classes["GUZ-1"], ChartClass::Complete);
        assert_eq!(classes["GUZ-2"], ChartClass::InProgress);
        assert_eq!(classes["GUZ-3"], ChartClass::Frontier);
        assert_eq!(classes["GUZ-4"], ChartClass::Fog);
        assert_eq!(classes["GUZ-5"], ChartClass::Frontier);
        assert_eq!(classes["GUZ-7"], ChartClass::Fog);
        let stats = chart_stats(&nodes);
        assert_eq!(stats.total, 7);
        assert_eq!(stats.complete, 2);
        assert_eq!(stats.inprogress, 1);
        assert_eq!(stats.frontier, 2);
        assert_eq!(stats.fog, 2);
    }

    #[test]
    fn canceled_blocker_is_dropped_and_satisfies_dependents() {
        let nodes = vec![
            node("GUZ-8", "canceled", None, &[]),
            node("GUZ-9", "unstarted", None, &["GUZ-8"]),
        ];
        let classes = classify_nodes(&nodes);
        assert!(!classes.contains_key("GUZ-8"));
        assert_eq!(classes["GUZ-9"], ChartClass::Frontier);
        let stats = chart_stats(&nodes);
        assert_eq!(stats.total, 1);
        let mermaid = build_mermaid(&nodes);
        assert!(!mermaid.contains("GUZ_8"));
        assert!(!mermaid.contains("-->"));
    }

    #[test]
    fn mermaid_builder_emits_template_shape() {
        let nodes = vec![
            node("GUZ-185", "started", None, &[]),
            node("GUZ-191", "started", Some("GUZ-185"), &[]),
            node("GUZ-192", "backlog", Some("GUZ-185"), &[]),
            node("GUZ-200", "backlog", None, &["GUZ-191"]),
        ];
        let mut quoted = nodes[1].clone();
        quoted.title = "Data \"model\" [v2]".to_string();
        let mut input = nodes.clone();
        input[1] = quoted.clone();

        let mermaid = build_mermaid(&input);
        assert!(mermaid.starts_with("flowchart TB\n"));
        assert!(mermaid.contains("subgraph E185[\"GUZ-185: Title GUZ-185\"]\n"));
        assert!(mermaid.contains("GUZ_191[\"GUZ-191: Data #quot;model#quot; v2\"]\n"));
        assert!(mermaid.contains("GUZ_192[\"GUZ-192: Title GUZ-192\"]\n"));
        assert!(mermaid.contains("GUZ_200[\"GUZ-200: Title GUZ-200\"]\n"));
        assert!(mermaid.contains("  GUZ_191 --> GUZ_200\n"));
        assert!(!mermaid.contains("GUZ_200 --> GUZ_191"));
        assert!(!mermaid.contains("GUZ_185 -->"));
        assert!(mermaid.contains("subgraph Legend[\"State legend\"]\n"));
        assert!(mermaid.contains(
            "classDef complete fill:#dcfce7,stroke:#15803d,color:#14532d,stroke-width:2px"
        ));
        assert!(mermaid.contains(
            "classDef inprogress fill:#fef9c3,stroke:#ca8a04,color:#713f12,stroke-width:4px"
        ));
        assert!(mermaid.contains(
            "classDef frontier fill:#dbeafe,stroke:#2563eb,color:#1e3a8a,stroke-width:3px"
        ));
        assert!(mermaid.contains(
            "classDef fog fill:#f1f5f9,stroke:#64748b,color:#334155,stroke-dasharray:5 5"
        ));
        assert!(!mermaid.contains("class LComplete"));
        assert!(mermaid.contains("  class LProgress,GUZ_191 inprogress\n"));
        assert!(mermaid.contains("  class LFrontier,GUZ_192 frontier\n"));
        assert!(mermaid.contains("  class LFog,GUZ_200 fog\n"));
        assert!(mermaid.contains("  style E185 fill:#12141d,stroke:#8b7bb8,stroke-width:1.5px\n"));
        assert!(mermaid.contains("  style Legend fill:#12141d,stroke:#f59e0b,stroke-width:1.5px\n"));
        let frontier = mermaid.find("LFrontier[\"").unwrap();
        let complete = mermaid.find("LComplete[\"").unwrap();
        let progress = mermaid.find("LProgress[\"").unwrap();
        let fog = mermaid.find("LFog[\"").unwrap();
        assert!(frontier < complete && complete < progress && progress < fog);

        assert_eq!(mermaid, build_mermaid(&input));
        let mut reversed = input.clone();
        reversed.reverse();
        assert_eq!(mermaid, build_mermaid(&reversed));
    }

    #[test]
    fn top_level_childless_issues_share_standalone_subgraph() {
        let nodes = vec![
            node("GUZ-10", "started", None, &[]),
            node("GUZ-11", "backlog", Some("GUZ-10"), &[]),
            node("GUZ-20", "backlog", None, &[]),
            node("GUZ-21", "backlog", None, &["GUZ-20"]),
        ];
        let mermaid = build_mermaid(&nodes);
        assert!(mermaid.contains("  subgraph Ungrouped[\"Standalone issues\"]\n"));
        let start = mermaid.find("subgraph Ungrouped").unwrap();
        let tail = &mermaid[start..];
        let end = tail.find("\n  end\n").unwrap();
        let body = &tail[..end];
        assert!(body.contains("GUZ_20["));
        assert!(body.contains("GUZ_21["));
        assert!(!body.contains("GUZ_11["));
        assert!(mermaid.contains("GUZ_20 --> GUZ_21"));
        assert!(
            mermaid.contains("  style Ungrouped fill:#12141d,stroke:#64748b,stroke-width:1.5px\n")
        );
    }

    #[test]
    fn no_standalone_subgraph_without_top_level_leaves() {
        let nodes = vec![
            node("GUZ-10", "started", None, &[]),
            node("GUZ-11", "backlog", Some("GUZ-10"), &[]),
        ];
        let mermaid = build_mermaid(&nodes);
        assert!(!mermaid.contains("Ungrouped"));
    }

    #[test]
    fn cluster_titles_hard_wrap_and_node_labels_stay_flat() {
        let long = "electable PNG and JPEG render outputs plus extras";
        let mut epic = node("GUZ-400", "started", None, &[]);
        epic.title = long.to_string();
        let mut plain = node("GUZ-401", "backlog", None, &[]);
        plain.title = long.to_string();
        let child = node("GUZ-402", "backlog", Some("GUZ-400"), &[]);
        let mermaid = build_mermaid(&[epic, child, plain]);
        let subgraph = mermaid
            .lines()
            .find(|l| l.contains("subgraph E400"))
            .unwrap();
        let title = &subgraph[subgraph.find('[').unwrap() + 2..subgraph.rfind('"').unwrap()];
        let lines: Vec<&str> = title.split("<br/>").collect();
        assert!(lines.len() > 1);
        for line in lines {
            assert!(line.chars().count() <= CLUSTER_WRAP);
        }
        let node_line = format!("GUZ_401[\"GUZ-401: {long}\"]");
        assert!(mermaid.contains(&node_line));
    }

    #[test]
    fn subgraph_hues_cycle_by_sorted_epic_number() {
        let nodes = vec![
            node("GUZ-300", "started", None, &[]),
            node("GUZ-301", "started", Some("GUZ-300"), &[]),
            node("GUZ-302", "backlog", Some("GUZ-301"), &[]),
            node("GUZ-100", "started", None, &[]),
            node("GUZ-101", "started", Some("GUZ-100"), &[]),
        ];
        let mermaid = build_mermaid(&nodes);
        assert!(mermaid.contains("style E100 fill:#12141d,stroke:#8b7bb8,stroke-width:1.5px"));
        assert!(mermaid.contains("style E300 fill:#12141d,stroke:#2dd4bf,stroke-width:1.5px"));
        assert!(mermaid.contains("style E301 fill:#12141d,stroke:#f59e0b,stroke-width:1.5px"));
    }
}
