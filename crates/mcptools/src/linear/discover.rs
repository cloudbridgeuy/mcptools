use crate::linear::client::{execute, execute_with_url, LINEAR_API_URL};
use crate::prelude::*;
use mcptools_core::linear::{
    match_projects, match_teams, parse_project_selector, parse_team_selector, project_create_input,
    transform_project_create, transform_team_cycles, transform_team_labels,
    transform_team_projects, transform_team_states, transform_teams, transform_users, Cycle, Label,
    Paginated, Project, ProjectResolution, Team, TeamResolution, User, WorkflowState,
};

pub const TEAMS_QUERY: &str = "query ($first: Int!, $after: String) { teams(first: $first, after: $after) { nodes { id key name } pageInfo { hasNextPage endCursor } } }";
pub const TEAM_PROJECTS_QUERY: &str = "query ($id: String!, $first: Int!, $after: String) { team(id: $id) { id key name projects(first: $first, after: $after) { nodes { id name } pageInfo { hasNextPage endCursor } } } }";
pub const PROJECT_CREATE_MUTATION: &str = "mutation ($input: ProjectCreateInput!) { projectCreate(input: $input) { success project { id name } } }";
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
    teams_get_data_with_url(client, LINEAR_API_URL, selector).await
}

async fn teams_get_data_with_url(
    client: &reqwest::Client,
    url: &str,
    selector: &str,
) -> Result<Team> {
    let parsed = parse_team_selector(selector)
        .ok_or_else(|| eyre!("Linear team selector must not be empty"))?;
    let data = execute_with_url(
        client,
        url,
        TEAMS_QUERY,
        serde_json::json!({"first": 50, "after": null}),
    )
    .await?;
    let listed = transform_teams(data).map_err(|e| eyre!("{}", e))?;
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

pub async fn projects_create_data(
    client: &reqwest::Client,
    team: &str,
    name: &str,
    description: Option<&str>,
    content: Option<&str>,
) -> Result<Project> {
    projects_create_data_with_url(client, LINEAR_API_URL, team, name, description, content).await
}

async fn projects_create_data_with_url(
    client: &reqwest::Client,
    url: &str,
    team: &str,
    name: &str,
    description: Option<&str>,
    content: Option<&str>,
) -> Result<Project> {
    for (field, value) in [
        ("team", Some(team)),
        ("name", Some(name)),
        ("description", description),
        ("content", content),
    ] {
        if value.is_some_and(|text| text.trim().is_empty()) {
            return Err(eyre!("Linear project create {} must not be empty", field));
        }
    }
    let resolved = teams_get_data_with_url(client, url, team.trim()).await?;
    let input = project_create_input(&resolved.id, name, description, content);
    let data = execute_with_url(
        client,
        url,
        PROJECT_CREATE_MUTATION,
        serde_json::json!({"input": input}),
    )
    .await?;
    transform_project_create(data).map_err(|e| eyre!("{}", e))
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
    async fn project_create_resolves_team_and_sends_only_supplied_fields() {
        use wiremock::matchers::{body_json, method};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let team_id = "12345678-1234-1234-1234-123456789abc";
        for selector in [team_id, " GUZ ", " Example "] {
            for optional in [false, true] {
                let server = MockServer::start().await;
                Mock::given(method("POST"))
                    .and(body_json(serde_json::json!({"query": TEAMS_QUERY, "variables": {"first": 50, "after": null}})))
                    .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"data": {"teams": {
                        "nodes": [{"id": team_id, "key": "GUZ", "name": "Example"}],
                        "pageInfo": {"hasNextPage": false, "endCursor": null}
                    }}})))
                    .expect(1)
                    .mount(&server)
                    .await;
                let content = "    indented code\n\n# Heading\n";
                let input = if optional {
                    serde_json::json!({"teamIds": [team_id], "name": "New project", "description": "Summary", "content": content})
                } else {
                    serde_json::json!({"teamIds": [team_id], "name": "New project"})
                };
                Mock::given(method("POST"))
                    .and(body_json(serde_json::json!({"query": PROJECT_CREATE_MUTATION, "variables": {"input": input}})))
                    .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"data": {"projectCreate": {
                        "success": true, "project": {"id": "p1", "name": "New project"}
                    }}})))
                    .expect(1)
                    .mount(&server)
                    .await;
                let client = reqwest::Client::new();
                let created = projects_create_data_with_url(
                    &client,
                    &server.uri(),
                    selector,
                    " New project ",
                    optional.then_some(" Summary "),
                    optional.then_some(content),
                )
                .await
                .unwrap();
                assert_eq!(
                    created,
                    Project {
                        id: "p1".to_string(),
                        name: "New project".to_string()
                    }
                );
                assert_eq!(server.received_requests().await.unwrap().len(), 2);
            }
        }
    }

    #[tokio::test]
    async fn project_create_rejects_blank_inputs_before_io() {
        let server = wiremock::MockServer::start().await;
        let client = reqwest::Client::new();
        for (team, name, description, content, field) in [
            (" \n", "Project", None, None, "team"),
            ("GUZ", "\t", None, None, "name"),
            ("GUZ", "Project", Some(" "), None, "description"),
            ("GUZ", "Project", None, Some("\n"), "content"),
        ] {
            let error = projects_create_data_with_url(
                &client,
                &server.uri(),
                team,
                name,
                description,
                content,
            )
            .await
            .unwrap_err();
            assert!(error
                .to_string()
                .contains(&format!("{} must not be empty", field)));
        }
        assert!(server.received_requests().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn project_create_rejects_failed_missing_malformed_and_graphql_responses() {
        use wiremock::matchers::{body_partial_json, method};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        for (response, expected) in [
            (
                serde_json::json!({"data": {"projectCreate": {"success": false, "project": {"id": "p1", "name": "Project"}}}}),
                "success was not true",
            ),
            (
                serde_json::json!({"data": {"projectCreate": {"success": true, "project": null}}}),
                "missing project",
            ),
            (
                serde_json::json!({"data": {"projectCreate": {"success": true}}}),
                "missing project",
            ),
            (
                serde_json::json!({"data": {"projectCreate": {"success": true, "project": {"id": 1, "name": "Project"}}}}),
                "Failed to parse",
            ),
            (
                serde_json::json!({"data": {"projectCreate": {"success": true, "project": {"id": "p1"}}}}),
                "Failed to parse",
            ),
            (
                serde_json::json!({"data": {"projectCreate": null}}),
                "missing project",
            ),
            (
                serde_json::json!({"data": {"projectCreate": {"success": "true", "project": {"id": "p1", "name": "Project"}}}}),
                "success was not true",
            ),
            (
                serde_json::json!({"errors": [{"message": "Permission denied"}]}),
                "Permission denied",
            ),
            (
                serde_json::json!({"data": {"projectCreate": {"success": true, "project": {"id": "p1", "name": "Project"}}}, "errors": [{"message": "Partial failure"}]}),
                "Partial failure",
            ),
        ] {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .and(body_partial_json(serde_json::json!({"query": TEAMS_QUERY})))
                .respond_with(ResponseTemplate::new(200).set_body_json(
                    serde_json::json!({"data": {"teams": {
                        "nodes": [{"id": "t1", "key": "GUZ", "name": "Example"}],
                        "pageInfo": {"hasNextPage": false, "endCursor": null}
                    }}}),
                ))
                .expect(1)
                .mount(&server)
                .await;
            Mock::given(method("POST"))
                .and(body_partial_json(
                    serde_json::json!({"query": PROJECT_CREATE_MUTATION}),
                ))
                .respond_with(ResponseTemplate::new(200).set_body_json(response))
                .expect(1)
                .mount(&server)
                .await;
            let error = projects_create_data_with_url(
                &reqwest::Client::new(),
                &server.uri(),
                "GUZ",
                "Project",
                None,
                None,
            )
            .await
            .unwrap_err();
            assert!(error.to_string().contains(expected), "{error}");
            assert_eq!(server.received_requests().await.unwrap().len(), 2);
        }
    }

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
