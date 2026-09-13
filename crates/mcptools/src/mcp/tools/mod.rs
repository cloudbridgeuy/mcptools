mod annotations;
mod atlas;
mod atlassian;
mod hn;
mod images;
mod linear;
mod md;
mod pdf;
pub mod schema;

use serde::{Deserialize, Serialize};

// Re-export types needed by tool handlers
pub use super::{JsonRpcError, Tool};

// MCP Protocol types for tools
#[derive(Debug, Serialize)]
pub struct ServerInfo {
    pub name: String,
    pub version: String,
}

#[derive(Debug, Serialize)]
pub struct ServerCapabilities {
    pub tools: Option<ToolsCapability>,
    pub resources: Option<ResourcesCapability>,
}

#[derive(Debug, Serialize)]
pub struct ToolsCapability {}

#[derive(Debug, Serialize)]
pub struct ResourcesCapability {}

#[derive(Debug, Serialize)]
pub struct InitializeResult {
    #[serde(rename = "protocolVersion")]
    pub protocol_version: String,
    pub capabilities: ServerCapabilities,
    #[serde(rename = "serverInfo")]
    pub server_info: ServerInfo,
}

#[derive(Debug, Serialize)]
pub struct ToolsList {
    pub tools: Vec<Tool>,
}

#[derive(Debug, Deserialize)]
pub struct CallToolParams {
    pub name: String,
    pub arguments: Option<serde_json::Value>,
}

#[derive(Debug, Serialize)]
pub struct CallToolResult {
    pub content: Vec<Content>,
    #[serde(rename = "structuredContent", skip_serializing_if = "Option::is_none")]
    pub structured_content: Option<serde_json::Value>,
    #[serde(rename = "isError", skip_serializing_if = "Option::is_none")]
    pub is_error: Option<bool>,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type")]
pub enum Content {
    #[serde(rename = "text")]
    Text { text: String },
}

pub fn to_dual_result(value: impl Serialize) -> Result<serde_json::Value, JsonRpcError> {
    let structured = serde_json::to_value(&value).map_err(|e| JsonRpcError {
        code: -32603,
        message: format!("Serialization error: {e}"),
        data: None,
    })?;
    let text = serde_json::to_string_pretty(&structured).map_err(|e| JsonRpcError {
        code: -32603,
        message: format!("Serialization error: {e}"),
        data: None,
    })?;
    let result = CallToolResult {
        content: vec![Content::Text { text }],
        structured_content: Some(structured),
        is_error: None,
    };
    serde_json::to_value(result).map_err(|e| JsonRpcError {
        code: -32603,
        message: format!("Internal error: {e}"),
        data: None,
    })
}

pub fn handle_initialize() -> Result<serde_json::Value, JsonRpcError> {
    let result = InitializeResult {
        protocol_version: "2024-11-05".to_string(),
        capabilities: ServerCapabilities {
            tools: Some(ToolsCapability {}),
            resources: Some(ResourcesCapability {}),
        },
        server_info: ServerInfo {
            name: "mcptools".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        },
    };

    serde_json::to_value(result).map_err(|e| JsonRpcError {
        code: -32603,
        message: format!("Internal error: {e}"),
        data: None,
    })
}

mod atlassian_args {
    #![allow(dead_code)]

    use schemars::JsonSchema;
    use serde::Deserialize;

    #[derive(Deserialize, JsonSchema)]
    pub struct JiraSearchArgs {
        query: Option<String>,
        #[serde(rename = "queryName")]
        query_name: Option<String>,
        limit: Option<usize>,
        #[serde(rename = "nextPageToken")]
        next_page_token: Option<String>,
    }

    #[derive(Deserialize, JsonSchema)]
    pub struct ConfluenceSearchArgs {
        query: String,
        limit: Option<usize>,
    }

    #[derive(Deserialize, JsonSchema)]
    pub struct JiraCreateArgs {
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
    pub struct JiraGetArgs {
        #[serde(rename = "issueKey")]
        issue_key: String,
    }

    #[derive(Deserialize, JsonSchema)]
    pub struct JiraUpdateArgs {
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
    pub struct JiraCommentAddArgs {
        #[serde(rename = "issueKey")]
        issue_key: String,
        comment: String,
    }

    #[derive(Deserialize, JsonSchema)]
    pub struct JiraCommentListArgs {
        #[serde(rename = "issueKey")]
        issue_key: String,
    }

    #[derive(Deserialize, JsonSchema)]
    pub struct JiraCommentUpdateArgs {
        #[serde(rename = "issueKey")]
        issue_key: String,
        #[serde(rename = "commentId")]
        comment_id: String,
        comment: String,
    }

    #[derive(Deserialize, JsonSchema)]
    pub struct JiraCommentDeleteArgs {
        #[serde(rename = "issueKey")]
        issue_key: String,
        #[serde(rename = "commentId")]
        comment_id: String,
    }

    #[derive(Deserialize, JsonSchema)]
    pub struct JiraSprintListArgs {
        #[serde(rename = "boardId")]
        board_id: u64,
        state: Option<String>,
    }

    #[derive(Deserialize, JsonSchema)]
    pub struct JiraAttachmentListArgs {
        #[serde(rename = "issueKey")]
        issue_key: String,
    }

    #[derive(Deserialize, JsonSchema)]
    pub struct JiraAttachmentDownloadArgs {
        #[serde(rename = "issueKey")]
        issue_key: String,
        #[serde(rename = "attachmentId")]
        attachment_id: String,
        #[serde(rename = "outputPath")]
        output_path: Option<String>,
    }

    #[derive(Deserialize, JsonSchema)]
    pub struct JiraAttachmentUploadArgs {
        #[serde(rename = "issueKey")]
        issue_key: String,
        #[serde(rename = "filePaths")]
        file_paths: Vec<String>,
    }

    #[derive(Deserialize, JsonSchema)]
    pub struct JiraQueryListArgs {}

    #[derive(Deserialize, JsonSchema)]
    pub struct JiraQuerySaveArgs {
        name: String,
        query: String,
        update: Option<bool>,
    }

    #[derive(Deserialize, JsonSchema)]
    pub struct JiraQueryDeleteArgs {
        name: String,
    }

    #[derive(Deserialize, JsonSchema)]
    pub struct JiraQueryLoadArgs {
        name: String,
    }

    #[derive(Deserialize, JsonSchema)]
    pub struct BitbucketPrListArgs {
        repo: String,
        state: Option<Vec<String>>,
        limit: Option<usize>,
        #[serde(rename = "nextPage")]
        next_page: Option<String>,
    }

    #[derive(Deserialize, JsonSchema)]
    pub struct BitbucketPrReadArgs {
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
    pub struct BitbucketPrCreateArgs {
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
    pub struct BitbucketWorkspaceListArgs {
        limit: Option<usize>,
        #[serde(rename = "nextPage")]
        next_page: Option<String>,
    }

    #[derive(Deserialize, JsonSchema)]
    pub struct BitbucketRepoListArgs {
        workspace: String,
        limit: Option<usize>,
        #[serde(rename = "nextPage")]
        next_page: Option<String>,
    }

    #[derive(Deserialize, JsonSchema)]
    pub struct BitbucketRepoBranchesArgs {
        workspace: String,
        repo: String,
        limit: Option<usize>,
        #[serde(rename = "nextPage")]
        next_page: Option<String>,
        query: Option<String>,
        sort: Option<String>,
    }
}

pub fn handle_tools_list() -> Result<serde_json::Value, JsonRpcError> {
    let tools = vec![
        Tool {
            output_schema: Some(schema::input_schema_for::<mcptools_core::atlassian::jira::SearchOutput>()),
            name: "jira_search".to_string(),
            description: "Search Jira issues using JQL (Jira Query Language) or a saved query. Returns a list of issues matching the query with details like key, summary, status, and assignee. Supports token-based pagination using nextPageToken. Requires JIRA_BASE_URL, JIRA_EMAIL, and JIRA_API_TOKEN environment variables (or ATLASSIAN_* as fallback).".to_string(),
            input_schema: schema::input_schema_for::<atlassian_args::JiraSearchArgs>()
        },
        Tool {
            output_schema: Some(schema::input_schema_for::<mcptools_core::atlassian::confluence::SearchOutput>()),
            name: "confluence_search".to_string(),
            description: "Search Confluence pages using CQL (Confluence Query Language). Returns a list of pages matching the query with title, type, URL, and optionally the plain text content. Requires CONFLUENCE_BASE_URL, CONFLUENCE_EMAIL, and CONFLUENCE_API_TOKEN environment variables (or ATLASSIAN_* as fallback).".to_string(),
            input_schema: schema::input_schema_for::<atlassian_args::ConfluenceSearchArgs>()
        },
        Tool {
            output_schema: Some(schema::input_schema_for::<mcptools_core::hn::PostOutput>()),
            name: "hn_read_item".to_string(),
            description: "Read a HackerNews post and its comments. Accepts HackerNews item ID (e.g., '8863') or full URL (e.g., 'https://news.ycombinator.com/item?id=8863'). Returns post details with paginated comments.".to_string(),
            input_schema: schema::input_schema_for::<hn::HnReadItemArgs>()
        },
        Tool {
            output_schema: Some(schema::input_schema_for::<mcptools_core::hn::ListOutput>()),
            name: "hn_list_items".to_string(),
            description: "List HackerNews stories with pagination. Supports different story types: top, new, best, ask, show, job. Returns a paginated list of stories with their details.".to_string(),
            input_schema: schema::input_schema_for::<hn::HnListItemsArgs>()
        },
        Tool {
            output_schema: Some(schema::input_schema_for::<mcptools_core::md::FetchOutput>()),
            name: "md_fetch".to_string(),
            description: "Fetch a web page using headless Chrome, wait for all XHR requests to complete (network idle), and convert the HTML to Markdown. Supports CSS selector filtering to extract specific page elements. Returns the page title, markdown content, selector metadata, and fetch statistics.".to_string(),
            input_schema: schema::input_schema_for::<md::MdFetchArgs>()
        },
        Tool {
            output_schema: Some(schema::input_schema_for::<crate::md::toc::TocOutput>()),
            name: "md_toc".to_string(),
            description: "Extract table of contents from a web page by parsing markdown headings (H1-H6). Fetches the page using headless Chrome, converts to markdown, and extracts all heading levels with their character offsets and limits. Each TOC entry includes char_offset and char_limit values that can be used with md_fetch to extract specific sections. Sections are defined as heading + content until the next same-or-higher-level heading. Supports CSS selector filtering to extract TOC from specific page elements.".to_string(),
            input_schema: schema::input_schema_for::<md::MdTocArgs>()
        },
        Tool {
            output_schema: Some(schema::input_schema_for::<mcptools_core::atlassian::jira::TicketOutput>()),
            name: "jira_create".to_string(),
            description: "Create a new Jira ticket with required summary. Supports optional fields like description, issue type, priority, assignee, and sprint assignment. Returns the created ticket key. Requires JIRA_BASE_URL, JIRA_EMAIL, and JIRA_API_TOKEN environment variables (or ATLASSIAN_* as fallback).".to_string(),
            input_schema: schema::input_schema_for::<atlassian_args::JiraCreateArgs>()
        },
        Tool {
            output_schema: Some(schema::input_schema_for::<mcptools_core::atlassian::jira::TicketOutput>()),
            name: "jira_get".to_string(),
            description: "Get detailed information about a Jira ticket. Returns comprehensive information about a specific issue using its issue key. Requires JIRA_BASE_URL, JIRA_EMAIL, and JIRA_API_TOKEN environment variables (or ATLASSIAN_* as fallback).".to_string(),
            input_schema: schema::input_schema_for::<atlassian_args::JiraGetArgs>()
        },
        Tool {
            output_schema: Some(schema::input_schema_for::<mcptools_core::atlassian::jira::UpdateOutput>()),
            name: "jira_update".to_string(),
            description: "Update Jira ticket fields. Supports updating Status, Priority, Type, Assignee, Description (markdown), and Sprint assignment. Can update multiple fields in a single call. Handles status transitions automatically and supports assignee lookup by email, display name, or account ID. Requires JIRA_BASE_URL, JIRA_EMAIL, and JIRA_API_TOKEN environment variables (or ATLASSIAN_* as fallback).".to_string(),
            input_schema: schema::input_schema_for::<atlassian_args::JiraUpdateArgs>()
        },
        Tool {
            output_schema: Some(schema::input_schema_for::<mcptools_core::atlassian::jira::CommentOutput>()),
            name: "jira_comment_add".to_string(),
            description: "Post a comment on a Jira ticket. Supports markdown in the comment body (bold, italic, headings, lists, code blocks, links) which is automatically converted to Atlassian Document Format. Requires JIRA_BASE_URL, JIRA_EMAIL, and JIRA_API_TOKEN environment variables (or ATLASSIAN_* as fallback).".to_string(),
            input_schema: schema::input_schema_for::<atlassian_args::JiraCommentAddArgs>()
        },
        Tool {
            output_schema: Some(schema::input_schema_for::<mcptools_core::atlassian::jira::CommentListOutput>()),
            name: "jira_comment_list".to_string(),
            description: "List all comments on a Jira ticket. Returns comment details including ID, author, body text, and creation date. Requires JIRA_BASE_URL, JIRA_EMAIL, and JIRA_API_TOKEN environment variables (or ATLASSIAN_* as fallback).".to_string(),
            input_schema: schema::input_schema_for::<atlassian_args::JiraCommentListArgs>()
        },
        Tool {
            output_schema: Some(schema::input_schema_for::<mcptools_core::atlassian::jira::CommentOutput>()),
            name: "jira_comment_update".to_string(),
            description: "Update an existing comment on a Jira ticket. Supports markdown in the comment body. Use jira_comment_list first to get comment IDs. Requires JIRA_BASE_URL, JIRA_EMAIL, and JIRA_API_TOKEN environment variables (or ATLASSIAN_* as fallback).".to_string(),
            input_schema: schema::input_schema_for::<atlassian_args::JiraCommentUpdateArgs>()
        },
        Tool {
            output_schema: Some(schema::input_schema_for::<mcptools_core::atlassian::jira::CommentDeleteOutput>()),
            name: "jira_comment_delete".to_string(),
            description: "Delete a comment from a Jira ticket by comment ID. Use jira_comment_list first to get comment IDs. Requires JIRA_BASE_URL, JIRA_EMAIL, and JIRA_API_TOKEN environment variables (or ATLASSIAN_* as fallback).".to_string(),
            input_schema: schema::input_schema_for::<atlassian_args::JiraCommentDeleteArgs>()
        },
        Tool {
            output_schema: Some(schema::input_schema_for::<mcptools_core::atlassian::jira::SprintListOutput>()),
            name: "jira_sprint_list".to_string(),
            description: "List sprints for a Jira board. Returns sprint metadata including ID, name, state, and dates. Use this to discover sprint IDs and names before assigning issues to sprints via jira_update or jira_create. Requires JIRA_BASE_URL, JIRA_EMAIL, and JIRA_API_TOKEN environment variables (or ATLASSIAN_* as fallback).".to_string(),
            input_schema: schema::input_schema_for::<atlassian_args::JiraSprintListArgs>()
        },
        Tool {
            output_schema: Some(schema::input_schema_for::<mcptools_core::atlassian::jira::AttachmentListOutput>()),
            name: "jira_attachment_list".to_string(),
            description: "List all attachments on a Jira ticket. Returns attachment metadata including ID, filename, size, MIME type, and creation date. Requires JIRA_BASE_URL, JIRA_EMAIL, and JIRA_API_TOKEN environment variables (or ATLASSIAN_* as fallback).".to_string(),
            input_schema: schema::input_schema_for::<atlassian_args::JiraAttachmentListArgs>()
        },
        Tool {
            output_schema: Some(schema::input_schema_for::<mcptools_core::atlassian::jira::AttachmentDownloadOutput>()),
            name: "jira_attachment_download".to_string(),
            description: "Download a specific attachment from a Jira ticket by attachment ID. Use jira_attachment_list first to get attachment IDs. Saves to a temp file by default, or to a specified output path. Returns the saved file path.".to_string(),
            input_schema: schema::input_schema_for::<atlassian_args::JiraAttachmentDownloadArgs>()
        },
        Tool {
            output_schema: Some(schema::input_schema_for::<mcptools_core::atlassian::jira::AttachmentListOutput>()),
            name: "jira_attachment_upload".to_string(),
            description: "Upload one or more files as attachments to a Jira ticket. Accepts an array of local file paths. Requires JIRA_BASE_URL, JIRA_EMAIL, and JIRA_API_TOKEN environment variables (or ATLASSIAN_* as fallback).".to_string(),
            input_schema: schema::input_schema_for::<atlassian_args::JiraAttachmentUploadArgs>()
        },
        Tool {
            output_schema: Some(schema::input_schema_for::<mcptools_core::atlassian::jira::QueryListOutput>()),
            name: "jira_query_list".to_string(),
            description: "List all saved Jira queries. Returns a list of query names stored in ~/.config/mcptools/queries/".to_string(),
            input_schema: schema::input_schema_for::<atlassian_args::JiraQueryListArgs>()
        },
        Tool {
            output_schema: Some(schema::input_schema_for::<mcptools_core::atlassian::jira::QueryStatusOutput>()),
            name: "jira_query_save".to_string(),
            description: "Save a Jira JQL query with a name for later reuse. Queries are stored in ~/.config/mcptools/queries/ as .jql files.".to_string(),
            input_schema: schema::input_schema_for::<atlassian_args::JiraQuerySaveArgs>()
        },
        Tool {
            output_schema: Some(schema::input_schema_for::<mcptools_core::atlassian::jira::QueryStatusOutput>()),
            name: "jira_query_delete".to_string(),
            description: "Delete a saved Jira query by name. Removes the query from ~/.config/mcptools/queries/".to_string(),
            input_schema: schema::input_schema_for::<atlassian_args::JiraQueryDeleteArgs>()
        },
        Tool {
            output_schema: Some(schema::input_schema_for::<mcptools_core::atlassian::jira::QueryLoadOutput>()),
            name: "jira_query_load".to_string(),
            description: "Load and display the contents of a saved Jira query. Returns the query name and the JQL query text.".to_string(),
            input_schema: schema::input_schema_for::<atlassian_args::JiraQueryLoadArgs>()
        },
        Tool {
            output_schema: Some(schema::input_schema_for::<mcptools_core::atlassian::bitbucket::PRListOutput>()),
            name: "bitbucket_pr_list".to_string(),
            description: "List pull requests for a Bitbucket repository. Returns PR details including ID, title, author, state, and branches. Supports filtering by state and pagination. Requires BITBUCKET_USERNAME and BITBUCKET_APP_PASSWORD environment variables.".to_string(),
            input_schema: schema::input_schema_for::<atlassian_args::BitbucketPrListArgs>()
        },
        Tool {
            output_schema: Some(schema::input_schema_for::<mcptools_core::atlassian::bitbucket::PROutput>()),
            name: "bitbucket_pr_read".to_string(),
            description: "Read details of a specific Bitbucket pull request including diff, diffstat, and comments. Use lineLimit to control diff output size (default: 500 lines, use -1 for unlimited). Requires BITBUCKET_USERNAME and BITBUCKET_APP_PASSWORD environment variables.".to_string(),
            input_schema: schema::input_schema_for::<atlassian_args::BitbucketPrReadArgs>()
        },
        Tool {
            output_schema: Some(schema::input_schema_for::<mcptools_core::atlassian::bitbucket::PRCreateOutput>()),
            name: "bitbucket_pr_create".to_string(),
            description: "Create a new pull request in a Bitbucket repository. Requires repo, title, and source branch. Optionally specify destination branch (defaults to repo's main branch), description, and whether to close the source branch after merge. Requires BITBUCKET_USERNAME and BITBUCKET_APP_PASSWORD environment variables.".to_string(),
            input_schema: schema::input_schema_for::<atlassian_args::BitbucketPrCreateArgs>()
        },
        Tool {
            output_schema: Some(schema::input_schema_for::<mcptools_core::atlassian::bitbucket::WorkspaceListOutput>()),
            name: "bitbucket_workspace_list".to_string(),
            description: "List Bitbucket workspaces accessible to the authenticated user. Returns workspace slugs and names. Supports pagination. Requires BITBUCKET_USERNAME and BITBUCKET_APP_PASSWORD environment variables.".to_string(),
            input_schema: schema::input_schema_for::<atlassian_args::BitbucketWorkspaceListArgs>()
        },
        Tool {
            output_schema: Some(schema::input_schema_for::<mcptools_core::atlassian::bitbucket::RepoListOutput>()),
            name: "bitbucket_repo_list".to_string(),
            description: "List repositories in a Bitbucket workspace. Returns repository names, full names, and clone URLs (SSH and HTTPS). Supports pagination. Requires BITBUCKET_USERNAME and BITBUCKET_APP_PASSWORD environment variables.".to_string(),
            input_schema: schema::input_schema_for::<atlassian_args::BitbucketRepoListArgs>()
        },
        Tool {
            output_schema: Some(schema::input_schema_for::<mcptools_core::atlassian::bitbucket::BranchListOutput>()),
            name: "bitbucket_repo_branches".to_string(),
            description: "List branches in a Bitbucket repository. Returns branch names, latest commit hash, date, message, and author. Supports filtering and sorting. Requires BITBUCKET_USERNAME and BITBUCKET_APP_PASSWORD environment variables.".to_string(),
            input_schema: schema::input_schema_for::<atlassian_args::BitbucketRepoBranchesArgs>()
        },
        Tool {
            output_schema: Some(schema::input_schema_for::<mcptools_core::annotations::ListAnnotationsResponse>()),
            name: "ui_annotations_list".to_string(),
            description: "List all UI annotations from the calendsync dev server. Returns selector, component name, note, and resolution status for each annotation.".to_string(),
            input_schema: schema::input_schema_for::<annotations::AnnotationsListArgs>()
        },
        Tool {
            output_schema: Some(schema::input_schema_for::<mcptools_core::annotations::DevAnnotation>()),
            name: "ui_annotations_get".to_string(),
            description: "Get a single UI annotation by ID with full details including computed styles, bounding box, and optional screenshot.".to_string(),
            input_schema: schema::input_schema_for::<annotations::AnnotationsGetArgs>()
        },
        Tool {
            output_schema: Some(schema::input_schema_for::<annotations::AnnotationResolveOutput>()),
            name: "ui_annotations_resolve".to_string(),
            description: "Mark a UI annotation as resolved with a summary of the changes made.".to_string(),
            input_schema: schema::input_schema_for::<annotations::AnnotationsResolveArgs>()
        },
        Tool {
            output_schema: Some(schema::input_schema_for::<annotations::AnnotationClearOutput>()),
            name: "ui_annotations_clear".to_string(),
            description: "Clear all UI annotations from the dev server.".to_string(),
            input_schema: schema::input_schema_for::<annotations::AnnotationsClearArgs>()
        },
        Tool {
            output_schema: Some(schema::input_schema_for::<::pdf::DocumentTree>()),
            name: "pdf_toc".to_string(),
            description: "Parse a PDF file and return its document tree (table of contents) with section IDs, headings, content previews, and image counts. Use the section IDs with pdf_read to read specific sections.".to_string(),
            input_schema: schema::input_schema_for::<pdf::PdfTocArgs>()
        },
        Tool {
            output_schema: Some(schema::input_schema_for::<::pdf::SectionContent>()),
            name: "pdf_read".to_string(),
            description: "Read a section of a PDF document as Markdown, or the entire document if no section specified. Returns the section title, rendered Markdown text, and image references.".to_string(),
            input_schema: schema::input_schema_for::<pdf::PdfReadArgs>()
        },
        Tool {
            output_schema: Some(schema::input_schema_for::<::pdf::PeekContent>()),
            name: "pdf_peek".to_string(),
            description: "Sample a text snippet from a PDF section at a given position (beginning, middle, ending, random) without reading the full content. Returns the snippet with total character count so you know how much content remains. Defaults to the whole document if no section specified.".to_string(),
            input_schema: schema::input_schema_for::<pdf::PdfPeekArgs>()
        },
        Tool {
            output_schema: Some(schema::input_schema_for::<pdf::PdfImagesOutput>()),
            name: "pdf_images".to_string(),
            description: "List all images in a PDF section or the whole document. Returns image IDs, formats, section IDs, section titles, and page numbers. Use with pdf_image to extract specific images. NOTE: PDFs often reuse decorative images (logos, backgrounds, headers) across many pages — the same image ID will appear on multiple pages. To find meaningful content images (screenshots, diagrams, photos), filter out IDs that repeat across many pages and focus on IDs that appear only within the target section.".to_string(),
            input_schema: schema::input_schema_for::<pdf::PdfImagesArgs>()
        },
        Tool {
            output_schema: Some(schema::input_schema_for::<pdf::PdfImageOutput>()),
            name: "pdf_image".to_string(),
            description: "Extract a specific image from a PDF document by ID, or pick a random image. Returns the image as base64-encoded data with format information. Optionally scope to a section.".to_string(),
            input_schema: schema::input_schema_for::<pdf::PdfImageArgs>()
        },
        Tool {
            output_schema: Some(schema::input_schema_for::<::pdf::DocumentMetadata>()),
            name: "pdf_info".to_string(),
            description: "Get metadata about a PDF document including title, author, page count, and creator.".to_string(),
            input_schema: schema::input_schema_for::<pdf::PdfInfoArgs>()
        },
        Tool {
            output_schema: Some(schema::input_schema_for::<crate::images::SavedOutput>()),
            name: "images_generate".to_string(),
            description: "Generate images with ChatGPT Images 2.5 (gpt-image-2.5-flare default, gpt-image-2.5-sunburst for premium precision). Text-to-image via POST /v1/images/generations. Saves PNG/JPEG/WebP files and returns paths plus usage. Defaults to the ChatGPT subscription (llm-stream auth.json); pass api=openai with OPENAI_API_KEY for the metered API.".to_string(),
            input_schema: schema::input_schema_for::<images::ImagesGenerateArgs>()
        },
        Tool {
            output_schema: Some(schema::input_schema_for::<crate::images::SavedOutput>()),
            name: "images_edit".to_string(),
            description: "Edit ChatGPT Images 2.5 images with a prompt plus 1-16 reference images and optional mask via POST /v1/images/edits. Preserves subject/composition outside the edit. Saves files and returns paths plus usage. Defaults to the ChatGPT subscription (llm-stream auth.json); pass api=openai with OPENAI_API_KEY for the metered API.".to_string(),
            input_schema: schema::input_schema_for::<images::ImagesEditArgs>()
        },
        Tool {
            output_schema: Some(schema::input_schema_for::<crate::images::SavedOutput>()),
            name: "images_vary".to_string(),
            description: "Create variations of ChatGPT Images 2.5 images anchored to 1-16 reference images. Same as images_edit with a default variation prompt when prompt is omitted. Defaults to the ChatGPT subscription (llm-stream auth.json); pass api=openai with OPENAI_API_KEY for the metered API.".to_string(),
            input_schema: schema::input_schema_for::<images::ImagesVaryArgs>()
        },
        Tool {
            output_schema: Some(schema::input_schema_for::<atlas::AtlasTextOutput>()),
            name: "atlas_tree_view".to_string(),
            description: "Browse an annotated directory tree of the codebase. Each entry includes a short description of what the file or directory contains. Use this to navigate unfamiliar codebases — start at the root, then drill into directories of interest.".to_string(),
            input_schema: schema::input_schema_for::<atlas::AtlasTreeViewArgs>()
        },
        Tool {
            output_schema: Some(schema::input_schema_for::<atlas::AtlasTextOutput>()),
            name: "atlas_peek".to_string(),
            description: "Get a detailed summary of a file or directory. For files: long description, extracted symbols with signatures. For directories: long description, children with descriptions, aggregated symbols. Use this after tree_view to understand a specific file before reading it.".to_string(),
            input_schema: schema::input_schema_for::<atlas::AtlasPeekArgs>()
        },
        Tool {
            output_schema: Some(schema::input_schema_for::<atlas::AtlasTextOutput>()),
            name: "atlas_status".to_string(),
            description: "Check the health of the Atlas codebase index. Shows when it was last updated, how many files are tracked, and whether descriptions are available.".to_string(),
            input_schema: schema::input_schema_for::<atlas::AtlasStatusArgs>()
        },
        Tool {
            output_schema: Some(schema::input_schema_for::<mcptools_core::linear::Viewer>()),
            name: "linear_auth_status".to_string(),
            description: "Show the Linear viewer identity for LINEAR_API_KEY. Returns id, name, and email. Requires LINEAR_API_KEY environment variable.".to_string(),
            input_schema: schema::input_schema_for::<crate::linear::args::AuthStatusArgs>(),
        },
        Tool {
            output_schema: Some(schema::input_schema_for::<
                mcptools_core::linear::IssueGetOutput,
            >()),
            name: "linear_issue_get".to_string(),
            description: "Get one Linear issue by id or identifier (e.g. GUZ-85). Returns id, identifier, title, URL, state, parent, and blocked-by relations. Requires LINEAR_API_KEY environment variable.".to_string(),
            input_schema: schema::input_schema_for::<crate::linear::args::IssueGetArgs>(),
        },
        Tool {
            output_schema: Some(schema::input_schema_for::<mcptools_core::linear::IssueListOutput>()),
            name: "linear_issue_list".to_string(),
            description: "List Linear issues with filters. Returns nodes with id, identifier, title, state, parent, blocked-by plus pageInfo. Requires LINEAR_API_KEY environment variable.".to_string(),
            input_schema: schema::input_schema_for::<crate::linear::args::IssueListArgs>(),
        },
        Tool {
            output_schema: Some(schema::input_schema_for::<mcptools_core::linear::CommentListOutput>()),
            name: "linear_comment_list".to_string(),
            description: "List comments on one Linear issue. Returns nodes with id, author, body, createdAt plus pageInfo. Requires LINEAR_API_KEY environment variable.".to_string(),
            input_schema: schema::input_schema_for::<crate::linear::args::CommentListArgs>(),
        },
        Tool {
            output_schema: Some(schema::input_schema_for::<mcptools_core::linear::RelationListOutput>()),
            name: "linear_relation_list".to_string(),
            description: "List relations on one Linear issue. Returns nodes with id, type, issue, related issue, direction plus pageInfo. Requires LINEAR_API_KEY environment variable.".to_string(),
            input_schema: schema::input_schema_for::<crate::linear::args::RelationListArgs>(),
        },
        Tool {
            output_schema: Some(schema::input_schema_for::<mcptools_core::linear::TeamListOutput>()),
            name: "linear_team_list".to_string(),
            description: "List Linear teams. Returns nodes with id, key, name plus pageInfo. Requires LINEAR_API_KEY environment variable.".to_string(),
            input_schema: schema::input_schema_for::<crate::linear::args::TeamListArgs>(),
        },
        Tool {
            output_schema: Some(schema::input_schema_for::<mcptools_core::linear::Team>()),
            name: "linear_team_get".to_string(),
            description: "Get one Linear team by id, key, or name. Returns id, key, name. Requires LINEAR_API_KEY environment variable.".to_string(),
            input_schema: schema::input_schema_for::<crate::linear::args::TeamGetArgs>(),
        },
        Tool {
            output_schema: Some(schema::input_schema_for::<mcptools_core::linear::ProjectListOutput>()),
            name: "linear_project_list".to_string(),
            description: "List Linear projects in a team. Returns nodes with id, name plus pageInfo. Requires LINEAR_API_KEY environment variable.".to_string(),
            input_schema: schema::input_schema_for::<crate::linear::args::ProjectListArgs>(),
        },
        Tool {
            output_schema: Some(schema::input_schema_for::<mcptools_core::linear::Project>()),
            name: "linear_project_get".to_string(),
            description: "Get one Linear project by id or name within a team. Returns id, name. Requires LINEAR_API_KEY environment variable.".to_string(),
            input_schema: schema::input_schema_for::<crate::linear::args::ProjectGetArgs>(),
        },
        Tool {
            output_schema: Some(schema::input_schema_for::<mcptools_core::linear::UserListOutput>()),
            name: "linear_user_list".to_string(),
            description: "List Linear users matching a name query. Returns nodes with id, name, email plus pageInfo. Requires LINEAR_API_KEY environment variable.".to_string(),
            input_schema: schema::input_schema_for::<crate::linear::args::UserListArgs>(),
        },
        Tool {
            output_schema: Some(schema::input_schema_for::<mcptools_core::linear::StateListOutput>()),
            name: "linear_state_list".to_string(),
            description: "List workflow states in a Linear team. Returns nodes with id, name, type plus pageInfo. Requires LINEAR_API_KEY environment variable.".to_string(),
            input_schema: schema::input_schema_for::<crate::linear::args::StateListArgs>(),
        },
        Tool {
            output_schema: Some(schema::input_schema_for::<mcptools_core::linear::LabelListOutput>()),
            name: "linear_label_list".to_string(),
            description: "List labels in a Linear team. Returns nodes with id, name plus pageInfo. Requires LINEAR_API_KEY environment variable.".to_string(),
            input_schema: schema::input_schema_for::<crate::linear::args::LabelListArgs>(),
        },
        Tool {
            output_schema: Some(schema::input_schema_for::<mcptools_core::linear::CycleListOutput>()),
            name: "linear_cycle_list".to_string(),
            description: "List cycles in a Linear team. Returns nodes with id, number, name plus pageInfo. Requires LINEAR_API_KEY environment variable.".to_string(),
            input_schema: schema::input_schema_for::<crate::linear::args::CycleListArgs>(),
        },
        Tool {
            output_schema: Some(schema::input_schema_for::<mcptools_core::linear::IssueMini>()),
            name: "linear_issue_create".to_string(),
            description: "Create a Linear issue in a team. Returns id, identifier, title, URL, state, parent. Requires LINEAR_API_KEY environment variable.".to_string(),
            input_schema: schema::input_schema_for::<crate::linear::args::IssueCreateArgs>(),
        },
        Tool {
            output_schema: Some(schema::input_schema_for::<mcptools_core::linear::IssueMini>()),
            name: "linear_issue_update".to_string(),
            description: "Update a Linear issue by id or identifier. Needs at least one of title, description, state, assignee, parent, clearParent. Returns the updated issue. Requires LINEAR_API_KEY environment variable.".to_string(),
            input_schema: schema::input_schema_for::<crate::linear::args::IssueUpdateArgs>(),
        },
        Tool {
            output_schema: Some(schema::input_schema_for::<mcptools_core::linear::Comment>()),
            name: "linear_comment_create".to_string(),
            description: "Create a comment on a Linear issue. Returns id, body, URL, author, createdAt. Requires LINEAR_API_KEY environment variable.".to_string(),
            input_schema: schema::input_schema_for::<crate::linear::args::CommentCreateArgs>(),
        },
        Tool {
            output_schema: Some(schema::input_schema_for::<mcptools_core::linear::RelationAddOutput>()),
            name: "linear_relation_add".to_string(),
            description: "Add a relation between two Linear issues. Type is 'blocks' or 'related'. Returns status created or already_exists plus the relation. Requires LINEAR_API_KEY environment variable.".to_string(),
            input_schema: schema::input_schema_for::<crate::linear::args::RelationAddArgs>(),
        },
        Tool {
            output_schema: Some(schema::input_schema_for::<mcptools_core::linear::RelationRemoveOutput>()),
            name: "linear_relation_remove".to_string(),
            description: "Remove a relation between two Linear issues matched by source, related, type triple. Returns the deleted relation id. Requires LINEAR_API_KEY environment variable.".to_string(),
            input_schema: schema::input_schema_for::<crate::linear::args::RelationRemoveArgs>(),
        },
    ];

    let result = ToolsList { tools };

    serde_json::to_value(result).map_err(|e| JsonRpcError {
        code: -32603,
        message: format!("Internal error: {e}"),
        data: None,
    })
}

pub async fn handle_tools_call(
    params: Option<serde_json::Value>,
    global: &crate::Global,
) -> Result<serde_json::Value, JsonRpcError> {
    let params: CallToolParams = serde_json::from_value(params.unwrap_or(serde_json::Value::Null))
        .map_err(|e| JsonRpcError {
            code: -32602,
            message: format!("Invalid params: {e}"),
            data: None,
        })?;

    match params.name.as_str() {
        "jira_search" => atlassian::handle_jira_search(params.arguments, global).await,
        "jira_create" => atlassian::handle_jira_create(params.arguments, global).await,
        "jira_get" => atlassian::handle_jira_get(params.arguments, global).await,
        "jira_update" => atlassian::handle_jira_update(params.arguments, global).await,
        "jira_comment_add" => atlassian::handle_jira_comment_add(params.arguments, global).await,
        "jira_comment_list" => atlassian::handle_jira_comment_list(params.arguments, global).await,
        "jira_comment_update" => {
            atlassian::handle_jira_comment_update(params.arguments, global).await
        }
        "jira_comment_delete" => {
            atlassian::handle_jira_comment_delete(params.arguments, global).await
        }
        "jira_sprint_list" => atlassian::handle_jira_sprint_list(params.arguments, global).await,
        "jira_attachment_list" => {
            atlassian::handle_jira_attachment_list(params.arguments, global).await
        }
        "jira_attachment_download" => {
            atlassian::handle_jira_attachment_download(params.arguments, global).await
        }
        "jira_attachment_upload" => {
            atlassian::handle_jira_attachment_upload(params.arguments, global).await
        }
        "jira_query_list" => atlassian::handle_jira_query_list(params.arguments, global).await,
        "jira_query_save" => atlassian::handle_jira_query_save(params.arguments, global).await,
        "jira_query_delete" => atlassian::handle_jira_query_delete(params.arguments, global).await,
        "jira_query_load" => atlassian::handle_jira_query_load(params.arguments, global).await,
        "confluence_search" => atlassian::handle_confluence_search(params.arguments, global).await,
        "bitbucket_pr_list" => atlassian::handle_bitbucket_pr_list(params.arguments, global).await,
        "bitbucket_pr_read" => atlassian::handle_bitbucket_pr_read(params.arguments, global).await,
        "bitbucket_pr_create" => {
            atlassian::handle_bitbucket_pr_create(params.arguments, global).await
        }
        "bitbucket_workspace_list" => {
            atlassian::handle_bitbucket_workspace_list(params.arguments, global).await
        }
        "bitbucket_repo_list" => {
            atlassian::handle_bitbucket_repo_list(params.arguments, global).await
        }
        "bitbucket_repo_branches" => {
            atlassian::handle_bitbucket_repo_branches(params.arguments, global).await
        }
        "hn_read_item" => hn::handle_hn_read_item(params.arguments, global).await,
        "hn_list_items" => hn::handle_hn_list_items(params.arguments, global).await,
        "md_fetch" => md::handle_md_fetch(params.arguments, global).await,
        "md_toc" => md::handle_md_toc(params.arguments, global).await,
        "images_generate" => images::handle_images_generate(params.arguments, global).await,
        "images_edit" => images::handle_images_edit(params.arguments, global).await,
        "images_vary" => images::handle_images_vary(params.arguments, global).await,
        "ui_annotations_list" => {
            annotations::handle_ui_annotations_list(params.arguments, global).await
        }
        "ui_annotations_get" => {
            annotations::handle_ui_annotations_get(params.arguments, global).await
        }
        "ui_annotations_resolve" => {
            annotations::handle_ui_annotations_resolve(params.arguments, global).await
        }
        "ui_annotations_clear" => {
            annotations::handle_ui_annotations_clear(params.arguments, global).await
        }
        "pdf_toc" => pdf::handle_pdf_toc(params.arguments, global).await,
        "pdf_read" => pdf::handle_pdf_read(params.arguments, global).await,
        "pdf_peek" => pdf::handle_pdf_peek(params.arguments, global).await,
        "pdf_images" => pdf::handle_pdf_images(params.arguments, global).await,
        "pdf_image" => pdf::handle_pdf_image(params.arguments, global).await,
        "pdf_info" => pdf::handle_pdf_info(params.arguments, global).await,
        "atlas_tree_view" => atlas::handle_atlas_tree_view(params.arguments, global).await,
        "atlas_peek" => atlas::handle_atlas_peek(params.arguments, global).await,
        "atlas_status" => atlas::handle_atlas_status(params.arguments, global).await,
        "linear_auth_status" => linear::handle_linear_auth_status(params.arguments, global).await,
        "linear_issue_get" => linear::handle_linear_issue_get(params.arguments, global).await,
        "linear_issue_list" => linear::handle_linear_issue_list(params.arguments, global).await,
        "linear_comment_list" => linear::handle_linear_comment_list(params.arguments, global).await,
        "linear_relation_list" => {
            linear::handle_linear_relation_list(params.arguments, global).await
        }
        "linear_team_list" => linear::handle_linear_team_list(params.arguments, global).await,
        "linear_team_get" => linear::handle_linear_team_get(params.arguments, global).await,
        "linear_project_list" => linear::handle_linear_project_list(params.arguments, global).await,
        "linear_project_get" => linear::handle_linear_project_get(params.arguments, global).await,
        "linear_user_list" => linear::handle_linear_user_list(params.arguments, global).await,
        "linear_state_list" => linear::handle_linear_state_list(params.arguments, global).await,
        "linear_label_list" => linear::handle_linear_label_list(params.arguments, global).await,
        "linear_cycle_list" => linear::handle_linear_cycle_list(params.arguments, global).await,
        "linear_issue_create" => linear::handle_linear_issue_create(params.arguments, global).await,
        "linear_issue_update" => linear::handle_linear_issue_update(params.arguments, global).await,
        "linear_comment_create" => {
            linear::handle_linear_comment_create(params.arguments, global).await
        }
        "linear_relation_add" => linear::handle_linear_relation_add(params.arguments, global).await,
        "linear_relation_remove" => {
            linear::handle_linear_relation_remove(params.arguments, global).await
        }
        _ => Err(JsonRpcError {
            code: -32602,
            message: format!("Unknown tool: {}", params.name),
            data: None,
        }),
    }
}

#[cfg(test)]
mod dual_tests {
    use super::*;

    #[test]
    fn dual_result_carries_text_and_structured() {
        let value = to_dual_result(serde_json::json!({"identifier": "GUZ-85"})).unwrap();
        assert_eq!(
            value
                .get("structuredContent")
                .and_then(|v| v.get("identifier")),
            Some(&serde_json::json!("GUZ-85"))
        );
        let text = value
            .get("content")
            .and_then(|v| v.get(0))
            .and_then(|v| v.get("text"))
            .and_then(|v| v.as_str())
            .unwrap();
        assert!(text.contains("GUZ-85"));
        assert!(value.get("isError").is_none());
    }
}
