use clap::Args;
use colored::Colorize;
use mcptools_core::atlassian::jira::{
    build_board_list_request, parse_board_list_response, BoardListFilter, BoardListOutput,
    JiraBoardType,
};

use crate::atlassian::create_jira_client;
use crate::prelude::*;

#[derive(Debug, clap::Subcommand)]
pub enum BoardCommands {
    #[clap(name = "list")]
    List(BoardListOptions),
}

#[derive(Args, Debug, Clone)]
pub struct BoardListOptions {
    #[arg(long)]
    pub project: Option<String>,
    #[arg(long)]
    pub name: Option<String>,
    #[arg(long = "type")]
    pub board_type: Option<JiraBoardType>,
    #[arg(long, global = true)]
    pub json: bool,
}

pub async fn list_boards_data(
    filter: BoardListFilter,
    config: &super::super::JiraConfig,
) -> Result<BoardListOutput> {
    let client = create_jira_client(config)?;
    let mut boards = Vec::new();
    let mut start_at = 0;
    let mut total = 0;
    for _ in 0..1000 {
        let request = build_board_list_request(&config.base_url, &filter, start_at, 50)
            .map_err(|error| eyre!(error.to_string()))?;
        let response = client
            .get(request.url)
            .send()
            .await
            .map_err(|error| eyre!("Failed to fetch Jira boards: {error}"))?;
        let body = super::check_response(response, "Failed to list Jira boards")
            .await?
            .text()
            .await
            .map_err(|error| eyre!("Failed to read Jira board response: {error}"))?;
        let page = parse_board_list_response(&body).map_err(|error| eyre!(error.to_string()))?;
        total = page.total;
        boards.extend(page.boards);
        match page.next_start_at {
            None => return Ok(BoardListOutput { boards, total }),
            Some(next) if next > start_at => start_at = next,
            Some(_) => return Err(eyre!("Jira board pagination did not advance")),
        }
    }
    Err(eyre!("Jira board pagination exceeded 1000 pages"))
}

pub async fn handler(cmd: BoardCommands, config: &super::super::JiraConfig) -> Result<()> {
    match cmd {
        BoardCommands::List(options) => {
            let output = list_boards_data(
                BoardListFilter {
                    project: options.project,
                    name: options.name,
                    board_type: options.board_type,
                },
                config,
            )
            .await?;
            if options.json {
                std::println!("{}", serde_json::to_string_pretty(&output)?);
            } else if output.boards.is_empty() {
                std::println!("No boards found.");
            } else {
                let mut table = new_table();
                table.add_row(prettytable::row![
                    "ID".bold().cyan(),
                    "Name".bold().cyan(),
                    "Type".bold().cyan(),
                    "Project".bold().cyan()
                ]);
                for board in output.boards {
                    table.add_row(prettytable::row![
                        board.id.to_string().green(),
                        board.name.bright_white(),
                        board.board_type.bright_yellow(),
                        board.project_key.as_deref().unwrap_or("-")
                    ]);
                }
                table.printstd();
            }
        }
    }
    Ok(())
}
