//! Transformation functions for Jira API responses

use serde::{Deserialize, Serialize};

/// Jira issue response from API
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct JiraIssueResponse {
    pub key: String,
    pub fields: JiraIssueFields,
}

/// Fields from Jira issue
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct JiraIssueFields {
    pub summary: String,
    #[serde(default)]
    pub description: Option<serde_json::Value>,
    pub status: JiraStatus,
    #[serde(default)]
    pub assignee: Option<JiraAssignee>,
}

/// Jira status field
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct JiraStatus {
    pub name: String,
}

/// Jira assignee field
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
pub struct JiraAssignee {
    #[serde(rename = "displayName", default)]
    pub display_name: Option<String>,
    #[serde(default)]
    #[serde(rename = "emailAddress")]
    pub email_address: Option<String>,
}

/// Search response from Jira API
/// The GET /rest/api/3/search/jql endpoint returns this structure
#[derive(Debug, Deserialize, Clone)]
pub struct JiraSearchResponse {
    pub issues: Vec<JiraIssueResponse>,
    #[serde(default)]
    pub total: Option<u64>,
    #[serde(default)]
    #[serde(rename = "isLast")]
    pub is_last: Option<bool>,
    #[serde(default)]
    #[serde(rename = "nextPageToken")]
    pub next_page_token: Option<String>,
    #[serde(default)]
    #[serde(rename = "startAt")]
    pub start_at: Option<u64>,
    #[serde(default)]
    #[serde(rename = "maxResults")]
    pub max_results: Option<u64>,
}

/// Output structure for a single issue
#[derive(Debug, Serialize, Clone, PartialEq)]
pub struct IssueOutput {
    pub key: String,
    pub summary: String,
    pub description: Option<String>,
    pub status: String,
    pub assignee: Option<String>,
}

/// Output structure for search command
#[derive(Debug, Serialize, PartialEq)]
pub struct SearchOutput {
    pub issues: Vec<IssueOutput>,
    pub total: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_page_token: Option<String>,
}

/// Jira priority field
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct JiraPriority {
    #[serde(default)]
    pub name: String,
}

/// Jira issue type field
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct JiraIssueType {
    pub name: String,
}

/// Jira component field
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct JiraComponent {
    pub name: String,
}

/// Extended fields for detailed ticket read
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct JiraExtendedFields {
    pub summary: String,
    #[serde(default)]
    pub description: Option<serde_json::Value>, // Can be a string or ADF (Atlassian Document Format)
    pub status: JiraStatus,
    #[serde(default)]
    pub assignee: Option<JiraAssignee>,
    #[serde(default)]
    pub priority: Option<JiraPriority>,
    #[serde(default)]
    pub issuetype: Option<JiraIssueType>,
    #[serde(default)]
    pub created: Option<String>,
    #[serde(default)]
    pub updated: Option<String>,
    #[serde(default)]
    pub duedate: Option<String>,
    #[serde(default)]
    pub labels: Vec<String>,
    #[serde(default)]
    pub components: Vec<JiraComponent>,
}

/// Extended issue response for detailed read
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct JiraExtendedIssueResponse {
    pub key: String,
    pub fields: JiraExtendedFields,
}

/// Comment on a Jira ticket
#[derive(Debug, Serialize, Clone, Deserialize, PartialEq)]
pub struct JiraComment {
    #[serde(rename = "id")]
    pub comment_id: String,
    #[serde(rename = "body")]
    pub body: serde_json::Value,
    #[serde(rename = "created")]
    pub created_at: String,
    pub author: Option<JiraAssignee>,
}

/// Output structure for detailed ticket information
#[derive(Debug, Serialize, Clone, Deserialize, PartialEq)]
pub struct TicketOutput {
    pub key: String,
    pub summary: String,
    pub description: Option<String>,
    pub status: String,
    pub priority: Option<String>,
    pub issue_type: Option<String>,
    pub assignee: Option<String>,
    pub created: Option<String>,
    pub updated: Option<String>,
    pub due_date: Option<String>,
    pub labels: Vec<String>,
    pub components: Vec<String>,
    pub comments: Vec<JiraComment>,
    pub attachments: Vec<AttachmentOutput>,
}

/// Jira attachment response from API
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct JiraAttachmentResponse {
    pub id: String,
    pub filename: String,
    #[serde(rename = "mimeType")]
    pub mime_type: String,
    pub size: u64,
    pub created: String,
    pub content: String,
}

/// Output struct for displaying attachment information
#[derive(Debug, Serialize, Clone, Deserialize, PartialEq)]
pub struct AttachmentOutput {
    pub id: String,
    pub filename: String,
    pub mime_type: String,
    pub size_bytes: u64,
    pub size_human: String,
    pub created: String,
}

/// Extract description from Jira field (handles both string and ADF)
///
/// Jira descriptions can be either plain strings or ADF (Atlassian Document Format) JSON.
/// This function handles both cases and extracts readable text.
///
/// # Arguments
/// * `value` - The description field value from Jira API
///
/// # Returns
/// * `Option<String>` - Extracted text, or None if empty/invalid
pub fn extract_description(value: Option<serde_json::Value>) -> Option<String> {
    value.and_then(|v| match &v {
        serde_json::Value::String(s) => Some(s.clone()),
        serde_json::Value::Object(_) => {
            // Check if this is an ADF (Atlassian Document Format) object
            if v.get("type").and_then(|t| t.as_str()) == Some("doc") {
                // Extract text from ADF content
                render_adf(&v)
            } else {
                // For other objects, just return empty
                None
            }
        }
        _ => None,
    })
}

/// Render ADF (Atlassian Document Format) to readable text
///
/// ADF is a JSON-based document format used by Atlassian products.
/// This function walks the ADF tree and extracts human-readable text.
///
/// # Arguments
/// * `value` - The ADF document as JSON
///
/// # Returns
/// * `Option<String>` - Rendered text, or None if empty
pub fn render_adf(value: &serde_json::Value) -> Option<String> {
    let mut output = String::new();

    if let Some(content) = value.get("content").and_then(|c| c.as_array()) {
        for node in content {
            if let Some(rendered) = render_adf_node(node, 0) {
                output.push_str(&rendered);
                if !rendered.ends_with('\n') {
                    output.push('\n');
                }
            }
        }
    }

    if output.is_empty() {
        None
    } else {
        Some(output.trim().to_string())
    }
}

/// Render a single ADF node recursively
fn render_adf_node(node: &serde_json::Value, depth: usize) -> Option<String> {
    let node_type = node.get("type")?.as_str()?;
    let indent = "  ".repeat(depth);

    match node_type {
        "paragraph" => {
            let mut text = String::new();
            if let Some(content) = node.get("content").and_then(|c| c.as_array()) {
                for child in content {
                    if let Some(rendered) = render_adf_node(child, depth) {
                        text.push_str(&rendered);
                    }
                }
            }
            if text.is_empty() {
                Some("\n".to_string())
            } else {
                Some(format!("{text}\n"))
            }
        }
        "heading" => {
            let level = node
                .get("attrs")
                .and_then(|a| a.get("level"))
                .and_then(|l| l.as_u64())
                .unwrap_or(1) as usize;
            let heading_marker = "#".repeat(level.min(6));
            let mut text = String::new();
            if let Some(content) = node.get("content").and_then(|c| c.as_array()) {
                for child in content {
                    if let Some(rendered) = render_adf_node(child, 0) {
                        text.push_str(&rendered);
                    }
                }
            }
            Some(format!("{}{} {}\n", indent, heading_marker, text.trim()))
        }
        "bulletList" => {
            let mut text = String::new();
            if let Some(items) = node.get("content").and_then(|c| c.as_array()) {
                for item in items {
                    if let Some(rendered) = render_adf_node(item, depth + 1) {
                        text.push_str(&rendered);
                    }
                }
            }
            Some(text)
        }
        "listItem" => {
            let mut text = String::new();
            if let Some(content) = node.get("content").and_then(|c| c.as_array()) {
                for child in content {
                    if let Some(rendered) = render_adf_node(child, depth) {
                        text.push_str(&rendered);
                    }
                }
            }
            Some(format!("{}• {}\n", indent, text.trim()))
        }
        "codeBlock" => {
            let mut text = String::new();
            if let Some(content) = node.get("content").and_then(|c| c.as_array()) {
                for child in content {
                    if let Some(rendered) = render_adf_node(child, 0) {
                        text.push_str(&rendered);
                    }
                }
            }
            Some(format!(
                "{}```\n{}{}\n{}```\n",
                indent,
                indent,
                text.trim(),
                indent
            ))
        }
        "text" => node
            .get("text")
            .and_then(|t| t.as_str())
            .map(|text| text.to_string()),
        "hardBreak" => Some("\n".to_string()),
        _ => {
            // For unknown node types, try to extract text content
            if let Some(content) = node.get("content").and_then(|c| c.as_array()) {
                let mut text = String::new();
                for child in content {
                    if let Some(rendered) = render_adf_node(child, depth) {
                        text.push_str(&rendered);
                    }
                }
                if !text.is_empty() {
                    return Some(text);
                }
            }
            None
        }
    }
}

/// Convert Jira API response to domain model
///
/// Transforms the raw API response into our clean domain model.
///
/// # Arguments
/// * `search_response` - The raw response from Jira search API
///
/// # Returns
/// * `SearchOutput` - Cleaned and transformed search results
pub fn transform_search_response(search_response: JiraSearchResponse) -> SearchOutput {
    let issues: Vec<IssueOutput> = search_response
        .issues
        .into_iter()
        .map(|issue| {
            // Prefer displayName over emailAddress for assignee
            let assignee = issue
                .fields
                .assignee
                .and_then(|a| a.display_name.or(a.email_address));

            IssueOutput {
                key: issue.key,
                summary: issue.fields.summary,
                description: None, // Description is now ADF format, skip for now
                status: issue.fields.status.name,
                assignee,
            }
        })
        .collect();

    // GET /rest/api/3/search/jql always returns 'total' field
    let total = search_response.total.map(|t| t as usize).unwrap_or(0);

    SearchOutput {
        issues,
        total,
        next_page_token: search_response.next_page_token,
    }
}

/// Convert Jira extended issue response + comments + attachments to ticket output
///
/// Transforms the detailed API response into our clean domain model.
///
/// # Arguments
/// * `issue` - The raw extended issue response from Jira API
/// * `comments` - The parsed comments array
/// * `attachments` - The transformed attachment outputs
///
/// # Returns
/// * `TicketOutput` - Cleaned and transformed ticket with all details
pub fn transform_ticket_response(
    issue: JiraExtendedIssueResponse,
    comments: Vec<JiraComment>,
    attachments: Vec<AttachmentOutput>,
) -> TicketOutput {
    TicketOutput {
        key: issue.key,
        summary: issue.fields.summary,
        description: extract_description(issue.fields.description),
        status: issue.fields.status.name,
        priority: issue
            .fields
            .priority
            .as_ref()
            .map(|p| p.name.clone())
            .filter(|n| !n.is_empty()),
        issue_type: issue.fields.issuetype.as_ref().map(|it| it.name.clone()),
        assignee: issue
            .fields
            .assignee
            .as_ref()
            .and_then(|a| a.display_name.clone()),
        created: issue.fields.created,
        updated: issue.fields.updated,
        due_date: issue.fields.duedate,
        labels: issue.fields.labels,
        components: issue
            .fields
            .components
            .into_iter()
            .map(|c| c.name)
            .collect(),
        comments,
        attachments,
    }
}

/// Transition representation from Jira API
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct JiraTransition {
    pub id: String,
    pub name: String,
    pub to: JiraStatus,
}

/// Transitions response from Jira API
#[derive(Debug, Deserialize, Clone)]
pub struct JiraTransitionsResponse {
    pub transitions: Vec<JiraTransition>,
}

/// User search result from Jira API
#[derive(Debug, Deserialize, Clone)]
pub struct JiraUser {
    #[serde(rename = "accountId")]
    pub account_id: String,
    #[serde(rename = "displayName", default)]
    pub display_name: Option<String>,
    #[serde(default)]
    #[serde(rename = "emailAddress")]
    pub email_address: Option<String>,
}

/// User search response from Jira API - the API returns a bare array
/// See: https://developer.atlassian.com/cloud/jira/platform/rest/v3/api-group-user-search/#api-rest-api-3-users-search-get
pub type JiraUserSearchResponse = Vec<JiraUser>;

/// Field update result (success or error)
#[derive(Debug, Serialize, Clone, PartialEq)]
pub struct FieldUpdateResult {
    pub field: String,
    pub success: bool,
    pub value: Option<String>,
    pub error: Option<String>,
}

/// Output structure for update command
#[derive(Debug, Serialize, Clone, PartialEq)]
pub struct UpdateOutput {
    pub ticket_key: String,
    pub fields_updated: Vec<FieldUpdateResult>,
    pub partial_failure: bool,
}

/// Represents an assignee identifier in various formats
#[derive(Debug, Clone, PartialEq)]
pub enum AssigneeIdentifier {
    Email(String),
    DisplayName(String),
    AccountId(String),
    CurrentUser,
}

/// Parse assignee string to identify its format
///
/// # Arguments
/// * `input` - The assignee identifier string
///
/// # Returns
/// * `AssigneeIdentifier` - The identified format
pub fn parse_assignee_identifier(input: &str) -> AssigneeIdentifier {
    // Check if it's the special "me" keyword
    if input.eq_ignore_ascii_case("me") {
        return AssigneeIdentifier::CurrentUser;
    }

    // Check if it looks like an email
    if input.contains('@') && input.contains('.') {
        return AssigneeIdentifier::Email(input.to_string());
    }

    // Check if it looks like a Jira accountId (typically starts with a number or is alphanumeric)
    // Jira accountIds often have a specific format, but we'll be lenient here
    if input.len() > 10 && !input.contains(' ') && input.chars().all(|c| c.is_alphanumeric()) {
        return AssigneeIdentifier::AccountId(input.to_string());
    }

    // Otherwise treat as display name
    AssigneeIdentifier::DisplayName(input.to_string())
}

/// Find transition by status name
///
/// # Arguments
/// * `transitions` - List of available transitions
/// * `target_status` - The status name to find
///
/// # Returns
/// * `Option<String>` - The transition ID if found
pub fn find_transition_by_status(
    transitions: &[JiraTransition],
    target_status: &str,
) -> Option<String> {
    transitions
        .iter()
        .find(|t| t.to.name.eq_ignore_ascii_case(target_status))
        .map(|t| t.id.clone())
}

/// Build update payload from field values
///
/// # Arguments
/// * `priority` - Optional priority name
/// * `issue_type` - Optional issue type name
/// * `assignee_account_id` - Optional assignee account ID
/// * `description` - Optional description in ADF format
///
/// # Returns
/// * `serde_json::Value` - The fields object for the update request
pub fn build_update_payload(
    priority: Option<&str>,
    issue_type: Option<&str>,
    assignee_account_id: Option<&str>,
    description: Option<&serde_json::Value>,
) -> serde_json::Value {
    let mut fields = serde_json::json!({});

    if let Some(priority_name) = priority {
        fields["priority"] = serde_json::json!({ "name": priority_name });
    }

    if let Some(type_name) = issue_type {
        fields["issuetype"] = serde_json::json!({ "name": type_name });
    }

    if let Some(account_id) = assignee_account_id {
        fields["assignee"] = serde_json::json!({ "id": account_id });
    }

    if let Some(desc) = description {
        fields["description"] = desc.clone();
    }

    fields
}

/// Format a byte count as a human-readable size string (e.g., "1.5 KB", "3.0 MB").
pub fn format_file_size(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    let size_float = bytes as f64;
    if bytes == 0 {
        return "0 B".to_string();
    }
    if size_float < KB {
        format!("{} B", bytes)
    } else if size_float < MB {
        format!("{:.1} KB", size_float / KB)
    } else if size_float < GB {
        format!("{:.1} MB", size_float / MB)
    } else {
        format!("{:.1} GB", size_float / GB)
    }
}

/// Convert raw Jira attachment API responses into the domain output model.
pub fn transform_attachment_response(raw: Vec<JiraAttachmentResponse>) -> Vec<AttachmentOutput> {
    raw.into_iter()
        .map(|resp| AttachmentOutput {
            id: resp.id,
            filename: resp.filename,
            mime_type: resp.mime_type,
            size_bytes: resp.size,
            size_human: format_file_size(resp.size),
            created: resp.created,
        })
        .collect()
}

/// Convert markdown text to Atlassian Document Format (ADF) JSON.
///
/// Handles block-level elements (headings, code blocks, lists, paragraphs)
/// and inline marks (bold, italic, code, links). Never fails — malformed
/// markdown degrades to plain text paragraphs.
pub fn markdown_to_adf(input: &str) -> serde_json::Value {
    let lines: Vec<&str> = input.lines().collect();
    let mut blocks: Vec<serde_json::Value> = Vec::new();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i];

        // Fenced code block
        if let Some(rest) = line.strip_prefix("```") {
            let language = rest.trim().to_string();
            let mut code_lines: Vec<&str> = Vec::new();
            i += 1;
            while i < lines.len() && !lines[i].starts_with("```") {
                code_lines.push(lines[i]);
                i += 1;
            }
            if i < lines.len() {
                i += 1; // skip closing fence
            }
            let code_text = code_lines.join("\n");
            let mut node = serde_json::json!({
                "type": "codeBlock",
                "content": [{
                    "type": "text",
                    "text": code_text
                }]
            });
            if !language.is_empty() {
                node["attrs"] = serde_json::json!({ "language": language });
            }
            blocks.push(node);
            continue;
        }

        // Heading
        if is_heading_line(line) {
            let level = line.chars().take_while(|c| *c == '#').count().min(6);
            let text = line[level..].trim();
            if !text.is_empty() {
                blocks.push(serde_json::json!({
                    "type": "heading",
                    "attrs": { "level": level },
                    "content": parse_inline_marks(text)
                }));
            }
            i += 1;
            continue;
        }

        // Unordered list
        if is_unordered_list_item(line) {
            let mut items: Vec<serde_json::Value> = Vec::new();
            while i < lines.len() && is_unordered_list_item(lines[i]) {
                let item_text = strip_unordered_prefix(lines[i]);
                items.push(serde_json::json!({
                    "type": "listItem",
                    "content": [{
                        "type": "paragraph",
                        "content": parse_inline_marks(item_text)
                    }]
                }));
                i += 1;
            }
            blocks.push(serde_json::json!({
                "type": "bulletList",
                "content": items
            }));
            continue;
        }

        // Ordered list
        if is_ordered_list_item(line) {
            let mut items: Vec<serde_json::Value> = Vec::new();
            while i < lines.len() && is_ordered_list_item(lines[i]) {
                let item_text = strip_ordered_prefix(lines[i]);
                items.push(serde_json::json!({
                    "type": "listItem",
                    "content": [{
                        "type": "paragraph",
                        "content": parse_inline_marks(item_text)
                    }]
                }));
                i += 1;
            }
            blocks.push(serde_json::json!({
                "type": "orderedList",
                "content": items
            }));
            continue;
        }

        // Blank line — skip
        if line.trim().is_empty() {
            i += 1;
            continue;
        }

        // Paragraph — collect consecutive non-empty, non-special lines
        let mut para_text = String::new();
        while i < lines.len()
            && !lines[i].trim().is_empty()
            && !is_heading_line(lines[i])
            && !lines[i].starts_with("```")
            && !is_unordered_list_item(lines[i])
            && !is_ordered_list_item(lines[i])
        {
            if !para_text.is_empty() {
                para_text.push(' ');
            }
            para_text.push_str(lines[i]);
            i += 1;
        }
        if !para_text.is_empty() {
            blocks.push(serde_json::json!({
                "type": "paragraph",
                "content": parse_inline_marks(&para_text)
            }));
        }
    }

    serde_json::json!({
        "version": 1,
        "type": "doc",
        "content": blocks
    })
}

/// Check if a line is a CommonMark heading (starts with 1-6 `#` followed by a space or end of string).
fn is_heading_line(line: &str) -> bool {
    if !line.starts_with('#') {
        return false;
    }
    let level = line.chars().take_while(|c| *c == '#').count();
    if level > 6 {
        return false;
    }
    let rest = &line[level..];
    rest.is_empty() || rest.starts_with(' ')
}

/// Check if a line is an unordered list item (starts with `- ` or `* `).
fn is_unordered_list_item(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.starts_with("- ") || trimmed.starts_with("* ")
}

/// Strip the unordered list prefix from a line.
fn strip_unordered_prefix(line: &str) -> &str {
    let trimmed = line.trim_start();
    trimmed
        .strip_prefix("- ")
        .or_else(|| trimmed.strip_prefix("* "))
        .map(|s| s.trim_start())
        .unwrap_or(trimmed)
}

/// Check if a line is an ordered list item (starts with `N. `).
fn is_ordered_list_item(line: &str) -> bool {
    let trimmed = line.trim_start();
    let mut chars = trimmed.chars();
    // Must start with at least one digit
    match chars.next() {
        Some(c) if c.is_ascii_digit() => {}
        _ => return false,
    }
    // Skip remaining digits
    for c in chars.by_ref() {
        if c == '.' {
            // Must be followed by a space
            return chars.next() == Some(' ');
        }
        if !c.is_ascii_digit() {
            return false;
        }
    }
    false
}

/// Strip the ordered list prefix from a line.
fn strip_ordered_prefix(line: &str) -> &str {
    let trimmed = line.trim_start();
    if let Some(dot_pos) = trimmed.find(". ") {
        let prefix = &trimmed[..dot_pos];
        if prefix.chars().all(|c| c.is_ascii_digit()) {
            return trimmed[dot_pos + 2..].trim_start();
        }
    }
    trimmed
}

/// Parse inline markdown marks into ADF text nodes.
///
/// Handles: **bold**, *italic*, ***bold italic***, `code`, [text](url).
/// Unclosed marks are treated as literal text.
fn parse_inline_marks(text: &str) -> Vec<serde_json::Value> {
    let chars: Vec<char> = text.chars().collect();
    let len = chars.len();
    let mut result: Vec<serde_json::Value> = Vec::new();
    let mut buf = String::new();
    let mut i = 0;

    while i < len {
        // Backtick — inline code
        if chars[i] == '`' {
            if let Some(close) = find_char(&chars, '`', i + 1) {
                flush_text(&mut buf, &mut result);
                let content: String = chars[i + 1..close].iter().collect();
                result.push(serde_json::json!({
                    "type": "text",
                    "text": content,
                    "marks": [{ "type": "code" }]
                }));
                i = close + 1;
                continue;
            }
            // No closing backtick — literal
            buf.push('`');
            i += 1;
            continue;
        }

        // Link — [text](url)
        if chars[i] == '[' {
            if let Some(close_bracket) = find_char(&chars, ']', i + 1) {
                if close_bracket + 1 < len && chars[close_bracket + 1] == '(' {
                    if let Some(close_paren) = find_char(&chars, ')', close_bracket + 2) {
                        flush_text(&mut buf, &mut result);
                        let link_text: String = chars[i + 1..close_bracket].iter().collect();
                        let href: String = chars[close_bracket + 2..close_paren].iter().collect();
                        result.push(serde_json::json!({
                            "type": "text",
                            "text": link_text,
                            "marks": [{
                                "type": "link",
                                "attrs": { "href": href }
                            }]
                        }));
                        i = close_paren + 1;
                        continue;
                    }
                }
            }
            // Malformed link — literal
            buf.push('[');
            i += 1;
            continue;
        }

        // Bold+italic (***), bold (**), or italic (*)
        if chars[i] == '*' {
            // Count consecutive asterisks
            let star_count = chars[i..].iter().take_while(|c| **c == '*').count();

            if star_count >= 3 {
                // Try ***bold italic***
                if let Some(close) = find_sequence(&chars, "***", i + 3) {
                    flush_text(&mut buf, &mut result);
                    let content: String = chars[i + 3..close].iter().collect();
                    result.push(serde_json::json!({
                        "type": "text",
                        "text": content,
                        "marks": [{ "type": "strong" }, { "type": "em" }]
                    }));
                    i = close + 3;
                    continue;
                }
            }

            if star_count >= 2 {
                // Try **bold**
                if let Some(close) = find_sequence(&chars, "**", i + 2) {
                    flush_text(&mut buf, &mut result);
                    let content: String = chars[i + 2..close].iter().collect();
                    result.push(serde_json::json!({
                        "type": "text",
                        "text": content,
                        "marks": [{ "type": "strong" }]
                    }));
                    i = close + 2;
                    continue;
                }
            }

            // Try *italic* — closing must be single * not followed by *
            if let Some(close) = find_single_star_close(&chars, i + 1) {
                flush_text(&mut buf, &mut result);
                let content: String = chars[i + 1..close].iter().collect();
                result.push(serde_json::json!({
                    "type": "text",
                    "text": content,
                    "marks": [{ "type": "em" }]
                }));
                i = close + 1;
                continue;
            }

            // Unclosed — treat stars as literal
            for _ in 0..star_count {
                buf.push('*');
            }
            i += star_count;
            continue;
        }

        // Regular character
        buf.push(chars[i]);
        i += 1;
    }

    flush_text(&mut buf, &mut result);
    result
}

/// Flush accumulated plain text into a text node.
fn flush_text(buf: &mut String, result: &mut Vec<serde_json::Value>) {
    if !buf.is_empty() {
        result.push(serde_json::json!({ "type": "text", "text": buf.clone() }));
        buf.clear();
    }
}

/// Find the first occurrence of `target` in `chars` starting at index `from`.
fn find_char(chars: &[char], target: char, from: usize) -> Option<usize> {
    (from..chars.len()).find(|&j| chars[j] == target)
}

/// Find the first occurrence of a multi-character sequence starting at `from`.
fn find_sequence(chars: &[char], seq: &str, from: usize) -> Option<usize> {
    let seq_chars: Vec<char> = seq.chars().collect();
    let seq_len = seq_chars.len();
    if chars.len() < from + seq_len {
        return None;
    }
    (from..=chars.len() - seq_len).find(|&j| chars[j..j + seq_len] == seq_chars[..])
}

/// Find closing single `*` that is NOT part of `**`.
fn find_single_star_close(chars: &[char], from: usize) -> Option<usize> {
    let len = chars.len();
    for j in from..len {
        if chars[j] == '*' {
            // Check it's a single star (not preceded or followed by *)
            let preceded_by_star = j > 0 && chars[j - 1] == '*';
            let followed_by_star = j + 1 < len && chars[j + 1] == '*';
            if !preceded_by_star && !followed_by_star {
                return Some(j);
            }
        }
    }
    None
}

/// Representation of a single sprint from the Jira Agile API
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct JiraSprintResponse {
    pub id: u64,
    pub name: String,
    pub state: String,
    #[serde(rename = "startDate", default)]
    pub start_date: Option<String>,
    #[serde(rename = "endDate", default)]
    pub end_date: Option<String>,
    #[serde(rename = "completeDate", default)]
    pub complete_date: Option<String>,
}

/// Container for a paginated list of sprints
#[derive(Debug, Deserialize, Clone)]
pub struct JiraSprintListResponse {
    #[serde(rename = "maxResults")]
    pub max_results: u64,
    #[serde(rename = "startAt")]
    pub start_at: u64,
    #[serde(rename = "isLast")]
    pub is_last: bool,
    pub values: Vec<JiraSprintResponse>,
}

/// Output structure for a sprint after transformation
#[derive(Debug, Serialize, Clone, PartialEq)]
pub struct SprintOutput {
    pub id: u64,
    pub name: String,
    pub state: String,
    pub start_date: Option<String>,
    pub end_date: Option<String>,
    pub complete_date: Option<String>,
}

/// Output structure for a list of sprints
#[derive(Debug, Serialize, Clone, PartialEq)]
pub struct SprintListOutput {
    pub sprints: Vec<SprintOutput>,
    pub total: usize,
    pub has_more: bool,
}

/// Convert raw sprint list response to the clean domain model.
pub fn transform_sprint_list_response(response: JiraSprintListResponse) -> SprintListOutput {
    let sprints: Vec<SprintOutput> = response
        .values
        .into_iter()
        .map(|s| SprintOutput {
            id: s.id,
            name: s.name,
            state: s.state,
            start_date: s.start_date,
            end_date: s.end_date,
            complete_date: s.complete_date,
        })
        .collect();

    let total = sprints.len();

    SprintListOutput {
        sprints,
        total,
        has_more: !response.is_last,
    }
}

/// Find a sprint by name (case-insensitive) in the provided list.
/// Returns the sprint ID if found.
pub fn find_sprint_by_name(sprints: &[JiraSprintResponse], target_name: &str) -> Option<u64> {
    sprints
        .iter()
        .find(|s| s.name.eq_ignore_ascii_case(target_name))
        .map(|s| s.id)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Helper to create a basic issue response for testing
    fn create_issue_response(
        key: &str,
        summary: &str,
        status: &str,
        assignee: Option<JiraAssignee>,
    ) -> JiraIssueResponse {
        JiraIssueResponse {
            key: key.to_string(),
            fields: JiraIssueFields {
                summary: summary.to_string(),
                description: None,
                status: JiraStatus {
                    name: status.to_string(),
                },
                assignee,
            },
        }
    }

    #[test]
    fn test_transform_search_response_basic() {
        // Arrange: Create a basic search response with one issue
        let response = JiraSearchResponse {
            issues: vec![create_issue_response(
                "PROJ-123",
                "Fix bug in authentication",
                "In Progress",
                Some(JiraAssignee {
                    display_name: Some("John Doe".to_string()),
                    email_address: Some("john@example.com".to_string()),
                }),
            )],
            total: Some(1),
            is_last: Some(true),
            next_page_token: None,
            start_at: Some(0),
            max_results: Some(10),
        };

        // Act: Transform the response
        let output = transform_search_response(response);

        // Assert: Verify the transformation
        assert_eq!(output.total, 1);
        assert_eq!(output.issues.len(), 1);
        assert_eq!(output.next_page_token, None);

        let issue = &output.issues[0];
        assert_eq!(issue.key, "PROJ-123");
        assert_eq!(issue.summary, "Fix bug in authentication");
        assert_eq!(issue.status, "In Progress");
        assert_eq!(issue.assignee, Some("John Doe".to_string()));
        assert_eq!(issue.description, None);
    }

    #[test]
    fn test_transform_search_response_empty() {
        // Arrange: Create an empty search response
        let response = JiraSearchResponse {
            issues: vec![],
            total: Some(0),
            is_last: Some(true),
            next_page_token: None,
            start_at: Some(0),
            max_results: Some(10),
        };

        // Act: Transform the response
        let output = transform_search_response(response);

        // Assert: Verify empty results
        assert_eq!(output.total, 0);
        assert_eq!(output.issues.len(), 0);
        assert_eq!(output.next_page_token, None);
    }

    #[test]
    fn test_transform_search_response_multiple_issues() {
        // Arrange: Create a search response with multiple issues
        let response = JiraSearchResponse {
            issues: vec![
                create_issue_response(
                    "PROJ-1",
                    "First issue",
                    "Open",
                    Some(JiraAssignee {
                        display_name: Some("Alice".to_string()),
                        email_address: None,
                    }),
                ),
                create_issue_response(
                    "PROJ-2",
                    "Second issue",
                    "Done",
                    Some(JiraAssignee {
                        display_name: Some("Bob".to_string()),
                        email_address: None,
                    }),
                ),
                create_issue_response("PROJ-3", "Third issue", "Closed", None),
            ],
            total: Some(3),
            is_last: Some(true),
            next_page_token: None,
            start_at: Some(0),
            max_results: Some(10),
        };

        // Act: Transform the response
        let output = transform_search_response(response);

        // Assert: Verify all issues transformed correctly
        assert_eq!(output.total, 3);
        assert_eq!(output.issues.len(), 3);

        // Check first issue
        assert_eq!(output.issues[0].key, "PROJ-1");
        assert_eq!(output.issues[0].summary, "First issue");
        assert_eq!(output.issues[0].status, "Open");
        assert_eq!(output.issues[0].assignee, Some("Alice".to_string()));

        // Check second issue
        assert_eq!(output.issues[1].key, "PROJ-2");
        assert_eq!(output.issues[1].summary, "Second issue");
        assert_eq!(output.issues[1].status, "Done");

        // Check third issue (no assignee)
        assert_eq!(output.issues[2].key, "PROJ-3");
        assert_eq!(output.issues[2].assignee, None);
    }

    #[test]
    fn test_transform_search_response_missing_assignee() {
        // Arrange: Create an issue without assignee
        let response = JiraSearchResponse {
            issues: vec![create_issue_response(
                "PROJ-456",
                "Unassigned issue",
                "To Do",
                None, // No assignee
            )],
            total: Some(1),
            is_last: Some(true),
            next_page_token: None,
            start_at: Some(0),
            max_results: Some(10),
        };

        // Act: Transform the response
        let output = transform_search_response(response);

        // Assert: Verify issue with no assignee
        assert_eq!(output.issues.len(), 1);
        let issue = &output.issues[0];
        assert_eq!(issue.key, "PROJ-456");
        assert_eq!(issue.assignee, None);
    }

    #[test]
    fn test_transform_search_response_assignee_with_display_name() {
        // Arrange: Create an issue with assignee having displayName
        let response = JiraSearchResponse {
            issues: vec![create_issue_response(
                "PROJ-789",
                "Issue with display name",
                "In Review",
                Some(JiraAssignee {
                    display_name: Some("Jane Smith".to_string()),
                    email_address: Some("jane@example.com".to_string()),
                }),
            )],
            total: Some(1),
            is_last: Some(true),
            next_page_token: None,
            start_at: Some(0),
            max_results: Some(10),
        };

        // Act: Transform the response
        let output = transform_search_response(response);

        // Assert: Verify displayName is preferred over emailAddress
        assert_eq!(output.issues.len(), 1);
        assert_eq!(
            output.issues[0].assignee,
            Some("Jane Smith".to_string()),
            "Should use displayName when available"
        );
    }

    #[test]
    fn test_transform_search_response_assignee_email_only() {
        // Arrange: Create an issue with assignee having only emailAddress
        let response = JiraSearchResponse {
            issues: vec![create_issue_response(
                "PROJ-999",
                "Issue with email only",
                "Blocked",
                Some(JiraAssignee {
                    display_name: None,
                    email_address: Some("user@example.com".to_string()),
                }),
            )],
            total: Some(1),
            is_last: Some(true),
            next_page_token: None,
            start_at: Some(0),
            max_results: Some(10),
        };

        // Act: Transform the response
        let output = transform_search_response(response);

        // Assert: Verify emailAddress is used as fallback
        assert_eq!(output.issues.len(), 1);
        assert_eq!(
            output.issues[0].assignee,
            Some("user@example.com".to_string()),
            "Should use emailAddress when displayName is not available"
        );
    }

    #[test]
    fn test_transform_search_response_with_pagination() {
        // Arrange: Create a search response with pagination token
        let response = JiraSearchResponse {
            issues: vec![
                create_issue_response("PROJ-1", "First", "Open", None),
                create_issue_response("PROJ-2", "Second", "Open", None),
            ],
            total: Some(100), // More results available
            is_last: Some(false),
            next_page_token: Some("pagination-token-abc123".to_string()),
            start_at: Some(0),
            max_results: Some(2),
        };

        // Act: Transform the response
        let output = transform_search_response(response);

        // Assert: Verify pagination info is preserved
        assert_eq!(output.total, 100);
        assert_eq!(output.issues.len(), 2);
        assert_eq!(
            output.next_page_token,
            Some("pagination-token-abc123".to_string())
        );
    }

    #[test]
    fn test_transform_search_response_total_missing() {
        // Arrange: Create a response without total field (edge case)
        let response = JiraSearchResponse {
            issues: vec![create_issue_response("PROJ-1", "Test", "Open", None)],
            total: None, // Missing total
            is_last: Some(true),
            next_page_token: None,
            start_at: Some(0),
            max_results: Some(10),
        };

        // Act: Transform the response
        let output = transform_search_response(response);

        // Assert: Verify default value of 0 for missing total
        assert_eq!(output.total, 0);
        assert_eq!(output.issues.len(), 1);
    }

    // Helper to create an extended issue response for testing
    fn create_extended_issue_response(
        key: &str,
        summary: &str,
        status: &str,
        priority: Option<String>,
    ) -> JiraExtendedIssueResponse {
        JiraExtendedIssueResponse {
            key: key.to_string(),
            fields: JiraExtendedFields {
                summary: summary.to_string(),
                description: Some(serde_json::Value::String("Test description".to_string())),
                status: JiraStatus {
                    name: status.to_string(),
                },
                assignee: Some(JiraAssignee {
                    display_name: Some("John Doe".to_string()),
                    email_address: Some("john@example.com".to_string()),
                }),
                priority: priority.map(|name| JiraPriority { name }),
                issuetype: Some(JiraIssueType {
                    name: "Story".to_string(),
                }),
                created: Some("2024-01-01T10:00:00Z".to_string()),
                updated: Some("2024-01-02T10:00:00Z".to_string()),
                duedate: Some("2024-01-15".to_string()),
                labels: vec!["backend".to_string(), "api".to_string()],
                components: vec![
                    JiraComponent {
                        name: "Auth".to_string(),
                    },
                    JiraComponent {
                        name: "API".to_string(),
                    },
                ],
            },
        }
    }

    #[test]
    fn test_transform_ticket_response_full() {
        // Arrange: Create a full ticket with all fields
        let issue = create_extended_issue_response(
            "PROJ-456",
            "Implement authentication",
            "In Progress",
            Some("High".to_string()),
        );

        let comments = vec![JiraComment {
            comment_id: "1".to_string(),
            body: serde_json::Value::String("Great work!".to_string()),
            created_at: "2024-01-01T12:00:00Z".to_string(),
            author: Some(JiraAssignee {
                display_name: Some("Jane".to_string()),
                email_address: None,
            }),
        }];

        // Act: Transform the ticket
        let output = transform_ticket_response(issue, comments.clone(), vec![]);

        // Assert: Verify all fields are transformed correctly
        assert_eq!(output.key, "PROJ-456");
        assert_eq!(output.summary, "Implement authentication");
        assert_eq!(output.description, Some("Test description".to_string()));
        assert_eq!(output.status, "In Progress");
        assert_eq!(output.priority, Some("High".to_string()));
        assert_eq!(output.issue_type, Some("Story".to_string()));
        assert_eq!(output.assignee, Some("John Doe".to_string()));
        assert_eq!(output.created, Some("2024-01-01T10:00:00Z".to_string()));
        assert_eq!(output.updated, Some("2024-01-02T10:00:00Z".to_string()));
        assert_eq!(output.due_date, Some("2024-01-15".to_string()));
        assert_eq!(output.labels, vec!["backend", "api"]);
        assert_eq!(output.components, vec!["Auth", "API"]);
        assert_eq!(output.comments.len(), 1);
    }

    #[test]
    fn test_transform_ticket_response_minimal() {
        // Arrange: Create a minimal ticket with only required fields
        let issue = JiraExtendedIssueResponse {
            key: "PROJ-789".to_string(),
            fields: JiraExtendedFields {
                summary: "Minimal ticket".to_string(),
                description: None,
                status: JiraStatus {
                    name: "Open".to_string(),
                },
                assignee: None,
                priority: None,
                issuetype: None,
                created: None,
                updated: None,
                duedate: None,
                labels: vec![],
                components: vec![],
            },
        };

        // Act: Transform the ticket
        let output = transform_ticket_response(issue, vec![], vec![]);

        // Assert: Verify minimal fields work correctly
        assert_eq!(output.key, "PROJ-789");
        assert_eq!(output.summary, "Minimal ticket");
        assert_eq!(output.description, None);
        assert_eq!(output.status, "Open");
        assert_eq!(output.priority, None);
        assert_eq!(output.issue_type, None);
        assert_eq!(output.assignee, None);
        assert_eq!(output.created, None);
        assert_eq!(output.updated, None);
        assert_eq!(output.due_date, None);
        assert!(output.labels.is_empty());
        assert!(output.components.is_empty());
        assert!(output.comments.is_empty());
    }

    #[test]
    fn test_transform_ticket_response_with_comments() {
        // Arrange: Create a ticket with multiple comments
        let issue = create_extended_issue_response("PROJ-200", "Comment test", "In Review", None);

        let comments = vec![
            JiraComment {
                comment_id: "1".to_string(),
                body: serde_json::Value::String("First comment".to_string()),
                created_at: "2024-01-01T10:00:00Z".to_string(),
                author: Some(JiraAssignee {
                    display_name: Some("Alice".to_string()),
                    email_address: None,
                }),
            },
            JiraComment {
                comment_id: "2".to_string(),
                body: serde_json::Value::String("Second comment".to_string()),
                created_at: "2024-01-02T10:00:00Z".to_string(),
                author: Some(JiraAssignee {
                    display_name: Some("Bob".to_string()),
                    email_address: None,
                }),
            },
        ];

        // Act: Transform the ticket
        let output = transform_ticket_response(issue, comments, vec![]);

        // Assert: Verify comments are preserved
        assert_eq!(output.comments.len(), 2);
        assert_eq!(output.comments[0].comment_id, "1");
        assert_eq!(output.comments[1].comment_id, "2");
    }

    #[test]
    fn test_transform_ticket_response_empty_priority() {
        // Arrange: Create a ticket with empty priority string
        let issue = JiraExtendedIssueResponse {
            key: "PROJ-300".to_string(),
            fields: JiraExtendedFields {
                summary: "Empty priority test".to_string(),
                description: None,
                status: JiraStatus {
                    name: "Open".to_string(),
                },
                assignee: None,
                priority: Some(JiraPriority {
                    name: "".to_string(), // Empty priority name
                }),
                issuetype: None,
                created: None,
                updated: None,
                duedate: None,
                labels: vec![],
                components: vec![],
            },
        };

        // Act: Transform the ticket
        let output = transform_ticket_response(issue, vec![], vec![]);

        // Assert: Verify empty priority is filtered out
        assert_eq!(output.priority, None);
    }

    #[test]
    fn test_extract_description_string() {
        // Arrange: Create a simple string description
        let value = Some(serde_json::Value::String(
            "This is a plain text description".to_string(),
        ));

        // Act: Extract description
        let result = extract_description(value);

        // Assert: Verify string is returned as-is
        assert_eq!(result, Some("This is a plain text description".to_string()));
    }

    #[test]
    fn test_extract_description_adf_simple() {
        // Arrange: Create a simple ADF document
        let adf = serde_json::json!({
            "type": "doc",
            "content": [
                {
                    "type": "paragraph",
                    "content": [
                        {
                            "type": "text",
                            "text": "Hello world"
                        }
                    ]
                }
            ]
        });

        // Act: Extract description
        let result = extract_description(Some(adf));

        // Assert: Verify ADF is rendered to text
        assert_eq!(result, Some("Hello world".to_string()));
    }

    #[test]
    fn test_extract_description_adf_with_heading() {
        // Arrange: Create an ADF document with heading
        let adf = serde_json::json!({
            "type": "doc",
            "content": [
                {
                    "type": "heading",
                    "attrs": {"level": 2},
                    "content": [
                        {
                            "type": "text",
                            "text": "Important"
                        }
                    ]
                },
                {
                    "type": "paragraph",
                    "content": [
                        {
                            "type": "text",
                            "text": "This is important info"
                        }
                    ]
                }
            ]
        });

        // Act: Extract description
        let result = extract_description(Some(adf));

        // Assert: Verify heading is rendered with markdown
        let expected = "## Important\nThis is important info";
        assert_eq!(result, Some(expected.to_string()));
    }

    #[test]
    fn test_extract_description_adf_with_list() {
        // Arrange: Create an ADF document with bullet list
        let adf = serde_json::json!({
            "type": "doc",
            "content": [
                {
                    "type": "bulletList",
                    "content": [
                        {
                            "type": "listItem",
                            "content": [
                                {
                                    "type": "paragraph",
                                    "content": [
                                        {
                                            "type": "text",
                                            "text": "First item"
                                        }
                                    ]
                                }
                            ]
                        },
                        {
                            "type": "listItem",
                            "content": [
                                {
                                    "type": "paragraph",
                                    "content": [
                                        {
                                            "type": "text",
                                            "text": "Second item"
                                        }
                                    ]
                                }
                            ]
                        }
                    ]
                }
            ]
        });

        // Act: Extract description
        let result = extract_description(Some(adf));

        // Assert: Verify list is rendered with bullets (note: first item has no indent, nested items do)
        let expected = "• First item\n  • Second item";
        assert_eq!(result, Some(expected.to_string()));
    }

    #[test]
    fn test_extract_description_none() {
        // Arrange: None value
        let value = None;

        // Act: Extract description
        let result = extract_description(value);

        // Assert: Verify None is returned
        assert_eq!(result, None);
    }

    #[test]
    fn test_extract_description_non_adf_object() {
        // Arrange: Create a non-ADF object
        let value = Some(serde_json::json!({"foo": "bar"}));

        // Act: Extract description
        let result = extract_description(value);

        // Assert: Verify None is returned for non-ADF objects
        assert_eq!(result, None);
    }

    // Tests for parse_assignee_identifier
    #[test]
    fn test_parse_assignee_identifier_email() {
        // Arrange: Email string
        let input = "user@example.com";

        // Act: Parse identifier
        let result = parse_assignee_identifier(input);

        // Assert: Verify email is identified
        assert_eq!(
            result,
            AssigneeIdentifier::Email("user@example.com".to_string())
        );
    }

    #[test]
    fn test_parse_assignee_identifier_account_id() {
        // Arrange: Long alphanumeric string that looks like Jira accountId
        let input = "5b10a2844c20165700edge21g";

        // Act: Parse identifier
        let result = parse_assignee_identifier(input);

        // Assert: Verify accountId is identified
        assert_eq!(result, AssigneeIdentifier::AccountId(input.to_string()));
    }

    #[test]
    fn test_parse_assignee_identifier_display_name() {
        // Arrange: Display name with spaces
        let input = "John Doe";

        // Act: Parse identifier
        let result = parse_assignee_identifier(input);

        // Assert: Verify display name is identified
        assert_eq!(
            result,
            AssigneeIdentifier::DisplayName("John Doe".to_string())
        );
    }

    #[test]
    fn test_parse_assignee_identifier_single_word_name() {
        // Arrange: Single word display name
        let input = "Alice";

        // Act: Parse identifier
        let result = parse_assignee_identifier(input);

        // Assert: Verify display name is identified
        assert_eq!(result, AssigneeIdentifier::DisplayName("Alice".to_string()));
    }

    #[test]
    fn test_parse_assignee_identifier_me() {
        // Arrange: Special "me" keyword
        let input = "me";

        // Act: Parse identifier
        let result = parse_assignee_identifier(input);

        // Assert: Verify current user is identified
        assert_eq!(result, AssigneeIdentifier::CurrentUser);
    }

    #[test]
    fn test_parse_assignee_identifier_me_uppercase() {
        // Arrange: Special "ME" keyword (case-insensitive)
        let input = "ME";

        // Act: Parse identifier
        let result = parse_assignee_identifier(input);

        // Assert: Verify current user is identified (case-insensitive)
        assert_eq!(result, AssigneeIdentifier::CurrentUser);
    }

    #[test]
    fn test_parse_assignee_identifier_me_mixed_case() {
        // Arrange: Special "Me" keyword (case-insensitive)
        let input = "Me";

        // Act: Parse identifier
        let result = parse_assignee_identifier(input);

        // Assert: Verify current user is identified (case-insensitive)
        assert_eq!(result, AssigneeIdentifier::CurrentUser);
    }

    // Tests for find_transition_by_status
    #[test]
    fn test_find_transition_by_status_found() {
        // Arrange: Create transitions
        let transitions = vec![
            JiraTransition {
                id: "1".to_string(),
                name: "Start Progress".to_string(),
                to: JiraStatus {
                    name: "In Progress".to_string(),
                },
            },
            JiraTransition {
                id: "2".to_string(),
                name: "Done".to_string(),
                to: JiraStatus {
                    name: "Done".to_string(),
                },
            },
        ];

        // Act: Find transition
        let result = find_transition_by_status(&transitions, "In Progress");

        // Assert: Verify correct transition ID is returned
        assert_eq!(result, Some("1".to_string()));
    }

    #[test]
    fn test_find_transition_by_status_case_insensitive() {
        // Arrange: Create transitions
        let transitions = vec![JiraTransition {
            id: "3".to_string(),
            name: "Close".to_string(),
            to: JiraStatus {
                name: "Done".to_string(),
            },
        }];

        // Act: Find transition with different case
        let result = find_transition_by_status(&transitions, "DONE");

        // Assert: Verify case-insensitive match works
        assert_eq!(result, Some("3".to_string()));
    }

    #[test]
    fn test_find_transition_by_status_not_found() {
        // Arrange: Create transitions
        let transitions = vec![JiraTransition {
            id: "1".to_string(),
            name: "Start".to_string(),
            to: JiraStatus {
                name: "In Progress".to_string(),
            },
        }];

        // Act: Find non-existent transition
        let result = find_transition_by_status(&transitions, "Blocked");

        // Assert: Verify None is returned
        assert_eq!(result, None);
    }

    #[test]
    fn test_find_transition_by_status_empty_list() {
        // Arrange: Empty transitions list
        let transitions = vec![];

        // Act: Find transition
        let result = find_transition_by_status(&transitions, "Done");

        // Assert: Verify None is returned
        assert_eq!(result, None);
    }

    // Tests for build_update_payload
    #[test]
    fn test_build_update_payload_all_fields() {
        // Arrange: All fields provided
        let payload = build_update_payload(
            Some("High"),
            Some("Story"),
            Some("5b10a2844c20165700edge21g"),
            None,
        );

        // Assert: Verify all fields are in payload
        assert_eq!(payload["priority"]["name"], "High");
        assert_eq!(payload["issuetype"]["name"], "Story");
        assert_eq!(payload["assignee"]["id"], "5b10a2844c20165700edge21g");
    }

    #[test]
    fn test_build_update_payload_single_field() {
        // Arrange: Only priority provided
        let payload = build_update_payload(Some("Low"), None, None, None);

        // Assert: Verify only priority is in payload
        assert_eq!(payload["priority"]["name"], "Low");
        assert!(payload.get("issuetype").is_none());
        assert!(payload.get("assignee").is_none());
    }

    #[test]
    fn test_build_update_payload_empty() {
        // Arrange: No fields provided
        let payload = build_update_payload(None, None, None, None);

        // Assert: Verify payload is empty object
        assert_eq!(payload, serde_json::json!({}));
    }

    #[test]
    fn test_build_update_payload_with_assignee() {
        // Arrange: Assignee account ID provided
        let payload = build_update_payload(None, None, Some("account123"), None);

        // Assert: Verify assignee is in payload with id field
        assert_eq!(payload["assignee"]["id"], "account123");
    }

    #[test]
    fn test_build_update_payload_with_description() {
        // Arrange: Description ADF value provided
        let adf = serde_json::json!({
            "version": 1,
            "type": "doc",
            "content": [{
                "type": "paragraph",
                "content": [{ "type": "text", "text": "Updated description" }]
            }]
        });
        let payload = build_update_payload(None, None, None, Some(&adf));

        // Assert: Verify description is in payload
        assert_eq!(payload["description"]["type"], "doc");
        assert_eq!(payload["description"]["version"], 1);
        assert_eq!(
            payload["description"]["content"][0]["content"][0]["text"],
            "Updated description"
        );
    }

    // Tests for format_file_size
    #[test]
    fn test_format_file_size_zero() {
        assert_eq!(format_file_size(0), "0 B");
    }

    #[test]
    fn test_format_file_size_bytes() {
        assert_eq!(format_file_size(500), "500 B");
        assert_eq!(format_file_size(1), "1 B");
        assert_eq!(format_file_size(1023), "1023 B");
    }

    #[test]
    fn test_format_file_size_kilobytes() {
        assert_eq!(format_file_size(1024), "1.0 KB");
        assert_eq!(format_file_size(1536), "1.5 KB");
        assert_eq!(format_file_size(10240), "10.0 KB");
    }

    #[test]
    fn test_format_file_size_megabytes() {
        assert_eq!(format_file_size(1048576), "1.0 MB");
        assert_eq!(format_file_size(5242880), "5.0 MB");
    }

    #[test]
    fn test_format_file_size_gigabytes() {
        assert_eq!(format_file_size(1073741824), "1.0 GB");
        assert_eq!(format_file_size(2147483648), "2.0 GB");
    }

    // Tests for transform_attachment_response
    #[test]
    fn test_transform_attachment_response_empty() {
        let result = transform_attachment_response(vec![]);
        assert!(result.is_empty());
    }

    #[test]
    fn test_transform_attachment_response_single() {
        let raw = vec![JiraAttachmentResponse {
            id: "12345".to_string(),
            filename: "report.pdf".to_string(),
            mime_type: "application/pdf".to_string(),
            size: 2048,
            created: "2024-01-15T10:30:00Z".to_string(),
            content: "https://example.com/attachments/12345".to_string(),
        }];

        let result = transform_attachment_response(raw);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].id, "12345");
        assert_eq!(result[0].filename, "report.pdf");
        assert_eq!(result[0].mime_type, "application/pdf");
        assert_eq!(result[0].size_bytes, 2048);
        assert_eq!(result[0].size_human, "2.0 KB");
        assert_eq!(result[0].created, "2024-01-15T10:30:00Z");
    }

    #[test]
    fn test_transform_attachment_response_multiple() {
        let raw = vec![
            JiraAttachmentResponse {
                id: "1".to_string(),
                filename: "small.txt".to_string(),
                mime_type: "text/plain".to_string(),
                size: 100,
                created: "2024-01-01T00:00:00Z".to_string(),
                content: "https://example.com/1".to_string(),
            },
            JiraAttachmentResponse {
                id: "2".to_string(),
                filename: "large.zip".to_string(),
                mime_type: "application/zip".to_string(),
                size: 5242880,
                created: "2024-01-02T00:00:00Z".to_string(),
                content: "https://example.com/2".to_string(),
            },
        ];

        let result = transform_attachment_response(raw);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].size_human, "100 B");
        assert_eq!(result[1].size_human, "5.0 MB");
    }

    // Tests for transform_ticket_response with attachments
    #[test]
    fn test_transform_ticket_response_with_attachments() {
        let issue = create_extended_issue_response("PROJ-500", "Attachment test", "Open", None);

        let attachments = vec![AttachmentOutput {
            id: "att-1".to_string(),
            filename: "screenshot.png".to_string(),
            mime_type: "image/png".to_string(),
            size_bytes: 4096,
            size_human: "4.0 KB".to_string(),
            created: "2024-06-01T12:00:00Z".to_string(),
        }];

        let output = transform_ticket_response(issue, vec![], attachments);
        assert_eq!(output.attachments.len(), 1);
        assert_eq!(output.attachments[0].filename, "screenshot.png");
    }

    // Tests for markdown_to_adf

    #[test]
    fn test_markdown_to_adf_empty_input() {
        let result = markdown_to_adf("");
        assert_eq!(result["version"], 1);
        assert_eq!(result["type"], "doc");
        assert_eq!(result["content"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn test_markdown_to_adf_single_paragraph() {
        let result = markdown_to_adf("Hello world");
        let content = result["content"].as_array().unwrap();
        assert_eq!(content.len(), 1);
        assert_eq!(content[0]["type"], "paragraph");
        assert_eq!(content[0]["content"][0]["text"], "Hello world");
    }

    #[test]
    fn test_markdown_to_adf_heading() {
        let result = markdown_to_adf("## My Heading");
        let content = result["content"].as_array().unwrap();
        assert_eq!(content[0]["type"], "heading");
        assert_eq!(content[0]["attrs"]["level"], 2);
        assert_eq!(content[0]["content"][0]["text"], "My Heading");
    }

    #[test]
    fn test_markdown_to_adf_code_block() {
        let result = markdown_to_adf("```rust\nfn main() {}\n```");
        let content = result["content"].as_array().unwrap();
        assert_eq!(content[0]["type"], "codeBlock");
        assert_eq!(content[0]["attrs"]["language"], "rust");
        assert_eq!(content[0]["content"][0]["text"], "fn main() {}");
    }

    #[test]
    fn test_markdown_to_adf_code_block_no_language() {
        let result = markdown_to_adf("```\nsome code\n```");
        let content = result["content"].as_array().unwrap();
        assert_eq!(content[0]["type"], "codeBlock");
        assert!(content[0].get("attrs").is_none());
    }

    #[test]
    fn test_markdown_to_adf_bullet_list() {
        let result = markdown_to_adf("- first\n- second\n- third");
        let content = result["content"].as_array().unwrap();
        assert_eq!(content.len(), 1);
        assert_eq!(content[0]["type"], "bulletList");
        let items = content[0]["content"].as_array().unwrap();
        assert_eq!(items.len(), 3);
        assert_eq!(items[0]["content"][0]["content"][0]["text"], "first");
    }

    #[test]
    fn test_markdown_to_adf_ordered_list() {
        let result = markdown_to_adf("1. first\n2. second");
        let content = result["content"].as_array().unwrap();
        assert_eq!(content[0]["type"], "orderedList");
        let items = content[0]["content"].as_array().unwrap();
        assert_eq!(items.len(), 2);
    }

    #[test]
    fn test_markdown_to_adf_mixed_content() {
        let md =
            "# Title\n\nA paragraph with **bold**.\n\n- item one\n- item two\n\n```\ncode\n```";
        let result = markdown_to_adf(md);
        let content = result["content"].as_array().unwrap();
        assert_eq!(content[0]["type"], "heading");
        assert_eq!(content[1]["type"], "paragraph");
        assert_eq!(content[2]["type"], "bulletList");
        assert_eq!(content[3]["type"], "codeBlock");
    }

    #[test]
    fn test_markdown_to_adf_two_paragraphs() {
        let result = markdown_to_adf("First paragraph.\n\nSecond paragraph.");
        let content = result["content"].as_array().unwrap();
        assert_eq!(content.len(), 2);
        assert_eq!(content[0]["type"], "paragraph");
        assert_eq!(content[1]["type"], "paragraph");
    }

    // Tests for parse_inline_marks (via markdown_to_adf)

    #[test]
    fn test_inline_bold_in_paragraph() {
        let result = markdown_to_adf("hello **bold** world");
        let inlines = result["content"][0]["content"].as_array().unwrap();
        assert_eq!(inlines.len(), 3);
        assert_eq!(inlines[0]["text"], "hello ");
        assert_eq!(inlines[1]["text"], "bold");
        assert_eq!(inlines[1]["marks"][0]["type"], "strong");
        assert_eq!(inlines[2]["text"], " world");
    }

    #[test]
    fn test_inline_italic_in_paragraph() {
        let result = markdown_to_adf("hello *italic* world");
        let inlines = result["content"][0]["content"].as_array().unwrap();
        assert_eq!(inlines[1]["text"], "italic");
        assert_eq!(inlines[1]["marks"][0]["type"], "em");
    }

    #[test]
    fn test_inline_bold_italic_in_paragraph() {
        let result = markdown_to_adf("***both***");
        let inlines = result["content"][0]["content"].as_array().unwrap();
        assert_eq!(inlines.len(), 1);
        assert_eq!(inlines[0]["text"], "both");
        assert_eq!(inlines[0]["marks"][0]["type"], "strong");
        assert_eq!(inlines[0]["marks"][1]["type"], "em");
    }

    #[test]
    fn test_inline_code_in_paragraph() {
        let result = markdown_to_adf("use `println!` macro");
        let inlines = result["content"][0]["content"].as_array().unwrap();
        assert_eq!(inlines.len(), 3);
        assert_eq!(inlines[1]["text"], "println!");
        assert_eq!(inlines[1]["marks"][0]["type"], "code");
    }

    #[test]
    fn test_inline_link_in_paragraph() {
        let result = markdown_to_adf("click [here](https://example.com) now");
        let inlines = result["content"][0]["content"].as_array().unwrap();
        assert_eq!(inlines.len(), 3);
        assert_eq!(inlines[1]["text"], "here");
        assert_eq!(inlines[1]["marks"][0]["type"], "link");
        assert_eq!(
            inlines[1]["marks"][0]["attrs"]["href"],
            "https://example.com"
        );
    }

    #[test]
    fn test_inline_unclosed_bold() {
        let result = markdown_to_adf("hello **unclosed");
        let inlines = result["content"][0]["content"].as_array().unwrap();
        assert_eq!(inlines.len(), 1);
        assert_eq!(inlines[0]["text"], "hello **unclosed");
    }

    #[test]
    fn test_inline_unclosed_backtick() {
        let result = markdown_to_adf("hello `unclosed");
        let inlines = result["content"][0]["content"].as_array().unwrap();
        assert_eq!(inlines.len(), 1);
        assert_eq!(inlines[0]["text"], "hello `unclosed");
    }

    #[test]
    fn test_markdown_to_adf_heading_with_inline_marks() {
        let result = markdown_to_adf("# Title with **bold**");
        let content = result["content"].as_array().unwrap();
        assert_eq!(content[0]["type"], "heading");
        let inlines = content[0]["content"].as_array().unwrap();
        assert_eq!(inlines.len(), 2);
        assert_eq!(inlines[0]["text"], "Title with ");
        assert_eq!(inlines[1]["text"], "bold");
        assert_eq!(inlines[1]["marks"][0]["type"], "strong");
    }

    #[test]
    fn test_markdown_to_adf_star_bullet_list() {
        let result = markdown_to_adf("* alpha\n* beta");
        let content = result["content"].as_array().unwrap();
        assert_eq!(content[0]["type"], "bulletList");
        let items = content[0]["content"].as_array().unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0]["content"][0]["content"][0]["text"], "alpha");
    }

    #[test]
    fn test_markdown_to_adf_hashtag_is_paragraph_not_heading() {
        // CommonMark requires a space after # for headings; #hashtag should be a paragraph
        let result = markdown_to_adf("#hashtag");
        let content = result["content"].as_array().unwrap();
        assert_eq!(content.len(), 1);
        assert_eq!(content[0]["type"], "paragraph");
        assert_eq!(content[0]["content"][0]["text"], "#hashtag");
    }

    #[test]
    fn test_transform_sprint_list_response_basic() {
        let response = JiraSprintListResponse {
            max_results: 50,
            start_at: 0,
            is_last: true,
            values: vec![JiraSprintResponse {
                id: 42,
                name: "Sprint 30".to_string(),
                state: "active".to_string(),
                start_date: Some("2026-02-10T00:00:00.000Z".to_string()),
                end_date: Some("2026-02-24T00:00:00.000Z".to_string()),
                complete_date: None,
            }],
        };
        let output = transform_sprint_list_response(response);
        assert_eq!(output.sprints.len(), 1);
        assert_eq!(output.total, 1);
        assert!(!output.has_more);
        assert_eq!(output.sprints[0].id, 42);
        assert_eq!(output.sprints[0].name, "Sprint 30");
    }

    #[test]
    fn test_transform_sprint_list_response_empty() {
        let response = JiraSprintListResponse {
            max_results: 50,
            start_at: 0,
            is_last: true,
            values: vec![],
        };
        let output = transform_sprint_list_response(response);
        assert!(output.sprints.is_empty());
        assert_eq!(output.total, 0);
        assert!(!output.has_more);
    }

    #[test]
    fn test_transform_sprint_list_response_has_more() {
        let response = JiraSprintListResponse {
            max_results: 50,
            start_at: 0,
            is_last: false,
            values: vec![JiraSprintResponse {
                id: 1,
                name: "Sprint 1".to_string(),
                state: "closed".to_string(),
                start_date: None,
                end_date: None,
                complete_date: Some("2026-01-15T00:00:00.000Z".to_string()),
            }],
        };
        let output = transform_sprint_list_response(response);
        assert!(output.has_more);
        assert_eq!(output.total, 1);
    }

    #[test]
    fn test_find_sprint_by_name_exact() {
        let sprints = vec![
            JiraSprintResponse {
                id: 155,
                name: "Sprint 29".to_string(),
                state: "active".to_string(),
                start_date: None,
                end_date: None,
                complete_date: None,
            },
            JiraSprintResponse {
                id: 156,
                name: "Sprint 30".to_string(),
                state: "future".to_string(),
                start_date: None,
                end_date: None,
                complete_date: None,
            },
        ];
        assert_eq!(find_sprint_by_name(&sprints, "Sprint 30"), Some(156));
    }

    #[test]
    fn test_find_sprint_by_name_case_insensitive() {
        let sprints = vec![JiraSprintResponse {
            id: 156,
            name: "Sprint 30".to_string(),
            state: "future".to_string(),
            start_date: None,
            end_date: None,
            complete_date: None,
        }];
        assert_eq!(find_sprint_by_name(&sprints, "sprint 30"), Some(156));
    }

    #[test]
    fn test_find_sprint_by_name_not_found() {
        let sprints = vec![JiraSprintResponse {
            id: 156,
            name: "Sprint 30".to_string(),
            state: "future".to_string(),
            start_date: None,
            end_date: None,
            complete_date: None,
        }];
        assert_eq!(find_sprint_by_name(&sprints, "Sprint 99"), None);
    }

    #[test]
    fn test_find_sprint_by_name_empty_list() {
        let sprints: Vec<JiraSprintResponse> = vec![];
        assert_eq!(find_sprint_by_name(&sprints, "Sprint 30"), None);
    }
}
