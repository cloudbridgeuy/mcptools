mod annotations;
mod atlas;
mod atlassian;
mod execute;
mod find_tools;
mod hn;
mod images;
mod linear;
mod md;
mod pdf;
pub mod schema;

#[cfg(test)]
mod conformance_tests;

use serde::{Deserialize, Serialize};

// Re-export types needed by tool handlers
pub use super::{JsonRpcError, ServeFlags, Tool, ToolKind};
pub use find_tools::find_tools;

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

#[derive(Debug, PartialEq, Eq, Serialize)]
pub struct ToolAnnotations {
    #[serde(rename = "readOnlyHint")]
    pub read_only: bool,
    #[serde(rename = "destructiveHint")]
    pub destructive: bool,
}

pub fn annotations_for(kind: ToolKind) -> ToolAnnotations {
    match kind {
        ToolKind::Read => ToolAnnotations {
            read_only: true,
            destructive: false,
        },
        ToolKind::Write | ToolKind::Spend => ToolAnnotations {
            read_only: false,
            destructive: true,
        },
    }
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

pub fn to_dual_result_projected(
    value: impl Serialize,
    fields: Option<&[String]>,
) -> Result<serde_json::Value, JsonRpcError> {
    let Some(fields) = fields.filter(|fields| !fields.is_empty()) else {
        return to_dual_result(value);
    };
    let structured = serde_json::to_value(&value).map_err(|e| JsonRpcError {
        code: -32603,
        message: format!("Serialization error: {e}"),
        data: None,
    })?;
    let projected =
        mcptools_core::projection::project(structured, fields).map_err(|e| JsonRpcError {
            code: -32602,
            message: format!("Unknown field path: \"{}\"", e.path),
            data: None,
        })?;
    to_dual_result(projected)
}

pub fn handle_initialize() -> Result<serde_json::Value, JsonRpcError> {
    let result = InitializeResult {
        protocol_version: "2025-06-18".to_string(),
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

pub fn registered_tools() -> Vec<Tool> {
    vec![
        Tool {
            output_schema: schema::projected_output_schema_for::<mcptools_core::atlassian::jira::SearchOutput>(),
            name: "jira_search".to_string(),
            description: "Search Jira issues using JQL (Jira Query Language) or a saved query. Returns a list of issues matching the query with details like key, summary, status, and assignee. Supports token-based pagination using nextPageToken. Requires JIRA_BASE_URL, JIRA_EMAIL, and JIRA_API_TOKEN environment variables (or ATLASSIAN_* as fallback). Optional fields (array of dotted paths, e.g. [\"issues.key\",\"total\"]) returns only the named fields; omit it for the full output. Keep nextPageToken if you page further.".to_string(),
            input_schema: schema::input_schema_for::<atlassian::JiraSearchArgs>(),
            summary: "Search Jira issues with JQL or a saved query",
            kind: ToolKind::Read,
        },
        Tool {
            output_schema: schema::output_schema_for::<mcptools_core::atlassian::confluence::SearchOutput>(),
            name: "confluence_search".to_string(),
            description: "Search Confluence pages using CQL (Confluence Query Language). Returns a list of pages matching the query with title, type, URL, and optionally the plain text content. Requires CONFLUENCE_BASE_URL, CONFLUENCE_EMAIL, and CONFLUENCE_API_TOKEN environment variables (or ATLASSIAN_* as fallback).".to_string(),
            input_schema: schema::input_schema_for::<atlassian::ConfluenceSearchArgs>(),
            summary: "Search Confluence pages with CQL",
            kind: ToolKind::Read,
        },
        Tool {
            output_schema: schema::output_schema_for::<mcptools_core::hn::PostOutput>(),
            name: "hn_read_item".to_string(),
            description: "Read a HackerNews post and its comments. Accepts HackerNews item ID (e.g., '8863') or full URL (e.g., 'https://news.ycombinator.com/item?id=8863'). Returns post details with paginated comments.".to_string(),
            input_schema: schema::input_schema_for::<hn::HnReadItemArgs>(),
            summary: "Read a HackerNews post and its comments",
            kind: ToolKind::Read,
        },
        Tool {
            output_schema: schema::output_schema_for::<mcptools_core::hn::ListOutput>(),
            name: "hn_list_items".to_string(),
            description: "List HackerNews stories with pagination. Supports different story types: top, new, best, ask, show, job. Returns a paginated list of stories with their details.".to_string(),
            input_schema: schema::input_schema_for::<hn::HnListItemsArgs>(),
            summary: "List HackerNews stories: top, new, best, ask, show, job",
            kind: ToolKind::Read,
        },
        Tool {
            output_schema: schema::projected_output_schema_for::<mcptools_core::md::FetchOutput>(),
            name: "md_fetch".to_string(),
            description: "Fetch a web page using headless Chrome, wait for all XHR requests to complete (network idle), and convert the HTML to Markdown. Supports CSS selector filtering to extract specific page elements. Returns the page title, markdown content, selector metadata, and fetch statistics. Optional fields (array of dotted paths, e.g. [\"title\",\"content\"]) returns only the named fields; omit it for the full output. Keep the pagination fields if you page further.".to_string(),
            input_schema: schema::input_schema_for::<md::MdFetchArgs>(),
            summary: "Fetch a web page as Markdown",
            kind: ToolKind::Read,
        },
        Tool {
            output_schema: schema::output_schema_for::<crate::md::toc::TocOutput>(),
            name: "md_toc".to_string(),
            description: "Extract table of contents from a web page by parsing markdown headings (H1-H6). Fetches the page using headless Chrome, converts to markdown, and extracts all heading levels with their character offsets and limits. Each TOC entry includes char_offset and char_limit values that can be used with md_fetch to extract specific sections. Sections are defined as heading + content until the next same-or-higher-level heading. Supports CSS selector filtering to extract TOC from specific page elements.".to_string(),
            input_schema: schema::input_schema_for::<md::MdTocArgs>(),
            summary: "List the headings of a web page as a table of contents",
            kind: ToolKind::Read,
        },
        Tool {
            output_schema: schema::output_schema_for::<mcptools_core::atlassian::jira::TicketOutput>(),
            name: "jira_create".to_string(),
            description: "Create a new Jira ticket with required summary. Supports optional fields like description, issue type, priority, assignee, and sprint assignment. Returns the created ticket key. Requires JIRA_BASE_URL, JIRA_EMAIL, and JIRA_API_TOKEN environment variables (or ATLASSIAN_* as fallback).".to_string(),
            input_schema: schema::input_schema_for::<atlassian::JiraCreateArgs>(),
            summary: "Create a Jira ticket",
            kind: ToolKind::Write,
        },
        Tool {
            output_schema: schema::output_schema_for::<mcptools_core::atlassian::jira::TicketOutput>(),
            name: "jira_get".to_string(),
            description: "Get detailed information about a Jira ticket. Returns comprehensive information about a specific issue using its issue key. Requires JIRA_BASE_URL, JIRA_EMAIL, and JIRA_API_TOKEN environment variables (or ATLASSIAN_* as fallback).".to_string(),
            input_schema: schema::input_schema_for::<atlassian::JiraGetArgs>(),
            summary: "Get one Jira ticket by key",
            kind: ToolKind::Read,
        },
        Tool {
            output_schema: schema::output_schema_for::<mcptools_core::atlassian::jira::UpdateOutput>(),
            name: "jira_update".to_string(),
            description: "Update Jira ticket fields. Supports updating Status, Priority, Type, Assignee, Description (markdown), and Sprint assignment. Can update multiple fields in a single call. Handles status transitions automatically and supports assignee lookup by email, display name, or account ID. Requires JIRA_BASE_URL, JIRA_EMAIL, and JIRA_API_TOKEN environment variables (or ATLASSIAN_* as fallback).".to_string(),
            input_schema: schema::input_schema_for::<atlassian::JiraUpdateArgs>(),
            summary: "Update a Jira ticket: status (close, reopen), priority, assignee, sprint",
            kind: ToolKind::Write,
        },
        Tool {
            output_schema: schema::output_schema_for::<mcptools_core::atlassian::jira::CommentOutput>(),
            name: "jira_comment_add".to_string(),
            description: "Post a comment on a Jira ticket. Supports markdown in the comment body (bold, italic, headings, lists, code blocks, links) which is automatically converted to Atlassian Document Format. Requires JIRA_BASE_URL, JIRA_EMAIL, and JIRA_API_TOKEN environment variables (or ATLASSIAN_* as fallback).".to_string(),
            input_schema: schema::input_schema_for::<atlassian::JiraCommentAddArgs>(),
            summary: "Add a comment to a Jira ticket",
            kind: ToolKind::Write,
        },
        Tool {
            output_schema: schema::output_schema_for::<mcptools_core::atlassian::jira::CommentListOutput>(),
            name: "jira_comment_list".to_string(),
            description: "List all comments on a Jira ticket. Returns comment details including ID, author, body text, and creation date. Requires JIRA_BASE_URL, JIRA_EMAIL, and JIRA_API_TOKEN environment variables (or ATLASSIAN_* as fallback).".to_string(),
            input_schema: schema::input_schema_for::<atlassian::JiraCommentListArgs>(),
            summary: "List comments on a Jira ticket",
            kind: ToolKind::Read,
        },
        Tool {
            output_schema: schema::output_schema_for::<mcptools_core::atlassian::jira::CommentOutput>(),
            name: "jira_comment_update".to_string(),
            description: "Update an existing comment on a Jira ticket. Supports markdown in the comment body. Use jira_comment_list first to get comment IDs. Requires JIRA_BASE_URL, JIRA_EMAIL, and JIRA_API_TOKEN environment variables (or ATLASSIAN_* as fallback).".to_string(),
            input_schema: schema::input_schema_for::<atlassian::JiraCommentUpdateArgs>(),
            summary: "Edit a comment on a Jira ticket",
            kind: ToolKind::Write,
        },
        Tool {
            output_schema: schema::output_schema_for::<mcptools_core::atlassian::jira::CommentDeleteOutput>(),
            name: "jira_comment_delete".to_string(),
            description: "Delete a comment from a Jira ticket by comment ID. Use jira_comment_list first to get comment IDs. Requires JIRA_BASE_URL, JIRA_EMAIL, and JIRA_API_TOKEN environment variables (or ATLASSIAN_* as fallback).".to_string(),
            input_schema: schema::input_schema_for::<atlassian::JiraCommentDeleteArgs>(),
            summary: "Delete a comment from a Jira ticket",
            kind: ToolKind::Write,
        },
        Tool {
            output_schema: schema::output_schema_for::<mcptools_core::atlassian::jira::SprintListOutput>(),
            name: "jira_sprint_list".to_string(),
            description: "List sprints for a Jira board. Returns sprint metadata including ID, name, state, and dates. Use this to discover sprint IDs and names before assigning issues to sprints via jira_update or jira_create. Requires JIRA_BASE_URL, JIRA_EMAIL, and JIRA_API_TOKEN environment variables (or ATLASSIAN_* as fallback).".to_string(),
            input_schema: schema::input_schema_for::<atlassian::JiraSprintListArgs>(),
            summary: "List sprints of a Jira board",
            kind: ToolKind::Read,
        },
        Tool {
            output_schema: schema::output_schema_for::<mcptools_core::atlassian::jira::AttachmentListOutput>(),
            name: "jira_attachment_list".to_string(),
            description: "List all attachments on a Jira ticket. Returns attachment metadata including ID, filename, size, MIME type, and creation date. Requires JIRA_BASE_URL, JIRA_EMAIL, and JIRA_API_TOKEN environment variables (or ATLASSIAN_* as fallback).".to_string(),
            input_schema: schema::input_schema_for::<atlassian::JiraAttachmentListArgs>(),
            summary: "List attachments on a Jira ticket",
            kind: ToolKind::Read,
        },
        Tool {
            output_schema: schema::output_schema_for::<mcptools_core::atlassian::jira::AttachmentDownloadOutput>(),
            name: "jira_attachment_download".to_string(),
            description: "Download a specific attachment from a Jira ticket by attachment ID. Use jira_attachment_list first to get attachment IDs. Saves to a temp file by default, or to a specified output path. Returns the saved file path.".to_string(),
            input_schema: schema::input_schema_for::<atlassian::JiraAttachmentDownloadArgs>(),
            summary: "Download one attachment from a Jira ticket to a file",
            kind: ToolKind::Read,
        },
        Tool {
            output_schema: schema::output_schema_for::<mcptools_core::atlassian::jira::AttachmentListOutput>(),
            name: "jira_attachment_upload".to_string(),
            description: "Upload one or more files as attachments to a Jira ticket. Accepts an array of local file paths. Requires JIRA_BASE_URL, JIRA_EMAIL, and JIRA_API_TOKEN environment variables (or ATLASSIAN_* as fallback).".to_string(),
            input_schema: schema::input_schema_for::<atlassian::JiraAttachmentUploadArgs>(),
            summary: "Upload files as attachments to a Jira ticket",
            kind: ToolKind::Write,
        },
        Tool {
            output_schema: schema::output_schema_for::<mcptools_core::atlassian::jira::QueryListOutput>(),
            name: "jira_query_list".to_string(),
            description: "List all saved Jira queries. Returns a list of query names stored in ~/.config/mcptools/queries/".to_string(),
            input_schema: schema::input_schema_for::<atlassian::JiraQueryListArgs>(),
            summary: "List saved Jira JQL queries",
            kind: ToolKind::Read,
        },
        Tool {
            output_schema: schema::output_schema_for::<mcptools_core::atlassian::jira::QueryStatusOutput>(),
            name: "jira_query_save".to_string(),
            description: "Save a Jira JQL query with a name for later reuse. Queries are stored in ~/.config/mcptools/queries/ as .jql files.".to_string(),
            input_schema: schema::input_schema_for::<atlassian::JiraQuerySaveArgs>(),
            summary: "Save a named Jira JQL query",
            kind: ToolKind::Write,
        },
        Tool {
            output_schema: schema::output_schema_for::<mcptools_core::atlassian::jira::QueryStatusOutput>(),
            name: "jira_query_delete".to_string(),
            description: "Delete a saved Jira query by name. Removes the query from ~/.config/mcptools/queries/".to_string(),
            input_schema: schema::input_schema_for::<atlassian::JiraQueryDeleteArgs>(),
            summary: "Delete a saved Jira JQL query",
            kind: ToolKind::Write,
        },
        Tool {
            output_schema: schema::output_schema_for::<mcptools_core::atlassian::jira::QueryLoadOutput>(),
            name: "jira_query_load".to_string(),
            description: "Load and display the contents of a saved Jira query. Returns the query name and the JQL query text.".to_string(),
            input_schema: schema::input_schema_for::<atlassian::JiraQueryLoadArgs>(),
            summary: "Show the JQL text of a saved Jira query",
            kind: ToolKind::Read,
        },
        Tool {
            output_schema: schema::output_schema_for::<mcptools_core::atlassian::bitbucket::PRListOutput>(),
            name: "bitbucket_pr_list".to_string(),
            description: "List pull requests for a Bitbucket repository. Returns PR details including ID, title, author, state, and branches. Supports filtering by state and pagination. Requires BITBUCKET_USERNAME and BITBUCKET_APP_PASSWORD environment variables.".to_string(),
            input_schema: schema::input_schema_for::<atlassian::BitbucketPRListArgs>(),
            summary: "List pull requests of a Bitbucket repository",
            kind: ToolKind::Read,
        },
        Tool {
            output_schema: schema::projected_output_schema_for::<
                mcptools_core::atlassian::bitbucket::PROutput,
            >(),
            name: "bitbucket_pr_read".to_string(),
            description: "Read details of a specific Bitbucket pull request including diff, diffstat, and comments. Use lineLimit to control diff output size (default: 500 lines, use -1 for unlimited). Requires BITBUCKET_USERNAME and BITBUCKET_APP_PASSWORD environment variables. Optional fields (array of dotted paths, e.g. [\"id\",\"title\",\"diff_content\"]) returns only the named fields; omit it for the full output.".to_string(),
            input_schema: schema::input_schema_for::<atlassian::BitbucketPRReadArgs>(),
            summary: "Read one Bitbucket pull request with diff and comments",
            kind: ToolKind::Read,
        },
        Tool {
            output_schema: schema::output_schema_for::<mcptools_core::atlassian::bitbucket::PRCreateOutput>(),
            name: "bitbucket_pr_create".to_string(),
            description: "Create a new pull request in a Bitbucket repository. Requires repo, title, and source branch. Optionally specify destination branch (defaults to repo's main branch), description, and whether to close the source branch after merge. Requires BITBUCKET_USERNAME and BITBUCKET_APP_PASSWORD environment variables.".to_string(),
            input_schema: schema::input_schema_for::<atlassian::BitbucketPRCreateArgs>(),
            summary: "Create a pull request in a Bitbucket repository",
            kind: ToolKind::Write,
        },
        Tool {
            output_schema: schema::output_schema_for::<mcptools_core::atlassian::bitbucket::WorkspaceListOutput>(),
            name: "bitbucket_workspace_list".to_string(),
            description: "List Bitbucket workspaces accessible to the authenticated user. Returns workspace slugs and names. Supports pagination. Requires BITBUCKET_USERNAME and BITBUCKET_APP_PASSWORD environment variables.".to_string(),
            input_schema: schema::input_schema_for::<atlassian::BitbucketWorkspaceListArgs>(),
            summary: "List Bitbucket workspaces",
            kind: ToolKind::Read,
        },
        Tool {
            output_schema: schema::output_schema_for::<mcptools_core::atlassian::bitbucket::RepoListOutput>(),
            name: "bitbucket_repo_list".to_string(),
            description: "List repositories in a Bitbucket workspace. Returns repository names, full names, and clone URLs (SSH and HTTPS). Supports pagination. Requires BITBUCKET_USERNAME and BITBUCKET_APP_PASSWORD environment variables.".to_string(),
            input_schema: schema::input_schema_for::<atlassian::BitbucketRepoListArgs>(),
            summary: "List repositories in a Bitbucket workspace",
            kind: ToolKind::Read,
        },
        Tool {
            output_schema: schema::output_schema_for::<mcptools_core::atlassian::bitbucket::BranchListOutput>(),
            name: "bitbucket_repo_branches".to_string(),
            description: "List branches in a Bitbucket repository. Returns branch names, latest commit hash, date, message, and author. Supports filtering and sorting. Requires BITBUCKET_USERNAME and BITBUCKET_APP_PASSWORD environment variables.".to_string(),
            input_schema: schema::input_schema_for::<atlassian::BitbucketRepoBranchesArgs>(),
            summary: "List branches of a Bitbucket repository",
            kind: ToolKind::Read,
        },
        Tool {
            output_schema: schema::output_schema_for::<mcptools_core::annotations::ListAnnotationsResponse>(),
            name: "ui_annotations_list".to_string(),
            description: "List all UI annotations from the calendsync dev server. Returns selector, component name, note, and resolution status for each annotation.".to_string(),
            input_schema: schema::input_schema_for::<annotations::AnnotationsListArgs>(),
            summary: "List UI annotations from the calendsync dev overlay",
            kind: ToolKind::Read,
        },
        Tool {
            output_schema: schema::output_schema_for::<mcptools_core::annotations::DevAnnotation>(),
            name: "ui_annotations_get".to_string(),
            description: "Get a single UI annotation by ID with full details including computed styles, bounding box, and optional screenshot.".to_string(),
            input_schema: schema::input_schema_for::<annotations::AnnotationsGetArgs>(),
            summary: "Get one UI annotation with styles, bounding box, and screenshot",
            kind: ToolKind::Read,
        },
        Tool {
            output_schema: schema::output_schema_for::<annotations::AnnotationResolveOutput>(),
            name: "ui_annotations_resolve".to_string(),
            description: "Mark a UI annotation as resolved with a summary of the changes made.".to_string(),
            input_schema: schema::input_schema_for::<annotations::AnnotationsResolveArgs>(),
            summary: "Mark a UI annotation as resolved with a change summary",
            kind: ToolKind::Write,
        },
        Tool {
            output_schema: schema::output_schema_for::<annotations::AnnotationClearOutput>(),
            name: "ui_annotations_clear".to_string(),
            description: "Clear all UI annotations from the dev server.".to_string(),
            input_schema: schema::input_schema_for::<annotations::AnnotationsClearArgs>(),
            summary: "Delete all UI annotations",
            kind: ToolKind::Write,
        },
        Tool {
            output_schema: schema::output_schema_for::<::pdf::DocumentTree>(),
            name: "pdf_toc".to_string(),
            description: "Parse a PDF file and return its document tree (table of contents) with section IDs, headings, content previews, and image counts. Use the section IDs with pdf_read to read specific sections.".to_string(),
            input_schema: schema::input_schema_for::<pdf::PdfTocArgs>(),
            summary: "List the sections of a PDF as a table of contents",
            kind: ToolKind::Read,
        },
        Tool {
            output_schema: schema::projected_output_schema_for::<::pdf::SectionContent>(),
            name: "pdf_read".to_string(),
            description: "Read a section of a PDF document as Markdown, or the entire document if no section specified. Returns the section title, rendered Markdown text, and image references. Optional fields (array of dotted paths, e.g. [\"title\",\"text\"]) returns only the named fields; omit it for the full output.".to_string(),
            input_schema: schema::input_schema_for::<pdf::PdfReadArgs>(),
            summary: "Read a PDF section, or the whole PDF, as Markdown",
            kind: ToolKind::Read,
        },
        Tool {
            output_schema: schema::output_schema_for::<::pdf::PeekContent>(),
            name: "pdf_peek".to_string(),
            description: "Sample a text snippet from a PDF section at a given position (beginning, middle, ending, random) without reading the full content. Returns the snippet with total character count so you know how much content remains. Defaults to the whole document if no section specified.".to_string(),
            input_schema: schema::input_schema_for::<pdf::PdfPeekArgs>(),
            summary: "Sample a short text snippet from a PDF section",
            kind: ToolKind::Read,
        },
        Tool {
            output_schema: schema::output_schema_for::<pdf::PdfImagesOutput>(),
            name: "pdf_images".to_string(),
            description: "List all images in a PDF section or the whole document. Returns image IDs, formats, section IDs, section titles, and page numbers. Use with pdf_image to extract specific images. NOTE: PDFs often reuse decorative images (logos, backgrounds, headers) across many pages — the same image ID will appear on multiple pages. To find meaningful content images (screenshots, diagrams, photos), filter out IDs that repeat across many pages and focus on IDs that appear only within the target section.".to_string(),
            input_schema: schema::input_schema_for::<pdf::PdfImagesArgs>(),
            summary: "List the images in a PDF or in one section",
            kind: ToolKind::Read,
        },
        Tool {
            output_schema: schema::output_schema_for::<pdf::PdfImageOutput>(),
            name: "pdf_image".to_string(),
            description: "Extract a specific image from a PDF document by ID, or pick a random image. Returns the image as base64-encoded data with format information. Optionally scope to a section.".to_string(),
            input_schema: schema::input_schema_for::<pdf::PdfImageArgs>(),
            summary: "Extract one image from a PDF by id or at random",
            kind: ToolKind::Read,
        },
        Tool {
            output_schema: schema::output_schema_for::<::pdf::DocumentMetadata>(),
            name: "pdf_info".to_string(),
            description: "Get metadata about a PDF document including title, author, page count, and creator.".to_string(),
            input_schema: schema::input_schema_for::<pdf::PdfInfoArgs>(),
            summary: "Get PDF metadata: title, author, page count",
            kind: ToolKind::Read,
        },
        Tool {
            output_schema: schema::output_schema_for::<crate::images::SavedOutput>(),
            name: "images_generate".to_string(),
            description: "Generate images with ChatGPT Images 2.5 (gpt-image-2.5-flare default, gpt-image-2.5-sunburst for premium precision). Text-to-image via POST /v1/images/generations. Saves PNG/JPEG/WebP files and returns paths plus usage. Defaults to the ChatGPT subscription (llm-stream auth.json); pass api=openai with OPENAI_API_KEY for the metered API.".to_string(),
            input_schema: schema::input_schema_for::<images::ImagesGenerateArgs>(),
            summary: "Generate an image from a text prompt",
            kind: ToolKind::Spend,
        },
        Tool {
            output_schema: schema::output_schema_for::<crate::images::SavedOutput>(),
            name: "images_edit".to_string(),
            description: "Edit ChatGPT Images 2.5 images with a prompt plus 1-16 reference images and optional mask via POST /v1/images/edits. Preserves subject/composition outside the edit. Saves files and returns paths plus usage. Defaults to the ChatGPT subscription (llm-stream auth.json); pass api=openai with OPENAI_API_KEY for the metered API.".to_string(),
            input_schema: schema::input_schema_for::<images::ImagesEditArgs>(),
            summary: "Edit an image with a prompt, reference images, and an optional mask",
            kind: ToolKind::Spend,
        },
        Tool {
            output_schema: schema::output_schema_for::<crate::images::SavedOutput>(),
            name: "images_vary".to_string(),
            description: "Create variations of ChatGPT Images 2.5 images anchored to 1-16 reference images. Same as images_edit with a default variation prompt when prompt is omitted. Defaults to the ChatGPT subscription (llm-stream auth.json); pass api=openai with OPENAI_API_KEY for the metered API.".to_string(),
            input_schema: schema::input_schema_for::<images::ImagesVaryArgs>(),
            summary: "Create variations of reference images",
            kind: ToolKind::Spend,
        },
        Tool {
            output_schema: schema::output_schema_for::<atlas::AtlasTextOutput>(),
            name: "atlas_tree_view".to_string(),
            description: "Browse an annotated directory tree of the codebase. Each entry includes a short description of what the file or directory contains. Use this to navigate unfamiliar codebases — start at the root, then drill into directories of interest.".to_string(),
            input_schema: schema::input_schema_for::<atlas::AtlasTreeViewArgs>(),
            summary: "Browse the annotated directory tree of the codebase",
            kind: ToolKind::Read,
        },
        Tool {
            output_schema: schema::output_schema_for::<atlas::AtlasTextOutput>(),
            name: "atlas_peek".to_string(),
            description: "Get a detailed summary of a file or directory. For files: long description, extracted symbols with signatures. For directories: long description, children with descriptions, aggregated symbols. Use this after tree_view to understand a specific file before reading it.".to_string(),
            input_schema: schema::input_schema_for::<atlas::AtlasPeekArgs>(),
            summary: "Summarize one file or directory of the codebase with its symbols",
            kind: ToolKind::Read,
        },
        Tool {
            output_schema: schema::output_schema_for::<atlas::AtlasTextOutput>(),
            name: "atlas_status".to_string(),
            description: "Check the health of the Atlas codebase index. Shows when it was last updated, how many files are tracked, and whether descriptions are available.".to_string(),
            input_schema: schema::input_schema_for::<atlas::AtlasStatusArgs>(),
            summary: "Check the health of the Atlas codebase index",
            kind: ToolKind::Read,
        },
        Tool {
            output_schema: schema::output_schema_for::<mcptools_core::linear::Viewer>(),
            name: "linear_auth_status".to_string(),
            description: "Show the Linear viewer identity for LINEAR_API_KEY. Returns id, name, and email. Requires LINEAR_API_KEY environment variable.".to_string(),
            input_schema: schema::input_schema_for::<crate::linear::args::AuthStatusArgs>(),
            summary: "Show the authenticated Linear user",
            kind: ToolKind::Read,
        },
        Tool {
            output_schema: schema::output_schema_for::<
                mcptools_core::linear::IssueGetOutput,
            >(),
            name: "linear_issue_get".to_string(),
            description: "Get one Linear issue by id or identifier (e.g. GUZ-85). Returns id, identifier, title, URL, state, parent, and blocked-by relations. Requires LINEAR_API_KEY environment variable.".to_string(),
            input_schema: schema::input_schema_for::<crate::linear::args::IssueGetArgs>(),
            summary: "Get one Linear issue with its comments and activity",
            kind: ToolKind::Read,
        },
        Tool {
            output_schema: schema::projected_output_schema_for::<
                mcptools_core::linear::IssueListOutput,
            >(),
            name: "linear_issue_list".to_string(),
            description: "List Linear issues with filters. Returns nodes with id, identifier, title, state, parent, blocked-by plus pageInfo. Requires LINEAR_API_KEY environment variable. Optional fields (array of dotted paths, e.g. [\"nodes.identifier\",\"nodes.title\",\"pageInfo\"]) returns only the named fields; omit it for the full output. Keep pageInfo if you page further.".to_string(),
            input_schema: schema::input_schema_for::<crate::linear::args::IssueListArgs>(),
            summary: "List Linear issues by team, state, assignee, project, label, or cycle",
            kind: ToolKind::Read,
        },
        Tool {
            output_schema: schema::output_schema_for::<mcptools_core::linear::CommentListOutput>(),
            name: "linear_comment_list".to_string(),
            description: "List comments on one Linear issue. Returns nodes with id, author, body, createdAt plus pageInfo. Requires LINEAR_API_KEY environment variable.".to_string(),
            input_schema: schema::input_schema_for::<crate::linear::args::CommentListArgs>(),
            summary: "List comments on a Linear issue",
            kind: ToolKind::Read,
        },
        Tool {
            output_schema: schema::output_schema_for::<mcptools_core::linear::RelationListOutput>(),
            name: "linear_relation_list".to_string(),
            description: "List relations on one Linear issue. Returns nodes with id, type, issue, related issue, direction plus pageInfo. Requires LINEAR_API_KEY environment variable.".to_string(),
            input_schema: schema::input_schema_for::<crate::linear::args::RelationListArgs>(),
            summary: "List relations of a Linear issue",
            kind: ToolKind::Read,
        },
        Tool {
            output_schema: schema::output_schema_for::<mcptools_core::linear::TeamListOutput>(),
            name: "linear_team_list".to_string(),
            description: "List Linear teams. Returns nodes with id, key, name plus pageInfo. Requires LINEAR_API_KEY environment variable.".to_string(),
            input_schema: schema::input_schema_for::<crate::linear::args::TeamListArgs>(),
            summary: "List Linear teams",
            kind: ToolKind::Read,
        },
        Tool {
            output_schema: schema::output_schema_for::<mcptools_core::linear::Team>(),
            name: "linear_team_get".to_string(),
            description: "Get one Linear team by id, key, or name. Returns id, key, name. Requires LINEAR_API_KEY environment variable.".to_string(),
            input_schema: schema::input_schema_for::<crate::linear::args::TeamGetArgs>(),
            summary: "Get one Linear team by id, key, or name",
            kind: ToolKind::Read,
        },
        Tool {
            output_schema: schema::output_schema_for::<mcptools_core::linear::ProjectListOutput>(),
            name: "linear_project_list".to_string(),
            description: "List Linear projects in a team. Returns nodes with id, name plus pageInfo. Requires LINEAR_API_KEY environment variable.".to_string(),
            input_schema: schema::input_schema_for::<crate::linear::args::ProjectListArgs>(),
            summary: "List projects in a Linear team",
            kind: ToolKind::Read,
        },
        Tool {
            output_schema: schema::output_schema_for::<mcptools_core::linear::Project>(),
            name: "linear_project_get".to_string(),
            description: "Get one Linear project by id or name within a team. Returns id, name. Requires LINEAR_API_KEY environment variable.".to_string(),
            input_schema: schema::input_schema_for::<crate::linear::args::ProjectGetArgs>(),
            summary: "Get one Linear project by id or name",
            kind: ToolKind::Read,
        },
        Tool {
            output_schema: schema::output_schema_for::<mcptools_core::linear::UserListOutput>(),
            name: "linear_user_list".to_string(),
            description: "List Linear users matching a name query. Returns nodes with id, name, email plus pageInfo. Requires LINEAR_API_KEY environment variable.".to_string(),
            input_schema: schema::input_schema_for::<crate::linear::args::UserListArgs>(),
            summary: "Find Linear users by name",
            kind: ToolKind::Read,
        },
        Tool {
            output_schema: schema::output_schema_for::<mcptools_core::linear::StateListOutput>(),
            name: "linear_state_list".to_string(),
            description: "List workflow states in a Linear team. Returns nodes with id, name, type plus pageInfo. Requires LINEAR_API_KEY environment variable.".to_string(),
            input_schema: schema::input_schema_for::<crate::linear::args::StateListArgs>(),
            summary: "List workflow states of a Linear team",
            kind: ToolKind::Read,
        },
        Tool {
            output_schema: schema::output_schema_for::<mcptools_core::linear::LabelListOutput>(),
            name: "linear_label_list".to_string(),
            description: "List labels in a Linear team. Returns nodes with id, name plus pageInfo. Requires LINEAR_API_KEY environment variable.".to_string(),
            input_schema: schema::input_schema_for::<crate::linear::args::LabelListArgs>(),
            summary: "List labels of a Linear team",
            kind: ToolKind::Read,
        },
        Tool {
            output_schema: schema::output_schema_for::<mcptools_core::linear::CycleListOutput>(),
            name: "linear_cycle_list".to_string(),
            description: "List cycles in a Linear team. Returns nodes with id, number, name plus pageInfo. Requires LINEAR_API_KEY environment variable.".to_string(),
            input_schema: schema::input_schema_for::<crate::linear::args::CycleListArgs>(),
            summary: "List cycles of a Linear team",
            kind: ToolKind::Read,
        },
        Tool {
            output_schema: schema::output_schema_for::<mcptools_core::linear::IssueMini>(),
            name: "linear_issue_create".to_string(),
            description: "Create a Linear issue in a team. Returns id, identifier, title, URL, state, parent. Requires LINEAR_API_KEY environment variable.".to_string(),
            input_schema: schema::input_schema_for::<crate::linear::args::IssueCreateArgs>(),
            summary: "Create a Linear issue in a team",
            kind: ToolKind::Write,
        },
        Tool {
            output_schema: schema::output_schema_for::<mcptools_core::linear::IssueMini>(),
            name: "linear_issue_update".to_string(),
            description: "Update a Linear issue by id or identifier. Needs at least one of title, description, state, assignee, parent, clearParent. Returns the updated issue. Requires LINEAR_API_KEY environment variable.".to_string(),
            input_schema: schema::input_schema_for::<crate::linear::args::IssueUpdateArgs>(),
            summary: "Update a Linear issue: state (close, reopen), title, assignee, parent",
            kind: ToolKind::Write,
        },
        Tool {
            output_schema: schema::output_schema_for::<mcptools_core::linear::Comment>(),
            name: "linear_comment_create".to_string(),
            description: "Create a comment on a Linear issue. Returns id, body, URL, author, createdAt. Requires LINEAR_API_KEY environment variable.".to_string(),
            input_schema: schema::input_schema_for::<crate::linear::args::CommentCreateArgs>(),
            summary: "Add a comment to a Linear issue",
            kind: ToolKind::Write,
        },
        Tool {
            output_schema: schema::output_schema_for::<mcptools_core::linear::RelationAddOutput>(),
            name: "linear_relation_add".to_string(),
            description: "Add a relation between two Linear issues. Type is 'blocks' or 'related'. Returns status created or already_exists plus the relation. Requires LINEAR_API_KEY environment variable.".to_string(),
            input_schema: schema::input_schema_for::<crate::linear::args::RelationAddArgs>(),
            summary: "Add a blocks or related relation between two Linear issues",
            kind: ToolKind::Write,
        },
        Tool {
            output_schema: schema::output_schema_for::<mcptools_core::linear::RelationRemoveOutput>(),
            name: "linear_relation_remove".to_string(),
            description: "Remove a relation between two Linear issues matched by source, related, type triple. Returns the deleted relation id. Requires LINEAR_API_KEY environment variable.".to_string(),
            input_schema: schema::input_schema_for::<crate::linear::args::RelationRemoveArgs>(),
            summary: "Remove a relation between two Linear issues",
            kind: ToolKind::Write,
        },
        Tool {
            output_schema: schema::output_schema_for::<find_tools::FoundTools>(),
            name: "find_tools".to_string(),
            description: "Ranks the tool catalog against a task and returns a TypeScript declaration per tool, and never calls another tool. Every tool it returns is callable by name through tools/call, even when tools/list does not list it.".to_string(),
            input_schema: schema::input_schema_for::<find_tools::FindToolsArgs>(),
            summary: "Find the best-fitting tools for a task",
            kind: ToolKind::Read,
        },
        Tool {
            output_schema: schema::output_schema_for::<mcptools_core::sandbox::ExecuteOutput>(),
            name: "execute".to_string(),
            description: execute::DESCRIPTION.to_string(),
            input_schema: schema::input_schema_for::<execute::ExecuteArgs>(),
            summary: "Run JavaScript that calls the tools find_tools returns",
            kind: ToolKind::Write,
        },
    ]
}

pub fn listed_tools(tools: Vec<Tool>, flags: ServeFlags) -> Vec<Tool> {
    if flags.code_mode {
        tools
            .into_iter()
            .filter(|tool| tool.name == "find_tools" || tool.name == "execute")
            .collect()
    } else if flags.discovery {
        tools
            .into_iter()
            .filter(|tool| tool.name == "find_tools")
            .collect()
    } else {
        tools
    }
}

pub fn handle_tools_list(flags: ServeFlags) -> Result<serde_json::Value, JsonRpcError> {
    let tools = listed_tools(registered_tools(), flags);
    let kinds: Vec<ToolKind> = tools.iter().map(|tool| tool.kind).collect();
    let mut list = serde_json::to_value(ToolsList { tools }).map_err(|e| JsonRpcError {
        code: -32603,
        message: format!("Internal error: {e}"),
        data: None,
    })?;
    if let Some(entries) = list.get_mut("tools").and_then(|tools| tools.as_array_mut()) {
        for (entry, kind) in entries.iter_mut().zip(kinds) {
            entry["annotations"] =
                serde_json::to_value(annotations_for(kind)).map_err(|e| JsonRpcError {
                    code: -32603,
                    message: format!("Internal error: {e}"),
                    data: None,
                })?;
        }
    }
    Ok(list)
}

pub fn tool_catalog() -> Vec<mcptools_core::catalog::CatalogEntry> {
    mcptools_core::catalog::build_catalog(
        registered_tools()
            .iter()
            .filter(|tool| tool.name != "find_tools" && tool.name != "execute")
            .map(|tool| (tool.name.as_str(), tool.summary)),
    )
}

pub fn declaration(tool: &Tool) -> String {
    mcptools_core::ts_decl::tool_declaration(
        &tool.name,
        &tool.description,
        &tool.input_schema,
        &tool.output_schema,
    )
}

pub fn declarations(names: &[String]) -> crate::prelude::Result<String> {
    let tools = registered_tools();
    if names.is_empty() {
        return Ok(tools.iter().map(declaration).collect::<Vec<_>>().join("\n"));
    }
    let mut blocks = Vec::with_capacity(names.len());
    for name in names {
        match tools.iter().find(|tool| &tool.name == name) {
            Some(tool) => blocks.push(declaration(tool)),
            None => return Err(crate::prelude::eyre!("unknown tool: {name}")),
        }
    }
    Ok(blocks.join("\n"))
}

pub async fn handle_tools_call(
    params: Option<serde_json::Value>,
    global: &crate::Global,
    flags: ServeFlags,
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
        "find_tools" => find_tools::handle_find_tools(params.arguments, global, flags).await,
        "execute" => execute::handle_execute(params.arguments, global, flags).await,
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

#[cfg(test)]
mod catalog_tests {
    use super::ServeFlags;
    use std::collections::BTreeSet;

    fn serve_flags(discovery: bool) -> ServeFlags {
        ServeFlags {
            discovery,
            code_mode: false,
        }
    }

    #[test]
    fn catalog_names_equal_registered_names_minus_find_tools_and_execute() {
        let catalog = super::tool_catalog();
        let registered = super::registered_tools();
        assert_eq!(catalog.len() + 2, registered.len());
        let catalog_names: BTreeSet<String> = catalog.into_iter().map(|entry| entry.name).collect();
        let registry_names: BTreeSet<String> = registered
            .into_iter()
            .map(|tool| tool.name)
            .filter(|n| n != "find_tools" && n != "execute")
            .collect();
        assert_eq!(catalog_names, registry_names);
    }

    #[test]
    fn summaries_fit_the_cap() {
        for entry in super::tool_catalog() {
            assert!(!entry.summary.is_empty(), "{}", entry.name);
            assert!(
                entry.summary.chars().count() <= mcptools_core::catalog::SUMMARY_MAX_CHARS,
                "{}",
                entry.name
            );
        }
    }

    #[test]
    fn domains_equal_the_known_prefixes() {
        let catalog = super::tool_catalog();
        let domains: BTreeSet<&str> = catalog.iter().map(|entry| entry.domain.as_str()).collect();
        assert_eq!(
            domains,
            BTreeSet::from([
                "atlas",
                "bitbucket",
                "confluence",
                "hn",
                "images",
                "jira",
                "linear",
                "md",
                "pdf",
                "ui"
            ])
        );
    }

    #[test]
    fn tools_list_json_omits_summary() {
        let list = super::handle_tools_list(serve_flags(false)).unwrap();
        for tool in list["tools"].as_array().unwrap() {
            assert!(tool.get("summary").is_none());
            assert!(tool.get("name").is_some());
            assert!(tool.get("description").is_some());
            assert!(tool.get("inputSchema").is_some());
            assert!(tool.get("outputSchema").is_some());
        }
    }

    #[test]
    fn mcp_tools_list_includes_find_tools() {
        let list = super::handle_tools_list(serve_flags(false)).unwrap();
        let tools = list["tools"].as_array().unwrap();
        assert!(tools
            .iter()
            .any(|t| t.get("name") == Some(&serde_json::json!("find_tools"))));
    }
    #[test]
    fn registry_holds_sixty_three_tools() {
        assert_eq!(super::registered_tools().len(), 63);
    }

    const FIND_TOOLS_BUDGET_CHARS: usize = 1300;
    const JOINT_BUDGET_TOKENS: usize = 1000;

    #[test]
    fn joint_definition_fits_budget() {
        let list = super::handle_tools_list(ServeFlags {
            discovery: false,
            code_mode: true,
        })
        .unwrap();
        let tools = list["tools"].as_array().unwrap();
        let mut total = 0;
        for name in ["find_tools", "execute"] {
            let entry = tools
                .iter()
                .find(|tool| tool.get("name") == Some(&serde_json::json!(name)))
                .unwrap_or_else(|| panic!("{name} missing from code mode tools/list"));
            total += mcptools_core::atlas::prompts::estimate_tokens(
                &serde_json::to_string(entry).unwrap(),
            );
        }
        println!("joint tokens = {total} (budget {JOINT_BUDGET_TOKENS})");
        assert!(
            total <= JOINT_BUDGET_TOKENS,
            "joint tokens = {total} (budget {JOINT_BUDGET_TOKENS})"
        );
    }

    #[test]
    fn find_tools_definition_fits_budget() {
        let list = super::handle_tools_list(serve_flags(true)).unwrap();
        let tools = list["tools"].as_array().unwrap();
        let entry = tools
            .iter()
            .find(|tool| tool.get("name") == Some(&serde_json::json!("find_tools")))
            .expect("find_tools missing from discovery tools/list");
        let length = serde_json::to_string(entry).unwrap().len();
        assert!(
            length <= FIND_TOOLS_BUDGET_CHARS,
            "find_tools tools/list entry is {length} chars, budget is {FIND_TOOLS_BUDGET_CHARS}"
        );
    }

    #[test]
    fn listed_tools_discovery_keeps_only_find_tools() {
        let tools = super::listed_tools(super::registered_tools(), serve_flags(true));
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "find_tools");
    }

    #[test]
    fn listed_tools_full_mode_is_unchanged() {
        let full = super::listed_tools(super::registered_tools(), serve_flags(false));
        let registered = super::registered_tools();
        assert_eq!(full.len(), registered.len());
        assert_eq!(
            full.iter().map(|t| &t.name).collect::<Vec<_>>(),
            registered.iter().map(|t| &t.name).collect::<Vec<_>>()
        );
    }

    #[test]
    fn gate_truth_table_for_flag_combinations() {
        let names = |flags: ServeFlags| -> Vec<String> {
            let list = super::handle_tools_list(flags).unwrap();
            list["tools"]
                .as_array()
                .unwrap()
                .iter()
                .map(|tool| tool["name"].as_str().unwrap().to_string())
                .collect()
        };
        assert_eq!(names(serve_flags(false)).len(), 63);
        assert_eq!(names(serve_flags(true)), ["find_tools"]);
        assert_eq!(
            names(ServeFlags {
                discovery: false,
                code_mode: true
            }),
            ["find_tools", "execute"]
        );
        assert_eq!(
            names(ServeFlags {
                discovery: true,
                code_mode: true
            }),
            ["find_tools", "execute"]
        );
    }

    #[test]
    fn annotations_follow_kind() {
        assert_eq!(
            super::annotations_for(super::ToolKind::Read),
            super::ToolAnnotations {
                read_only: true,
                destructive: false,
            }
        );
        assert_eq!(
            super::annotations_for(super::ToolKind::Write),
            super::ToolAnnotations {
                read_only: false,
                destructive: true,
            }
        );
        assert_eq!(
            super::annotations_for(super::ToolKind::Spend),
            super::ToolAnnotations {
                read_only: false,
                destructive: true,
            }
        );
    }

    #[test]
    fn execute_entry_is_destructive() {
        let list = super::handle_tools_list(serve_flags(false)).unwrap();
        let tools = list["tools"].as_array().unwrap();
        let entry = tools
            .iter()
            .find(|tool| tool.get("name") == Some(&serde_json::json!("execute")))
            .expect("execute missing from tools/list");
        assert_eq!(
            entry["annotations"]["destructiveHint"],
            serde_json::json!(true)
        );
    }

    #[test]
    fn full_table_matches_oracle_annotations() {
        let write = [
            "jira_create",
            "jira_update",
            "jira_comment_add",
            "jira_comment_update",
            "jira_comment_delete",
            "jira_attachment_upload",
            "jira_query_save",
            "jira_query_delete",
            "bitbucket_pr_create",
            "linear_issue_create",
            "linear_issue_update",
            "linear_comment_create",
            "linear_relation_add",
            "linear_relation_remove",
            "ui_annotations_resolve",
            "ui_annotations_clear",
            "execute",
        ];
        let spend = ["images_generate", "images_edit", "images_vary"];
        let list = super::handle_tools_list(serve_flags(false)).unwrap();
        let tools = list["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 63);
        let entry = |name: &str| {
            tools
                .iter()
                .find(|t| t.get("name") == Some(&serde_json::json!(name)))
                .unwrap_or_else(|| panic!("missing tool {name}"))
        };
        let mutable = serde_json::json!({"readOnlyHint": false, "destructiveHint": true});
        let readonly = serde_json::json!({"readOnlyHint": true, "destructiveHint": false});
        for name in write.into_iter().chain(spend) {
            assert_eq!(entry(name)["annotations"], mutable, "{name}");
        }
        let mutable_names: std::collections::BTreeSet<&str> =
            write.into_iter().chain(spend).collect();
        let read_count = tools
            .iter()
            .filter(|t| t["annotations"] == readonly)
            .count();
        assert_eq!(read_count, 63 - mutable_names.len());
    }

    #[test]
    fn registered_names_match_oracle_sets() {
        let write: std::collections::BTreeSet<&str> = [
            "jira_create",
            "jira_update",
            "jira_comment_add",
            "jira_comment_update",
            "jira_comment_delete",
            "jira_attachment_upload",
            "jira_query_save",
            "jira_query_delete",
            "bitbucket_pr_create",
            "linear_issue_create",
            "linear_issue_update",
            "linear_comment_create",
            "linear_relation_add",
            "linear_relation_remove",
            "ui_annotations_resolve",
            "ui_annotations_clear",
            "execute",
        ]
        .into_iter()
        .collect();
        let spend: std::collections::BTreeSet<&str> =
            ["images_generate", "images_edit", "images_vary"]
                .into_iter()
                .collect();
        let read: std::collections::BTreeSet<&str> = [
            "jira_search",
            "confluence_search",
            "hn_read_item",
            "hn_list_items",
            "md_fetch",
            "md_toc",
            "jira_get",
            "jira_comment_list",
            "jira_sprint_list",
            "jira_attachment_list",
            "jira_attachment_download",
            "jira_query_list",
            "jira_query_load",
            "bitbucket_pr_list",
            "bitbucket_pr_read",
            "bitbucket_workspace_list",
            "bitbucket_repo_list",
            "bitbucket_repo_branches",
            "ui_annotations_list",
            "ui_annotations_get",
            "pdf_toc",
            "pdf_read",
            "pdf_peek",
            "pdf_images",
            "pdf_image",
            "pdf_info",
            "atlas_tree_view",
            "atlas_peek",
            "atlas_status",
            "linear_auth_status",
            "linear_issue_get",
            "linear_issue_list",
            "linear_comment_list",
            "linear_relation_list",
            "linear_team_list",
            "linear_team_get",
            "linear_project_list",
            "linear_project_get",
            "linear_user_list",
            "linear_state_list",
            "linear_label_list",
            "linear_cycle_list",
            "find_tools",
        ]
        .into_iter()
        .collect();
        let union: std::collections::BTreeSet<&str> =
            write.union(&spend).chain(read.iter()).copied().collect();
        assert_eq!(union.len(), 63);
        let mut actual_write = BTreeSet::new();
        let mut actual_spend = BTreeSet::new();
        let mut actual_read = BTreeSet::new();
        for tool in super::registered_tools() {
            match tool.kind {
                super::ToolKind::Write => actual_write.insert(tool.name),
                super::ToolKind::Spend => actual_spend.insert(tool.name),
                super::ToolKind::Read => actual_read.insert(tool.name),
            };
        }
        let names = |set: std::collections::BTreeSet<&str>| {
            set.into_iter().map(str::to_string).collect::<BTreeSet<_>>()
        };
        assert_eq!(actual_write, names(write));
        assert_eq!(actual_spend, names(spend));
        assert_eq!(actual_read, names(read));
    }

    #[test]
    fn pilot_entries_carry_kind_annotations() {
        let list = super::handle_tools_list(serve_flags(false)).unwrap();
        let tools = list["tools"].as_array().unwrap();
        let entry = |name: &str| {
            tools
                .iter()
                .find(|t| t.get("name") == Some(&serde_json::json!(name)))
                .unwrap_or_else(|| panic!("missing tool {name}"))
        };
        assert_eq!(
            entry("jira_search")["annotations"],
            serde_json::json!({"readOnlyHint": true, "destructiveHint": false})
        );
        assert_eq!(
            entry("jira_create")["annotations"],
            serde_json::json!({"readOnlyHint": false, "destructiveHint": true})
        );
        assert_eq!(
            entry("images_generate")["annotations"],
            serde_json::json!({"readOnlyHint": false, "destructiveHint": true})
        );
        for tool in tools {
            let annotations = tool.get("annotations").unwrap();
            assert!(annotations.get("idempotentHint").is_none());
            assert!(annotations.get("openWorldHint").is_none());
        }
    }

    #[test]
    fn catalog_never_contains_a_tool_named_none() {
        let catalog = super::tool_catalog();
        assert!(!catalog.iter().any(|e| e.name == "none"));
    }
}

#[cfg(test)]
mod declaration_tests {
    use super::*;

    fn named(name: &str) -> Tool {
        super::registered_tools()
            .into_iter()
            .find(|tool| tool.name == name)
            .unwrap()
    }

    #[test]
    fn jira_search_declaration() {
        let tool = named("jira_search");
        let head = "interface JiraSearchInput {\n  fields?: string[];\n  limit?: number;\n  nextPageToken?: string;\n  query?: string;\n  queryName?: string;\n}\n\ninterface JiraSearchIssueOutput {\n  assignee?: string | null;\n  description?: string | null;\n  key?: string;\n  status?: string;\n  summary?: string;\n}\n\ninterface JiraSearchOutput {\n  issues?: JiraSearchIssueOutput[];\n  next_page_token?: string | null;\n  total?: number;\n}\n\n";
        let want =
            format!("{head}/** {} */\ndeclare function jira_search(input: JiraSearchInput): Promise<JiraSearchOutput>;\n", tool.description);
        assert_eq!(super::declaration(&tool), want);
    }

    #[test]
    fn pdf_read_declaration() {
        let tool = named("pdf_read");
        let head = "interface PdfReadInput {\n  fields?: string[];\n  path: string;\n  sectionId?: string;\n}\n\ntype PdfReadImageFormat = \"Jpeg\" | \"Png\" | \"Jpeg2000\" | \"Gif\" | \"Tiff\" | \"Bmp\" | \"WebP\" | \"Unknown\";\n\ntype PdfReadImageId = string;\n\ninterface PdfReadImageRef {\n  format?: PdfReadImageFormat;\n  id?: PdfReadImageId;\n}\n\ntype PdfReadSectionId = string;\n\ninterface PdfReadOutput {\n  id?: PdfReadSectionId;\n  images?: PdfReadImageRef[];\n  text?: string;\n  title?: string;\n}\n\n";
        let want =
            format!("{head}/** {} */\ndeclare function pdf_read(input: PdfReadInput): Promise<PdfReadOutput>;\n", tool.description);
        assert_eq!(super::declaration(&tool), want);
    }

    #[test]
    fn linear_issue_list_declaration() {
        let tool = named("linear_issue_list");
        let head = "interface LinearIssueListInput {\n  /** Fetch all pages (up to 50 items) */\n  all?: boolean;\n  /** Assignee user UUID or 'me' */\n  assignee?: string;\n  /** Page cursor for pagination */\n  cursor?: string;\n  /** Cycle number or id */\n  cycle?: string;\n  fields?: string[];\n  /** Label name */\n  label?: string;\n  /** Max items per page @default 25 */\n  limit?: number;\n  /** Project id or name (names need team) */\n  project?: string;\n  /** Title substring to search */\n  query?: string;\n  /** Workflow state name (e.g. Todo) */\n  state?: string;\n  /** Team id, key, or name */\n  team?: string;\n  /** Only issues updated at or after RFC3339 time (e.g. 2026-01-01T00:00:00Z) */\n  updatedAfter?: string;\n}\n\ninterface LinearIssueListIssueMini {\n  blocked_by?: string[];\n  description?: string | null;\n  id?: string;\n  identifier?: string;\n  parent?: string | null;\n  state?: string;\n  title?: string;\n  url?: string;\n}\n\ninterface LinearIssueListPageInfo {\n  endCursor?: string | null;\n  hasNextPage?: boolean;\n}\n\ninterface LinearIssueListOutput {\n  nodes?: LinearIssueListIssueMini[];\n  pageInfo?: LinearIssueListPageInfo;\n}\n\n";
        let want =
            format!("{head}/** {} */\ndeclare function linear_issue_list(input: LinearIssueListInput): Promise<LinearIssueListOutput>;\n", tool.description);
        assert_eq!(super::declaration(&tool), want);
    }

    #[test]
    fn md_fetch_declaration() {
        let tool = named("md_fetch");
        let head = "interface MdFetchInput {\n  /** @default null */\n  fields?: string[];\n  /** @default null */\n  index?: number;\n  /** @default null */\n  limit?: number;\n  /** @default null */\n  offset?: number;\n  /** @default null */\n  page?: number;\n  /** @default null */\n  raw_html?: boolean;\n  /** @default null */\n  selector?: string;\n  /** @default null */\n  strategy?: string;\n  /** @default null */\n  timeout?: number;\n  url: string;\n}\n\ninterface MdFetchMdPaginationInfo {\n  current_page?: number;\n  has_more?: boolean;\n  limit?: number;\n  total_characters?: number;\n  total_pages?: number;\n}\n\ninterface MdFetchOutput {\n  content?: string;\n  elements_found?: number | null;\n  fetch_time_ms?: number;\n  html_length?: number;\n  pagination?: MdFetchMdPaginationInfo;\n  selector_used?: string | null;\n  strategy_applied?: string | null;\n  title?: string | null;\n  url?: string;\n}\n\n";
        let want =
            format!("{head}/** {} */\ndeclare function md_fetch(input: MdFetchInput): Promise<MdFetchOutput>;\n", tool.description);
        assert_eq!(super::declaration(&tool), want);
    }

    #[test]
    fn bitbucket_pr_read_declaration() {
        let tool = named("bitbucket_pr_read");
        let head = "interface BitbucketPrReadInput {\n  diffLimit?: number;\n  fields?: string[];\n  limit?: number;\n  lineLimit?: number;\n  noDiff?: boolean;\n  prNumber: number;\n  repo: string;\n}\n\ninterface BitbucketPrReadCommentOutput {\n  author?: string;\n  content?: string;\n  created_on?: string;\n  id?: number;\n  inline_line?: number | null;\n  inline_path?: string | null;\n  is_inline?: boolean;\n}\n\ninterface BitbucketPrReadDiffstatOutput {\n  files?: BitbucketPrReadFileStatOutput[];\n  total_deletions?: number;\n  total_files?: number;\n  total_insertions?: number;\n}\n\ninterface BitbucketPrReadFileStatOutput {\n  lines_added?: number;\n  lines_removed?: number;\n  old_path?: string | null;\n  path?: string;\n  status?: string;\n}\n\ninterface BitbucketPrReadOutput {\n  approvals?: string[];\n  author?: string;\n  comments?: BitbucketPrReadCommentOutput[];\n  created_on?: string;\n  description?: string | null;\n  destination_branch?: string;\n  destination_commit?: string | null;\n  destination_repo?: string;\n  diff_content?: string | null;\n  diffstat?: BitbucketPrReadDiffstatOutput;\n  html_link?: string;\n  id?: number;\n  reviewers?: string[];\n  source_branch?: string;\n  source_commit?: string | null;\n  source_repo?: string;\n  state?: string;\n  title?: string;\n  updated_on?: string;\n}\n\n";
        let want =
            format!("{head}/** {} */\ndeclare function bitbucket_pr_read(input: BitbucketPrReadInput): Promise<BitbucketPrReadOutput>;\n", tool.description);
        assert_eq!(super::declaration(&tool), want);
    }

    #[test]
    fn declarations_keep_given_order_and_reject_unknown() {
        let names = vec!["pdf_read".to_string(), "jira_search".to_string()];
        let text = super::declarations(&names).unwrap();
        assert!(
            text.find("declare function pdf_read").unwrap()
                < text.find("declare function jira_search").unwrap()
        );
        assert!(text.ends_with(";\n") && !text.ends_with("\n\n"));

        let all = super::declarations(&[]).unwrap();
        assert_eq!(
            all.matches("declare function ").count(),
            super::registered_tools().len()
        );

        let err = super::declarations(&["nope_tool".to_string()]).unwrap_err();
        assert!(err.to_string().contains("nope_tool"));
    }

    fn pascal_case(name: &str) -> String {
        name.split('_')
            .map(|piece| {
                let mut chars = piece.chars();
                match chars.next() {
                    None => String::new(),
                    Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                }
            })
            .collect()
    }

    fn ref_targets(schema: &serde_json::Value, out: &mut Vec<String>) {
        match schema {
            serde_json::Value::Object(map) => {
                if let Some(name) = map
                    .get("$ref")
                    .and_then(|link| link.as_str())
                    .and_then(|link| link.strip_prefix("#/$defs/"))
                {
                    out.push(name.to_string());
                }
                for value in map.values() {
                    ref_targets(value, out);
                }
            }
            serde_json::Value::Array(items) => {
                for item in items {
                    ref_targets(item, out);
                }
            }
            _ => {}
        }
    }

    #[test]
    fn sweep_covers_every_registered_tool() {
        let tools = super::registered_tools();
        let mut unknown_tools = std::collections::BTreeSet::new();
        for tool in &tools {
            let text = super::declaration(tool);
            let mut targets = vec![];
            ref_targets(&tool.input_schema, &mut targets);
            ref_targets(&tool.output_schema, &mut targets);
            let prefix = pascal_case(&tool.name);
            for target in targets {
                let named = format!("{prefix}{target}");
                assert!(
                    text.contains(&format!("interface {named} "))
                        || text.contains(&format!("type {named} =")),
                    "{}: missing definition for {target}",
                    tool.name
                );
            }
            assert!(!text.contains("$ref"), "{}: raw $ref leaked", tool.name);
            assert!(
                !text.contains("#/$defs"),
                "{}: raw #/$defs leaked",
                tool.name
            );
            if text.contains("unknown") && tool.name != "execute" {
                unknown_tools.insert(tool.name.clone());
            }
        }
        assert_eq!(unknown_tools, std::collections::BTreeSet::new());
    }
}
