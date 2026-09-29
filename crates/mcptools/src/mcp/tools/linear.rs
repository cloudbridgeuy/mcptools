use crate::prelude::{eprintln, *};
use serde::Deserialize;

use super::JsonRpcError;
use mcptools_core::linear::{
    CommentListOutput, CycleListOutput, IssueListOutput, LabelListOutput, Paginated,
    ProjectListOutput, RelationAddOutput, RelationListOutput, RelationRemoveOutput,
    StateListOutput, TeamListOutput, UserListOutput,
};

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
    let _args: Option<crate::linear::args::AuthStatusArgs> =
        serde_json::from_value(arguments.unwrap_or(serde_json::Value::Null)).map_err(invalid)?;
    let _ = _args;
    if global.verbose {
        eprintln!("Calling linear_auth_status");
    }
    let client = linear_client()?;
    let viewer = crate::linear::auth::auth_status_data(&client)
        .await
        .map_err(exec)?;
    super::to_dual_result(viewer)
}

pub async fn handle_linear_issue_get(
    arguments: Option<serde_json::Value>,
    global: &crate::Global,
) -> Result<serde_json::Value, JsonRpcError> {
    let args: crate::linear::args::IssueGetArgs = parse_args(arguments)?;
    if global.verbose {
        eprintln!("Calling linear_issue_get: id={}", args.id);
    }
    let client = linear_client()?;
    let output = crate::linear::issue::issue_get_output(&client, &args.id)
        .await
        .map_err(exec)?;
    super::to_dual_result(output)
}

pub async fn handle_linear_issue_list(
    arguments: Option<serde_json::Value>,
    global: &crate::Global,
) -> Result<serde_json::Value, JsonRpcError> {
    let args: crate::linear::args::IssueListArgs = parse_args(arguments)?;
    let fields = args.fields;
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
        let page = crate::linear::issue::issues_list_data(
            &client,
            &filter,
            args.sort,
            limit,
            cursor.clone(),
        )
        .await
        .map_err(exec)?;
        page_info = page.page_info.clone();
        nodes.extend(page.nodes);
        if !fetch_all || !page_info.has_next {
            break;
        }
        cursor = page_info.end_cursor.clone();
    }
    super::to_dual_result_projected(
        IssueListOutput::from(Paginated { nodes, page_info }),
        fields.as_deref(),
    )
}

pub async fn handle_linear_comment_list(
    arguments: Option<serde_json::Value>,
    global: &crate::Global,
) -> Result<serde_json::Value, JsonRpcError> {
    let args: crate::linear::args::CommentListArgs = parse_args(arguments)?;
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
        if !fetch_all || !page_info.has_next {
            break;
        }
        cursor = page_info.end_cursor.clone();
    }
    super::to_dual_result(CommentListOutput::from(Paginated { nodes, page_info }))
}

pub async fn handle_linear_relation_list(
    arguments: Option<serde_json::Value>,
    global: &crate::Global,
) -> Result<serde_json::Value, JsonRpcError> {
    let args: crate::linear::args::RelationListArgs = parse_args(arguments)?;
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
        if !fetch_all || !page_info.has_next {
            break;
        }
        cursor = page_info.end_cursor.clone();
    }
    super::to_dual_result(RelationListOutput::from(Paginated { nodes, page_info }))
}

pub async fn handle_linear_team_list(
    arguments: Option<serde_json::Value>,
    global: &crate::Global,
) -> Result<serde_json::Value, JsonRpcError> {
    let args: crate::linear::args::TeamListArgs = parse_args(arguments)?;
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
        if !fetch_all || !page_info.has_next {
            break;
        }
        cursor = page_info.end_cursor.clone();
    }
    super::to_dual_result(TeamListOutput::from(Paginated { nodes, page_info }))
}

pub async fn handle_linear_team_get(
    arguments: Option<serde_json::Value>,
    global: &crate::Global,
) -> Result<serde_json::Value, JsonRpcError> {
    let args: crate::linear::args::TeamGetArgs = parse_args(arguments)?;
    let selector = [
        args.selector.as_str(),
        args.team.as_deref().unwrap_or(""),
        args.id.as_deref().unwrap_or(""),
    ]
    .into_iter()
    .find(|s| !s.trim().is_empty())
    .ok_or(JsonRpcError {
        code: -32602,
        message: "Must provide 'selector'".to_string(),
        data: None,
    })?;
    if global.verbose {
        eprintln!("Calling linear_team_get: selector={}", selector);
    }
    let client = linear_client()?;
    let team = crate::linear::discover::teams_get_data(&client, selector)
        .await
        .map_err(exec)?;
    super::to_dual_result(team)
}

pub async fn handle_linear_project_list(
    arguments: Option<serde_json::Value>,
    global: &crate::Global,
) -> Result<serde_json::Value, JsonRpcError> {
    let args: crate::linear::args::ProjectListArgs = parse_args(arguments)?;
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
        if !fetch_all || !page_info.has_next {
            break;
        }
        cursor = page_info.end_cursor.clone();
    }
    super::to_dual_result(ProjectListOutput::from(Paginated { nodes, page_info }))
}

pub async fn handle_linear_project_get(
    arguments: Option<serde_json::Value>,
    global: &crate::Global,
) -> Result<serde_json::Value, JsonRpcError> {
    let args: crate::linear::args::ProjectGetArgs = parse_args(arguments)?;
    if global.verbose {
        eprintln!("Calling linear_project_get: id={}", args.id);
    }
    let client = linear_client()?;
    let item = crate::linear::discover::projects_get_data(&client, &args.id, &args.team)
        .await
        .map_err(exec)?;
    super::to_dual_result(item)
}

pub async fn handle_linear_user_list(
    arguments: Option<serde_json::Value>,
    global: &crate::Global,
) -> Result<serde_json::Value, JsonRpcError> {
    let args: crate::linear::args::UserListArgs = parse_args(arguments)?;
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
    super::to_dual_result(UserListOutput::from(data))
}

pub async fn handle_linear_state_list(
    arguments: Option<serde_json::Value>,
    global: &crate::Global,
) -> Result<serde_json::Value, JsonRpcError> {
    let args: crate::linear::args::StateListArgs = parse_args(arguments)?;
    if global.verbose {
        eprintln!("Calling linear_state_list: team={}", args.team);
    }
    let client = linear_client()?;
    let data = crate::linear::discover::states_list_data(&client, &args.team)
        .await
        .map_err(exec)?;
    super::to_dual_result(StateListOutput::from(data))
}

pub async fn handle_linear_label_list(
    arguments: Option<serde_json::Value>,
    global: &crate::Global,
) -> Result<serde_json::Value, JsonRpcError> {
    let args: crate::linear::args::LabelListArgs = parse_args(arguments)?;
    if global.verbose {
        eprintln!("Calling linear_label_list: team={}", args.team);
    }
    let client = linear_client()?;
    let data = crate::linear::discover::labels_list_data(&client, &args.team)
        .await
        .map_err(exec)?;
    super::to_dual_result(LabelListOutput::from(data))
}

pub async fn handle_linear_issue_create(
    arguments: Option<serde_json::Value>,
    global: &crate::Global,
) -> Result<serde_json::Value, JsonRpcError> {
    let args: crate::linear::args::IssueCreateArgs = parse_args(arguments)?;
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
        args.project.as_deref(),
        args.parent.as_deref(),
    )
    .await
    .map_err(exec)?;
    super::to_dual_result(created)
}

pub async fn handle_linear_issue_update(
    arguments: Option<serde_json::Value>,
    global: &crate::Global,
) -> Result<serde_json::Value, JsonRpcError> {
    let args: crate::linear::args::IssueUpdateArgs = parse_args(arguments)?;
    let clear_parent = args.clear_parent.unwrap_or(false);
    if args.parent.is_some() && clear_parent {
        return Err(exec(color_eyre::eyre::eyre!(
            "Linear issue update accepts only one of parent or clearParent"
        )));
    }
    if global.verbose {
        eprintln!("Calling linear_issue_update: id={}", args.id);
    }
    let parent = if clear_parent {
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
    super::to_dual_result(updated)
}

pub async fn handle_linear_comment_create(
    arguments: Option<serde_json::Value>,
    global: &crate::Global,
) -> Result<serde_json::Value, JsonRpcError> {
    let args: crate::linear::args::CommentCreateArgs = parse_args(arguments)?;
    if global.verbose {
        eprintln!("Calling linear_comment_create: id={}", args.id);
    }
    let body = crate::linear::comments::normalize_comment_body(&args.body).map_err(exec)?;
    let client = linear_client()?;
    let created = crate::linear::comments::comment_create_data(&client, &args.id, &body)
        .await
        .map_err(exec)?;
    super::to_dual_result(created)
}

pub async fn handle_linear_relation_add(
    arguments: Option<serde_json::Value>,
    global: &crate::Global,
) -> Result<serde_json::Value, JsonRpcError> {
    let args: crate::linear::args::RelationAddArgs = parse_args(arguments)?;
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
    super::to_dual_result(RelationAddOutput {
        status: if created {
            "created".to_string()
        } else {
            "already_exists".to_string()
        },
        id: relation.id,
        rel_type: relation.rel_type,
        issue: relation.issue,
        related_issue: relation.related_issue,
    })
}

pub async fn handle_linear_relation_remove(
    arguments: Option<serde_json::Value>,
    global: &crate::Global,
) -> Result<serde_json::Value, JsonRpcError> {
    let args: crate::linear::args::RelationRemoveArgs = parse_args(arguments)?;
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
    super::to_dual_result(RelationRemoveOutput {
        deleted_relation_id: deleted,
    })
}

pub async fn handle_linear_cycle_list(
    arguments: Option<serde_json::Value>,
    global: &crate::Global,
) -> Result<serde_json::Value, JsonRpcError> {
    let args: crate::linear::args::CycleListArgs = parse_args(arguments)?;
    if global.verbose {
        eprintln!("Calling linear_cycle_list: team={}", args.team);
    }
    let client = linear_client()?;
    let data = crate::linear::discover::cycles_list_data(&client, &args.team)
        .await
        .map_err(exec)?;
    super::to_dual_result(CycleListOutput::from(data))
}
