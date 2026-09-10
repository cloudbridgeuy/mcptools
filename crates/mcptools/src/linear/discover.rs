use crate::linear::client::execute;
use crate::prelude::*;
use mcptools_core::linear::{
    match_projects, match_teams, parse_project_selector, parse_team_selector,
    transform_team_cycles, transform_team_labels, transform_team_projects, transform_team_states,
    transform_teams, transform_users, Cycle, Label, Paginated, Project, ProjectResolution, Team,
    TeamResolution, User, WorkflowState,
};

pub const TEAMS_QUERY: &str = "query ($first: Int!, $after: String) { teams(first: $first, after: $after) { nodes { id key name } pageInfo { hasNextPage endCursor } } }";
pub const TEAM_PROJECTS_QUERY: &str = "query ($id: String!, $first: Int!, $after: String) { team(id: $id) { id key name projects(first: $first, after: $after) { nodes { id name } pageInfo { hasNextPage endCursor } } } }";
pub const USERS_QUERY: &str = "query ($query: String!, $first: Int!, $after: String) { users(first: $first, after: $after, filter: { name: { contains: $query } }) { nodes { id name email } pageInfo { hasNextPage endCursor } } }";
pub const TEAM_STATES_QUERY: &str = "query ($id: String!, $first: Int!, $after: String) { team(id: $id) { id states(first: $first, after: $after) { nodes { id name type } pageInfo { hasNextPage endCursor } } } }";
pub const TEAM_LABELS_QUERY: &str = "query ($id: String!, $first: Int!, $after: String) { team(id: $id) { id labels(first: $first, after: $after) { nodes { id name } pageInfo { hasNextPage endCursor } } } }";
pub const TEAM_CYCLES_QUERY: &str = "query ($id: String!, $first: Int!, $after: String) { team(id: $id) { id cycles(first: $first, after: $after) { nodes { id number name } pageInfo { hasNextPage endCursor } } } }";

pub async fn teams_list_data(
    client: &reqwest::Client,
    limit: u32,
    cursor: Option<String>,
) -> Result<Paginated<Team>> {
    let data = execute(
        client,
        TEAMS_QUERY,
        serde_json::json!({"first": limit, "after": cursor}),
    )
    .await?;
    transform_teams(data).map_err(|e| eyre!("{}", e))
}

pub async fn teams_get_data(client: &reqwest::Client, selector: &str) -> Result<Team> {
    let parsed = parse_team_selector(selector)
        .ok_or_else(|| eyre!("Linear team selector must not be empty"))?;
    let listed = teams_list_data(client, 50, None).await?;
    match match_teams(&parsed, &listed.nodes) {
        TeamResolution::Resolved(team) => Ok(team),
        TeamResolution::NotFound(input) => Err(eyre!("Linear team not found: {}", input)),
        TeamResolution::Ambiguous(hits) => {
            let names: Vec<String> = hits
                .iter()
                .map(|team| format!("{} ({})", team.name, team.key))
                .collect();
            Err(eyre!(
                "Ambiguous Linear team '{}'. Candidates: {}",
                selector.trim(),
                names.join(", ")
            ))
        }
    }
}

pub async fn projects_list_data(
    client: &reqwest::Client,
    team: &str,
    limit: u32,
    cursor: Option<String>,
) -> Result<Paginated<Project>> {
    let resolved = teams_get_data(client, team).await?;
    let data = execute(
        client,
        TEAM_PROJECTS_QUERY,
        serde_json::json!({"id": resolved.id, "first": limit, "after": cursor}),
    )
    .await?;
    transform_team_projects(data).map_err(|e| eyre!("{}", e))
}

pub async fn projects_get_data(
    client: &reqwest::Client,
    id_or_name: &str,
    team: &str,
) -> Result<Project> {
    let parsed = parse_project_selector(id_or_name)
        .ok_or_else(|| eyre!("Linear project selector must not be empty"))?;
    let listed = projects_list_data(client, team, 50, None).await?;
    match match_projects(&parsed, &listed.nodes) {
        ProjectResolution::Resolved(item) => Ok(item),
        ProjectResolution::NotFound(input) => Err(eyre!("Linear project not found: {}", input)),
        ProjectResolution::Ambiguous(hits) => {
            let names: Vec<String> = hits
                .iter()
                .map(|item| format!("{} ({})", item.name, item.id))
                .collect();
            Err(eyre!(
                "Ambiguous Linear project '{}'. Candidates: {}",
                id_or_name.trim(),
                names.join(", ")
            ))
        }
    }
}

pub async fn users_list_data(
    client: &reqwest::Client,
    query: &str,
    limit: u32,
    cursor: Option<String>,
) -> Result<Paginated<User>> {
    if query.trim().is_empty() {
        return Err(eyre!("Linear users --query must not be empty"));
    }
    let data = execute(
        client,
        USERS_QUERY,
        serde_json::json!({"query": query, "first": limit, "after": cursor}),
    )
    .await?;
    transform_users(data).map_err(|e| eyre!("{}", e))
}

pub async fn states_list_data(
    client: &reqwest::Client,
    team: &str,
) -> Result<Paginated<WorkflowState>> {
    let resolved = teams_get_data(client, team).await?;
    let data = execute(
        client,
        TEAM_STATES_QUERY,
        serde_json::json!({"id": resolved.id, "first": 50, "after": None::<String>}),
    )
    .await?;
    transform_team_states(data).map_err(|e| eyre!("{}", e))
}

pub async fn labels_list_data(client: &reqwest::Client, team: &str) -> Result<Paginated<Label>> {
    let resolved = teams_get_data(client, team).await?;
    let data = execute(
        client,
        TEAM_LABELS_QUERY,
        serde_json::json!({"id": resolved.id, "first": 50, "after": None::<String>}),
    )
    .await?;
    transform_team_labels(data).map_err(|e| eyre!("{}", e))
}

pub async fn cycles_list_data(client: &reqwest::Client, team: &str) -> Result<Paginated<Cycle>> {
    let resolved = teams_get_data(client, team).await?;
    let data = execute(
        client,
        TEAM_CYCLES_QUERY,
        serde_json::json!({"id": resolved.id, "first": 50, "after": None::<String>}),
    )
    .await?;
    transform_team_cycles(data).map_err(|e| eyre!("{}", e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::linear::config::LinearConfig;

    #[tokio::test]
    async fn rejects_empty_team_selector_before_io() {
        let cfg = LinearConfig {
            api_key: "test-key".to_string(),
        };
        let client = crate::linear::client::build_client(&cfg).unwrap();
        let err = teams_get_data(&client, "   ").await.unwrap_err();
        assert!(err.to_string().contains("must not be empty"));
    }

    #[tokio::test]
    async fn rejects_empty_project_selector_before_io() {
        let cfg = LinearConfig {
            api_key: "test-key".to_string(),
        };
        let client = crate::linear::client::build_client(&cfg).unwrap();
        let err = projects_get_data(&client, "", "GUZ").await.unwrap_err();
        assert!(err.to_string().contains("must not be empty"));
    }

    #[tokio::test]
    async fn rejects_empty_users_query_before_io() {
        let cfg = LinearConfig {
            api_key: "test-key".to_string(),
        };
        let client = crate::linear::client::build_client(&cfg).unwrap();
        let err = users_list_data(&client, "   ", 25, None).await.unwrap_err();
        assert!(err.to_string().contains("--query"));
    }
}
