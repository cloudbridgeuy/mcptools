use crate::linear::client::{execute, execute_with_url, LINEAR_API_URL};
use crate::prelude::*;
use mcptools_core::linear::{
    match_projects, match_teams, parse_project_selector, parse_team_selector, project_create_input,
    transform_project_create, transform_team_cycles, transform_team_labels,
    transform_team_projects, transform_team_states, transform_teams, transform_users, Cycle, Label,
    Paginated, Project, ProjectResolution, Team, TeamResolution, User, WorkflowState,
};
use mcptools_core::linear::{transform_project_statuses, ProjectStatusListOutput};

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

pub const PROJECT_STATUSES_QUERY: &str = "query ($first: Int!, $after: String) { projectStatuses(first: $first, after: $after, orderBy: createdAt) { nodes { id name type color position description } pageInfo { hasNextPage endCursor } } }";

pub async fn project_statuses_list_data(
    client: &reqwest::Client,
    args: crate::linear::args::ProjectStatusListArgs,
) -> Result<ProjectStatusListOutput> {
    project_statuses_list_data_with_url(client, LINEAR_API_URL, args).await
}

async fn project_statuses_list_data_with_url(
    client: &reqwest::Client,
    url: &str,
    args: crate::linear::args::ProjectStatusListArgs,
) -> Result<ProjectStatusListOutput> {
    args.validate().map_err(|error| eyre!(error))?;
    let mut cursor = args.cursor;
    let mut seen = std::collections::HashSet::new();
    if let Some(cursor) = &cursor {
        seen.insert(cursor.clone());
    }
    let mut nodes = Vec::new();
    loop {
        let data = execute_with_url(
            client,
            url,
            PROJECT_STATUSES_QUERY,
            serde_json::json!({"first": args.limit, "after": cursor}),
        )
        .await?;
        let page = transform_project_statuses(data).map_err(|error| eyre!(error))?;
        nodes.extend(page.nodes);
        if page.page_info.has_next {
            let next = page
                .page_info
                .end_cursor
                .as_ref()
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| {
                    eyre!("Linear project statuses pagination cursor is missing or blank")
                })?;
            if !seen.insert(next.clone()) {
                return Err(eyre!(
                    "Linear project statuses pagination cursor did not progress"
                ));
            }
        }
        if !args.all || !page.page_info.has_next {
            return Ok(ProjectStatusListOutput {
                nodes,
                page_info: page.page_info,
            });
        }
        cursor = page.page_info.end_cursor;
    }
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
    async fn project_status_pages_preserve_order_and_final_page_info() {
        use wiremock::matchers::{body_json, method};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        for (all, count) in [
            (false, 0),
            (false, 1),
            (false, 3),
            (true, 0),
            (true, 1),
            (true, 3),
        ] {
            let server = MockServer::start().await;
            let pages = if all { count.max(1) } else { 1 };
            for index in 0..pages {
                let after = if index == 0 {
                    "start".to_string()
                } else {
                    format!("c{index}")
                };
                let has_next = index + 1 < count;
                let nodes = if count == 0 {
                    serde_json::json!([])
                } else {
                    serde_json::json!([{
                        "id": format!("s{index}"), "name": format!("Custom {index}"), "type": "planned",
                        "color": "#123456", "position": 3.5 - index as f64,
                        "description": if index == 0 { Some("Description") } else { None },
                    }])
                };
                Mock::given(method("POST"))
                    .and(body_json(serde_json::json!({"query": PROJECT_STATUSES_QUERY, "variables": {"first": 2, "after": after}})))
                    .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"data": {"projectStatuses": {
                        "nodes": nodes, "pageInfo": {"hasNextPage": has_next, "endCursor": if count == 0 { None } else { Some(format!("c{}", index + 1)) }}
                    }}})))
                    .expect(1)
                    .mount(&server).await;
            }
            let args = serde_json::from_value(
                serde_json::json!({"limit": 2, "cursor": "start", "all": all}),
            )
            .unwrap();
            let output =
                project_statuses_list_data_with_url(&reqwest::Client::new(), &server.uri(), args)
                    .await
                    .unwrap();
            assert_eq!(output.nodes.len(), if count == 0 { 0 } else { pages });
            assert_eq!(output.page_info.has_next, !all && count > 1);
            assert_eq!(
                output.page_info.end_cursor,
                if count == 0 {
                    None
                } else {
                    Some(format!("c{pages}"))
                }
            );
            for (index, node) in output.nodes.iter().enumerate() {
                assert_eq!(node.id, format!("s{index}"));
                assert_eq!(node.name, format!("Custom {index}"));
                assert_eq!(node.position, 3.5 - index as f64);
                assert_eq!(node.status_type, "planned");
                assert_eq!(node.color, "#123456");
                assert_eq!(
                    node.description.as_deref(),
                    if index == 0 {
                        Some("Description")
                    } else {
                        None
                    }
                );
            }
            let result = crate::mcp::tools::to_dual_result(output).unwrap();
            let text: serde_json::Value =
                serde_json::from_str(result["content"][0]["text"].as_str().unwrap()).unwrap();
            assert_eq!(text, result["structuredContent"]);
            if pages > 1 {
                assert!(text["nodes"][1].get("description").is_none());
            }
        }
    }

    #[tokio::test]
    async fn project_status_errors_and_bad_cursors_fail_without_partial_output() {
        use wiremock::matchers::{body_json, method};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        for (status, body) in [
            (
                401,
                serde_json::json!({"errors": [{"message": "unauthorized"}]}),
            ),
            (
                200,
                serde_json::json!({"errors": [{"message": "bad query"}]}),
            ),
            (200, serde_json::json!({"data": {}})),
            (200, serde_json::json!({"data": {"projectStatuses": {}}})),
            (
                200,
                serde_json::json!({"data": {"projectStatuses": {"nodes": []}}}),
            ),
            (
                200,
                serde_json::json!({"data": {"projectStatuses": {"pageInfo": {"hasNextPage": false, "endCursor": null}}}}),
            ),
            (
                200,
                serde_json::json!({"data": {"projectStatuses": {"nodes": [], "pageInfo": {"hasNextPage": false}}}}),
            ),
            (
                200,
                serde_json::json!({"data": {"projectStatuses": {"nodes": [], "pageInfo": {"hasNextPage": "yes", "endCursor": null}}}}),
            ),
            (
                200,
                serde_json::json!({"data": {"projectStatuses": {"nodes": [{"id": "s"}], "pageInfo": {"hasNextPage": false, "endCursor": null}}}}),
            ),
        ] {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .and(body_json(serde_json::json!({"query": PROJECT_STATUSES_QUERY, "variables": {"first": 25, "after": null}})))
                .respond_with(ResponseTemplate::new(status).set_body_json(body))
                .expect(1).mount(&server).await;
            let args = serde_json::from_value(serde_json::json!({})).unwrap();
            assert!(project_statuses_list_data_with_url(
                &reqwest::Client::new(),
                &server.uri(),
                args
            )
            .await
            .is_err());
        }
        for (all, cursors) in [
            (false, vec![None]),
            (false, vec![Some(" ")]),
            (false, vec![Some("start")]),
            (true, vec![None]),
            (true, vec![Some(" ")]),
            (true, vec![Some("start")]),
            (true, vec![Some("a"), Some("b"), Some("a")]),
        ] {
            let server = MockServer::start().await;
            let mut after = Some("start");
            for next in &cursors {
                Mock::given(method("POST"))
                    .and(body_json(serde_json::json!({"query": PROJECT_STATUSES_QUERY, "variables": {"first": 25, "after": after}})))
                    .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"data": {"projectStatuses": {
                        "nodes": [], "pageInfo": {"hasNextPage": true, "endCursor": next}
                    }}})))
                    .expect(1).mount(&server).await;
                after = *next;
            }
            let args =
                serde_json::from_value(serde_json::json!({"cursor": "start", "all": all})).unwrap();
            let error =
                project_statuses_list_data_with_url(&reqwest::Client::new(), &server.uri(), args)
                    .await
                    .unwrap_err();
            assert!(error.to_string().contains("pagination cursor"));
        }
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_string("not json"))
            .expect(1)
            .mount(&server)
            .await;
        let args = serde_json::from_value(serde_json::json!({})).unwrap();
        assert!(
            project_statuses_list_data_with_url(&reqwest::Client::new(), &server.uri(), args)
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn project_status_later_page_errors_discard_collected_nodes() {
        use wiremock::matchers::{body_json, method};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        for (status, body) in [
            (
                401,
                serde_json::json!({"errors": [{"message": "unauthorized"}]}),
            ),
            (
                200,
                serde_json::json!({"errors": [{"message": "bad query"}], "data": {"projectStatuses": {"nodes": []}}}),
            ),
            (200, serde_json::json!({"data": {"projectStatuses": {}}})),
        ] {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .and(body_json(serde_json::json!({"query": PROJECT_STATUSES_QUERY, "variables": {"first": 25, "after": null}})))
                .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"data": {"projectStatuses": {
                    "nodes": [{"id": "s", "name": "Custom", "type": "planned", "color": "#123456", "position": 1.5, "description": null}],
                    "pageInfo": {"hasNextPage": true, "endCursor": "next"}
                }}})))
                .expect(1).mount(&server).await;
            Mock::given(method("POST"))
                .and(body_json(serde_json::json!({"query": PROJECT_STATUSES_QUERY, "variables": {"first": 25, "after": "next"}})))
                .respond_with(ResponseTemplate::new(status).set_body_json(body))
                .expect(1).mount(&server).await;
            let args = serde_json::from_value(serde_json::json!({"all": true})).unwrap();
            assert!(project_statuses_list_data_with_url(
                &reqwest::Client::new(),
                &server.uri(),
                args
            )
            .await
            .is_err());
        }
    }

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
