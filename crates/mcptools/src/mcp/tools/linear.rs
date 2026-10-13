use crate::prelude::{eprintln, *};
use serde::Deserialize;

use super::JsonRpcError;
use mcptools_core::linear::{
    CommentListOutput, CycleListOutput, IssueListOutput, LabelListOutput, Paginated,
    ProjectListOutput, RelationAddOutput, RelationListOutput, RelationRemoveOutput,
    StateListOutput, TeamListOutput, UserListOutput,
};

#[derive(Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct IssueDeleteOutput {
    pub id: String,
    pub identifier: String,
}

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

pub async fn handle_linear_issue_delete(
    arguments: Option<serde_json::Value>,
    global: &crate::Global,
) -> Result<serde_json::Value, JsonRpcError> {
    let args: crate::linear::args::IssueDeleteArgs = parse_args(arguments)?;
    if global.verbose {
        eprintln!("Calling linear_issue_delete: id={}", args.id);
    }
    let client = linear_client()?;
    let deleted = crate::linear::issue::issue_delete_data(&client, &args.id)
        .await
        .map_err(exec)?;
    super::to_dual_result(IssueDeleteOutput {
        id: deleted.id,
        identifier: deleted.identifier,
    })
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

pub async fn handle_linear_issue_graph(
    arguments: Option<serde_json::Value>,
    global: &crate::Global,
) -> Result<serde_json::Value, JsonRpcError> {
    let args: crate::linear::args::IssueGraphArgs = parse_args(arguments)?;
    let request =
        crate::linear::chart::IssueGraphRequest::new(&args.ids, args.limit, args.max_pages)
            .map_err(|error| JsonRpcError {
                code: -32602,
                message: format!("Invalid arguments: {error}"),
                data: None,
            })?;
    if global.verbose {
        eprintln!(
            "Calling linear_issue_graph: limit={}, maxPages={}",
            args.limit, args.max_pages
        );
    }
    let client = linear_client()?;
    let output = crate::linear::chart::issue_graph_data(&client, request)
        .await
        .map_err(exec)?;
    super::to_dual_result(output)
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

pub async fn handle_linear_initiative_list(
    arguments: Option<serde_json::Value>,
    global: &crate::Global,
) -> Result<serde_json::Value, JsonRpcError> {
    let args: crate::linear::args::InitiativeListArgs =
        parse_args(Some(arguments.unwrap_or_else(|| serde_json::json!({}))))?;
    args.validate().map_err(|error| JsonRpcError {
        code: -32602,
        message: format!("Invalid arguments: {error}"),
        data: None,
    })?;
    if global.verbose {
        eprintln!("Calling linear_initiative_list");
    }
    let client = linear_client()?;
    let output = crate::linear::discover::initiatives_list_data(&client, args)
        .await
        .map_err(exec)?;
    super::to_dual_result(output)
}

pub async fn handle_linear_initiative_project_add(
    arguments: Option<serde_json::Value>,
    global: &crate::Global,
) -> Result<serde_json::Value, JsonRpcError> {
    let args: crate::linear::args::InitiativeProjectAddArgs = parse_args(arguments)?;
    args.validate().map_err(|error| JsonRpcError {
        code: -32602,
        message: format!("Invalid arguments: {error}"),
        data: None,
    })?;
    if global.verbose {
        eprintln!("Calling linear_initiative_project_add");
    }
    let client = linear_client()?;
    let output = crate::linear::discover::initiative_project_add_data(&client, &args)
        .await
        .map_err(exec)?;
    super::to_dual_result(output)
}

pub async fn handle_linear_initiative_get(
    arguments: Option<serde_json::Value>,
    global: &crate::Global,
) -> Result<serde_json::Value, JsonRpcError> {
    let args: crate::linear::args::InitiativeGetArgs = parse_args(arguments)?;
    args.validate().map_err(|error| JsonRpcError {
        code: -32602,
        message: format!("Invalid arguments: {error}"),
        data: None,
    })?;
    if global.verbose {
        eprintln!("Calling linear_initiative_get: id={}", args.id);
    }
    let client = linear_client()?;
    let output = crate::linear::discover::initiatives_get_data(&client, args)
        .await
        .map_err(exec)?;
    super::to_dual_result(output)
}

pub async fn handle_linear_document_create(
    arguments: Option<serde_json::Value>,
    global: &crate::Global,
) -> Result<serde_json::Value, JsonRpcError> {
    let args: crate::linear::args::DocumentCreateArgs = parse_args(arguments)?;
    mcptools_core::linear::document_create_input(&args).map_err(|error| JsonRpcError {
        code: -32602,
        message: format!("Invalid arguments: {error}"),
        data: None,
    })?;
    if global.verbose {
        eprintln!("Calling linear_document_create: project={}", args.project);
    }
    let client = linear_client()?;
    let output = crate::linear::discover::document_create_data(&client, &args)
        .await
        .map_err(exec)?;
    super::to_dual_result(output)
}

pub async fn handle_linear_document_get(
    arguments: Option<serde_json::Value>,
    global: &crate::Global,
) -> Result<serde_json::Value, JsonRpcError> {
    let args: crate::linear::args::DocumentGetArgs = parse_args(arguments)?;
    args.validate().map_err(|error| JsonRpcError {
        code: -32602,
        message: format!("Invalid arguments: {error}"),
        data: None,
    })?;
    if global.verbose {
        eprintln!("Calling linear_document_get: id={}", args.id);
    }
    let client = linear_client()?;
    let output = crate::linear::discover::document_get_data(&client, args)
        .await
        .map_err(exec)?;
    super::to_dual_result(output)
}

pub async fn handle_linear_document_update(
    arguments: Option<serde_json::Value>,
    global: &crate::Global,
) -> Result<serde_json::Value, JsonRpcError> {
    let args: crate::linear::args::DocumentUpdateArgs = parse_args(arguments)?;
    mcptools_core::linear::document_update_input(&args).map_err(|error| JsonRpcError {
        code: -32602,
        message: format!("Invalid arguments: {error}"),
        data: None,
    })?;
    if global.verbose {
        eprintln!("Calling linear_document_update: id={}", args.id);
    }
    let client = linear_client()?;
    let output = crate::linear::discover::document_update_data(&client, &args)
        .await
        .map_err(exec)?;
    super::to_dual_result(output)
}

pub async fn handle_linear_project_status_list(
    arguments: Option<serde_json::Value>,
    global: &crate::Global,
) -> Result<serde_json::Value, JsonRpcError> {
    let args: crate::linear::args::ProjectStatusListArgs =
        parse_args(Some(arguments.unwrap_or_else(|| serde_json::json!({}))))?;
    args.validate().map_err(|error| JsonRpcError {
        code: -32602,
        message: format!("Invalid arguments: {error}"),
        data: None,
    })?;
    if global.verbose {
        eprintln!("Calling linear_project_status_list");
    }
    let client = linear_client()?;
    let output = crate::linear::discover::project_statuses_list_data(&client, args)
        .await
        .map_err(exec)?;
    super::to_dual_result(output)
}

pub async fn handle_linear_project_milestone_list(
    arguments: Option<serde_json::Value>,
    global: &crate::Global,
) -> Result<serde_json::Value, JsonRpcError> {
    let args: crate::linear::args::ProjectMilestoneListArgs = parse_args(arguments)?;
    args.validate().map_err(|error| JsonRpcError {
        code: -32602,
        message: format!("Invalid arguments: {error}"),
        data: None,
    })?;
    if global.verbose {
        eprintln!(
            "Calling linear_project_milestone_list: project={}",
            args.project
        );
    }
    let client = linear_client()?;
    let output = crate::linear::discover::project_milestones_list_data(&client, args)
        .await
        .map_err(exec)?;
    super::to_dual_result(output)
}

pub async fn handle_linear_project_update_list(
    arguments: Option<serde_json::Value>,
    global: &crate::Global,
) -> Result<serde_json::Value, JsonRpcError> {
    let args: crate::linear::args::ProjectUpdateListArgs = parse_args(arguments)?;
    args.validate().map_err(|error| JsonRpcError {
        code: -32602,
        message: format!("Invalid arguments: {error}"),
        data: None,
    })?;
    if global.verbose {
        eprintln!(
            "Calling linear_project_update_list: project={}",
            args.project
        );
    }
    let client = linear_client()?;
    let output = crate::linear::discover::project_updates_list_data(&client, args)
        .await
        .map_err(exec)?;
    super::to_dual_result(output)
}

pub async fn handle_linear_project_create(
    arguments: Option<serde_json::Value>,
    global: &crate::Global,
) -> Result<serde_json::Value, JsonRpcError> {
    let args: crate::linear::args::ProjectCreateArgs = parse_args(arguments)?;
    if global.verbose {
        eprintln!(
            "Calling linear_project_create: team={}, name={}",
            args.team, args.name
        );
    }
    let client = linear_client()?;
    let created = crate::linear::discover::projects_create_data(
        &client,
        &args.team,
        &args.name,
        args.description.as_deref(),
        args.content.as_deref(),
    )
    .await
    .map_err(exec)?;
    super::to_dual_result(created)
}

pub async fn handle_linear_project_milestone_create(
    arguments: Option<serde_json::Value>,
    global: &crate::Global,
) -> Result<serde_json::Value, JsonRpcError> {
    let args: crate::linear::args::ProjectMilestoneCreateArgs = parse_args(arguments)?;
    mcptools_core::linear::project_milestone_create_input(&args).map_err(|error| JsonRpcError {
        code: -32602,
        message: format!("Invalid arguments: {error}"),
        data: None,
    })?;
    if global.verbose {
        eprintln!(
            "Calling linear_project_milestone_create: project={}",
            args.project
        );
    }
    let client = linear_client()?;
    let created = crate::linear::discover::project_milestones_create_data(&client, &args)
        .await
        .map_err(exec)?;
    super::to_dual_result(created)
}

pub async fn handle_linear_project_milestone_update(
    arguments: Option<serde_json::Value>,
    global: &crate::Global,
) -> Result<serde_json::Value, JsonRpcError> {
    let args: crate::linear::args::ProjectMilestoneUpdateArgs = parse_args(arguments)?;
    mcptools_core::linear::project_milestone_update_input(&args).map_err(|error| JsonRpcError {
        code: -32602,
        message: format!("Invalid arguments: {error}"),
        data: None,
    })?;
    if global.verbose {
        eprintln!("Calling linear_project_milestone_update: id={}", args.id);
    }
    let client = linear_client()?;
    let updated = crate::linear::discover::project_milestones_update_data(&client, &args)
        .await
        .map_err(exec)?;
    super::to_dual_result(updated)
}

pub async fn handle_linear_project_update(
    arguments: Option<serde_json::Value>,
    global: &crate::Global,
) -> Result<serde_json::Value, JsonRpcError> {
    let args: crate::linear::args::ProjectUpdateArgs = parse_args(arguments)?;
    mcptools_core::linear::project_update_input(&args).map_err(|error| JsonRpcError {
        code: -32602,
        message: format!("Invalid arguments: {error}"),
        data: None,
    })?;
    if global.verbose {
        eprintln!("Calling linear_project_update: id={}", args.id);
    }
    let client = linear_client()?;
    let updated = crate::linear::discover::projects_update_data(&client, &args)
        .await
        .map_err(exec)?;
    super::to_dual_result(updated)
}

pub async fn handle_linear_project_update_create(
    arguments: Option<serde_json::Value>,
    global: &crate::Global,
) -> Result<serde_json::Value, JsonRpcError> {
    let args: crate::linear::args::ProjectUpdateCreateArgs = parse_args(arguments)?;
    mcptools_core::linear::project_update_create_input(&args).map_err(|error| JsonRpcError {
        code: -32602,
        message: format!("Invalid arguments: {error}"),
        data: None,
    })?;
    if global.verbose {
        eprintln!(
            "Calling linear_project_update_create: project={}",
            args.project
        );
    }
    let client = linear_client()?;
    let created = crate::linear::discover::project_updates_create_data(&client, &args)
        .await
        .map_err(exec)?;
    super::to_dual_result(created)
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
    let labels = args.labels.unwrap_or_default();
    let created = crate::linear::issue::issue_create_data(
        &client,
        &args.team,
        &args.title,
        args.description.as_deref(),
        args.state.as_deref(),
        args.assignee.as_deref(),
        args.priority,
        args.estimate,
        args.due_date.as_deref(),
        args.cycle.as_deref(),
        args.project.as_deref(),
        args.parent.as_deref(),
        &labels,
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
    let clear_labels = args.clear_labels.unwrap_or(false);
    if args.labels.is_some() && clear_labels {
        return Err(exec(color_eyre::eyre::eyre!(
            "Linear issue update accepts only one of labels or clearLabels"
        )));
    }
    let clear_estimate = args.clear_estimate.unwrap_or(false);
    let clear_due_date = args.clear_due_date.unwrap_or(false);
    let clear_cycle = args.clear_cycle.unwrap_or(false);
    let clear_project = args.clear_project.unwrap_or(false);
    for (field, supplied, clear) in [
        ("estimate", args.estimate.is_some(), clear_estimate),
        ("dueDate", args.due_date.is_some(), clear_due_date),
        ("cycle", args.cycle.is_some(), clear_cycle),
        ("project", args.project.is_some(), clear_project),
    ] {
        if supplied && clear {
            return Err(exec(color_eyre::eyre::eyre!(
                "Linear issue update accepts only one of {} or clear{}",
                field,
                match field {
                    "dueDate" => "DueDate",
                    "estimate" => "Estimate",
                    "cycle" => "Cycle",
                    _ => "Project",
                }
            )));
        }
    }
    if global.verbose {
        eprintln!("Calling linear_issue_update: id={}", args.id);
    }
    let parent = if clear_parent {
        Some(None)
    } else {
        args.parent.map(Some)
    };
    let estimate = match clear_estimate {
        true => Some(None),
        false => args.estimate.map(Some),
    };
    let due_date = match clear_due_date {
        true => Some(None),
        false => args.due_date.map(Some),
    };
    let cycle = match clear_cycle {
        true => Some(None),
        false => args.cycle.map(Some),
    };
    let project = match clear_project {
        true => Some(None),
        false => args.project.map(Some),
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
        args.priority,
        estimate,
        due_date,
        cycle,
        project,
        parent,
        args.labels.as_deref(),
        clear_labels,
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

pub async fn handle_linear_comment_update(
    arguments: Option<serde_json::Value>,
    global: &crate::Global,
) -> Result<serde_json::Value, JsonRpcError> {
    let args: crate::linear::args::CommentUpdateArgs = parse_args(arguments)?;
    mcptools_core::linear::comment_update_request(&args.id, &args.body).map_err(|error| {
        JsonRpcError {
            code: -32602,
            message: format!("Invalid arguments: {error}"),
            data: None,
        }
    })?;
    if global.verbose {
        eprintln!("Calling linear_comment_update: id={}", args.id);
    }
    let client = linear_client()?;
    let updated = crate::linear::comments::comment_update_data(&client, &args.id, &args.body)
        .await
        .map_err(exec)?;
    super::to_dual_result(updated)
}

pub async fn handle_linear_comment_delete(
    arguments: Option<serde_json::Value>,
    global: &crate::Global,
) -> Result<serde_json::Value, JsonRpcError> {
    let args: crate::linear::args::CommentDeleteArgs = parse_args(arguments)?;
    mcptools_core::linear::comment_delete_request(&args.id).map_err(|error| JsonRpcError {
        code: -32602,
        message: format!("Invalid arguments: {error}"),
        data: None,
    })?;
    if global.verbose {
        eprintln!("Calling linear_comment_delete: id={}", args.id);
    }
    let client = linear_client()?;
    let deleted = crate::linear::comments::comment_delete_data(&client, &args.id)
        .await
        .map_err(exec)?;
    super::to_dual_result(deleted)
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

#[cfg(test)]
mod tests {
    #[tokio::test(flavor = "multi_thread")]
    async fn comment_mutations_validate_and_require_write_permission() {
        let global = crate::Global {
            verbose: false,
            execute_timeout_secs: 30,
            execute_memory_mb: 64,
            execute_output_kb: 256,
        };
        for name in ["linear_comment_update", "linear_comment_delete"] {
            let tool = super::super::registered_tools()
                .into_iter()
                .find(|tool| tool.name == name)
                .unwrap();
            assert_eq!(tool.kind, super::super::ToolKind::Write);
            assert!(tool.destructive);
            for flags in [
                super::super::ServeFlags {
                    discovery: false,
                    code_mode: false,
                },
                super::super::ServeFlags {
                    discovery: true,
                    code_mode: false,
                },
                super::super::ServeFlags {
                    discovery: false,
                    code_mode: true,
                },
            ] {
                for input in [
                    serde_json::json!({}),
                    serde_json::json!({"id": " ", "body": "note"}),
                    serde_json::json!({"id": 1}),
                    serde_json::json!({"id": "c1", "extra": true}),
                ] {
                    let direct = super::super::handle_tools_call(
                        Some(serde_json::json!({"name": name, "arguments": input})),
                        &global,
                        flags,
                    )
                    .await
                    .unwrap();
                    assert!(direct["content"][0]["text"]
                        .as_str()
                        .unwrap()
                        .contains("allowWrites"));
                    let mut allowed = input.clone();
                    allowed["allowWrites"] = serde_json::json!(true);
                    assert_eq!(
                        super::super::handle_tools_call(
                            Some(serde_json::json!({"name": name, "arguments": allowed})),
                            &global,
                            flags
                        )
                        .await
                        .unwrap_err()
                        .code,
                        -32602
                    );
                    for allow in [false, true] {
                        let called = super::super::handle_tools_call(Some(serde_json::json!({"name": "call_tool", "arguments": {"name": name, "input": input, "allowWrites": allow}})), &global, flags).await;
                        if allow {
                            assert_eq!(called.unwrap_err().code, -32602);
                        } else {
                            assert!(called.unwrap()["content"][0]["text"]
                                .as_str()
                                .unwrap()
                                .contains("allowWrites"));
                        }
                        let executed = super::super::handle_tools_call(Some(serde_json::json!({"name": "execute", "arguments": {"code": format!("return await {name}({input})"), "allowWrites": allow}})), &global, flags).await.unwrap();
                        assert!(executed["structuredContent"]["error"]["message"]
                            .as_str()
                            .unwrap()
                            .contains(if allow {
                                "Invalid arguments:"
                            } else {
                                "allowWrites"
                            }));
                    }
                }
            }
        }
        assert_eq!(
            super::handle_linear_comment_update(
                Some(serde_json::json!({"id": "c1", "body": " "})),
                &global
            )
            .await
            .unwrap_err()
            .code,
            -32602
        );
    }

    use super::*;

    #[tokio::test(flavor = "multi_thread")]
    async fn document_get_inputs_dispatch_and_read_contract() {
        let global = crate::Global {
            verbose: false,
            execute_timeout_secs: 30,
            execute_memory_mb: 64,
            execute_output_kb: 256,
        };
        for input in [
            serde_json::Value::Null,
            serde_json::json!({}),
            serde_json::json!({"id": null}),
            serde_json::json!({"id": 1}),
            serde_json::json!({"id": ""}),
            serde_json::json!({"id": " "}),
            serde_json::json!({"id": "document-slug"}),
            serde_json::json!({"id": "12345678-1234-1234-1234-123456789abz"}),
            serde_json::json!({"id": "12345678-1234-1234-1234-123456789abc", "project": "Delivery"}),
        ] {
            assert_eq!(
                handle_linear_document_get(Some(input), &global)
                    .await
                    .unwrap_err()
                    .code,
                -32602
            );
        }
        assert_eq!(
            handle_linear_document_get(None, &global)
                .await
                .unwrap_err()
                .code,
            -32602
        );
        for (discovery, code_mode) in [(false, false), (true, false), (false, true)] {
            let flags = super::super::ServeFlags {
                discovery,
                code_mode,
            };
            for request in [
                serde_json::json!({"name": "linear_document_get", "arguments": {"id": "bad"}}),
                serde_json::json!({"name": "call_tool", "arguments": {"name": "linear_document_get", "input": {"id": "bad"}}}),
            ] {
                let error = super::super::handle_tools_call(Some(request), &global, flags)
                    .await
                    .unwrap_err();
                assert_eq!(error.code, -32602);
                assert!(error.message.starts_with("Invalid arguments:"));
            }
            let result = super::super::handle_tools_call(Some(serde_json::json!({"name": "execute", "arguments": {"code": "return await linear_document_get({id: 'bad'})"}})), &global, flags).await.unwrap();
            assert!(result["structuredContent"]["error"]["message"]
                .as_str()
                .unwrap()
                .contains("Invalid arguments:"));
        }
        let tool = super::super::registered_tools()
            .into_iter()
            .find(|tool| tool.name == "linear_document_get")
            .unwrap();
        assert_eq!(tool.kind, super::super::ToolKind::Read);
        assert_eq!(tool.input_schema["required"], serde_json::json!(["id"]));
        assert_eq!(tool.input_schema["additionalProperties"], false);
        let declaration = super::super::declaration(&tool);
        for field in [
            "declare function linear_document_get(",
            "id: string",
            "title: string",
            "content?: string | null",
            "createdAt: string",
            "updatedAt: string",
            "owner?: LinearDocumentGetUser | null",
            "project?: LinearDocumentGetProject | null",
            "url: string",
        ] {
            assert!(declaration.contains(field), "{field}: {declaration}");
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn initiative_get_inputs_dispatch_and_read_contract() {
        let global = crate::Global {
            verbose: false,
            execute_timeout_secs: 30,
            execute_memory_mb: 64,
            execute_output_kb: 256,
        };
        for input in [
            serde_json::Value::Null,
            serde_json::json!({}),
            serde_json::json!({"id": null}),
            serde_json::json!({"id": 1}),
            serde_json::json!({"id": ""}),
            serde_json::json!({"id": " "}),
            serde_json::json!({"id": "Roadmap"}),
            serde_json::json!({"id": "12345678-1234-1234-1234-123456789abz"}),
            serde_json::json!({"id": "12345678-1234-1234-1234-123456789abc", "team": "GUZ"}),
        ] {
            assert_eq!(
                handle_linear_initiative_get(Some(input), &global)
                    .await
                    .unwrap_err()
                    .code,
                -32602
            );
        }
        assert_eq!(
            handle_linear_initiative_get(None, &global)
                .await
                .unwrap_err()
                .code,
            -32602
        );
        for (discovery, code_mode) in [(false, false), (true, false), (false, true)] {
            let flags = super::super::ServeFlags {
                discovery,
                code_mode,
            };
            for request in [
                serde_json::json!({"name": "linear_initiative_get", "arguments": {"id": "bad"}}),
                serde_json::json!({"name": "call_tool", "arguments": {"name": "linear_initiative_get", "input": {"id": "bad"}}}),
            ] {
                let error = super::super::handle_tools_call(Some(request), &global, flags)
                    .await
                    .unwrap_err();
                assert_eq!(error.code, -32602);
                assert!(error.message.starts_with("Invalid arguments:"));
            }
            let result = super::super::handle_tools_call(Some(serde_json::json!({"name": "execute", "arguments": {"code": "return await linear_initiative_get({id: 'bad'})"}})), &global, flags).await.unwrap();
            assert!(result["structuredContent"]["error"]["message"]
                .as_str()
                .unwrap()
                .contains("Invalid arguments:"));
        }
        let tool = super::super::registered_tools()
            .into_iter()
            .find(|tool| tool.name == "linear_initiative_get")
            .unwrap();
        assert_eq!(tool.kind, super::super::ToolKind::Read);
        assert_eq!(tool.input_schema["required"], serde_json::json!(["id"]));
        assert_eq!(tool.input_schema["additionalProperties"], false);
        let declaration = super::super::declaration(&tool);
        for field in [
            "declare function linear_initiative_get(",
            "id: string",
            "content?: string | null",
            "projects:",
            "targetDate?: string | null",
            "url: string",
        ] {
            assert!(declaration.contains(field), "{field}: {declaration}");
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn initiative_inputs_discovery_and_code_mode_are_read_only() {
        let global = crate::Global {
            verbose: false,
            execute_timeout_secs: 30,
            execute_memory_mb: 64,
            execute_output_kb: 256,
        };
        for input in [
            serde_json::json!({"limit": 0}),
            serde_json::json!({"limit": 251}),
            serde_json::json!({"limit": -1}),
            serde_json::json!({"limit": 1.5}),
            serde_json::json!({"limit": null}),
            serde_json::json!({"cursor": " "}),
            serde_json::json!({"cursor": "bad\n"}),
            serde_json::json!({"cursor": null}),
            serde_json::json!({"query": " "}),
            serde_json::json!({"query": null}),
            serde_json::json!({"query": 1}),
            serde_json::json!({"all": null}),
            serde_json::json!({"team": "GUZ"}),
            serde_json::json!({"unknown": true}),
        ] {
            assert_eq!(
                handle_linear_initiative_list(Some(input), &global)
                    .await
                    .unwrap_err()
                    .code,
                -32602
            );
        }
        let args: crate::linear::args::InitiativeListArgs =
            serde_json::from_value(serde_json::json!({})).unwrap();
        assert_eq!(args.limit, 25);
        assert!(!args.all);
        assert!(args.validate().is_ok());
        for (discovery, code_mode) in [(false, false), (true, false), (false, true)] {
            let flags = super::super::ServeFlags {
                discovery,
                code_mode,
            };
            for request in [
                serde_json::json!({"name": "linear_initiative_list", "arguments": {"limit": 0}}),
                serde_json::json!({"name": "call_tool", "arguments": {"name": "linear_initiative_list", "input": {"limit": 0}}}),
            ] {
                let error = super::super::handle_tools_call(Some(request), &global, flags)
                    .await
                    .unwrap_err();
                assert_eq!(error.code, -32602);
                assert!(error.message.starts_with("Invalid arguments:"));
            }
            let result = super::super::handle_tools_call(Some(serde_json::json!({"name": "execute", "arguments": {"code": "return await linear_initiative_list({limit: 0})"}})), &global, flags).await.unwrap();
            assert!(result["structuredContent"]["error"]["message"]
                .as_str()
                .unwrap()
                .contains("Invalid arguments:"));
        }
        let tool = super::super::registered_tools()
            .into_iter()
            .find(|tool| tool.name == "linear_initiative_list")
            .unwrap();
        assert_eq!(tool.kind, super::super::ToolKind::Read);
        let declaration = super::super::declaration(&tool);
        for field in [
            "declare function linear_initiative_list(input: LinearInitiativeListInput): Promise<LinearInitiativeListOutput>",
            "limit?: number", "cursor?: string", "all?: boolean", "query?: string", "targetDate?: string | null", "owner?: LinearInitiativeListUser | null", "pageInfo:",
            "\"Active\"", "\"Proposed\"", "url: string", "id: string",
        ] { assert!(declaration.contains(field), "{field}: {declaration}"); }
        assert_eq!(tool.input_schema["additionalProperties"], false);
        assert_eq!(tool.input_schema["properties"]["limit"]["default"], 25);
        assert!(tool.input_schema["properties"].get("team").is_none());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn initiative_project_add_contract_dispatch_and_write_gates() {
        let global = crate::Global {
            verbose: false,
            execute_timeout_secs: 30,
            execute_memory_mb: 64,
            execute_output_kb: 256,
        };
        let id = "12345678-1234-1234-1234-123456789abc";
        for input in [
            serde_json::Value::Null,
            serde_json::json!({}),
            serde_json::json!({"initiative": "bad", "project": id}),
            serde_json::json!({"initiative": id, "project": "Example"}),
            serde_json::json!({"initiative": id, "project": id, "team": " "}),
            serde_json::json!({"initiative": id, "project": id, "extra": true}),
            serde_json::json!({"initiative": id, "project": "12345678-1234-1234-1234-123456789xyz", "team": "GUZ"}),
        ] {
            assert_eq!(
                handle_linear_initiative_project_add(Some(input), &global)
                    .await
                    .unwrap_err()
                    .code,
                -32602
            );
        }
        for flags in [
            super::super::ServeFlags {
                discovery: false,
                code_mode: false,
            },
            super::super::ServeFlags {
                discovery: true,
                code_mode: false,
            },
            super::super::ServeFlags {
                discovery: false,
                code_mode: true,
            },
        ] {
            assert_eq!(super::super::handle_tools_call(Some(serde_json::json!({"name": "linear_initiative_project_add", "arguments": {"allowWrites": true}})), &global, flags).await.unwrap_err().code, -32602);
            for allow in [false, true] {
                let call = super::super::handle_tools_call(Some(serde_json::json!({"name": "call_tool", "arguments": {"name": "linear_initiative_project_add", "input": {}, "allowWrites": allow}})), &global, flags).await;
                if allow {
                    assert_eq!(call.unwrap_err().code, -32602);
                } else {
                    let denied = call.unwrap();
                    assert_eq!(denied["isError"], true);
                    assert!(denied["content"][0]["text"]
                        .as_str()
                        .unwrap()
                        .contains("allowWrites"));
                }
                let result = super::super::handle_tools_call(Some(serde_json::json!({"name": "execute", "arguments": {"code": "return await linear_initiative_project_add({})", "allowWrites": allow}})), &global, flags).await.unwrap();
                assert!(result["structuredContent"]["error"]["message"]
                    .as_str()
                    .unwrap()
                    .contains(if allow {
                        "Invalid arguments:"
                    } else {
                        "allowWrites"
                    }));
            }
        }
        let tool = super::super::registered_tools()
            .into_iter()
            .find(|tool| tool.name == "linear_initiative_project_add")
            .unwrap();
        assert_eq!(tool.kind, super::super::ToolKind::Write);
        let declaration = super::super::declaration(&tool);
        for field in [
            "declare function linear_initiative_project_add",
            "initiative: string",
            "project: string",
            "team?: string",
        ] {
            assert!(declaration.contains(field), "{declaration}");
        }
        let sample: serde_json::Value = serde_json::from_str(include_str!(
            "../../../tests/contract_samples/linear_initiative_project_add.json"
        ))
        .unwrap();
        let validator = jsonschema::draft202012::new(&tool.output_schema).unwrap();
        assert!(validator.is_valid(&sample));
        for field in ["id", "initiative", "project"] {
            let mut incomplete = sample.clone();
            incomplete.as_object_mut().unwrap().remove(field);
            assert!(!validator.is_valid(&incomplete));
        }
    }

    #[tokio::test]
    async fn project_update_create_contract_dispatch_and_write_gates() {
        let global = crate::Global {
            verbose: false,
            execute_timeout_secs: 30,
            execute_memory_mb: 64,
            execute_output_kb: 256,
        };
        let id = "12345678-1234-1234-1234-123456789abc";
        for input in [
            serde_json::Value::Null,
            serde_json::json!({}),
            serde_json::json!({"project": id}),
            serde_json::json!({"body": "Report"}),
            serde_json::json!({"project": id, "body": null}),
            serde_json::json!({"project": id, "body": 1}),
            serde_json::json!({"project": id, "body": " "}),
            serde_json::json!({"project": "Example", "body": "Report"}),
            serde_json::json!({"project": id, "body": "Report", "team": 1}),
            serde_json::json!({"project": id, "body": "Report", "health": "unknown"}),
            serde_json::json!({"project": id, "body": "Report", "health": 1}),
            serde_json::json!({"project": id, "body": "Report", "extra": 1}),
        ] {
            assert_eq!(
                handle_linear_project_update_create(Some(input), &global)
                    .await
                    .unwrap_err()
                    .code,
                -32602
            );
        }
        for flags in [
            super::super::ServeFlags {
                discovery: false,
                code_mode: false,
            },
            super::super::ServeFlags {
                discovery: true,
                code_mode: false,
            },
            super::super::ServeFlags {
                discovery: false,
                code_mode: true,
            },
        ] {
            let direct = super::super::handle_tools_call(Some(serde_json::json!({"name": "linear_project_update_create", "arguments": {"project": id, "allowWrites": true}})), &global, flags).await.unwrap_err();
            assert_eq!(direct.code, -32602);
            for allow in [false, true] {
                let call = super::super::handle_tools_call(Some(serde_json::json!({"name": "call_tool", "arguments": {"name": "linear_project_update_create", "input": {"project": id}, "allowWrites": allow}})), &global, flags).await;
                if allow {
                    assert_eq!(call.unwrap_err().code, -32602);
                } else {
                    let denied = call.unwrap();
                    assert_eq!(denied["isError"], true);
                    assert!(denied["content"][0]["text"]
                        .as_str()
                        .unwrap()
                        .contains("allowWrites"));
                }
                let result = super::super::handle_tools_call(Some(serde_json::json!({"name": "execute", "arguments": {"code": format!("return await linear_project_update_create({{project: '{id}'}})"), "allowWrites": allow}})), &global, flags).await.unwrap();
                assert!(result["structuredContent"]["error"]["message"]
                    .as_str()
                    .unwrap()
                    .contains(if allow {
                        "Invalid arguments:"
                    } else {
                        "allowWrites"
                    }));
            }
        }
        let tool = super::super::registered_tools()
            .into_iter()
            .find(|tool| tool.name == "linear_project_update_create")
            .unwrap();
        assert_eq!(tool.kind, super::super::ToolKind::Write);
        let declaration = super::super::declaration(&tool);
        for field in [
            "declare function linear_project_update_create",
            "project: string",
            "body: string",
            "health?:",
            "onTrack",
            "atRisk",
            "offTrack",
        ] {
            assert!(declaration.contains(field), "{field}: {declaration}");
        }
        let validator = jsonschema::draft202012::new(&tool.input_schema).unwrap();
        assert!(validator.is_valid(&serde_json::json!({"project": id, "body": "Report"})));
        assert!(!validator
            .is_valid(&serde_json::json!({"project": id, "body": "Report", "health": "unknown"})));
        let sample: serde_json::Value = serde_json::from_str(include_str!(
            "../../../tests/contract_samples/linear_project_update_create.json"
        ))
        .unwrap();
        let validator = jsonschema::draft202012::new(&tool.output_schema).unwrap();
        assert!(validator.is_valid(&sample));
        for field in ["id", "body", "health", "createdAt", "url", "project"] {
            let mut incomplete = sample.clone();
            incomplete.as_object_mut().unwrap().remove(field);
            assert!(!validator.is_valid(&incomplete));
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn document_update_contract_dispatch_and_write_gates() {
        let global = crate::Global {
            verbose: false,
            execute_timeout_secs: 30,
            execute_memory_mb: 64,
            execute_output_kb: 256,
        };
        let id = "12345678-1234-1234-1234-123456789abc";
        for input in [
            serde_json::Value::Null,
            serde_json::json!({}),
            serde_json::json!({"title": "Plan"}),
            serde_json::json!({"id": null, "title": "Plan"}),
            serde_json::json!({"id": 1, "title": "Plan"}),
            serde_json::json!({"id": " ", "title": "Plan"}),
            serde_json::json!({"id": "slug", "title": "Plan"}),
            serde_json::json!({"id": id}),
            serde_json::json!({"id": id, "title": null, "content": null}),
            serde_json::json!({"id": id, "title": 1}),
            serde_json::json!({"id": id, "title": " "}),
            serde_json::json!({"id": id, "content": []}),
            serde_json::json!({"id": id, "title": "Plan", "projectId": id}),
        ] {
            assert_eq!(
                handle_linear_document_update(Some(input), &global)
                    .await
                    .unwrap_err()
                    .code,
                -32602
            );
        }
        assert_eq!(
            handle_linear_document_update(None, &global)
                .await
                .unwrap_err()
                .code,
            -32602
        );
        for flags in [
            super::super::ServeFlags {
                discovery: false,
                code_mode: false,
            },
            super::super::ServeFlags {
                discovery: true,
                code_mode: false,
            },
            super::super::ServeFlags {
                discovery: false,
                code_mode: true,
            },
        ] {
            let direct = super::super::handle_tools_call(
                Some(
                    serde_json::json!({"name": "linear_document_update", "arguments": {"id": id, "allowWrites": true}}),
                ),
                &global,
                flags,
            )
            .await
            .unwrap_err();
            assert_eq!(direct.code, -32602);
            for allow in [false, true] {
                let call = super::super::handle_tools_call(Some(serde_json::json!({"name": "call_tool", "arguments": {"name": "linear_document_update", "input": {"id": id}, "allowWrites": allow}})), &global, flags).await;
                if allow {
                    assert_eq!(call.unwrap_err().code, -32602);
                } else {
                    let denied = call.unwrap();
                    assert_eq!(denied["isError"], true);
                    assert!(denied["content"][0]["text"]
                        .as_str()
                        .unwrap()
                        .contains("allowWrites"));
                }
                let result = super::super::handle_tools_call(Some(serde_json::json!({"name": "execute", "arguments": {"code": format!("return await linear_document_update({{id: '{id}'}})"), "allowWrites": allow}})), &global, flags).await.unwrap();
                assert!(result["structuredContent"]["error"]["message"]
                    .as_str()
                    .unwrap()
                    .contains(if allow {
                        "Invalid arguments:"
                    } else {
                        "allowWrites"
                    }));
            }
        }
        let tool = super::super::registered_tools()
            .into_iter()
            .find(|tool| tool.name == "linear_document_update")
            .unwrap();
        assert_eq!(tool.kind, super::super::ToolKind::Write);
        let declaration = super::super::declaration(&tool);
        for field in [
            "declare function linear_document_update",
            "id: string",
            "title?: string",
            "content?: string",
        ] {
            assert!(declaration.contains(field), "{field}: {declaration}");
        }
        let validator = jsonschema::draft202012::new(&tool.input_schema).unwrap();
        assert!(validator.is_valid(&serde_json::json!({"id": id, "title": "Plan"})));
        assert!(validator.is_valid(&serde_json::json!({"id": id, "content": ""})));
        let mut sample: serde_json::Value = serde_json::from_str(include_str!(
            "../../../tests/contract_samples/linear_document_update.json"
        ))
        .unwrap();
        let output_validator = jsonschema::draft202012::new(&tool.output_schema).unwrap();
        assert!(output_validator.is_valid(&sample));
        sample["content"] = serde_json::Value::Null;
        sample["project"] = serde_json::Value::Null;
        assert!(output_validator.is_valid(&sample));
        let output: mcptools_core::linear::DocumentUpdateOutput =
            serde_json::from_value(sample.clone()).unwrap();
        assert_eq!(serde_json::to_value(output).unwrap(), sample);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn document_create_contract_dispatch_and_write_gates() {
        let global = crate::Global {
            verbose: false,
            execute_timeout_secs: 30,
            execute_memory_mb: 64,
            execute_output_kb: 256,
        };
        let id = "12345678-1234-1234-1234-123456789abc";
        for input in [
            serde_json::Value::Null,
            serde_json::json!({}),
            serde_json::json!({"title": "Plan"}),
            serde_json::json!({"project": id, "title": null}),
            serde_json::json!({"project": id, "title": 1}),
            serde_json::json!({"project": id, "title": " "}),
            serde_json::json!({"project": id, "title": "Plan", "team": 1}),
            serde_json::json!({"project": id, "title": "Plan", "content": []}),
            serde_json::json!({"project": id, "title": "Plan", "extra": 1}),
        ] {
            assert_eq!(
                handle_linear_document_create(Some(input), &global)
                    .await
                    .unwrap_err()
                    .code,
                -32602
            );
        }
        for flags in [
            super::super::ServeFlags {
                discovery: false,
                code_mode: false,
            },
            super::super::ServeFlags {
                discovery: true,
                code_mode: false,
            },
            super::super::ServeFlags {
                discovery: false,
                code_mode: true,
            },
        ] {
            let direct = super::super::handle_tools_call(Some(serde_json::json!({"name": "linear_document_create", "arguments": {"project": id, "allowWrites": true}})), &global, flags).await.unwrap_err();
            assert_eq!(direct.code, -32602);
            for allow in [false, true] {
                let call = super::super::handle_tools_call(Some(serde_json::json!({"name": "call_tool", "arguments": {"name": "linear_document_create", "input": {"project": id}, "allowWrites": allow}})), &global, flags).await;
                if allow {
                    assert_eq!(call.unwrap_err().code, -32602);
                } else {
                    let denied = call.unwrap();
                    assert_eq!(denied["isError"], true);
                    assert!(denied["content"][0]["text"]
                        .as_str()
                        .unwrap()
                        .contains("allowWrites"));
                }
                let result = super::super::handle_tools_call(Some(serde_json::json!({"name": "execute", "arguments": {"code": format!("return await linear_document_create({{project: '{id}'}})"), "allowWrites": allow}})), &global, flags).await.unwrap();
                assert!(result["structuredContent"]["error"]["message"]
                    .as_str()
                    .unwrap()
                    .contains(if allow {
                        "Invalid arguments:"
                    } else {
                        "allowWrites"
                    }));
            }
        }
        let tool = super::super::registered_tools()
            .into_iter()
            .find(|tool| tool.name == "linear_document_create")
            .unwrap();
        assert_eq!(tool.kind, super::super::ToolKind::Write);
        let declaration = super::super::declaration(&tool);
        for field in [
            "declare function linear_document_create",
            "project: string",
            "title: string",
            "content?: string",
            "team?: string",
        ] {
            assert!(declaration.contains(field), "{field}: {declaration}");
        }
        let validator = jsonschema::draft202012::new(&tool.input_schema).unwrap();
        assert!(validator.is_valid(&serde_json::json!({"project": id, "title": "Plan"})));
        assert!(validator.is_valid(&serde_json::json!({"project": "Delivery", "title": "Plan", "team": "GUZ", "content": "    code\n"})));
        let mut sample: serde_json::Value = serde_json::from_str(include_str!(
            "../../../tests/contract_samples/linear_document_create.json"
        ))
        .unwrap();
        let output_validator = jsonschema::draft202012::new(&tool.output_schema).unwrap();
        sample["content"] = serde_json::Value::Null;
        assert!(output_validator.is_valid(&sample));
        let output: mcptools_core::linear::DocumentCreateOutput =
            serde_json::from_value(sample.clone()).unwrap();
        assert_eq!(serde_json::to_value(output).unwrap(), sample);
        sample["project"] = serde_json::Value::Null;
        assert!(!output_validator.is_valid(&sample));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn milestone_create_contract_dispatch_and_write_gates() {
        let global = crate::Global {
            verbose: false,
            execute_timeout_secs: 30,
            execute_memory_mb: 64,
            execute_output_kb: 256,
        };
        let id = "12345678-1234-1234-1234-123456789abc";
        for input in [
            serde_json::Value::Null,
            serde_json::json!({}),
            serde_json::json!({"project": id, "name": null}),
            serde_json::json!({"project": id, "name": 1}),
            serde_json::json!({"project": id, "name": " "}),
            serde_json::json!({"project": id, "name": "Release", "team": 1}),
            serde_json::json!({"project": id, "name": "Release", "description": []}),
            serde_json::json!({"project": id, "name": "Release", "targetDate": false}),
            serde_json::json!({"project": id, "name": "Release", "sortOrder": "1"}),
            serde_json::json!({"project": id, "name": "Release", "extra": 1}),
        ] {
            assert_eq!(
                handle_linear_project_milestone_create(Some(input), &global)
                    .await
                    .unwrap_err()
                    .code,
                -32602
            );
        }
        for flags in [
            super::super::ServeFlags {
                discovery: false,
                code_mode: false,
            },
            super::super::ServeFlags {
                discovery: true,
                code_mode: false,
            },
            super::super::ServeFlags {
                discovery: false,
                code_mode: true,
            },
        ] {
            let direct = super::super::handle_tools_call(Some(serde_json::json!({"name": "linear_project_milestone_create", "arguments": {"project": id, "allowWrites": true}})), &global, flags).await.unwrap_err();
            assert_eq!(direct.code, -32602);
            for allow in [false, true] {
                let call = super::super::handle_tools_call(Some(serde_json::json!({"name": "call_tool", "arguments": {"name": "linear_project_milestone_create", "input": {"project": id}, "allowWrites": allow}})), &global, flags).await;
                if allow {
                    assert_eq!(call.unwrap_err().code, -32602);
                } else {
                    let denied = call.unwrap();
                    assert_eq!(denied["isError"], true);
                    assert!(denied["content"][0]["text"]
                        .as_str()
                        .unwrap()
                        .contains("allowWrites"));
                }
                let result = super::super::handle_tools_call(Some(serde_json::json!({"name": "execute", "arguments": {"code": format!("return await linear_project_milestone_create({{project: '{id}'}})"), "allowWrites": allow}})), &global, flags).await.unwrap();
                assert!(result["structuredContent"]["error"]["message"]
                    .as_str()
                    .unwrap()
                    .contains(if allow {
                        "Invalid arguments:"
                    } else {
                        "allowWrites"
                    }));
            }
        }
        let tool = super::super::registered_tools()
            .into_iter()
            .find(|tool| tool.name == "linear_project_milestone_create")
            .unwrap();
        assert_eq!(tool.kind, super::super::ToolKind::Write);
        let declaration = super::super::declaration(&tool);
        for field in [
            "declare function linear_project_milestone_create",
            "project: string",
            "name: string",
            "description?: string",
            "targetDate?: string",
            "sortOrder?: number",
        ] {
            assert!(declaration.contains(field), "{field}: {declaration}");
        }
        let validator = jsonschema::draft202012::new(&tool.input_schema).unwrap();
        assert!(validator.is_valid(&serde_json::json!({"project": id, "name": "Release", "team": null, "description": null, "targetDate": null, "sortOrder": null})));
        let output_validator = jsonschema::draft202012::new(&tool.output_schema).unwrap();
        let mut sample: serde_json::Value = serde_json::from_str(include_str!(
            "../../../tests/contract_samples/linear_project_milestone_create.json"
        ))
        .unwrap();
        sample["description"] = serde_json::Value::Null;
        sample["targetDate"] = serde_json::Value::Null;
        assert!(output_validator.is_valid(&sample));
        let milestone: mcptools_core::linear::ProjectMilestone =
            serde_json::from_value(sample.clone()).unwrap();
        assert_eq!(serde_json::to_value(milestone).unwrap(), sample);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn milestone_update_contract_dispatch_and_write_gates() {
        let global = crate::Global {
            verbose: false,
            execute_timeout_secs: 30,
            execute_memory_mb: 64,
            execute_output_kb: 256,
        };
        let id = "22345678-1234-1234-1234-123456789abc";
        for input in [
            serde_json::Value::Null,
            serde_json::json!({}),
            serde_json::json!({"id": id}),
            serde_json::json!({"id": "", "name": "New"}),
            serde_json::json!({"id": "not-uuid", "name": "New"}),
            serde_json::json!({"id": id, "name": " "}),
            serde_json::json!({"id": id, "name": null}),
            serde_json::json!({"id": id, "description": null}),
            serde_json::json!({"id": id, "targetDate": null}),
            serde_json::json!({"id": id, "sortOrder": null}),
            serde_json::json!({"id": id, "sortOrder": "1"}),
            serde_json::json!({"id": id, "targetDate": "2026-02-29"}),
            serde_json::json!({"id": id, "targetDate": "2100-02-29"}),
            serde_json::json!({"id": id, "clearDescription": false, "clearTargetDate": false}),
            serde_json::json!({"id": id, "clearDescription": null}),
            serde_json::json!({"id": id, "clearTargetDate": "true"}),
            serde_json::json!({"id": id, "description": "", "clearDescription": true}),
            serde_json::json!({"id": id, "targetDate": "2028-02-29", "clearTargetDate": true}),
            serde_json::json!({"id": id, "name": "New", "projectId": id}),
        ] {
            assert_eq!(
                handle_linear_project_milestone_update(Some(input.clone()), &global)
                    .await
                    .unwrap_err()
                    .code,
                -32602
            );
            let flags = super::super::ServeFlags {
                discovery: false,
                code_mode: true,
            };
            let call = super::super::handle_tools_call(
                Some(serde_json::json!({"name": "call_tool", "arguments": {"name": "linear_project_milestone_update", "input": input, "allowWrites": true}})),
                &global,
                flags,
            ).await.unwrap_err();
            assert_eq!(call.code, -32602);
            let execute = super::super::handle_tools_call(
                Some(serde_json::json!({"name": "execute", "arguments": {"code": format!("return await linear_project_milestone_update({input})"), "allowWrites": true}})),
                &global,
                flags,
            ).await.unwrap();
            assert_eq!(execute["isError"], true);
            assert!(execute["structuredContent"]["error"]["message"]
                .as_str()
                .unwrap()
                .starts_with("Invalid arguments:"));
        }
        for flags in [
            super::super::ServeFlags {
                discovery: false,
                code_mode: false,
            },
            super::super::ServeFlags {
                discovery: true,
                code_mode: false,
            },
            super::super::ServeFlags {
                discovery: false,
                code_mode: true,
            },
        ] {
            assert_eq!(super::super::handle_tools_call(Some(serde_json::json!({"name": "linear_project_milestone_update", "arguments": {"id": id, "allowWrites": true}})), &global, flags).await.unwrap_err().code, -32602);
            for allow in [false, true] {
                let call = super::super::handle_tools_call(Some(serde_json::json!({"name": "call_tool", "arguments": {"name": "linear_project_milestone_update", "input": {"id": id}, "allowWrites": allow}})), &global, flags).await;
                if allow {
                    assert_eq!(call.unwrap_err().code, -32602);
                } else {
                    assert!(call.unwrap()["content"][0]["text"]
                        .as_str()
                        .unwrap()
                        .contains("allowWrites"));
                }
                let result = super::super::handle_tools_call(Some(serde_json::json!({"name": "execute", "arguments": {"code": format!("return await linear_project_milestone_update({{id: '{id}'}})"), "allowWrites": allow}})), &global, flags).await.unwrap();
                assert!(result["structuredContent"]["error"]["message"]
                    .as_str()
                    .unwrap()
                    .contains(if allow {
                        "Invalid arguments:"
                    } else {
                        "allowWrites"
                    }));
            }
        }
        let tool = super::super::registered_tools()
            .into_iter()
            .find(|tool| tool.name == "linear_project_milestone_update")
            .unwrap();
        assert_eq!(tool.kind, super::super::ToolKind::Write);
        let declaration = super::super::declaration(&tool);
        for field in [
            "declare function linear_project_milestone_update",
            "id: string",
            "name?: string",
            "description?: string",
            "targetDate?: string",
            "sortOrder?: number",
            "clearDescription?: boolean",
            "clearTargetDate?: boolean",
        ] {
            assert!(declaration.contains(field), "{field}: {declaration}");
        }
        let validator = jsonschema::draft202012::new(&tool.input_schema).unwrap();
        assert!(validator.is_valid(&serde_json::json!({"id": id, "description": ""})));
        assert!(!validator.is_valid(&serde_json::json!({"id": id, "description": null})));
        assert!(!validator.is_valid(&serde_json::json!({"name": "New"})));
        assert!(!validator.is_valid(&serde_json::json!({"id": id, "name": "New", "projectId": id})));
        let flags = super::super::ServeFlags {
            discovery: true,
            code_mode: true,
        };
        let discovered = super::super::handle_tools_call(
            Some(serde_json::json!({"name": "find_tools", "arguments": {"domain": "linear"}})),
            &global,
            flags,
        )
        .await
        .unwrap();
        let found = discovered["structuredContent"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .find(|entry| entry["name"] == tool.name)
            .unwrap();
        assert_eq!(found["declaration"], declaration);
        let denied = super::super::handle_tools_call(
            Some(serde_json::json!({"name": "call_tool", "arguments": {"name": tool.name, "input": {"id": id, "name": "New"}}})),
            &global,
            flags,
        ).await.unwrap();
        assert_eq!(denied["isError"], true);
        assert!(denied["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("allowWrites"));
        let denied = super::super::handle_tools_call(
            Some(serde_json::json!({"name": "execute", "arguments": {"code": format!("return await linear_project_milestone_update({{id: '{id}', name: 'New'}})")}})),
            &global,
            flags,
        ).await.unwrap();
        assert_eq!(denied["isError"], true);
        assert!(denied["structuredContent"]["error"]["message"]
            .as_str()
            .unwrap()
            .contains("allowWrites"));
        for number in ["NaN", "Infinity", "-Infinity"] {
            let result = super::super::handle_tools_call(Some(serde_json::json!({"name": "execute", "arguments": {"code": format!("return await linear_project_milestone_update({{id: '{id}', name: 'New', sortOrder: {number}}})"), "allowWrites": true}})), &global, super::super::ServeFlags { discovery: false, code_mode: true }).await.unwrap();
            assert_eq!(
                result["structuredContent"]["error"]["message"],
                "Tool arguments must not contain non-finite numbers"
            );
        }
        let sample: serde_json::Value = serde_json::from_str(include_str!(
            "../../../tests/contract_samples/linear_project_milestone_update.json"
        ))
        .unwrap();
        assert!(jsonschema::draft202012::new(&tool.output_schema)
            .unwrap()
            .is_valid(&sample));
        let milestone: mcptools_core::linear::ProjectMilestone =
            serde_json::from_value(sample.clone()).unwrap();
        let dual = super::super::to_dual_result(milestone).unwrap();
        assert_eq!(dual["structuredContent"], sample);
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(dual["content"][0]["text"].as_str().unwrap())
                .unwrap(),
            sample
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn project_update_validates_before_configuration_and_requires_write_permission() {
        let global = crate::Global {
            verbose: false,
            execute_timeout_secs: 30,
            execute_memory_mb: 64,
            execute_output_kb: 256,
        };
        let id = "12345678-1234-1234-1234-123456789abc";
        for input in [
            serde_json::json!({"id": id}),
            serde_json::json!({"id": id, "name": " "}),
            serde_json::json!({"id": id, "startDate": "2026-02-29"}),
            serde_json::json!({"id": id, "priority": 5}),
            serde_json::json!({"id": id, "lead": "Ada"}),
            serde_json::json!({"id": id, "content": "", "clearContent": true}),
            serde_json::json!({"id": id, "clearStatus": true}),
            serde_json::json!({"id": id, "lead": null}),
        ] {
            let error = handle_linear_project_update(Some(input), &global)
                .await
                .unwrap_err();
            assert_eq!(error.code, -32602);
        }
        for flags in [
            super::super::ServeFlags {
                discovery: false,
                code_mode: false,
            },
            super::super::ServeFlags {
                discovery: true,
                code_mode: false,
            },
            super::super::ServeFlags {
                discovery: false,
                code_mode: true,
            },
        ] {
            let direct = super::super::handle_tools_call(
                Some(serde_json::json!({"name": "linear_project_update", "arguments": {"id": id, "allowWrites": true}})),
                &global,
                flags,
            )
            .await
            .unwrap_err();
            assert_eq!(direct.code, -32602);
            for allow in [false, true] {
                let call = super::super::handle_tools_call(Some(serde_json::json!({"name": "call_tool", "arguments": {"name": "linear_project_update", "input": {"id": id}, "allowWrites": allow}})), &global, flags).await;
                if allow {
                    let error = call.unwrap_err();
                    assert_eq!(error.code, -32602);
                    assert!(error.message.contains("Invalid arguments:"));
                } else {
                    let denied = call.unwrap();
                    assert_eq!(denied["isError"], true);
                    assert!(denied["content"][0]["text"]
                        .as_str()
                        .unwrap()
                        .contains("allowWrites"));
                }
                let result = super::super::handle_tools_call(Some(serde_json::json!({"name": "execute", "arguments": {"code": format!("return await linear_project_update({{id: '{id}'}})"), "allowWrites": allow}})), &global, flags).await.unwrap();
                assert!(result["structuredContent"]["error"]["message"]
                    .as_str()
                    .unwrap()
                    .contains(if allow {
                        "Invalid arguments:"
                    } else {
                        "allowWrites"
                    }));
            }
        }
        let tool = super::super::registered_tools()
            .into_iter()
            .find(|tool| tool.name == "linear_project_update")
            .unwrap();
        assert_eq!(tool.kind, super::super::ToolKind::Write);
        let declaration = super::super::declaration(&tool);
        for field in [
            "declare function linear_project_update",
            "id: string",
            "status?: string",
            "startDate?: string",
            "clearDescription?: boolean",
            "clearLead?: boolean",
            "priority?: number",
            "description: string",
        ] {
            assert!(declaration.contains(field), "{field}: {declaration}");
        }
        assert_eq!(tool.input_schema["additionalProperties"], false);
        assert_eq!(tool.input_schema["required"], serde_json::json!(["id"]));
        let validator = jsonschema::draft202012::new(&tool.input_schema).unwrap();
        for (field, value) in [
            ("team", serde_json::json!("GUZ")),
            ("name", serde_json::json!("New")),
            ("description", serde_json::json!("")),
            ("content", serde_json::json!("    code\n")),
            ("status", serde_json::json!("Custom")),
            ("lead", serde_json::json!("me")),
            ("startDate", serde_json::json!("2028-02-29")),
            ("targetDate", serde_json::json!("2028-03-01")),
            ("priority", serde_json::json!(0)),
        ] {
            let valid = serde_json::json!({"id": id, field: value});
            assert!(validator.is_valid(&valid), "{field}");
            assert!(
                serde_json::from_value::<crate::linear::args::ProjectUpdateArgs>(valid).is_ok()
            );
            let null = serde_json::json!({"id": id, field: null});
            assert!(!validator.is_valid(&null), "{field}");
            assert!(
                serde_json::from_value::<crate::linear::args::ProjectUpdateArgs>(null).is_err()
            );
        }
        assert!(tool.output_schema["properties"].get("status").is_some());
    }

    #[tokio::test]
    async fn project_status_rejects_invalid_arguments_before_configuration() {
        let global = crate::Global {
            verbose: false,
            execute_timeout_secs: 30,
            execute_memory_mb: 64,
            execute_output_kb: 256,
        };
        for input in [
            serde_json::json!({"limit": 0}),
            serde_json::json!({"limit": 251}),
            serde_json::json!({"limit": -1}),
            serde_json::json!({"limit": 1.5}),
            serde_json::json!({"limit": "25"}),
            serde_json::json!({"limit": null}),
            serde_json::json!({"cursor": " "}),
            serde_json::json!({"cursor": 1}),
            serde_json::json!({"all": "true"}),
            serde_json::json!({"all": null}),
            serde_json::json!({"team": "GUZ"}),
            serde_json::json!([]),
        ] {
            let error = handle_linear_project_status_list(Some(input), &global)
                .await
                .unwrap_err();
            assert_eq!(error.code, -32602);
        }
        let defaults: crate::linear::args::ProjectStatusListArgs =
            serde_json::from_value(serde_json::json!({})).unwrap();
        assert_eq!(defaults.limit, 25);
        assert!(!defaults.all);
        assert_eq!(defaults.cursor, None);
        for limit in [1, 250] {
            let args: crate::linear::args::ProjectStatusListArgs =
                serde_json::from_value(serde_json::json!({"limit": limit})).unwrap();
            assert!(args.validate().is_ok());
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn milestone_strict_inputs_read_dispatch_discovery_and_declarations() {
        let global = crate::Global {
            verbose: false,
            execute_timeout_secs: 30,
            execute_memory_mb: 64,
            execute_output_kb: 256,
        };
        let id = "12345678-1234-1234-1234-123456789abc";
        for input in [
            serde_json::json!({}),
            serde_json::json!({"project": " "}),
            serde_json::json!({"project": "Example"}),
            serde_json::json!({"project": "Example", "team": " "}),
            serde_json::json!({"project": "12345678-1234-1234-1234-123456789xyz"}),
            serde_json::json!({"project": id, "team": null}),
            serde_json::json!({"project": id, "limit": 0}),
            serde_json::json!({"project": id, "limit": 251}),
            serde_json::json!({"project": id, "limit": 1.5}),
            serde_json::json!({"project": id, "limit": null}),
            serde_json::json!({"project": id, "cursor": " "}),
            serde_json::json!({"project": id, "cursor": null}),
            serde_json::json!({"project": id, "all": null}),
            serde_json::json!({"project": id, "all": "true"}),
            serde_json::json!({"project": id, "id": id}),
            serde_json::json!([]),
        ] {
            assert_eq!(
                handle_linear_project_milestone_list(Some(input), &global)
                    .await
                    .unwrap_err()
                    .code,
                -32602
            );
        }
        for limit in [1, 250] {
            let args: crate::linear::args::ProjectMilestoneListArgs =
                serde_json::from_value(serde_json::json!({"project": id, "limit": limit})).unwrap();
            assert!(args.validate().is_ok());
        }
        for flags in [
            super::super::ServeFlags {
                discovery: false,
                code_mode: false,
            },
            super::super::ServeFlags {
                discovery: true,
                code_mode: false,
            },
            super::super::ServeFlags {
                discovery: false,
                code_mode: true,
            },
        ] {
            for tool in [
                serde_json::json!({"name": "linear_project_milestone_list", "arguments": {"project": id, "limit": 0}}),
                serde_json::json!({"name": "call_tool", "arguments": {"name": "linear_project_milestone_list", "input": {"project": id, "limit": 0}}}),
            ] {
                let error = super::super::handle_tools_call(Some(tool), &global, flags)
                    .await
                    .unwrap_err();
                assert_eq!(error.code, -32602);
                assert!(error.message.starts_with("Invalid arguments:"));
            }
        }
        let flags = super::super::ServeFlags {
            discovery: false,
            code_mode: true,
        };
        let result = super::super::handle_tools_call(Some(serde_json::json!({"name": "execute", "arguments": {"code": format!("return await linear_project_milestone_list({{project: '{id}', limit: 0}})")}})), &global, flags).await.unwrap();
        assert!(result["structuredContent"]["error"]["message"]
            .as_str()
            .unwrap()
            .contains("Invalid arguments:"));
        let tool = super::super::registered_tools()
            .into_iter()
            .find(|tool| tool.name == "linear_project_milestone_list")
            .unwrap();
        assert_eq!(tool.kind, super::super::ToolKind::Read);
        let declaration = super::super::declaration(&tool);
        for field in [
            "declare function linear_project_milestone_list(input: LinearProjectMilestoneListInput): Promise<LinearProjectMilestoneListOutput>",
            "project: string", "team?: string", "limit?: number", "cursor?: string", "all?: boolean",
            "description: string | null", "targetDate: string | null", "sortOrder: number", "pageInfo:",
        ] {
            assert!(declaration.contains(field), "{field}: {declaration}");
        }
        assert_eq!(tool.input_schema["properties"]["limit"]["default"], 25);
        assert_eq!(tool.input_schema["additionalProperties"], false);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn progress_report_list_strict_inputs_read_dispatch_and_declarations() {
        let global = crate::Global {
            verbose: false,
            execute_timeout_secs: 30,
            execute_memory_mb: 64,
            execute_output_kb: 256,
        };
        let id = "12345678-1234-1234-1234-123456789abc";
        for input in [
            serde_json::json!({}),
            serde_json::json!({"project": " "}),
            serde_json::json!({"project": "Example"}),
            serde_json::json!({"project": "Example", "team": " "}),
            serde_json::json!({"project": "12345678-1234-1234-1234-123456789xyz"}),
            serde_json::json!({"project": id, "team": null}),
            serde_json::json!({"project": id, "team": "bad\nteam"}),
            serde_json::json!({"project": id, "limit": 0}),
            serde_json::json!({"project": id, "limit": 251}),
            serde_json::json!({"project": id, "limit": 1.5}),
            serde_json::json!({"project": id, "limit": null}),
            serde_json::json!({"project": id, "cursor": " "}),
            serde_json::json!({"project": id, "cursor": null}),
            serde_json::json!({"project": id, "all": null}),
            serde_json::json!({"project": id, "all": "true"}),
            serde_json::json!({"project": id, "body": "Not accepted"}),
        ] {
            assert_eq!(
                handle_linear_project_update_list(Some(input), &global)
                    .await
                    .unwrap_err()
                    .code,
                -32602
            );
        }
        for flags in [
            super::super::ServeFlags {
                discovery: false,
                code_mode: false,
            },
            super::super::ServeFlags {
                discovery: true,
                code_mode: false,
            },
            super::super::ServeFlags {
                discovery: false,
                code_mode: true,
            },
        ] {
            for request in [
                serde_json::json!({"name": "linear_project_update_list", "arguments": {"project": id, "limit": 0}}),
                serde_json::json!({"name": "call_tool", "arguments": {"name": "linear_project_update_list", "input": {"project": id, "limit": 0}, "allowWrites": false}}),
            ] {
                let error = super::super::handle_tools_call(Some(request), &global, flags)
                    .await
                    .unwrap_err();
                assert_eq!(error.code, -32602);
                assert!(error.message.starts_with("Invalid arguments:"));
            }
            let result = super::super::handle_tools_call(Some(serde_json::json!({"name": "execute", "arguments": {"code": format!("return await linear_project_update_list({{project: '{id}', limit: 0}})"), "allowWrites": false}})), &global, flags).await.unwrap();
            assert!(result["structuredContent"]["error"]["message"]
                .as_str()
                .unwrap()
                .contains("Invalid arguments:"));
        }
        let tool = super::super::registered_tools()
            .into_iter()
            .find(|tool| tool.name == "linear_project_update_list")
            .unwrap();
        assert_eq!(tool.kind, super::super::ToolKind::Read);
        let declaration = super::super::declaration(&tool);
        for field in [
            "declare function linear_project_update_list(input: LinearProjectUpdateListInput): Promise<LinearProjectUpdateListOutput>",
            "project: string", "team?: string", "limit?: number", "cursor?: string", "all?: boolean",
            "body: string", "createdAt: string", "updatedAt: string", "url: string", "pageInfo:",
            "\"onTrack\"", "\"atRisk\"", "\"offTrack\"",
        ] { assert!(declaration.contains(field), "{field}: {declaration}"); }
        assert_eq!(tool.input_schema["properties"]["limit"]["default"], 25);
        assert_eq!(tool.input_schema["additionalProperties"], false);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn project_status_is_read_only_in_discovery_and_code_mode() {
        let global = crate::Global {
            verbose: false,
            execute_timeout_secs: 30,
            execute_memory_mb: 64,
            execute_output_kb: 256,
        };
        for flags in [
            super::super::ServeFlags {
                discovery: false,
                code_mode: false,
            },
            super::super::ServeFlags {
                discovery: true,
                code_mode: false,
            },
            super::super::ServeFlags {
                discovery: false,
                code_mode: true,
            },
        ] {
            for tool in [
                serde_json::json!({"name": "linear_project_status_list", "arguments": {"limit": 0}}),
                serde_json::json!({"name": "call_tool", "arguments": {"name": "linear_project_status_list", "input": {"limit": 0}}}),
            ] {
                let error = super::super::handle_tools_call(Some(tool), &global, flags)
                    .await
                    .unwrap_err();
                assert_eq!(error.code, -32602);
                assert!(error.message.starts_with("Invalid arguments:"));
            }
        }
        let flags = super::super::ServeFlags {
            discovery: false,
            code_mode: true,
        };
        let result = super::super::handle_tools_call(Some(serde_json::json!({"name": "execute", "arguments": {"code": "return await linear_project_status_list({limit: 0})"}})), &global, flags).await.unwrap();
        assert!(result["structuredContent"]["error"]["message"]
            .as_str()
            .unwrap()
            .contains("Invalid arguments:"));
        let tool = super::super::registered_tools()
            .into_iter()
            .find(|tool| tool.name == "linear_project_status_list")
            .unwrap();
        assert_eq!(tool.kind, super::super::ToolKind::Read);
        let declaration = super::super::declaration(&tool);
        assert!(declaration.contains("declare function linear_project_status_list(input: LinearProjectStatusListInput): Promise<LinearProjectStatusListOutput>"));
        for field in [
            "limit?: number",
            "cursor?: string",
            "all?: boolean",
            "position: number",
            "type: string",
            "pageInfo:",
        ] {
            assert!(declaration.contains(field), "{field}: {declaration}");
        }
        assert_eq!(tool.input_schema["properties"]["limit"]["type"], "integer");
        assert_eq!(tool.input_schema["properties"]["limit"]["default"], 25);
        let schema = schemars::schema_for!(crate::linear::args::ProjectStatusListArgs);
        assert_eq!(
            schema.as_value()["properties"]["limit"]["minimum"].as_f64(),
            Some(1.0)
        );
        assert_eq!(
            schema.as_value()["properties"]["limit"]["maximum"].as_f64(),
            Some(250.0)
        );
        assert_eq!(tool.input_schema["additionalProperties"], false);
    }

    #[tokio::test]
    async fn graph_rejects_invalid_inputs_before_configuration_or_http() {
        let global = crate::Global {
            verbose: false,
            execute_timeout_secs: 30,
            execute_memory_mb: 64,
            execute_output_kb: 256,
        };
        for input in [
            serde_json::json!({"ids": []}),
            serde_json::json!({"ids": [" "]}),
            serde_json::json!({"ids": ["GUZ-1"], "limit": 301}),
            serde_json::json!({"ids": ["GUZ-1"], "maxPages": 0}),
            serde_json::json!({"ids": ["GUZ-1"], "limit": -1}),
            serde_json::json!({"ids": ["GUZ-1"], "maxPages": null}),
            serde_json::json!({"ids": ["GUZ-1"], "unexpected": true}),
        ] {
            let error = handle_linear_issue_graph(Some(input), &global)
                .await
                .unwrap_err();
            assert_eq!(error.code, -32602);
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn graph_is_available_in_full_discovery_and_code_mode_without_write_permission() {
        let global = crate::Global {
            verbose: false,
            execute_timeout_secs: 30,
            execute_memory_mb: 64,
            execute_output_kb: 256,
        };
        let flags = super::super::ServeFlags {
            discovery: false,
            code_mode: true,
        };
        for tool in [
            serde_json::json!({"name": "linear_issue_graph", "arguments": {"ids": []}}),
            serde_json::json!({"name": "call_tool", "arguments": {"name": "linear_issue_graph", "input": {"ids": []}}}),
        ] {
            let error = super::super::handle_tools_call(Some(tool), &global, flags)
                .await
                .unwrap_err();
            assert_eq!(error.code, -32602);
            assert!(error.message.starts_with("Invalid arguments:"));
        }
        let result = super::super::handle_tools_call(Some(serde_json::json!({"name": "execute", "arguments": {"code": "return await linear_issue_graph({ids: []})"}})), &global, flags).await.unwrap();
        assert!(result["structuredContent"]["error"]["message"]
            .as_str()
            .unwrap()
            .contains("Invalid arguments:"));
        let tool = super::super::registered_tools()
            .into_iter()
            .find(|tool| tool.name == "linear_issue_graph")
            .unwrap();
        assert_eq!(tool.kind, super::super::ToolKind::Read);
        let declaration = super::super::declaration(&tool);
        assert!(declaration.contains("declare function linear_issue_graph(input: LinearIssueGraphInput): Promise<LinearIssueGraphOutput>"));
        assert!(declaration.contains("maxPages?: number"));
        assert!(declaration.contains("ids: string[]"));
    }
}
