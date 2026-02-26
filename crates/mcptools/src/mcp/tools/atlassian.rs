use crate::prelude::{eprintln, *};
use serde::Deserialize;

use super::{CallToolResult, Content, JsonRpcError};

/// Handle Jira search command via MCP
pub async fn handle_jira_search(
    arguments: Option<serde_json::Value>,
    global: &crate::Global,
) -> Result<serde_json::Value, JsonRpcError> {
    use mcptools_core::queries;
    use std::env;
    use std::path::PathBuf;

    #[derive(Deserialize)]
    struct JiraSearchArgs {
        query: Option<String>,
        #[serde(rename = "queryName")]
        query_name: Option<String>,
        limit: Option<usize>,
        #[serde(rename = "nextPageToken")]
        next_page_token: Option<String>,
    }

    let args: JiraSearchArgs = serde_json::from_value(arguments.unwrap_or(serde_json::Value::Null))
        .map_err(|e| JsonRpcError {
            code: -32602,
            message: format!("Invalid arguments: {e}"),
            data: None,
        })?;

    // Resolve query: either use provided query or load saved query
    let resolved_query = if let Some(query_name) = args.query_name {
        // Load saved query
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

    // Call the Jira module's data function
    let search_data = crate::atlassian::jira::search_issues_data(
        resolved_query,
        args.limit.unwrap_or(10),
        args.next_page_token,
    )
    .await
    .map_err(|e| JsonRpcError {
        code: -32603,
        message: format!("Tool execution error: {e}"),
        data: None,
    })?;

    // Convert to JSON and wrap in MCP result format
    let json_string = serde_json::to_string_pretty(&search_data).map_err(|e| JsonRpcError {
        code: -32603,
        message: format!("Serialization error: {e}"),
        data: None,
    })?;

    let result = CallToolResult {
        content: vec![Content::Text { text: json_string }],
        is_error: None,
    };

    serde_json::to_value(result).map_err(|e| JsonRpcError {
        code: -32603,
        message: format!("Internal error: {e}"),
        data: None,
    })
}

/// Handle Confluence search command via MCP
pub async fn handle_confluence_search(
    arguments: Option<serde_json::Value>,
    global: &crate::Global,
) -> Result<serde_json::Value, JsonRpcError> {
    #[derive(Deserialize)]
    struct ConfluenceSearchArgs {
        query: String,
        limit: Option<usize>,
    }

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

    // Call the Confluence module's data function
    let search_data =
        crate::atlassian::confluence::search_pages_data(args.query, args.limit.unwrap_or(10))
            .await
            .map_err(|e| JsonRpcError {
                code: -32603,
                message: format!("Tool execution error: {e}"),
                data: None,
            })?;

    // Convert to JSON and wrap in MCP result format
    let json_string = serde_json::to_string_pretty(&search_data).map_err(|e| JsonRpcError {
        code: -32603,
        message: format!("Serialization error: {e}"),
        data: None,
    })?;

    let result = CallToolResult {
        content: vec![Content::Text { text: json_string }],
        is_error: None,
    };

    serde_json::to_value(result).map_err(|e| JsonRpcError {
        code: -32603,
        message: format!("Internal error: {e}"),
        data: None,
    })
}

/// Handle Jira get command via MCP
pub async fn handle_jira_get(
    arguments: Option<serde_json::Value>,
    global: &crate::Global,
) -> Result<serde_json::Value, JsonRpcError> {
    #[derive(Deserialize)]
    struct JiraGetArgs {
        #[serde(rename = "issueKey")]
        issue_key: String,
    }

    let args: JiraGetArgs = serde_json::from_value(arguments.unwrap_or(serde_json::Value::Null))
        .map_err(|e| JsonRpcError {
            code: -32602,
            message: format!("Invalid arguments: {e}"),
            data: None,
        })?;

    if global.verbose {
        eprintln!("Calling jira_get: issueKey={}", args.issue_key);
    }

    // Call the Jira module's data function
    let ticket_data = crate::atlassian::jira::get_ticket_data(args.issue_key)
        .await
        .map_err(|e| JsonRpcError {
            code: -32603,
            message: format!("Tool execution error: {e}"),
            data: None,
        })?;

    // Convert to JSON and wrap in MCP result format
    let json_string = serde_json::to_string_pretty(&ticket_data).map_err(|e| JsonRpcError {
        code: -32603,
        message: format!("Serialization error: {e}"),
        data: None,
    })?;

    let result = CallToolResult {
        content: vec![Content::Text { text: json_string }],
        is_error: None,
    };

    serde_json::to_value(result).map_err(|e| JsonRpcError {
        code: -32603,
        message: format!("Internal error: {e}"),
        data: None,
    })
}

/// Handle Jira update command via MCP
pub async fn handle_jira_update(
    arguments: Option<serde_json::Value>,
    global: &crate::Global,
) -> Result<serde_json::Value, JsonRpcError> {
    #[derive(Deserialize)]
    struct JiraUpdateArgs {
        #[serde(rename = "ticketKey")]
        ticket_key: String,
        status: Option<String>,
        priority: Option<String>,
        #[serde(rename = "issueType")]
        issue_type: Option<String>,
        assignee: Option<String>,
    }

    let args: JiraUpdateArgs = serde_json::from_value(arguments.unwrap_or(serde_json::Value::Null))
        .map_err(|e| JsonRpcError {
            code: -32602,
            message: format!("Invalid arguments: {e}"),
            data: None,
        })?;

    if global.verbose {
        eprintln!(
            "Calling jira_update: ticketKey={}, status={:?}, priority={:?}, issueType={:?}, assignee={:?}",
            args.ticket_key,
            args.status,
            args.priority,
            args.issue_type,
            args.assignee,
        );
    }

    // Build UpdateOptions from MCP arguments
    let update_options = crate::atlassian::jira::update::UpdateOptions {
        ticket_key: args.ticket_key,
        status: args.status,
        priority: args.priority,
        issue_type: args.issue_type,
        assignee: args.assignee,
        json: true, // MCP always returns JSON
    };

    // Call the Jira module's data function
    let update_data = crate::atlassian::jira::update_ticket_data(update_options)
        .await
        .map_err(|e| JsonRpcError {
            code: -32603,
            message: format!("Tool execution error: {e}"),
            data: None,
        })?;

    // Convert to JSON and wrap in MCP result format
    let json_string = serde_json::to_string_pretty(&update_data).map_err(|e| JsonRpcError {
        code: -32603,
        message: format!("Serialization error: {e}"),
        data: None,
    })?;

    let result = CallToolResult {
        content: vec![Content::Text { text: json_string }],
        is_error: None,
    };

    serde_json::to_value(result).map_err(|e| JsonRpcError {
        code: -32603,
        message: format!("Internal error: {e}"),
        data: None,
    })
}

/// Handle Jira create command via MCP
pub async fn handle_jira_create(
    arguments: Option<serde_json::Value>,
    global: &crate::Global,
) -> Result<serde_json::Value, JsonRpcError> {
    #[derive(Deserialize)]
    struct JiraCreateArgs {
        summary: String,
        description: Option<String>,
        project: Option<String>,
        #[serde(rename = "issueType")]
        issue_type: Option<String>,
        priority: Option<String>,
        assignee: Option<String>,
    }

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
            "Calling jira_create: summary={}, description={:?}, project={:?}, issueType={:?}, priority={:?}, assignee={:?}",
            args.summary,
            desc_preview,
            args.project,
            args.issue_type,
            args.priority,
            args.assignee,
        );
    }

    // Build CreateOptions from MCP arguments
    let create_options = crate::atlassian::jira::create::CreateOptions {
        summary: args.summary,
        description: args.description,
        project: args.project.unwrap_or_else(|| "PROD".to_string()),
        issue_type: args.issue_type.unwrap_or_else(|| "Task".to_string()),
        priority: args.priority,
        assignee: args.assignee,
        json: true, // MCP always returns JSON
    };

    // Call the Jira module's data function
    let create_data = crate::atlassian::jira::create_ticket_data(create_options)
        .await
        .map_err(|e| JsonRpcError {
            code: -32603,
            message: format!("Tool execution error: {e}"),
            data: None,
        })?;

    // Convert to JSON and wrap in MCP result format
    let json_string = serde_json::to_string_pretty(&create_data).map_err(|e| JsonRpcError {
        code: -32603,
        message: format!("Serialization error: {e}"),
        data: None,
    })?;

    let result = CallToolResult {
        content: vec![Content::Text { text: json_string }],
        is_error: None,
    };

    serde_json::to_value(result).map_err(|e| JsonRpcError {
        code: -32603,
        message: format!("Internal error: {e}"),
        data: None,
    })
}

/// Handle Jira query list command via MCP
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

    let json_string = serde_json::to_string_pretty(&serde_json::json!({
        "queries": queries_list
    }))
    .map_err(|e| JsonRpcError {
        code: -32603,
        message: format!("Serialization error: {e}"),
        data: None,
    })?;

    let result = CallToolResult {
        content: vec![Content::Text { text: json_string }],
        is_error: None,
    };

    serde_json::to_value(result).map_err(|e| JsonRpcError {
        code: -32603,
        message: format!("Internal error: {e}"),
        data: None,
    })
}

/// Handle Jira query save command via MCP
pub async fn handle_jira_query_save(
    arguments: Option<serde_json::Value>,
    global: &crate::Global,
) -> Result<serde_json::Value, JsonRpcError> {
    use mcptools_core::queries;
    use std::env;
    use std::path::PathBuf;

    #[derive(Deserialize)]
    struct JiraQuerySaveArgs {
        name: String,
        query: String,
        update: Option<bool>,
    }

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

    let json_string = serde_json::to_string_pretty(&serde_json::json!({
        "status": "success",
        "message": format!("Query '{}' saved", args.name)
    }))
    .map_err(|e| JsonRpcError {
        code: -32603,
        message: format!("Serialization error: {e}"),
        data: None,
    })?;

    let result = CallToolResult {
        content: vec![Content::Text { text: json_string }],
        is_error: None,
    };

    serde_json::to_value(result).map_err(|e| JsonRpcError {
        code: -32603,
        message: format!("Internal error: {e}"),
        data: None,
    })
}

/// Handle Jira query delete command via MCP
pub async fn handle_jira_query_delete(
    arguments: Option<serde_json::Value>,
    global: &crate::Global,
) -> Result<serde_json::Value, JsonRpcError> {
    use mcptools_core::queries;
    use std::env;
    use std::path::PathBuf;

    #[derive(Deserialize)]
    struct JiraQueryDeleteArgs {
        name: String,
    }

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

    let json_string = serde_json::to_string_pretty(&serde_json::json!({
        "status": "success",
        "message": format!("Query '{}' deleted", args.name)
    }))
    .map_err(|e| JsonRpcError {
        code: -32603,
        message: format!("Serialization error: {e}"),
        data: None,
    })?;

    let result = CallToolResult {
        content: vec![Content::Text { text: json_string }],
        is_error: None,
    };

    serde_json::to_value(result).map_err(|e| JsonRpcError {
        code: -32603,
        message: format!("Internal error: {e}"),
        data: None,
    })
}

/// Handle Jira query load command via MCP
pub async fn handle_jira_query_load(
    arguments: Option<serde_json::Value>,
    global: &crate::Global,
) -> Result<serde_json::Value, JsonRpcError> {
    use mcptools_core::queries;
    use std::env;
    use std::path::PathBuf;

    #[derive(Deserialize)]
    struct JiraQueryLoadArgs {
        name: String,
    }

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

    let json_string = serde_json::to_string_pretty(&serde_json::json!({
        "name": args.name,
        "query": query
    }))
    .map_err(|e| JsonRpcError {
        code: -32603,
        message: format!("Serialization error: {e}"),
        data: None,
    })?;

    let result = CallToolResult {
        content: vec![Content::Text { text: json_string }],
        is_error: None,
    };

    serde_json::to_value(result).map_err(|e| JsonRpcError {
        code: -32603,
        message: format!("Internal error: {e}"),
        data: None,
    })
}

/// Handle Bitbucket PR list command via MCP
pub async fn handle_bitbucket_pr_list(
    arguments: Option<serde_json::Value>,
    global: &crate::Global,
) -> Result<serde_json::Value, JsonRpcError> {
    use crate::atlassian::bitbucket::{list_pr_data, ListPRParams};

    #[derive(Deserialize)]
    struct BitbucketPRListArgs {
        repo: String,
        state: Option<Vec<String>>,
        limit: Option<usize>,
        #[serde(rename = "nextPage")]
        next_page: Option<String>,
    }

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

    // Build ListPRParams from MCP arguments
    let params = ListPRParams {
        repo: args.repo,
        states: args.state,
        limit: args.limit.unwrap_or(10),
        next_page: args.next_page,
        base_url_override: None,
        app_password_override: global.bitbucket_app_password.clone(),
    };

    // Call the Bitbucket module's data function (no spinner for MCP)
    let list_data = list_pr_data(params, None).await.map_err(|e| JsonRpcError {
        code: -32603,
        message: format!("Tool execution error: {e}"),
        data: None,
    })?;

    // Convert to JSON and wrap in MCP result format
    let json_string = serde_json::to_string_pretty(&list_data).map_err(|e| JsonRpcError {
        code: -32603,
        message: format!("Serialization error: {e}"),
        data: None,
    })?;

    let result = CallToolResult {
        content: vec![Content::Text { text: json_string }],
        is_error: None,
    };

    serde_json::to_value(result).map_err(|e| JsonRpcError {
        code: -32603,
        message: format!("Internal error: {e}"),
        data: None,
    })
}

/// Handle Bitbucket PR read command via MCP
pub async fn handle_bitbucket_pr_read(
    arguments: Option<serde_json::Value>,
    global: &crate::Global,
) -> Result<serde_json::Value, JsonRpcError> {
    use crate::atlassian::bitbucket::{read_pr_data, ReadPRParams};

    #[derive(Deserialize)]
    struct BitbucketPRReadArgs {
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

    let args: BitbucketPRReadArgs =
        serde_json::from_value(arguments.unwrap_or(serde_json::Value::Null)).map_err(|e| {
            JsonRpcError {
                code: -32602,
                message: format!("Invalid arguments: {e}"),
                data: None,
            }
        })?;

    if global.verbose {
        eprintln!(
            "Calling bitbucket_pr_read: repo={}, prNumber={}, limit={:?}, diffLimit={:?}, lineLimit={:?}, noDiff={:?}",
            args.repo, args.pr_number, args.limit, args.diff_limit, args.line_limit, args.no_diff
        );
    }

    // Build ReadPRParams from MCP arguments
    let params = ReadPRParams {
        repo: args.repo,
        pr_number: args.pr_number,
        base_url_override: None,
        app_password_override: global.bitbucket_app_password.clone(),
        comment_limit: args.limit.unwrap_or(100),
        comment_next_page: None,
        diff_limit: args.diff_limit.unwrap_or(500),
        diff_next_page: None,
        no_diff: args.no_diff.unwrap_or(false),
    };

    // Call the Bitbucket module's data function (no spinner for MCP)
    let mut pr_data = read_pr_data(params, None).await.map_err(|e| JsonRpcError {
        code: -32603,
        message: format!("Tool execution error: {e}"),
        data: None,
    })?;

    // Apply line limit to diff content if specified
    // Default to 500 lines, -1 means unlimited
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

    // Convert to JSON and wrap in MCP result format
    let json_string = serde_json::to_string_pretty(&pr_data).map_err(|e| JsonRpcError {
        code: -32603,
        message: format!("Serialization error: {e}"),
        data: None,
    })?;

    let result = CallToolResult {
        content: vec![Content::Text { text: json_string }],
        is_error: None,
    };

    serde_json::to_value(result).map_err(|e| JsonRpcError {
        code: -32603,
        message: format!("Internal error: {e}"),
        data: None,
    })
}
