use std::path::PathBuf;

use base64::Engine;
use colored::Colorize;
use mcptools_core::atlassian::jira::{
    transform_attachment_response, AttachmentOutput, JiraAttachmentResponse,
};
use serde::Deserialize;

use crate::atlassian::{create_jira_client, JiraConfig};
use crate::prelude::*;

/// Attachment subcommands
#[derive(Debug, clap::Subcommand)]
pub enum AttachmentCommands {
    /// List attachments on a Jira ticket
    #[clap(name = "list")]
    List {
        /// Issue key (e.g., PROJ-123)
        issue_key: String,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Download an attachment by ID
    #[clap(name = "download")]
    Download {
        /// Issue key (e.g., PROJ-123)
        issue_key: String,

        /// Attachment ID
        attachment_id: String,

        /// Output file path (default: temp directory)
        #[arg(long)]
        output: Option<PathBuf>,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Upload files as attachments to a ticket
    #[clap(name = "upload")]
    Upload {
        /// Issue key (e.g., PROJ-123)
        issue_key: String,

        /// File paths to upload
        files: Vec<PathBuf>,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
}

// --- Local deserialization structs for the issue-with-attachments response ---

#[derive(Debug, Deserialize)]
struct IssueWithAttachments {
    fields: AttachmentFields,
}

#[derive(Debug, Deserialize)]
struct AttachmentFields {
    #[serde(default)]
    attachment: Vec<JiraAttachmentResponse>,
}

// --- Shared HTTP helpers ---

/// Check that an HTTP response was successful, returning a descriptive error otherwise.
async fn check_response(response: reqwest::Response, context: &str) -> Result<reqwest::Response> {
    if response.status().is_success() {
        return Ok(response);
    }
    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    Err(eyre!("{context} [{status}]: {body}"))
}

// --- Data functions (public, used by CLI and MCP) ---

/// Fetch all raw attachment metadata from a Jira issue.
async fn fetch_issue_attachments(
    client: &reqwest::Client,
    base_url: &str,
    issue_key: &str,
) -> Result<Vec<JiraAttachmentResponse>> {
    let url = format!("{base_url}/rest/api/3/issue/{issue_key}?fields=attachment");

    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|e| eyre!("Failed to fetch attachments: {e}"))?;

    let response = check_response(response, "Failed to fetch attachments").await?;

    let issue: IssueWithAttachments = response
        .json()
        .await
        .map_err(|e| eyre!("Failed to parse attachment response: {e}"))?;

    Ok(issue.fields.attachment)
}

/// List all attachments on a Jira ticket.
pub async fn list_attachments_data(issue_key: String) -> Result<Vec<AttachmentOutput>> {
    let config = JiraConfig::from_env()?;
    let client = create_jira_client(&config)?;
    let base_url = config.base_url.trim_end_matches('/');

    let raw = fetch_issue_attachments(&client, base_url, &issue_key).await?;
    Ok(transform_attachment_response(raw))
}

/// Download an attachment to disk. Returns the resolved output path.
pub async fn download_attachment_data(
    issue_key: String,
    attachment_id: String,
    output: Option<PathBuf>,
) -> Result<PathBuf> {
    let config = JiraConfig::from_env()?;
    let client = create_jira_client(&config)?;
    let base_url = config.base_url.trim_end_matches('/');

    let all = fetch_issue_attachments(&client, base_url, &issue_key).await?;
    let attachment = all
        .into_iter()
        .find(|a| a.id == attachment_id)
        .ok_or_else(|| eyre!("Attachment {attachment_id} not found on {issue_key}"))?;

    let response = client
        .get(&attachment.content)
        .send()
        .await
        .map_err(|e| eyre!("Failed to download attachment: {e}"))?;

    let response = check_response(response, "Failed to download attachment").await?;

    let bytes = response
        .bytes()
        .await
        .map_err(|e| eyre!("Failed to read attachment content: {e}"))?;

    let out_path = output.unwrap_or_else(|| std::env::temp_dir().join(&attachment.filename));

    tokio::fs::write(&out_path, &bytes)
        .await
        .map_err(|e| eyre!("Failed to write file to {}: {e}", out_path.display()))?;

    Ok(out_path)
}

/// Upload files as attachments to a Jira ticket.
pub async fn upload_attachment_data(
    issue_key: String,
    files: Vec<PathBuf>,
) -> Result<Vec<AttachmentOutput>> {
    let config = JiraConfig::from_env()?;
    let base_url = config.base_url.trim_end_matches('/');

    if files.is_empty() {
        return Err(eyre!("At least one file path is required for upload"));
    }

    // Validate all files exist before making any API calls
    for path in &files {
        if !path.is_file() {
            return Err(eyre!("File not found: {}", path.display()));
        }
    }

    // Build a client WITHOUT Content-Type: application/json
    // (multipart sets its own Content-Type boundary)
    let auth_value = base64::engine::general_purpose::STANDARD
        .encode(format!("{}:{}", config.email, config.api_token));
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(
        reqwest::header::AUTHORIZATION,
        reqwest::header::HeaderValue::from_str(&format!("Basic {auth_value}"))
            .map_err(|e| eyre!("Invalid auth header: {e}"))?,
    );
    // Required by Jira to bypass XSRF protection
    headers.insert(
        reqwest::header::HeaderName::from_static("x-atlassian-token"),
        reqwest::header::HeaderValue::from_static("no-check"),
    );

    let client = reqwest::Client::builder()
        .default_headers(headers)
        .build()
        .map_err(|e| eyre!("Failed to build upload client: {e}"))?;

    // Build multipart form -- each file is a "file" part
    let mut form = reqwest::multipart::Form::new();
    for file_path in &files {
        let filename = file_path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        let mime = mime_from_extension(&filename);

        let file_bytes = tokio::fs::read(file_path)
            .await
            .map_err(|e| eyre!("Failed to read {}: {e}", file_path.display()))?;

        let part = reqwest::multipart::Part::bytes(file_bytes)
            .file_name(filename)
            .mime_str(mime)
            .map_err(|e| eyre!("Invalid MIME type: {e}"))?;

        form = form.part("file", part);
    }

    let url = format!("{base_url}/rest/api/3/issue/{issue_key}/attachments");

    let response = client
        .post(&url)
        .multipart(form)
        .send()
        .await
        .map_err(|e| eyre!("Failed to upload attachments: {e}"))?;

    let response = check_response(response, "Failed to upload attachments").await?;

    let uploads: Vec<JiraAttachmentResponse> = response
        .json()
        .await
        .map_err(|e| eyre!("Failed to parse upload response: {e}"))?;

    Ok(transform_attachment_response(uploads))
}

/// Infer MIME type from file extension.
fn mime_from_extension(filename: &str) -> &'static str {
    let ext = filename.rsplit('.').next().unwrap_or("").to_lowercase();

    match ext.as_str() {
        "pdf" => "application/pdf",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "svg" => "image/svg+xml",
        "txt" => "text/plain",
        "json" => "application/json",
        "xml" => "application/xml",
        "html" | "htm" => "text/html",
        "css" => "text/css",
        "js" => "application/javascript",
        "zip" => "application/zip",
        "gz" | "gzip" => "application/gzip",
        "tar" => "application/x-tar",
        "csv" => "text/csv",
        "md" => "text/markdown",
        "doc" => "application/msword",
        "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        "xls" => "application/vnd.ms-excel",
        "xlsx" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        _ => "application/octet-stream",
    }
}

// --- CLI handler ---

/// Handle attachment subcommands.
pub async fn handler(cmd: AttachmentCommands) -> Result<()> {
    match cmd {
        AttachmentCommands::List { issue_key, json } => {
            let attachments = list_attachments_data(issue_key).await?;

            if json {
                std::println!("{}", serde_json::to_string_pretty(&attachments)?);
            } else if attachments.is_empty() {
                std::println!("No attachments found.");
            } else {
                let mut table = crate::prelude::new_table();
                table.add_row(prettytable::row![
                    "ID".bold().cyan(),
                    "Filename".bold().cyan(),
                    "Size".bold().cyan(),
                    "Type".bold().cyan(),
                    "Created".bold().cyan()
                ]);
                for att in &attachments {
                    table.add_row(prettytable::row![
                        att.id.green().to_string(),
                        att.filename.bright_white().to_string(),
                        att.size_human.bright_yellow().to_string(),
                        att.mime_type.bright_blue().to_string(),
                        att.created.bright_black().to_string()
                    ]);
                }
                table.printstd();
            }
        }

        AttachmentCommands::Download {
            issue_key,
            attachment_id,
            output,
            json,
        } => {
            let path = download_attachment_data(issue_key, attachment_id, output).await?;

            if json {
                std::println!(
                    "{}",
                    serde_json::to_string_pretty(&serde_json::json!({
                        "path": path.display().to_string()
                    }))?
                );
            } else {
                std::println!("{} {}", "Downloaded to:".green().bold(), path.display());
            }
        }

        AttachmentCommands::Upload {
            issue_key,
            files,
            json,
        } => {
            let uploads = upload_attachment_data(issue_key, files).await?;

            if json {
                std::println!("{}", serde_json::to_string_pretty(&uploads)?);
            } else {
                std::println!(
                    "{}",
                    format!("Uploaded {} attachment(s):", uploads.len())
                        .green()
                        .bold()
                );
                for att in &uploads {
                    std::println!("  - {} ({})", att.filename.bright_white(), att.size_human);
                }
            }
        }
    }

    Ok(())
}
