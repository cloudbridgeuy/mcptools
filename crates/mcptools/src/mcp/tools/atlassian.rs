use crate::prelude::{eprintln, *};
use schemars::JsonSchema;
use serde::Deserialize;

use super::JsonRpcError;

#[derive(Deserialize, JsonSchema)]
pub(crate) struct JiraSearchArgs {
    fields: Option<Vec<String>>,
    query: Option<String>,
    #[serde(rename = "queryName")]
    query_name: Option<String>,
    limit: Option<usize>,
    #[serde(rename = "nextPageToken")]
    next_page_token: Option<String>,
}
#[derive(Deserialize, JsonSchema)]
pub(crate) struct ConfluenceSearchArgs {
    query: String,
    limit: Option<usize>,
}
#[derive(Deserialize, JsonSchema)]
pub(crate) struct JiraGetArgs {
    #[serde(rename = "issueKey")]
    issue_key: String,
}
#[derive(Deserialize, JsonSchema)]
pub(crate) struct JiraSprintListArgs {
    #[serde(rename = "boardId")]
    board_id: u64,
    state: Option<String>,
}
#[derive(Deserialize, JsonSchema)]
pub(crate) struct JiraUpdateArgs {
    #[serde(rename = "ticketKey")]
    ticket_key: String,
    status: Option<String>,
    priority: Option<String>,
    #[serde(rename = "issueType")]
    issue_type: Option<String>,
    assignee: Option<String>,
    description: Option<String>,
    sprint: Option<String>,
    #[serde(rename = "boardId")]
    board_id: Option<u64>,
}
#[derive(Deserialize, JsonSchema)]
pub(crate) struct JiraCreateArgs {
    summary: String,
    description: Option<String>,
    project: Option<String>,
    #[serde(rename = "issueType")]
    issue_type: Option<String>,
    priority: Option<String>,
    assignee: Option<String>,
    sprint: Option<String>,
    #[serde(rename = "boardId")]
    board_id: Option<u64>,
}
#[derive(Deserialize, JsonSchema)]
pub(crate) struct JiraQuerySaveArgs {
    name: String,
    query: String,
    update: Option<bool>,
}
#[derive(Deserialize, JsonSchema)]
pub(crate) struct JiraQueryDeleteArgs {
    name: String,
}
#[derive(Deserialize, JsonSchema)]
pub(crate) struct JiraQueryLoadArgs {
    name: String,
}
#[derive(Deserialize, JsonSchema)]
pub(crate) struct BitbucketPRListArgs {
    repo: String,
    state: Option<Vec<String>>,
    limit: Option<usize>,
    #[serde(rename = "nextPage")]
    next_page: Option<String>,
}
#[derive(Deserialize, JsonSchema)]
pub(crate) struct BitbucketPRReadArgs {
    fields: Option<Vec<String>>,
    repo: String,
    #[serde(rename = "prNumber")]
    pr_number: u64,
    limit: Option<usize>,
    #[serde(rename = "diffLimit")]
    diff_limit: Option<usize>,
    #[serde(rename = "lineLimit")]
    line_limit: Option<i32>,
    #[serde(rename = "noDiff")]
    no_diff: Option<bool>,
}
#[derive(Deserialize, JsonSchema)]
pub(crate) struct BitbucketPRCreateArgs {
    repo: String,
    title: String,
    #[serde(rename = "sourceBranch")]
    source_branch: String,
    #[serde(rename = "destinationBranch")]
    destination_branch: Option<String>,
    description: Option<String>,
    #[serde(rename = "closeSourceBranch")]
    close_source_branch: Option<bool>,
}
#[derive(Deserialize, JsonSchema)]
pub(crate) struct BitbucketPRUpdateArgs {
    repo: String,
    #[serde(rename = "prNumber")]
    pr_number: u64,
    title: Option<String>,
    description: Option<String>,
    #[serde(rename = "destinationBranch")]
    destination_branch: Option<String>,
    reviewers: Option<Vec<String>>,
    #[serde(rename = "closeSourceBranch")]
    close_source_branch: Option<bool>,
    approve: Option<bool>,
    unapprove: Option<bool>,
    decline: Option<bool>,
    merge: Option<bool>,
    #[serde(rename = "mergeStrategy")]
    merge_strategy: Option<String>,
    #[serde(rename = "mergeMessage")]
    merge_message: Option<String>,
}
#[derive(Deserialize, JsonSchema)]
pub(crate) struct JiraAttachmentListArgs {
    #[serde(rename = "issueKey")]
    issue_key: String,
}
#[derive(Deserialize, JsonSchema)]
pub(crate) struct JiraAttachmentDownloadArgs {
    #[serde(rename = "issueKey")]
    issue_key: String,
    #[serde(rename = "attachmentId")]
    attachment_id: String,
    #[serde(rename = "outputPath")]
    output_path: Option<String>,
}
#[derive(Deserialize, JsonSchema)]
pub(crate) struct JiraAttachmentUploadArgs {
    #[serde(rename = "issueKey")]
    issue_key: String,
    #[serde(rename = "filePaths")]
    file_paths: Vec<String>,
}
#[derive(Deserialize, JsonSchema)]
pub(crate) struct JiraCommentAddArgs {
    #[serde(rename = "issueKey")]
    issue_key: String,
    comment: String,
}
#[derive(Deserialize, JsonSchema)]
pub(crate) struct JiraCommentListArgs {
    #[serde(rename = "issueKey")]
    issue_key: String,
}
#[derive(Deserialize, JsonSchema)]
pub(crate) struct JiraCommentUpdateArgs {
    #[serde(rename = "issueKey")]
    issue_key: String,
    #[serde(rename = "commentId")]
    comment_id: String,
    comment: String,
}
#[derive(Deserialize, JsonSchema)]
pub(crate) struct JiraCommentDeleteArgs {
    #[serde(rename = "issueKey")]
    issue_key: String,
    #[serde(rename = "commentId")]
    comment_id: String,
}
#[derive(Deserialize, JsonSchema)]
pub(crate) struct BitbucketWorkspaceListArgs {
    limit: Option<usize>,
    #[serde(rename = "nextPage")]
    next_page: Option<String>,
}
#[derive(Deserialize, JsonSchema)]
pub(crate) struct BitbucketRepoListArgs {
    workspace: String,
    limit: Option<usize>,
    #[serde(rename = "nextPage")]
    next_page: Option<String>,
}
#[derive(Deserialize, JsonSchema)]
pub(crate) struct BitbucketRepoBranchesArgs {
    workspace: String,
    repo: String,
    limit: Option<usize>,
    #[serde(rename = "nextPage")]
    next_page: Option<String>,
    query: Option<String>,
    sort: Option<String>,
}
#[derive(Deserialize, JsonSchema)]
pub(crate) struct JiraQueryListArgs {}

pub async fn handle_jira_search(
    arguments: Option<serde_json::Value>,
    global: &crate::Global,
) -> Result<serde_json::Value, JsonRpcError> {
    use mcptools_core::queries;
    use std::env;
    use std::path::PathBuf;

    let args: JiraSearchArgs = serde_json::from_value(arguments.unwrap_or(serde_json::Value::Null))
        .map_err(|e| JsonRpcError {
            code: -32602,
            message: format!("Invalid arguments: {e}"),
            data: None,
        })?;
    let fields = args.fields;

    let resolved_query = if let Some(query_name) = args.query_name {
        let home = env::var("HOME")
            .ok()
            .map(PathBuf::from)
            .ok_or_else(|| JsonRpcError {
                code: -32603,
                message: "Could not determine home directory".to_string(),
                data: None,
            })?;
        let queries_dir = home.join(".config/mcptools/queries");

        queries::load_query(&queries_dir, &query_name).map_err(|e| JsonRpcError {
            code: -32603,
            message: format!("Failed to load query: {e}"),
            data: None,
        })?
    } else {
        args.query.ok_or_else(|| JsonRpcError {
            code: -32602,
            message: "Must provide either 'query' or 'queryName'".to_string(),
            data: None,
        })?
    };

    if global.verbose {
        eprintln!(
            "Calling jira_search: query={}, limit={:?}, nextPageToken={:?}",
            resolved_query,
            args.limit,
            args.next_page_token
                .as_ref()
                .map(|t| format!("{}...", &t[..std::cmp::min(20, t.len())]))
        );
    }

    let config = crate::atlassian::JiraConfig::from_env().map_err(|e| JsonRpcError {
        code: -32603,
        message: format!("Configuration error: {e}"),
        data: None,
    })?;

    let search_data = crate::atlassian::jira::search_issues_data(
        resolved_query,
        args.limit.unwrap_or(10),
        args.next_page_token,
        &config,
    )
    .await
    .map_err(|e| JsonRpcError {
        code: -32603,
        message: format!("Tool execution error: {e}"),
        data: None,
    })?;

    super::to_dual_result_projected(search_data, fields.as_deref())
}

pub async fn handle_confluence_search(
    arguments: Option<serde_json::Value>,
    global: &crate::Global,
) -> Result<serde_json::Value, JsonRpcError> {
    let args: ConfluenceSearchArgs =
        serde_json::from_value(arguments.unwrap_or(serde_json::Value::Null)).map_err(|e| {
            JsonRpcError {
                code: -32602,
                message: format!("Invalid arguments: {e}"),
                data: None,
            }
        })?;

    if global.verbose {
        eprintln!(
            "Calling confluence_search: query={}, limit={:?}",
            args.query, args.limit
        );
    }

    let config = crate::atlassian::ConfluenceConfig::from_env().map_err(|e| JsonRpcError {
        code: -32603,
        message: format!("Configuration error: {e}"),
        data: None,
    })?;

    let search_data = crate::atlassian::confluence::search_pages_data(
        args.query,
        args.limit.unwrap_or(10),
        &config,
    )
    .await
    .map_err(|e| JsonRpcError {
        code: -32603,
        message: format!("Tool execution error: {e}"),
        data: None,
    })?;

    super::to_dual_result(search_data)
}

pub async fn handle_jira_get(
    arguments: Option<serde_json::Value>,
    global: &crate::Global,
) -> Result<serde_json::Value, JsonRpcError> {
    let args: JiraGetArgs = serde_json::from_value(arguments.unwrap_or(serde_json::Value::Null))
        .map_err(|e| JsonRpcError {
            code: -32602,
            message: format!("Invalid arguments: {e}"),
            data: None,
        })?;

    if global.verbose {
        eprintln!("Calling jira_get: issueKey={}", args.issue_key);
    }

    let config = crate::atlassian::JiraConfig::from_env().map_err(|e| JsonRpcError {
        code: -32603,
        message: format!("Configuration error: {e}"),
        data: None,
    })?;

    let ticket_data = crate::atlassian::jira::get_ticket_data(args.issue_key, &config)
        .await
        .map_err(|e| JsonRpcError {
            code: -32603,
            message: format!("Tool execution error: {e}"),
            data: None,
        })?;

    super::to_dual_result(ticket_data)
}

pub async fn handle_jira_sprint_list(
    arguments: Option<serde_json::Value>,
    global: &crate::Global,
) -> Result<serde_json::Value, JsonRpcError> {
    let args: JiraSprintListArgs =
        serde_json::from_value(arguments.unwrap_or(serde_json::Value::Null)).map_err(|e| {
            JsonRpcError {
                code: -32602,
                message: format!("Invalid arguments: {e}"),
                data: None,
            }
        })?;

    if global.verbose {
        eprintln!(
            "Calling jira_sprint_list: boardId={}, state={:?}",
            args.board_id, args.state
        );
    }

    let config = crate::atlassian::JiraConfig::from_env().map_err(|e| JsonRpcError {
        code: -32603,
        message: format!("Configuration error: {e}"),
        data: None,
    })?;

    let sprints = crate::atlassian::jira::list_sprints_data(
        args.board_id,
        args.state.as_deref().unwrap_or("active,future"),
        &config,
    )
    .await
    .map_err(|e| JsonRpcError {
        code: -32603,
        message: format!("Tool execution error: {e}"),
        data: None,
    })?;

    super::to_dual_result(sprints)
}

pub async fn handle_jira_update(
    arguments: Option<serde_json::Value>,
    global: &crate::Global,
) -> Result<serde_json::Value, JsonRpcError> {
    let args: JiraUpdateArgs = serde_json::from_value(arguments.unwrap_or(serde_json::Value::Null))
        .map_err(|e| JsonRpcError {
            code: -32602,
            message: format!("Invalid arguments: {e}"),
            data: None,
        })?;

    if global.verbose {
        eprintln!(
            "Calling jira_update: ticketKey={}, status={:?}, priority={:?}, issueType={:?}, assignee={:?}, description={:?}, sprint={:?}, boardId={:?}",
            args.ticket_key,
            args.status,
            args.priority,
            args.issue_type,
            args.assignee,
            args.description,
            args.sprint,
            args.board_id,
        );
    }

    let update_options = crate::atlassian::jira::update::UpdateOptions {
        ticket_key: args.ticket_key,
        status: args.status,
        priority: args.priority,
        issue_type: args.issue_type,
        assignee: args.assignee,
        description: args.description,
        sprint: args.sprint,
        board: args.board_id,
        json: true, // MCP always returns JSON
    };

    let config = crate::atlassian::JiraConfig::from_env().map_err(|e| JsonRpcError {
        code: -32603,
        message: format!("Configuration error: {e}"),
        data: None,
    })?;

    let update_data = crate::atlassian::jira::update_ticket_data(update_options, &config)
        .await
        .map_err(|e| JsonRpcError {
            code: -32603,
            message: format!("Tool execution error: {e}"),
            data: None,
        })?;

    super::to_dual_result(update_data)
}

pub async fn handle_jira_create(
    arguments: Option<serde_json::Value>,
    global: &crate::Global,
) -> Result<serde_json::Value, JsonRpcError> {
    let args: JiraCreateArgs = serde_json::from_value(arguments.unwrap_or(serde_json::Value::Null))
        .map_err(|e| JsonRpcError {
            code: -32602,
            message: format!("Invalid arguments: {e}"),
            data: None,
        })?;

    if global.verbose {
        let desc_preview = args.description.as_ref().map(|d| {
            let len = d.len();
            &d[..std::cmp::min(50, len)]
        });
        eprintln!(
            "Calling jira_create: summary={}, description={:?}, project={:?}, issueType={:?}, priority={:?}, assignee={:?}, sprint={:?}, boardId={:?}",
            args.summary,
            desc_preview,
            args.project,
            args.issue_type,
            args.priority,
            args.assignee,
            args.sprint,
            args.board_id,
        );
    }

    let create_options = crate::atlassian::jira::create::CreateOptions {
        summary: args.summary,
        description: args.description,
        project: args.project.unwrap_or_else(|| "PROD".to_string()),
        issue_type: args.issue_type.unwrap_or_else(|| "Task".to_string()),
        priority: args.priority,
        assignee: args.assignee,
        sprint: args.sprint,
        board: args.board_id,
        json: true, // MCP always returns JSON
    };

    let config = crate::atlassian::JiraConfig::from_env().map_err(|e| JsonRpcError {
        code: -32603,
        message: format!("Configuration error: {e}"),
        data: None,
    })?;

    let create_data = crate::atlassian::jira::create_ticket_data(create_options, &config)
        .await
        .map_err(|e| JsonRpcError {
            code: -32603,
            message: format!("Tool execution error: {e}"),
            data: None,
        })?;

    super::to_dual_result(create_data)
}

pub async fn handle_jira_query_list(
    _arguments: Option<serde_json::Value>,
    global: &crate::Global,
) -> Result<serde_json::Value, JsonRpcError> {
    use mcptools_core::queries;
    use std::env;
    use std::path::PathBuf;

    let home = env::var("HOME")
        .ok()
        .map(PathBuf::from)
        .ok_or_else(|| JsonRpcError {
            code: -32603,
            message: "Could not determine home directory".to_string(),
            data: None,
        })?;
    let queries_dir = home.join(".config/mcptools/queries");

    if global.verbose {
        eprintln!("Calling jira_query_list");
    }

    let queries_list = queries::list_queries(&queries_dir).map_err(|e| JsonRpcError {
        code: -32603,
        message: format!("Failed to list queries: {e}"),
        data: None,
    })?;

    super::to_dual_result(mcptools_core::atlassian::jira::QueryListOutput {
        queries: queries_list,
    })
}

pub async fn handle_jira_query_save(
    arguments: Option<serde_json::Value>,
    global: &crate::Global,
) -> Result<serde_json::Value, JsonRpcError> {
    use mcptools_core::queries;
    use std::env;
    use std::path::PathBuf;

    let args: JiraQuerySaveArgs =
        serde_json::from_value(arguments.unwrap_or(serde_json::Value::Null)).map_err(|e| {
            JsonRpcError {
                code: -32602,
                message: format!("Invalid arguments: {e}"),
                data: None,
            }
        })?;

    let home = env::var("HOME")
        .ok()
        .map(PathBuf::from)
        .ok_or_else(|| JsonRpcError {
            code: -32603,
            message: "Could not determine home directory".to_string(),
            data: None,
        })?;
    let queries_dir = home.join(".config/mcptools/queries");

    if global.verbose {
        eprintln!(
            "Calling jira_query_save: name={}, update={}",
            args.name,
            args.update.unwrap_or(false)
        );
    }

    queries::save_query(
        &queries_dir,
        &args.name,
        &args.query,
        args.update.unwrap_or(false),
    )
    .map_err(|e| JsonRpcError {
        code: -32603,
        message: format!("Failed to save query: {e}"),
        data: None,
    })?;

    super::to_dual_result(mcptools_core::atlassian::jira::QueryStatusOutput {
        status: "success".to_string(),
        message: format!("Query '{}' saved", args.name),
    })
}

pub async fn handle_jira_query_delete(
    arguments: Option<serde_json::Value>,
    global: &crate::Global,
) -> Result<serde_json::Value, JsonRpcError> {
    use mcptools_core::queries;
    use std::env;
    use std::path::PathBuf;

    let args: JiraQueryDeleteArgs =
        serde_json::from_value(arguments.unwrap_or(serde_json::Value::Null)).map_err(|e| {
            JsonRpcError {
                code: -32602,
                message: format!("Invalid arguments: {e}"),
                data: None,
            }
        })?;

    let home = env::var("HOME")
        .ok()
        .map(PathBuf::from)
        .ok_or_else(|| JsonRpcError {
            code: -32603,
            message: "Could not determine home directory".to_string(),
            data: None,
        })?;
    let queries_dir = home.join(".config/mcptools/queries");

    if global.verbose {
        eprintln!("Calling jira_query_delete: name={}", args.name);
    }

    queries::delete_query(&queries_dir, &args.name).map_err(|e| JsonRpcError {
        code: -32603,
        message: format!("Failed to delete query: {e}"),
        data: None,
    })?;

    super::to_dual_result(mcptools_core::atlassian::jira::QueryStatusOutput {
        status: "success".to_string(),
        message: format!("Query '{}' deleted", args.name),
    })
}

pub async fn handle_jira_query_load(
    arguments: Option<serde_json::Value>,
    global: &crate::Global,
) -> Result<serde_json::Value, JsonRpcError> {
    use mcptools_core::queries;
    use std::env;
    use std::path::PathBuf;

    let args: JiraQueryLoadArgs =
        serde_json::from_value(arguments.unwrap_or(serde_json::Value::Null)).map_err(|e| {
            JsonRpcError {
                code: -32602,
                message: format!("Invalid arguments: {e}"),
                data: None,
            }
        })?;

    let home = env::var("HOME")
        .ok()
        .map(PathBuf::from)
        .ok_or_else(|| JsonRpcError {
            code: -32603,
            message: "Could not determine home directory".to_string(),
            data: None,
        })?;
    let queries_dir = home.join(".config/mcptools/queries");

    if global.verbose {
        eprintln!("Calling jira_query_load: name={}", args.name);
    }

    let query = queries::load_query(&queries_dir, &args.name).map_err(|e| JsonRpcError {
        code: -32603,
        message: format!("Failed to load query: {e}"),
        data: None,
    })?;

    super::to_dual_result(mcptools_core::atlassian::jira::QueryLoadOutput {
        name: args.name,
        query,
    })
}

pub async fn handle_bitbucket_pr_list(
    arguments: Option<serde_json::Value>,
    global: &crate::Global,
) -> Result<serde_json::Value, JsonRpcError> {
    use crate::atlassian::bitbucket::{list_pr_data, ListPRParams};

    let args: BitbucketPRListArgs =
        serde_json::from_value(arguments.unwrap_or(serde_json::Value::Null)).map_err(|e| {
            JsonRpcError {
                code: -32602,
                message: format!("Invalid arguments: {e}"),
                data: None,
            }
        })?;

    if global.verbose {
        eprintln!(
            "Calling bitbucket_pr_list: repo={}, state={:?}, limit={:?}, nextPage={:?}",
            args.repo, args.state, args.limit, args.next_page
        );
    }

    let params = ListPRParams {
        repo: args.repo,
        states: args.state,
        limit: args.limit.unwrap_or(10),
        next_page: args.next_page,
        base_url_override: None,
    };

    let config = crate::atlassian::BitbucketConfig::from_env().map_err(|e| JsonRpcError {
        code: -32603,
        message: format!("Configuration error: {e}"),
        data: None,
    })?;

    let list_data = list_pr_data(params, &config, None)
        .await
        .map_err(|e| JsonRpcError {
            code: -32603,
            message: format!("Tool execution error: {e}"),
            data: None,
        })?;

    super::to_dual_result(list_data)
}

pub async fn handle_bitbucket_pr_read(
    arguments: Option<serde_json::Value>,
    global: &crate::Global,
) -> Result<serde_json::Value, JsonRpcError> {
    use crate::atlassian::bitbucket::{read_pr_data, ReadPRParams};

    let args: BitbucketPRReadArgs =
        serde_json::from_value(arguments.unwrap_or(serde_json::Value::Null)).map_err(|e| {
            JsonRpcError {
                code: -32602,
                message: format!("Invalid arguments: {e}"),
                data: None,
            }
        })?;

    let fields = args.fields;

    if global.verbose {
        eprintln!(
            "Calling bitbucket_pr_read: repo={}, prNumber={}, limit={:?}, diffLimit={:?}, lineLimit={:?}, noDiff={:?}",
            args.repo, args.pr_number, args.limit, args.diff_limit, args.line_limit, args.no_diff
        );
    }

    let params = ReadPRParams {
        repo: args.repo,
        pr_number: args.pr_number,
        base_url_override: None,
        comment_limit: args.limit.unwrap_or(100),
        comment_next_page: None,
        diff_limit: args.diff_limit.unwrap_or(500),
        diff_next_page: None,
        no_diff: args.no_diff.unwrap_or(false),
    };

    let config = crate::atlassian::BitbucketConfig::from_env().map_err(|e| JsonRpcError {
        code: -32603,
        message: format!("Configuration error: {e}"),
        data: None,
    })?;

    let mut pr_data = read_pr_data(params, &config, None)
        .await
        .map_err(|e| JsonRpcError {
            code: -32603,
            message: format!("Tool execution error: {e}"),
            data: None,
        })?;

    let line_limit = args.line_limit.unwrap_or(500);
    if line_limit >= 0 {
        if let Some(ref diff_content) = pr_data.diff_content {
            let lines: Vec<&str> = diff_content.lines().collect();
            if lines.len() > line_limit as usize {
                let truncated: String = lines[..line_limit as usize].join("\n");
                pr_data.diff_content = Some(format!(
                    "{}\n\n... (truncated at {} lines, use lineLimit=-1 for full diff or increase lineLimit)",
                    truncated,
                    line_limit
                ));
            }
        }
    }

    super::to_dual_result_projected(pr_data, fields.as_deref())
}

pub async fn handle_bitbucket_pr_create(
    arguments: Option<serde_json::Value>,
    global: &crate::Global,
) -> Result<serde_json::Value, JsonRpcError> {
    use crate::atlassian::bitbucket::{create_pr_data, CreatePRParams};

    let args: BitbucketPRCreateArgs =
        serde_json::from_value(arguments.unwrap_or(serde_json::Value::Null)).map_err(|e| {
            JsonRpcError {
                code: -32602,
                message: format!("Invalid arguments: {e}"),
                data: None,
            }
        })?;

    if global.verbose {
        eprintln!(
            "Calling bitbucket_pr_create: repo={}, title={}, sourceBranch={}, destinationBranch={:?}, description={:?}, closeSourceBranch={:?}",
            args.repo, args.title, args.source_branch, args.destination_branch, args.description, args.close_source_branch
        );
    }

    let params = CreatePRParams {
        repo: args.repo,
        title: args.title,
        source_branch: args.source_branch,
        destination_branch: args.destination_branch,
        description: args.description,
        close_source_branch: args.close_source_branch.unwrap_or(false),
        base_url_override: None,
    };

    let config = crate::atlassian::BitbucketConfig::from_env().map_err(|e| JsonRpcError {
        code: -32603,
        message: format!("Configuration error: {e}"),
        data: None,
    })?;

    let pr_data = create_pr_data(params, &config, None)
        .await
        .map_err(|e| JsonRpcError {
            code: -32603,
            message: format!("Tool execution error: {e}"),
            data: None,
        })?;

    super::to_dual_result(pr_data)
}

pub async fn handle_bitbucket_pr_comment_add(
    arguments: Option<serde_json::Value>,
    global: &crate::Global,
) -> Result<serde_json::Value, JsonRpcError> {
    let args: mcptools_core::atlassian::bitbucket::PRCommentAddArgs =
        serde_json::from_value(arguments.unwrap_or(serde_json::Value::Null)).map_err(|e| {
            JsonRpcError {
                code: -32602,
                message: format!("Invalid arguments: {e}"),
                data: None,
            }
        })?;
    let request = args.into_request().map_err(|e| JsonRpcError {
        code: -32602,
        message: format!("Invalid arguments: {e}"),
        data: None,
    })?;

    if global.verbose {
        eprintln!("Calling bitbucket_pr_comment_add: {}", request.endpoint());
    }

    let config = crate::atlassian::BitbucketConfig::from_env().map_err(|e| JsonRpcError {
        code: -32603,
        message: format!("Configuration error: {e}"),
        data: None,
    })?;
    let output = crate::atlassian::bitbucket::add_pr_comment_data(request, &config)
        .await
        .map_err(|e| JsonRpcError {
            code: -32603,
            message: format!("Tool execution error: {e}"),
            data: None,
        })?;
    super::to_dual_result(output)
}

pub async fn handle_bitbucket_pr_update(
    arguments: Option<serde_json::Value>,
    global: &crate::Global,
) -> Result<serde_json::Value, JsonRpcError> {
    use crate::atlassian::bitbucket::{update_pr_data, UpdatePRParams};

    let args: BitbucketPRUpdateArgs =
        serde_json::from_value(arguments.unwrap_or(serde_json::Value::Null)).map_err(|e| {
            JsonRpcError {
                code: -32602,
                message: format!("Invalid arguments: {e}"),
                data: None,
            }
        })?;

    if global.verbose {
        eprintln!(
            "Calling bitbucket_pr_update: repo={}, prNumber={}",
            args.repo, args.pr_number
        );
    }

    let params = UpdatePRParams {
        repo: args.repo,
        pr_number: args.pr_number,
        title: args.title,
        description: args.description,
        destination_branch: args.destination_branch,
        reviewers: args.reviewers,
        close_source_branch: args.close_source_branch,
        approve: args.approve.unwrap_or(false),
        unapprove: args.unapprove.unwrap_or(false),
        decline: args.decline.unwrap_or(false),
        merge: args.merge.unwrap_or(false),
        merge_strategy: args.merge_strategy,
        merge_message: args.merge_message,
        base_url_override: None,
    };

    let config = crate::atlassian::BitbucketConfig::from_env().map_err(|e| JsonRpcError {
        code: -32603,
        message: format!("Configuration error: {e}"),
        data: None,
    })?;

    let pr_data = update_pr_data(params, &config, None)
        .await
        .map_err(|e| JsonRpcError {
            code: -32603,
            message: format!("Tool execution error: {e}"),
            data: None,
        })?;

    super::to_dual_result(pr_data)
}

pub async fn handle_jira_attachment_list(
    arguments: Option<serde_json::Value>,
    global: &crate::Global,
) -> Result<serde_json::Value, JsonRpcError> {
    let args: JiraAttachmentListArgs =
        serde_json::from_value(arguments.unwrap_or(serde_json::Value::Null)).map_err(|e| {
            JsonRpcError {
                code: -32602,
                message: format!("Invalid arguments: {e}"),
                data: None,
            }
        })?;

    if global.verbose {
        eprintln!("Calling jira_attachment_list: issueKey={}", args.issue_key);
    }

    let config = crate::atlassian::JiraConfig::from_env().map_err(|e| JsonRpcError {
        code: -32603,
        message: format!("Configuration error: {e}"),
        data: None,
    })?;

    let attachments = crate::atlassian::jira::list_attachments_data(args.issue_key, &config)
        .await
        .map_err(|e| JsonRpcError {
            code: -32603,
            message: format!("Tool execution error: {e}"),
            data: None,
        })?;

    super::to_dual_result(mcptools_core::atlassian::jira::AttachmentListOutput { attachments })
}

pub async fn handle_jira_attachment_download(
    arguments: Option<serde_json::Value>,
    global: &crate::Global,
) -> Result<serde_json::Value, JsonRpcError> {
    let args: JiraAttachmentDownloadArgs =
        serde_json::from_value(arguments.unwrap_or(serde_json::Value::Null)).map_err(|e| {
            JsonRpcError {
                code: -32602,
                message: format!("Invalid arguments: {e}"),
                data: None,
            }
        })?;

    if global.verbose {
        eprintln!(
            "Calling jira_attachment_download: issueKey={}, attachmentId={}, outputPath={:?}",
            args.issue_key, args.attachment_id, args.output_path,
        );
    }

    let config = crate::atlassian::JiraConfig::from_env().map_err(|e| JsonRpcError {
        code: -32603,
        message: format!("Configuration error: {e}"),
        data: None,
    })?;

    let output = args.output_path.map(std::path::PathBuf::from);

    let path = crate::atlassian::jira::download_attachment_data(
        args.issue_key,
        args.attachment_id,
        output,
        &config,
    )
    .await
    .map_err(|e| JsonRpcError {
        code: -32603,
        message: format!("Tool execution error: {e}"),
        data: None,
    })?;

    super::to_dual_result(mcptools_core::atlassian::jira::AttachmentDownloadOutput {
        path: path.display().to_string(),
    })
}

pub async fn handle_jira_attachment_upload(
    arguments: Option<serde_json::Value>,
    global: &crate::Global,
) -> Result<serde_json::Value, JsonRpcError> {
    let args: JiraAttachmentUploadArgs =
        serde_json::from_value(arguments.unwrap_or(serde_json::Value::Null)).map_err(|e| {
            JsonRpcError {
                code: -32602,
                message: format!("Invalid arguments: {e}"),
                data: None,
            }
        })?;

    if global.verbose {
        eprintln!(
            "Calling jira_attachment_upload: issueKey={}, filePaths={:?}",
            args.issue_key, args.file_paths,
        );
    }

    let config = crate::atlassian::JiraConfig::from_env().map_err(|e| JsonRpcError {
        code: -32603,
        message: format!("Configuration error: {e}"),
        data: None,
    })?;

    let paths: Vec<std::path::PathBuf> = args
        .file_paths
        .into_iter()
        .map(std::path::PathBuf::from)
        .collect();

    let uploads = crate::atlassian::jira::upload_attachment_data(args.issue_key, paths, &config)
        .await
        .map_err(|e| JsonRpcError {
            code: -32603,
            message: format!("Tool execution error: {e}"),
            data: None,
        })?;

    super::to_dual_result(mcptools_core::atlassian::jira::AttachmentListOutput {
        attachments: uploads,
    })
}

pub async fn handle_jira_comment_add(
    arguments: Option<serde_json::Value>,
    global: &crate::Global,
) -> Result<serde_json::Value, JsonRpcError> {
    let args: JiraCommentAddArgs =
        serde_json::from_value(arguments.unwrap_or(serde_json::Value::Null)).map_err(|e| {
            JsonRpcError {
                code: -32602,
                message: format!("Invalid arguments: {e}"),
                data: None,
            }
        })?;

    if global.verbose {
        let comment_preview: String = args.comment.chars().take(50).collect();
        eprintln!(
            "Calling jira_comment_add: issueKey={}, comment={}",
            args.issue_key, comment_preview
        );
    }

    let config = crate::atlassian::JiraConfig::from_env().map_err(|e| JsonRpcError {
        code: -32603,
        message: format!("Configuration error: {e}"),
        data: None,
    })?;

    let output = crate::atlassian::jira::add_comment_data(args.issue_key, args.comment, &config)
        .await
        .map_err(|e| JsonRpcError {
            code: -32603,
            message: format!("Tool execution error: {e}"),
            data: None,
        })?;

    super::to_dual_result(output)
}

pub async fn handle_jira_comment_list(
    arguments: Option<serde_json::Value>,
    global: &crate::Global,
) -> Result<serde_json::Value, JsonRpcError> {
    let args: JiraCommentListArgs =
        serde_json::from_value(arguments.unwrap_or(serde_json::Value::Null)).map_err(|e| {
            JsonRpcError {
                code: -32602,
                message: format!("Invalid arguments: {e}"),
                data: None,
            }
        })?;

    if global.verbose {
        eprintln!("Calling jira_comment_list: issueKey={}", args.issue_key);
    }

    let config = crate::atlassian::JiraConfig::from_env().map_err(|e| JsonRpcError {
        code: -32603,
        message: format!("Configuration error: {e}"),
        data: None,
    })?;

    let comments = crate::atlassian::jira::list_comments_data(args.issue_key, &config)
        .await
        .map_err(|e| JsonRpcError {
            code: -32603,
            message: format!("Tool execution error: {e}"),
            data: None,
        })?;

    super::to_dual_result(mcptools_core::atlassian::jira::CommentListOutput { comments })
}

pub async fn handle_jira_comment_update(
    arguments: Option<serde_json::Value>,
    global: &crate::Global,
) -> Result<serde_json::Value, JsonRpcError> {
    let args: JiraCommentUpdateArgs =
        serde_json::from_value(arguments.unwrap_or(serde_json::Value::Null)).map_err(|e| {
            JsonRpcError {
                code: -32602,
                message: format!("Invalid arguments: {e}"),
                data: None,
            }
        })?;

    if global.verbose {
        let comment_preview: String = args.comment.chars().take(50).collect();
        eprintln!(
            "Calling jira_comment_update: issueKey={}, commentId={}, comment={}",
            args.issue_key, args.comment_id, comment_preview
        );
    }

    let config = crate::atlassian::JiraConfig::from_env().map_err(|e| JsonRpcError {
        code: -32603,
        message: format!("Configuration error: {e}"),
        data: None,
    })?;

    let output = crate::atlassian::jira::update_comment_data(
        args.issue_key,
        args.comment_id,
        args.comment,
        &config,
    )
    .await
    .map_err(|e| JsonRpcError {
        code: -32603,
        message: format!("Tool execution error: {e}"),
        data: None,
    })?;

    super::to_dual_result(output)
}

pub async fn handle_jira_comment_delete(
    arguments: Option<serde_json::Value>,
    global: &crate::Global,
) -> Result<serde_json::Value, JsonRpcError> {
    let args: JiraCommentDeleteArgs =
        serde_json::from_value(arguments.unwrap_or(serde_json::Value::Null)).map_err(|e| {
            JsonRpcError {
                code: -32602,
                message: format!("Invalid arguments: {e}"),
                data: None,
            }
        })?;

    if global.verbose {
        eprintln!(
            "Calling jira_comment_delete: issueKey={}, commentId={}",
            args.issue_key, args.comment_id
        );
    }

    let config = crate::atlassian::JiraConfig::from_env().map_err(|e| JsonRpcError {
        code: -32603,
        message: format!("Configuration error: {e}"),
        data: None,
    })?;

    crate::atlassian::jira::delete_comment_data(
        args.issue_key.clone(),
        args.comment_id.clone(),
        &config,
    )
    .await
    .map_err(|e| JsonRpcError {
        code: -32603,
        message: format!("Tool execution error: {e}"),
        data: None,
    })?;

    super::to_dual_result(mcptools_core::atlassian::jira::CommentDeleteOutput {
        deleted: true,
        issue_key: args.issue_key,
        comment_id: args.comment_id,
    })
}

pub async fn handle_bitbucket_workspace_list(
    arguments: Option<serde_json::Value>,
    global: &crate::Global,
) -> Result<serde_json::Value, JsonRpcError> {
    use crate::atlassian::bitbucket::{list_workspace_data, ListWorkspaceParams};

    let args: BitbucketWorkspaceListArgs =
        serde_json::from_value(arguments.unwrap_or(serde_json::Value::Null)).map_err(|e| {
            JsonRpcError {
                code: -32602,
                message: format!("Invalid arguments: {e}"),
                data: None,
            }
        })?;

    if global.verbose {
        eprintln!(
            "Calling bitbucket_workspace_list: limit={:?}, nextPage={:?}",
            args.limit, args.next_page
        );
    }

    let params = ListWorkspaceParams {
        limit: args.limit.unwrap_or(10),
        next_page: args.next_page,
        base_url_override: None,
    };

    let config = crate::atlassian::BitbucketConfig::from_env().map_err(|e| JsonRpcError {
        code: -32603,
        message: format!("Configuration error: {e}"),
        data: None,
    })?;

    let list_data = list_workspace_data(params, &config, None)
        .await
        .map_err(|e| JsonRpcError {
            code: -32603,
            message: format!("Tool execution error: {e}"),
            data: None,
        })?;

    super::to_dual_result(list_data)
}

pub async fn handle_bitbucket_repo_list(
    arguments: Option<serde_json::Value>,
    global: &crate::Global,
) -> Result<serde_json::Value, JsonRpcError> {
    use crate::atlassian::bitbucket::{list_repo_data, ListRepoParams};

    let args: BitbucketRepoListArgs =
        serde_json::from_value(arguments.unwrap_or(serde_json::Value::Null)).map_err(|e| {
            JsonRpcError {
                code: -32602,
                message: format!("Invalid arguments: {e}"),
                data: None,
            }
        })?;

    if global.verbose {
        eprintln!(
            "Calling bitbucket_repo_list: workspace={}, limit={:?}, nextPage={:?}",
            args.workspace, args.limit, args.next_page
        );
    }

    let params = ListRepoParams {
        workspace: args.workspace,
        limit: args.limit.unwrap_or(10),
        next_page: args.next_page,
        base_url_override: None,
    };

    let config = crate::atlassian::BitbucketConfig::from_env().map_err(|e| JsonRpcError {
        code: -32603,
        message: format!("Configuration error: {e}"),
        data: None,
    })?;

    let list_data = list_repo_data(params, &config, None)
        .await
        .map_err(|e| JsonRpcError {
            code: -32603,
            message: format!("Tool execution error: {e}"),
            data: None,
        })?;

    super::to_dual_result(list_data)
}

pub async fn handle_bitbucket_repo_branches(
    arguments: Option<serde_json::Value>,
    global: &crate::Global,
) -> Result<serde_json::Value, JsonRpcError> {
    use crate::atlassian::bitbucket::{list_branches_data, ListBranchesParams};

    let args: BitbucketRepoBranchesArgs =
        serde_json::from_value(arguments.unwrap_or(serde_json::Value::Null)).map_err(|e| {
            JsonRpcError {
                code: -32602,
                message: format!("Invalid arguments: {e}"),
                data: None,
            }
        })?;

    if global.verbose {
        eprintln!(
            "Calling bitbucket_repo_branches: workspace={}, repo={}, limit={:?}, query={:?}, sort={:?}",
            args.workspace, args.repo, args.limit, args.query, args.sort
        );
    }

    let params = ListBranchesParams {
        workspace: args.workspace,
        repo: args.repo,
        limit: args.limit.unwrap_or(10),
        next_page: args.next_page,
        query: args.query,
        sort: args.sort,
        base_url_override: None,
    };

    let config = crate::atlassian::BitbucketConfig::from_env().map_err(|e| JsonRpcError {
        code: -32603,
        message: format!("Configuration error: {e}"),
        data: None,
    })?;

    let list_data = list_branches_data(params, &config, None)
        .await
        .map_err(|e| JsonRpcError {
            code: -32603,
            message: format!("Tool execution error: {e}"),
            data: None,
        })?;

    super::to_dual_result(list_data)
}
