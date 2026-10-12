use colored::Colorize;
use mcptools_core::atlassian::jira::{
    build_issue_link_create_request, build_issue_link_delete_request,
    build_issue_link_list_request, build_issue_link_type_list_request,
    parse_issue_link_types_response, parse_issue_links_response, IssueLinkCreateOutput,
    IssueLinkDeleteOutput, IssueLinkDirection, IssueLinkListOutput, IssueLinkTypeListOutput,
};

use super::check_response;
use crate::atlassian::create_jira_client;
use crate::prelude::*;

#[derive(Debug, clap::Subcommand)]
pub enum LinkCommands {
    #[clap(name = "list")]
    List {
        #[arg(help = "Issue key (for example, PROJ-123)")]
        issue_key: String,

        #[arg(long)]
        json: bool,
    },

    #[clap(name = "add")]
    Add {
        #[arg(help = "FROM issue: uses the outward relation; maps to Jira inwardIssue")]
        from_issue: String,

        #[arg(
            long = "type",
            value_name = "NAME",
            help = "Link type name; creates FROM <outward relation> TO"
        )]
        link_type: String,

        #[arg(help = "TO issue: uses the inward relation; maps to Jira outwardIssue")]
        to_issue: String,

        #[arg(long)]
        json: bool,
    },

    #[clap(name = "delete")]
    Delete {
        #[arg(help = "Issue link ID returned by link list")]
        link_id: String,

        #[arg(long)]
        json: bool,
    },

    #[clap(name = "types")]
    Types {
        #[arg(long)]
        json: bool,
    },
}

pub async fn list_issue_links_data(
    issue_key: String,
    config: &super::super::JiraConfig,
) -> Result<IssueLinkListOutput> {
    let request = build_issue_link_list_request(&config.base_url, &issue_key)?;
    let response = create_jira_client(config)?
        .get(request.url)
        .send()
        .await
        .map_err(|error| eyre!("Failed to fetch issue links: {error}"))?;
    let response = check_response(response, "Failed to fetch issue links").await?;
    let body = response
        .text()
        .await
        .map_err(|error| eyre!("Failed to read issue links response: {error}"))?;
    parse_issue_links_response(&issue_key, &body)
        .map_err(|error| eyre!("Failed to parse issue links response: {error}"))
}

pub async fn list_issue_link_types_data(
    config: &super::super::JiraConfig,
) -> Result<IssueLinkTypeListOutput> {
    let request = build_issue_link_type_list_request(&config.base_url)?;
    let response = create_jira_client(config)?
        .get(request.url)
        .send()
        .await
        .map_err(|error| eyre!("Failed to fetch issue link types: {error}"))?;
    let response = check_response(response, "Failed to fetch issue link types").await?;
    let body = response
        .text()
        .await
        .map_err(|error| eyre!("Failed to read issue link types response: {error}"))?;
    parse_issue_link_types_response(&body)
        .map_err(|error| eyre!("Failed to parse issue link types response: {error}"))
}

pub async fn create_issue_link_data(
    from_issue: String,
    link_type: String,
    to_issue: String,
    config: &super::super::JiraConfig,
) -> Result<IssueLinkCreateOutput> {
    let request =
        build_issue_link_create_request(&config.base_url, &from_issue, &link_type, &to_issue)?;
    let response = create_jira_client(config)?
        .post(request.url)
        .json(&request.body)
        .send()
        .await
        .map_err(|error| eyre!("Failed to create issue link: {error}"))?;
    check_response(response, "Failed to create issue link").await?;
    Ok(IssueLinkCreateOutput {
        created: true,
        from_issue,
        link_type,
        to_issue,
    })
}

pub async fn delete_issue_link_data(
    link_id: String,
    config: &super::super::JiraConfig,
) -> Result<IssueLinkDeleteOutput> {
    let request = build_issue_link_delete_request(&config.base_url, &link_id)?;
    let response = create_jira_client(config)?
        .delete(request.url)
        .send()
        .await
        .map_err(|error| eyre!("Failed to delete issue link: {error}"))?;
    check_response(response, "Failed to delete issue link").await?;
    Ok(IssueLinkDeleteOutput {
        deleted: true,
        link_id,
    })
}

pub async fn handler(command: LinkCommands, config: &super::super::JiraConfig) -> Result<()> {
    match command {
        LinkCommands::List { issue_key, json } => {
            let output = list_issue_links_data(issue_key, config).await?;
            if json {
                std::println!("{}", serde_json::to_string_pretty(&output)?);
            } else {
                display_issue_links(output);
            }
        }
        LinkCommands::Add {
            from_issue,
            link_type,
            to_issue,
            json,
        } => {
            let output = create_issue_link_data(from_issue, link_type, to_issue, config).await?;
            if json {
                std::println!("{}", serde_json::to_string_pretty(&output)?);
            } else {
                std::println!(
                    "{} {} {} {}",
                    output.from_issue.bold().cyan(),
                    output.link_type.green(),
                    output.to_issue.bold().cyan(),
                    "linked".green().bold()
                );
            }
        }
        LinkCommands::Delete { link_id, json } => {
            let output = delete_issue_link_data(link_id, config).await?;
            if json {
                std::println!("{}", serde_json::to_string_pretty(&output)?);
            } else {
                std::println!(
                    "{} {}",
                    "Deleted issue link".green().bold(),
                    output.link_id.bright_white()
                );
            }
        }
        LinkCommands::Types { json } => {
            let output = list_issue_link_types_data(config).await?;
            if json {
                std::println!("{}", serde_json::to_string_pretty(&output)?);
            } else {
                display_issue_link_types(output);
            }
        }
    }
    Ok(())
}

fn display_issue_links(output: IssueLinkListOutput) {
    if output.links.is_empty() {
        std::println!("No issue links found.");
        return;
    }
    let mut table = new_table();
    table.add_row(prettytable::row![
        "ID".bold().cyan(),
        "Direction".bold().cyan(),
        "Relation".bold().cyan(),
        "Issue".bold().cyan(),
        "Status".bold().cyan(),
        "Summary".bold().cyan()
    ]);
    for link in output.links {
        let direction = match link.direction {
            IssueLinkDirection::Inward => "inward",
            IssueLinkDirection::Outward => "outward",
        };
        table.add_row(prettytable::row![
            link.id.green().to_string(),
            direction.bright_black().to_string(),
            link.relation.bright_white().to_string(),
            link.other_issue_key.bright_blue().to_string(),
            link.status.unwrap_or_default(),
            link.summary.unwrap_or_default()
        ]);
    }
    table.printstd();
}

fn display_issue_link_types(output: IssueLinkTypeListOutput) {
    if output.link_types.is_empty() {
        std::println!("No issue link types found.");
        return;
    }
    let mut table = new_table();
    table.add_row(prettytable::row![
        "ID".bold().cyan(),
        "Name".bold().cyan(),
        "Outward".bold().cyan(),
        "Inward".bold().cyan()
    ]);
    for link_type in output.link_types {
        table.add_row(prettytable::row![
            link_type.id.green().to_string(),
            link_type.name.bright_white().to_string(),
            link_type.outward.bright_blue().to_string(),
            link_type.inward.bright_magenta().to_string()
        ]);
    }
    table.printstd();
}
