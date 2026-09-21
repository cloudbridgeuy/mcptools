use std::collections::HashSet;

fn sample(tool: &str) -> serde_json::Value {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("contract_samples")
        .join(format!("{tool}.json"));
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|_| panic!("missing recorded sample for {tool}"));
    serde_json::from_str(&text).unwrap_or_else(|_| panic!("bad JSON sample for {tool}"))
}

fn roundtrip<T: serde::de::DeserializeOwned + serde::Serialize>(
    value: &serde_json::Value,
) -> serde_json::Value {
    let t: T = serde_json::from_value(value.clone()).expect("roundtrip deserialize");
    serde_json::to_value(&t).expect("roundtrip serialize")
}

fn roundtrip_by_tool(tool: &str, value: &serde_json::Value) -> serde_json::Value {
    match tool {
        "jira_search" => roundtrip::<mcptools_core::atlassian::jira::SearchOutput>(value),
        "confluence_search" => {
            roundtrip::<mcptools_core::atlassian::confluence::SearchOutput>(value)
        }
        "hn_read_item" => roundtrip::<mcptools_core::hn::PostOutput>(value),
        "hn_list_items" => roundtrip::<mcptools_core::hn::ListOutput>(value),
        "md_fetch" => roundtrip::<mcptools_core::md::FetchOutput>(value),
        "md_toc" => roundtrip::<crate::md::toc::TocOutput>(value),
        "jira_create" => roundtrip::<mcptools_core::atlassian::jira::TicketOutput>(value),
        "jira_get" => roundtrip::<mcptools_core::atlassian::jira::TicketOutput>(value),
        "jira_update" => roundtrip::<mcptools_core::atlassian::jira::UpdateOutput>(value),
        "jira_comment_add" => roundtrip::<mcptools_core::atlassian::jira::CommentOutput>(value),
        "jira_comment_list" => {
            roundtrip::<mcptools_core::atlassian::jira::CommentListOutput>(value)
        }
        "jira_comment_update" => roundtrip::<mcptools_core::atlassian::jira::CommentOutput>(value),
        "jira_comment_delete" => {
            roundtrip::<mcptools_core::atlassian::jira::CommentDeleteOutput>(value)
        }
        "jira_sprint_list" => roundtrip::<mcptools_core::atlassian::jira::SprintListOutput>(value),
        "jira_attachment_list" => {
            roundtrip::<mcptools_core::atlassian::jira::AttachmentListOutput>(value)
        }
        "jira_attachment_download" => {
            roundtrip::<mcptools_core::atlassian::jira::AttachmentDownloadOutput>(value)
        }
        "jira_attachment_upload" => {
            roundtrip::<mcptools_core::atlassian::jira::AttachmentListOutput>(value)
        }
        "jira_query_list" => roundtrip::<mcptools_core::atlassian::jira::QueryListOutput>(value),
        "jira_query_save" => roundtrip::<mcptools_core::atlassian::jira::QueryStatusOutput>(value),
        "jira_query_delete" => {
            roundtrip::<mcptools_core::atlassian::jira::QueryStatusOutput>(value)
        }
        "jira_query_load" => roundtrip::<mcptools_core::atlassian::jira::QueryLoadOutput>(value),
        "bitbucket_pr_list" => {
            roundtrip::<mcptools_core::atlassian::bitbucket::PRListOutput>(value)
        }
        "bitbucket_pr_read" => roundtrip::<mcptools_core::atlassian::bitbucket::PROutput>(value),
        "bitbucket_pr_create" => {
            roundtrip::<mcptools_core::atlassian::bitbucket::PRCreateOutput>(value)
        }
        "bitbucket_workspace_list" => {
            roundtrip::<mcptools_core::atlassian::bitbucket::WorkspaceListOutput>(value)
        }
        "bitbucket_repo_list" => {
            roundtrip::<mcptools_core::atlassian::bitbucket::RepoListOutput>(value)
        }
        "bitbucket_repo_branches" => {
            roundtrip::<mcptools_core::atlassian::bitbucket::BranchListOutput>(value)
        }
        "ui_annotations_list" => {
            roundtrip::<mcptools_core::annotations::ListAnnotationsResponse>(value)
        }
        "ui_annotations_get" => roundtrip::<mcptools_core::annotations::DevAnnotation>(value),
        "ui_annotations_resolve" => roundtrip::<super::annotations::AnnotationResolveOutput>(value),
        "ui_annotations_clear" => roundtrip::<super::annotations::AnnotationClearOutput>(value),
        "pdf_toc" => roundtrip::<::pdf::DocumentTree>(value),
        "pdf_read" => roundtrip::<::pdf::SectionContent>(value),
        "pdf_peek" => roundtrip::<::pdf::PeekContent>(value),
        "pdf_images" => roundtrip::<super::pdf::PdfImagesOutput>(value),
        "pdf_image" => roundtrip::<super::pdf::PdfImageOutput>(value),
        "pdf_info" => roundtrip::<::pdf::DocumentMetadata>(value),
        "images_generate" => roundtrip::<crate::images::SavedOutput>(value),
        "images_edit" => roundtrip::<crate::images::SavedOutput>(value),
        "images_vary" => roundtrip::<crate::images::SavedOutput>(value),
        "atlas_tree_view" => roundtrip::<super::atlas::AtlasTextOutput>(value),
        "atlas_peek" => roundtrip::<super::atlas::AtlasTextOutput>(value),
        "atlas_status" => roundtrip::<super::atlas::AtlasTextOutput>(value),
        "linear_auth_status" => roundtrip::<mcptools_core::linear::Viewer>(value),
        "linear_issue_get" => roundtrip::<mcptools_core::linear::IssueGetOutput>(value),
        "linear_issue_list" => roundtrip::<mcptools_core::linear::IssueListOutput>(value),
        "linear_comment_list" => roundtrip::<mcptools_core::linear::CommentListOutput>(value),
        "linear_relation_list" => roundtrip::<mcptools_core::linear::RelationListOutput>(value),
        "linear_team_list" => roundtrip::<mcptools_core::linear::TeamListOutput>(value),
        "linear_team_get" => roundtrip::<mcptools_core::linear::Team>(value),
        "linear_project_list" => roundtrip::<mcptools_core::linear::ProjectListOutput>(value),
        "linear_project_get" => roundtrip::<mcptools_core::linear::Project>(value),
        "linear_user_list" => roundtrip::<mcptools_core::linear::UserListOutput>(value),
        "linear_state_list" => roundtrip::<mcptools_core::linear::StateListOutput>(value),
        "linear_label_list" => roundtrip::<mcptools_core::linear::LabelListOutput>(value),
        "linear_cycle_list" => roundtrip::<mcptools_core::linear::CycleListOutput>(value),
        "linear_issue_create" => roundtrip::<mcptools_core::linear::IssueMini>(value),
        "linear_issue_update" => roundtrip::<mcptools_core::linear::IssueMini>(value),
        "linear_comment_create" => roundtrip::<mcptools_core::linear::Comment>(value),
        "linear_relation_add" => roundtrip::<mcptools_core::linear::RelationAddOutput>(value),
        "linear_relation_remove" => roundtrip::<mcptools_core::linear::RelationRemoveOutput>(value),
        "find_tools" => roundtrip::<super::find_tools::FoundTools>(value),
        _ => panic!("unknown tool in roundtrip_by_tool: {tool}"),
    }
}

fn strip_optional(
    schema: &serde_json::Value,
    root: &serde_json::Value,
    value: &serde_json::Value,
) -> serde_json::Value {
    strip_with_visited(schema, root, value, &mut HashSet::new())
}

fn strip_with_visited(
    schema: &serde_json::Value,
    root: &serde_json::Value,
    value: &serde_json::Value,
    visited: &mut HashSet<String>,
) -> serde_json::Value {
    let ref_str = schema
        .get("$ref")
        .and_then(|x| x.as_str())
        .map(str::to_string);
    let schema = if let Some(r) = &ref_str {
        if visited.contains(r) {
            return value.clone();
        }
        visited.insert(r.clone());
        resolve_ref(root, r)
    } else {
        schema
    };
    let result = if let Some(map) = value.as_object() {
        let empty = serde_json::Map::new();
        let sobj = schema.as_object().unwrap_or(&empty);
        let required: HashSet<String> = sobj
            .get("required")
            .and_then(|r| r.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        let props = sobj.get("properties").and_then(|p| p.as_object());
        let mut out = serde_json::Map::new();
        for (k, v) in map {
            if required.contains(k) {
                let ps = props
                    .and_then(|p| p.get(k))
                    .unwrap_or(&serde_json::Value::Null);
                let sv = strip_with_visited(ps, root, v, visited);
                out.insert(k.clone(), sv);
            }
        }
        serde_json::Value::Object(out)
    } else if let Some(arr) = value.as_array() {
        let it = schema.get("items").unwrap_or(&serde_json::Value::Null);
        let sarr = arr
            .iter()
            .map(|v| strip_with_visited(it, root, v, visited))
            .collect();
        serde_json::Value::Array(sarr)
    } else if let Some(any) = schema.get("anyOf").and_then(|a| a.as_array()) {
        for br in any {
            let b = resolve_schema(br, root);
            if !is_nullish(b) {
                let res = strip_with_visited(b, root, value, visited);
                if let Some(r) = &ref_str {
                    visited.remove(r);
                }
                return res;
            }
        }
        value.clone()
    } else {
        value.clone()
    };
    if let Some(r) = &ref_str {
        visited.remove(r);
    }
    result
}

fn resolve_schema<'a>(
    s: &'a serde_json::Value,
    root: &'a serde_json::Value,
) -> &'a serde_json::Value {
    if let Some(r) = s.get("$ref").and_then(|x| x.as_str()) {
        resolve_ref(root, r)
    } else {
        s
    }
}

fn resolve_ref<'a>(root: &'a serde_json::Value, r: &str) -> &'a serde_json::Value {
    let mut cur = root;
    let p = r.strip_prefix("#/").unwrap_or(r);
    for part in p.split('/') {
        if part.is_empty() {
            continue;
        }
        if let Some(o) = cur.as_object() {
            if let Some(next) = o.get(part) {
                cur = next;
                continue;
            }
            if let Some(d) = o.get("$defs").or_else(|| o.get("definitions")) {
                if let Some(next) = d.get(part) {
                    cur = next;
                    continue;
                }
            }
        }
        break;
    }
    cur
}

fn is_nullish(s: &serde_json::Value) -> bool {
    match s.get("type") {
        Some(serde_json::Value::String(t)) if t == "null" => true,
        Some(serde_json::Value::Array(ts)) => ts.iter().any(|t| t.as_str() == Some("null")),
        _ => false,
    }
}

fn loose_nodes(schema: &serde_json::Value) -> Vec<String> {
    let mut paths = vec![];
    loose_collect(schema, schema, "", &mut HashSet::new(), &mut paths);
    paths
}

fn loose_collect(
    node: &serde_json::Value,
    root: &serde_json::Value,
    path: &str,
    visited: &mut HashSet<String>,
    paths: &mut Vec<String>,
) {
    let ref_str = node
        .get("$ref")
        .and_then(|x| x.as_str())
        .map(str::to_string);
    if let Some(r) = &ref_str {
        if visited.contains(r) {
            return;
        }
        visited.insert(r.clone());
        let res = resolve_ref(root, r);
        if is_loose(res) {
            let p = if path.is_empty() {
                "/".to_string()
            } else {
                path.to_string()
            };
            paths.push(p);
        }
        visited.remove(r);
        return;
    }
    if is_loose(node) {
        let p = if path.is_empty() {
            "/".to_string()
        } else {
            path.to_string()
        };
        paths.push(p);
    }
    if let Some(obj) = node.as_object() {
        if let Some(props) = obj.get("properties").and_then(|p| p.as_object()) {
            for (k, v) in props {
                let child = if path.is_empty() {
                    format!("/properties/{}", k)
                } else {
                    format!("{}/properties/{}", path, k)
                };
                loose_collect(v, root, &child, visited, paths);
            }
        }
        if let Some(items) = obj.get("items") {
            let child = if path.is_empty() {
                "/items".to_string()
            } else {
                format!("{}/items", path)
            };
            loose_collect(items, root, &child, visited, paths);
        }
        if let Some(pis) = obj.get("prefixItems").and_then(|p| p.as_array()) {
            for (i, pi) in pis.iter().enumerate() {
                let child = if path.is_empty() {
                    format!("/prefixItems/{}", i)
                } else {
                    format!("{}/prefixItems/{}", path, i)
                };
                loose_collect(pi, root, &child, visited, paths);
            }
        }
        if let Some(aos) = obj.get("anyOf").and_then(|a| a.as_array()) {
            for (i, ao) in aos.iter().enumerate() {
                let child = if path.is_empty() {
                    format!("/anyOf/{}", i)
                } else {
                    format!("{}/anyOf/{}", path, i)
                };
                loose_collect(ao, root, &child, visited, paths);
            }
        }
        if let Some(defs) = obj
            .get("$defs")
            .or_else(|| obj.get("definitions"))
            .and_then(|d| d.as_object())
        {
            for (k, dv) in defs {
                let child = format!("/$defs/{}", k);
                let dref = format!("#/$defs/{}", k);
                if visited.contains(&dref) {
                    continue;
                }
                visited.insert(dref.clone());
                loose_collect(dv, root, &child, visited, paths);
                visited.remove(&dref);
            }
        }
    }
}

fn is_loose(s: &serde_json::Value) -> bool {
    if s.as_bool() == Some(true) {
        return true;
    }
    if let Some(o) = s.as_object() {
        if o.is_empty() {
            return true;
        }
        !o.contains_key("type")
            && !o.contains_key("$ref")
            && !o.contains_key("anyOf")
            && !o.contains_key("enum")
    } else {
        false
    }
}

fn unreached(schema: &serde_json::Value, sample: &serde_json::Value) -> Vec<String> {
    let mut paths = vec![];
    unreached_collect(schema, schema, sample, "", &mut HashSet::new(), &mut paths);
    paths
}

fn unreached_collect(
    node: &serde_json::Value,
    root: &serde_json::Value,
    value: &serde_json::Value,
    path: &str,
    visited: &mut HashSet<String>,
    paths: &mut Vec<String>,
) {
    let ref_str = node
        .get("$ref")
        .and_then(|x| x.as_str())
        .map(str::to_string);
    if let Some(r) = &ref_str {
        if visited.contains(r) {
            return;
        }
        visited.insert(r.clone());
        let res = resolve_ref(root, r);
        unreached_collect(res, root, value, path, visited, paths);
        visited.remove(r);
        return;
    }
    if let Some(aos) = node.get("anyOf").and_then(|a| a.as_array()) {
        if value.is_null() {
            return;
        }
        for ao in aos {
            let b = resolve_schema(ao, root);
            if !is_nullish(b) {
                unreached_collect(b, root, value, path, visited, paths);
                break;
            }
        }
        return;
    }
    if let Some(obj) = node.as_object() {
        if let Some(props) = obj.get("properties").and_then(|p| p.as_object()) {
            let empty = serde_json::Map::new();
            let map = value.as_object().unwrap_or(&empty);
            for (k, ps) in props {
                let child = if path.is_empty() {
                    format!("/properties/{}", k)
                } else {
                    format!("{}/properties/{}", path, k)
                };
                if let Some(v) = map.get(k) {
                    if v.is_null() {
                        paths.push(child);
                    } else {
                        unreached_collect(ps, root, v, &child, visited, paths);
                    }
                } else {
                    paths.push(child);
                }
            }
        }
        if let Some(it) = obj.get("items") {
            let child = if path.is_empty() {
                "/items".to_string()
            } else {
                format!("{}/items", path)
            };
            if let Some(arr) = value.as_array() {
                if arr.is_empty() {
                    paths.push(child);
                } else {
                    unreached_collect(it, root, &arr[0], &child, visited, paths);
                }
            } else {
                paths.push(child);
            }
        }
        if let Some(pis) = obj.get("prefixItems").and_then(|p| p.as_array()) {
            let empty: Vec<serde_json::Value> = vec![];
            let arr = value.as_array().unwrap_or(&empty);
            for (i, pi) in pis.iter().enumerate() {
                let child = if path.is_empty() {
                    format!("/prefixItems/{}", i)
                } else {
                    format!("{}/prefixItems/{}", path, i)
                };
                if i < arr.len() && !arr[i].is_null() {
                    unreached_collect(pi, root, &arr[i], &child, visited, paths);
                } else {
                    paths.push(child);
                }
            }
        }
    }
}

#[test]
fn samples_roundtrip_through_output_types() {
    for tool in super::registered_tools() {
        let name = &tool.name;
        let s = sample(name);
        let rt = roundtrip_by_tool(name, &s);
        assert_eq!(rt, s, "roundtrip mismatch for {name}");
    }
}

#[test]
fn sparse_outputs_validate() {
    for tool in super::registered_tools() {
        let name = &tool.name;
        let s = sample(name);
        let schema = &tool.output_schema;
        let sparse = strip_optional(schema, schema, &s);
        let rt = roundtrip_by_tool(name, &sparse);
        let validator = jsonschema::draft202012::new(schema)
            .unwrap_or_else(|e| panic!("invalid schema for {name}: {e}"));
        let mut errs = vec![];
        for e in validator.iter_errors(&rt) {
            let p = e.instance_path().as_str();
            let p = if p.is_empty() { "/" } else { p };
            errs.push(format!("{name}: {p}: {e}"));
        }
        assert!(errs.is_empty(), "sparse failed for {name}: {errs:?}");
    }
}

#[test]
fn output_schemas_have_no_any_nodes() {
    let mut bad = vec![];
    for tool in super::registered_tools() {
        let name = &tool.name;
        let schema = &tool.output_schema;
        let loose = loose_nodes(schema);
        if !loose.is_empty() {
            bad.push((name.to_string(), loose));
        }
    }
    assert!(bad.is_empty(), "loose nodes: {bad:?}");
}

#[test]
fn samples_cover_their_schemas() {
    let mut bad = vec![];
    for tool in super::registered_tools() {
        let name = &tool.name;
        let s = sample(name);
        let schema = &tool.output_schema;
        let unre = unreached(schema, &s);
        if !unre.is_empty() {
            bad.push((name.to_string(), unre));
        }
    }
    assert!(bad.is_empty(), "unreached paths: {bad:?}");
}
