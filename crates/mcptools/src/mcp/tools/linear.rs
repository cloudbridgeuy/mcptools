use crate::prelude::{eprintln, *};
use serde::Deserialize;

use super::{CallToolResult, Content, JsonRpcError};

fn invalid(e: serde_json::Error) -> JsonRpcError {
    JsonRpcError {
        code: -32602,
        message: format!("Invalid arguments: {e}"),
        data: None,
    }
}

fn config(e: color_eyre::eyre::Report) -> JsonRpcError {
    JsonRpcError {
        code: -32603,
        message: format!("Configuration error: {e}"),
        data: None,
    }
}

fn exec(e: color_eyre::eyre::Report) -> JsonRpcError {
    JsonRpcError {
        code: -32603,
        message: format!("Tool execution error: {e}"),
        data: None,
    }
}

fn text_result(data: impl serde::Serialize) -> Result<serde_json::Value, JsonRpcError> {
    let text = serde_json::to_string_pretty(&data).map_err(|e| JsonRpcError {
        code: -32603,
        message: format!("Serialization error: {e}"),
        data: None,
    })?;
    let result = CallToolResult {
        content: vec![Content::Text { text }],
        is_error: None,
    };
    serde_json::to_value(result).map_err(|e| JsonRpcError {
        code: -32603,
        message: format!("Internal error: {e}"),
        data: None,
    })
}

fn parse_args<T: for<'de> Deserialize<'de>>(
    arguments: Option<serde_json::Value>,
) -> Result<T, JsonRpcError> {
    serde_json::from_value(arguments.unwrap_or(serde_json::Value::Null)).map_err(invalid)
}

fn linear_client() -> Result<reqwest::Client, JsonRpcError> {
    let cfg = crate::linear::config::LinearConfig::from_env().map_err(config)?;
    crate::linear::client::build_client(&cfg).map_err(exec)
}

pub async fn handle_linear_auth_status(
    arguments: Option<serde_json::Value>,
    global: &crate::Global,
) -> Result<serde_json::Value, JsonRpcError> {
    #[derive(Deserialize)]
    struct NoArgs {}
    let _args: Option<NoArgs> =
        serde_json::from_value(arguments.unwrap_or(serde_json::Value::Null)).map_err(invalid)?;
    let _ = _args;
    if global.verbose {
        eprintln!("Calling linear_auth_status");
    }
    let client = linear_client()?;
    let viewer = crate::linear::auth::auth_status_data(&client)
        .await
        .map_err(exec)?;
    text_result(viewer)
}

pub async fn handle_linear_issue_get(
    arguments: Option<serde_json::Value>,
    global: &crate::Global,
) -> Result<serde_json::Value, JsonRpcError> {
    #[derive(Deserialize)]
    struct Args {
        id: String,
    }
    let args: Args = parse_args(arguments)?;
    if global.verbose {
        eprintln!("Calling linear_issue_get: id={}", args.id);
    }
    let client = linear_client()?;
    let found = crate::linear::issue::issue_get_data(&client, &args.id)
        .await
        .map_err(exec)?;
    text_result(found)
}

pub async fn handle_linear_issue_list(
    arguments: Option<serde_json::Value>,
    global: &crate::Global,
) -> Result<serde_json::Value, JsonRpcError> {
    #[derive(Deserialize)]
    struct Args {
        team: Option<String>,
        project: Option<String>,
        assignee: Option<String>,
        state: Option<String>,
        label: Option<String>,
        cycle: Option<String>,
        query: Option<String>,
        #[serde(rename = "updatedAfter")]
        updated_after: Option<String>,
        limit: Option<u32>,
        cursor: Option<String>,
        all: Option<bool>,
    }
    let args: Args = parse_args(arguments)?;
    if global.verbose {
        eprintln!(
            "Calling linear_issue_list: team={:?}, project={:?}, limit={:?}",
            args.team, args.project, args.limit
        );
    }
    let client = linear_client()?;
    let filter = crate::linear::issue::resolve_issue_filter(
        &client,
        args.team.as_deref(),
        args.project.as_deref(),
        args.assignee.as_deref(),
        args.state.as_deref(),
        args.label.as_deref(),
        args.cycle.as_deref(),
        args.query.as_deref(),
        args.updated_after.as_deref(),
    )
    .await
    .map_err(exec)?;
    let limit = args.limit.unwrap_or(25);
    let fetch_all = args.all.unwrap_or(false);
    let mut nodes = Vec::new();
    let mut cursor = args.cursor.clone();
    let mut page_info = mcptools_core::linear::PageInfo {
        has_next: false,
        end_cursor: None,
    };
    loop {
        let page = crate::linear::issue::issues_list_data(&client, &filter, limit, cursor.clone())
            .await
            .map_err(exec)?;
        page_info = page.page_info.clone();
        nodes.extend(page.nodes);
        if !fetch_all || !page_info.has_next || nodes.len() >= 50 {
            break;
        }
        cursor = page_info.end_cursor.clone();
    }
    nodes.truncate(50);
    text_result(serde_json::json!({"nodes": nodes, "pageInfo": page_info}))
}

pub async fn handle_linear_comment_list(
    arguments: Option<serde_json::Value>,
    global: &crate::Global,
) -> Result<serde_json::Value, JsonRpcError> {
    #[derive(Deserialize)]
    struct Args {
        id: String,
        limit: Option<u32>,
        cursor: Option<String>,
        all: Option<bool>,
    }
    let args: Args = parse_args(arguments)?;
    if global.verbose {
        eprintln!("Calling linear_comment_list: id={}", args.id);
    }
    let client = linear_client()?;
    let limit = args.limit.unwrap_or(25);
    let fetch_all = args.all.unwrap_or(false);
    let mut nodes = Vec::new();
    let mut cursor = args.cursor.clone();
    let mut page_info = mcptools_core::linear::PageInfo {
        has_next: false,
        end_cursor: None,
    };
    loop {
        let page =
            crate::linear::comments::comments_list_data(&client, &args.id, limit, cursor.clone())
                .await
                .map_err(exec)?;
        page_info = page.page_info.clone();
        nodes.extend(page.nodes);
        if !fetch_all || !page_info.has_next || nodes.len() >= 50 {
            break;
        }
        cursor = page_info.end_cursor.clone();
    }
    nodes.truncate(50);
    text_result(serde_json::json!({"nodes": nodes, "pageInfo": page_info}))
}

pub async fn handle_linear_relation_list(
    arguments: Option<serde_json::Value>,
    global: &crate::Global,
) -> Result<serde_json::Value, JsonRpcError> {
    #[derive(Deserialize)]
    struct Args {
        id: String,
        limit: Option<u32>,
        cursor: Option<String>,
        all: Option<bool>,
    }
    let args: Args = parse_args(arguments)?;
    if global.verbose {
        eprintln!("Calling linear_relation_list: id={}", args.id);
    }
    let client = linear_client()?;
    let limit = args.limit.unwrap_or(25);
    let fetch_all = args.all.unwrap_or(false);
    let mut nodes = Vec::new();
    let mut cursor = args.cursor.clone();
    let mut page_info = mcptools_core::linear::PageInfo {
        has_next: false,
        end_cursor: None,
    };
    loop {
        let page =
            crate::linear::relations::relations_list_data(&client, &args.id, limit, cursor.clone())
                .await
                .map_err(exec)?;
        page_info = page.page_info.clone();
        nodes.extend(page.nodes);
        if !fetch_all || !page_info.has_next || nodes.len() >= 50 {
            break;
        }
        cursor = page_info.end_cursor.clone();
    }
    nodes.truncate(50);
    text_result(serde_json::json!({"nodes": nodes, "pageInfo": page_info}))
}

pub async fn handle_linear_team_list(
    arguments: Option<serde_json::Value>,
    global: &crate::Global,
) -> Result<serde_json::Value, JsonRpcError> {
    #[derive(Deserialize)]
    struct Args {
        limit: Option<u32>,
        cursor: Option<String>,
        all: Option<bool>,
    }
    let args: Args = parse_args(arguments)?;
    if global.verbose {
        eprintln!("Calling linear_team_list");
    }
    let client = linear_client()?;
    let limit = args.limit.unwrap_or(25);
    let fetch_all = args.all.unwrap_or(false);
    let mut nodes = Vec::new();
    let mut cursor = args.cursor.clone();
    let mut page_info = mcptools_core::linear::PageInfo {
        has_next: false,
        end_cursor: None,
    };
    loop {
        let page = crate::linear::discover::teams_list_data(&client, limit, cursor.clone())
            .await
            .map_err(exec)?;
        page_info = page.page_info.clone();
        nodes.extend(page.nodes);
        if !fetch_all || !page_info.has_next || nodes.len() >= 50 {
            break;
        }
        cursor = page_info.end_cursor.clone();
    }
    nodes.truncate(50);
    text_result(serde_json::json!({"nodes": nodes, "pageInfo": page_info}))
}

pub async fn handle_linear_team_get(
    arguments: Option<serde_json::Value>,
    global: &crate::Global,
) -> Result<serde_json::Value, JsonRpcError> {
    #[derive(Deserialize)]
    struct Args {
        selector: Option<String>,
        team: Option<String>,
        id: Option<String>,
    }
    let args: Args = parse_args(arguments)?;
    let selector = args
        .selector
        .or(args.team)
        .or(args.id)
        .ok_or(JsonRpcError {
            code: -32602,
            message: "Must provide 'selector'".to_string(),
            data: None,
        })?;
    if global.verbose {
        eprintln!("Calling linear_team_get: selector={}", selector);
    }
    let client = linear_client()?;
    let team = crate::linear::discover::teams_get_data(&client, &selector)
        .await
        .map_err(exec)?;
    text_result(team)
}

pub async fn handle_linear_project_list(
    arguments: Option<serde_json::Value>,
    global: &crate::Global,
) -> Result<serde_json::Value, JsonRpcError> {
    #[derive(Deserialize)]
    struct Args {
        team: String,
        limit: Option<u32>,
        cursor: Option<String>,
        all: Option<bool>,
    }
    let args: Args = parse_args(arguments)?;
    if global.verbose {
        eprintln!("Calling linear_project_list: team={}", args.team);
    }
    let client = linear_client()?;
    let limit = args.limit.unwrap_or(25);
    let fetch_all = args.all.unwrap_or(false);
    let mut nodes = Vec::new();
    let mut cursor = args.cursor.clone();
    let mut page_info = mcptools_core::linear::PageInfo {
        has_next: false,
        end_cursor: None,
    };
    loop {
        let page =
            crate::linear::discover::projects_list_data(&client, &args.team, limit, cursor.clone())
                .await
                .map_err(exec)?;
        page_info = page.page_info.clone();
        nodes.extend(page.nodes);
        if !fetch_all || !page_info.has_next || nodes.len() >= 50 {
            break;
        }
        cursor = page_info.end_cursor.clone();
    }
    nodes.truncate(50);
    text_result(serde_json::json!({"nodes": nodes, "pageInfo": page_info}))
}

pub async fn handle_linear_project_get(
    arguments: Option<serde_json::Value>,
    global: &crate::Global,
) -> Result<serde_json::Value, JsonRpcError> {
    #[derive(Deserialize)]
    struct Args {
        id: String,
        team: String,
    }
    let args: Args = parse_args(arguments)?;
    if global.verbose {
        eprintln!("Calling linear_project_get: id={}", args.id);
    }
    let client = linear_client()?;
    let item = crate::linear::discover::projects_get_data(&client, &args.id, &args.team)
        .await
        .map_err(exec)?;
    text_result(item)
}

pub async fn handle_linear_user_list(
    arguments: Option<serde_json::Value>,
    global: &crate::Global,
) -> Result<serde_json::Value, JsonRpcError> {
    #[derive(Deserialize)]
    struct Args {
        query: String,
        limit: Option<u32>,
        cursor: Option<String>,
    }
    let args: Args = parse_args(arguments)?;
    if global.verbose {
        eprintln!("Calling linear_user_list: query={}", args.query);
    }
    let client = linear_client()?;
    let data = crate::linear::discover::users_list_data(
        &client,
        &args.query,
        args.limit.unwrap_or(25),
        args.cursor.clone(),
    )
    .await
    .map_err(exec)?;
    text_result(serde_json::json!({"nodes": data.nodes, "pageInfo": data.page_info}))
}

pub async fn handle_linear_state_list(
    arguments: Option<serde_json::Value>,
    global: &crate::Global,
) -> Result<serde_json::Value, JsonRpcError> {
    #[derive(Deserialize)]
    struct Args {
        team: String,
    }
    let args: Args = parse_args(arguments)?;
    if global.verbose {
        eprintln!("Calling linear_state_list: team={}", args.team);
    }
    let client = linear_client()?;
    let data = crate::linear::discover::states_list_data(&client, &args.team)
        .await
        .map_err(exec)?;
    text_result(serde_json::json!({"nodes": data.nodes, "pageInfo": data.page_info}))
}

pub async fn handle_linear_label_list(
    arguments: Option<serde_json::Value>,
    global: &crate::Global,
) -> Result<serde_json::Value, JsonRpcError> {
    #[derive(Deserialize)]
    struct Args {
        team: String,
    }
    let args: Args = parse_args(arguments)?;
    if global.verbose {
        eprintln!("Calling linear_label_list: team={}", args.team);
    }
    let client = linear_client()?;
    let data = crate::linear::discover::labels_list_data(&client, &args.team)
        .await
        .map_err(exec)?;
    text_result(serde_json::json!({"nodes": data.nodes, "pageInfo": data.page_info}))
}

pub async fn handle_linear_issue_create(
    arguments: Option<serde_json::Value>,
    global: &crate::Global,
) -> Result<serde_json::Value, JsonRpcError> {
    #[derive(Deserialize)]
    struct Args {
        team: String,
        title: String,
        description: Option<String>,
        state: Option<String>,
        assignee: Option<String>,
    }
    let args: Args = parse_args(arguments)?;
    if global.verbose {
        eprintln!(
            "Calling linear_issue_create: team={}, title={}",
            args.team, args.title
        );
    }
    let client = linear_client()?;
    let created = crate::linear::issue::issue_create_data(
        &client,
        &args.team,
        &args.title,
        args.description.as_deref(),
        args.state.as_deref(),
        args.assignee.as_deref(),
    )
    .await
    .map_err(exec)?;
    text_result(created)
}

pub async fn handle_linear_issue_update(
    arguments: Option<serde_json::Value>,
    global: &crate::Global,
) -> Result<serde_json::Value, JsonRpcError> {
    #[derive(Deserialize)]
    struct Args {
        id: String,
        title: Option<String>,
        description: Option<String>,
        state: Option<String>,
        team: Option<String>,
        assignee: Option<String>,
        parent: Option<String>,
        #[serde(default, rename = "clearParent", alias = "clear_parent")]
        clear_parent: bool,
    }
    let args: Args = parse_args(arguments)?;
    if args.parent.is_some() && args.clear_parent {
        return Err(exec(color_eyre::eyre::eyre!(
            "Linear issue update accepts only one of parent or clearParent"
        )));
    }
    if global.verbose {
        eprintln!("Calling linear_issue_update: id={}", args.id);
    }
    let parent = if args.clear_parent {
        Some(None)
    } else {
        args.parent.map(Some)
    };
    let client = linear_client()?;
    let updated = crate::linear::issue::issue_update_data(
        &client,
        &args.id,
        args.title.as_deref(),
        args.description.as_deref(),
        args.state.as_deref(),
        args.team.as_deref(),
        args.assignee.as_deref(),
        parent,
    )
    .await
    .map_err(exec)?;
    text_result(updated)
}

pub async fn handle_linear_comment_create(
    arguments: Option<serde_json::Value>,
    global: &crate::Global,
) -> Result<serde_json::Value, JsonRpcError> {
    #[derive(Deserialize)]
    struct Args {
        id: String,
        body: String,
    }
    let args: Args = parse_args(arguments)?;
    if global.verbose {
        eprintln!("Calling linear_comment_create: id={}", args.id);
    }
    let body = crate::linear::comments::normalize_comment_body(&args.body).map_err(exec)?;
    let client = linear_client()?;
    let created = crate::linear::comments::comment_create_data(&client, &args.id, &body)
        .await
        .map_err(exec)?;
    text_result(created)
}

pub async fn handle_linear_relation_add(
    arguments: Option<serde_json::Value>,
    global: &crate::Global,
) -> Result<serde_json::Value, JsonRpcError> {
    #[derive(Deserialize)]
    struct Args {
        source: String,
        related: String,
        #[serde(rename = "type")]
        rel_type: String,
    }
    let args: Args = parse_args(arguments)?;
    if global.verbose {
        eprintln!(
            "Calling linear_relation_add: source={}, related={}, type={}",
            args.source, args.related, args.rel_type
        );
    }
    let client = linear_client()?;
    let (relation, created) = crate::linear::relations::relation_add_data(
        &client,
        &args.source,
        &args.related,
        &args.rel_type,
    )
    .await
    .map_err(exec)?;
    text_result(serde_json::json!({
        "status": if created { "created" } else { "already_exists" },
        "id": relation.id,
        "type": relation.rel_type,
        "issue": relation.issue,
        "relatedIssue": relation.related_issue,
    }))
}

pub async fn handle_linear_relation_remove(
    arguments: Option<serde_json::Value>,
    global: &crate::Global,
) -> Result<serde_json::Value, JsonRpcError> {
    #[derive(Deserialize)]
    struct Args {
        source: String,
        related: String,
        #[serde(rename = "type")]
        rel_type: String,
    }
    let args: Args = parse_args(arguments)?;
    if global.verbose {
        eprintln!(
            "Calling linear_relation_remove: source={}, related={}, type={}",
            args.source, args.related, args.rel_type
        );
    }
    let client = linear_client()?;
    let deleted = crate::linear::relations::relation_remove_by_triple(
        &client,
        &args.source,
        &args.related,
        &args.rel_type,
    )
    .await
    .map_err(exec)?;
    text_result(serde_json::json!({"deleted_relation_id": deleted}))
}

pub async fn handle_linear_cycle_list(
    arguments: Option<serde_json::Value>,
    global: &crate::Global,
) -> Result<serde_json::Value, JsonRpcError> {
    #[derive(Deserialize)]
    struct Args {
        team: String,
    }
    let args: Args = parse_args(arguments)?;
    if global.verbose {
        eprintln!("Calling linear_cycle_list: team={}", args.team);
    }
    let client = linear_client()?;
    let data = crate::linear::discover::cycles_list_data(&client, &args.team)
        .await
        .map_err(exec)?;
    text_result(serde_json::json!({"nodes": data.nodes, "pageInfo": data.page_info}))
}
