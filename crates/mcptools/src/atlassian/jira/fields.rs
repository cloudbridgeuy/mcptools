//! List available values for Jira custom fields

use crate::atlassian::{create_jira_client, JiraConfig};
use crate::prelude::*;
use clap::Args;
use colored::Colorize;
use mcptools_core::atlassian::jira::{extract_field_options, FieldsOutput, JiraCreateMeta};

/// List available values for Jira custom fields
#[derive(Args, Debug, Clone)]
pub struct FieldsOptions {
    /// Project key (default: PROD)
    #[arg(long, default_value = "PROD")]
    pub project: String,

    /// Specific field to display (assigned-guild or assigned-pod), shows all by default
    #[arg(long)]
    pub field: Option<String>,

    /// Output as JSON
    #[arg(long)]
    pub json: bool,
}

/// Fetch field metadata from Jira API
pub async fn get_fields_data(options: FieldsOptions) -> Result<FieldsOutput> {
    let config = JiraConfig::from_env()?;
    let client = create_jira_client(&config)?;
    let base_url = config.base_url.trim_end_matches('/');

    // Build the URL to fetch create metadata
    let url = format!(
        "{base_url}/rest/api/3/issue/createmeta?projectKeys={}&expand=projects.issuetypes.fields",
        urlencoding::encode(&options.project)
    );

    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|e| eyre!("Failed to fetch field metadata from Jira: {}", e))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(eyre!(
            "Failed to fetch field metadata [{}]: {}",
            status,
            body
        ));
    }

    let body_text = response
        .text()
        .await
        .map_err(|e| eyre!("Failed to read response body: {}", e))?;

    let meta: JiraCreateMeta = serde_json::from_str(&body_text)
        .map_err(|e| eyre!("Failed to parse field metadata response: {}", e))?;

    // Determine which fields to fetch based on --field option
    let field_ids: Vec<&str> = if let Some(field) = &options.field {
        match field.to_lowercase().as_str() {
            "assigned-guild" => vec!["customfield_10527"],
            "assigned-pod" => vec!["customfield_10528"],
            _ => {
                return Err(eyre!(
                    "Unknown field '{}'. Valid options: assigned-guild, assigned-pod",
                    field
                ))
            }
        }
    } else {
        // Default: show both fields
        vec!["customfield_10527", "customfield_10528"]
    };

    // Extract field options using pure function
    let output = extract_field_options(meta, &field_ids).map_err(|e| eyre!("{}", e))?;

    Ok(output)
}

/// CLI handler for fields command
pub async fn handler(options: FieldsOptions) -> Result<()> {
    let output = get_fields_data(options.clone()).await?;

    if options.json {
        std::println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        // Display with each value on a new line
        for (idx, field) in output.fields.iter().enumerate() {
            let field_name = field.field_name.cyan().bold();
            std::println!("{}:", field_name);

            for value in &field.allowed_values {
                std::println!("  • {}", value.green());
            }

            // Add blank line between fields (but not after the last one)
            if idx < output.fields.len() - 1 {
                std::println!();
            }
        }
    }

    Ok(())
}
