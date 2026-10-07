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
pub const PROJECT_GET_QUERY: &str = "query ($id: String!) { project(id: $id) { id name } }";
pub const PROJECT_UPDATE_MUTATION: &str = "mutation ($id: String!, $input: ProjectUpdateInput!) { projectUpdate(id: $id, input: $input) { success project { id name url description content status { id name type } lead { id name email } startDate targetDate priority } } }";
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
    let mut nodes = Vec::new();
    let mut cursor = None;
    let mut seen = std::collections::HashSet::new();
    loop {
        let data = execute_with_url(
            client,
            url,
            TEAMS_QUERY,
            serde_json::json!({"first": 50, "after": cursor}),
        )
        .await?;
        let page = transform_teams(data).map_err(|error| eyre!(error))?;
        nodes.extend(page.nodes);
        if !page.page_info.has_next {
            break;
        }
        let next = page
            .page_info
            .end_cursor
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| eyre!("Team pagination cursor missing or blank"))?;
        if !seen.insert(next.clone()) {
            return Err(eyre!("Team pagination cursor did not progress"));
        }
        cursor = Some(next);
    }
    match match_teams(&parsed, &nodes) {
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

pub async fn projects_update_data(
    client: &reqwest::Client,
    args: &crate::linear::args::ProjectUpdateArgs,
) -> Result<mcptools_core::linear::ProjectUpdateOutput> {
    projects_update_data_with_url(client, LINEAR_API_URL, args).await
}

async fn projects_update_data_with_url(
    client: &reqwest::Client,
    url: &str,
    args: &crate::linear::args::ProjectUpdateArgs,
) -> Result<mcptools_core::linear::ProjectUpdateOutput> {
    use mcptools_core::linear::{
        project_update_input, resolve_project_status, transform_project_update, ProjectSelector,
    };
    let mut input = project_update_input(args).map_err(|error| eyre!(error))?;
    let selector =
        parse_project_selector(&args.id).ok_or_else(|| eyre!("Missing project selector"))?;
    let project = match &selector {
        ProjectSelector::Id(id) => {
            let data = execute_with_url(
                client,
                url,
                PROJECT_GET_QUERY,
                serde_json::json!({"id": id}),
            )
            .await?;
            let project = serde_json::from_value::<Project>(data["project"].clone())
                .map_err(|error| eyre!("Project not found or malformed: {error}"))?;
            if !project.id.eq_ignore_ascii_case(id) {
                return Err(eyre!(
                    "Project lookup returned a different project identity"
                ));
            }
            project
        }
        ProjectSelector::Name(_) => {
            let team = teams_get_data_with_url(
                client,
                url,
                args.team
                    .as_deref()
                    .ok_or_else(|| eyre!("Project names require team"))?,
            )
            .await?;
            let mut nodes = Vec::new();
            let mut cursor = None;
            let mut seen = std::collections::HashSet::new();
            loop {
                let data = execute_with_url(
                    client,
                    url,
                    TEAM_PROJECTS_QUERY,
                    serde_json::json!({"id": team.id, "first": 50, "after": cursor}),
                )
                .await?;
                let page = transform_team_projects(data).map_err(|error| eyre!(error))?;
                nodes.extend(page.nodes);
                if !page.page_info.has_next {
                    break;
                }
                let next = page
                    .page_info
                    .end_cursor
                    .filter(|value| !value.trim().is_empty())
                    .ok_or_else(|| eyre!("Project pagination cursor missing or blank"))?;
                if !seen.insert(next.clone()) {
                    return Err(eyre!("Project pagination cursor did not progress"));
                }
                cursor = Some(next);
            }
            match match_projects(&selector, &nodes) {
                ProjectResolution::Resolved(project) => project,
                ProjectResolution::NotFound(name) => {
                    return Err(eyre!("Linear project not found: {name}"))
                }
                ProjectResolution::Ambiguous(projects) => {
                    return Err(eyre!(
                        "Ambiguous Linear project. Candidates: {}",
                        projects
                            .iter()
                            .map(|project| format!("{} ({})", project.name, project.id))
                            .collect::<Vec<_>>()
                            .join(", ")
                    ))
                }
            }
        }
    };
    if project.id.trim().is_empty() || project.name.trim().is_empty() {
        return Err(eyre!("Malformed project identity"));
    }
    if let Some(status) = &args.status {
        let statuses = project_statuses_list_data_with_url(
            client,
            url,
            crate::linear::args::ProjectStatusListArgs {
                limit: 250,
                cursor: None,
                all: true,
            },
        )
        .await?;
        input["statusId"] = serde_json::json!(
            resolve_project_status(status, &statuses.nodes).map_err(|error| eyre!(error))?
        );
    }
    if args.lead.is_some() {
        let lead = super::issue::resolve_assignee_id_with_url(
            client,
            url,
            args.lead.as_deref(),
            "project update",
        )
        .await?;
        if lead.as_deref().is_none_or(|id| id.trim().is_empty()) {
            return Err(eyre!("Malformed lead identity"));
        }
        input["leadId"] = serde_json::json!(lead);
    }
    let data = execute_with_url(
        client,
        url,
        PROJECT_UPDATE_MUTATION,
        serde_json::json!({"id": project.id, "input": input}),
    )
    .await?;
    transform_project_update(data).map_err(|error| eyre!(error))
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
    async fn team_resolution_scans_all_pages_before_accepting_a_match() {
        use wiremock::matchers::{body_json, method};
        use wiremock::{Mock, MockServer, ResponseTemplate};
        for (first_matches, second_matches, succeeds) in [
            (false, true, true),
            (true, true, false),
            (false, false, false),
        ] {
            let server = MockServer::start().await;
            for (after, matches, next, id) in [
                (None, first_matches, Some("next"), "t1"),
                (Some("next"), second_matches, None, "t2"),
            ] {
                let nodes = if matches {
                    serde_json::json!([{"id": id, "key": "GUZ", "name": "Example"}])
                } else {
                    serde_json::json!([])
                };
                Mock::given(method("POST"))
                    .and(body_json(serde_json::json!({"query": TEAMS_QUERY, "variables": {"first": 50, "after": after}})))
                    .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"data": {"teams": {"nodes": nodes, "pageInfo": {"hasNextPage": next.is_some(), "endCursor": next}}}})))
                    .expect(1)
                    .mount(&server)
                    .await;
            }
            let result =
                teams_get_data_with_url(&reqwest::Client::new(), &server.uri(), "GUZ").await;
            assert_eq!(result.is_ok(), succeeds, "{result:?}");
            if succeeds {
                assert_eq!(result.unwrap().id, "t2");
            }
        }
    }

    #[tokio::test]
    async fn project_update_partial_combined_and_cleared_fields() {
        use wiremock::matchers::{body_json, method};
        use wiremock::{Mock, MockServer, ResponseTemplate};
        let id = "12345678-1234-1234-1234-123456789abc";
        let status_id = "22345678-1234-1234-1234-123456789abc";
        let lead_id = "32345678-1234-1234-1234-123456789abc";
        for (fields, input) in [
            (
                serde_json::json!({"name": "New"}),
                serde_json::json!({"name": "New"}),
            ),
            (
                serde_json::json!({"content": "    code\n\n# Heading\n"}),
                serde_json::json!({"content": "    code\n\n# Heading\n"}),
            ),
            (
                serde_json::json!({"clearDescription": true, "clearContent": true, "clearLead": true, "clearStartDate": true, "clearTargetDate": true, "priority": 0}),
                serde_json::json!({"description": "", "content": "", "leadId": null, "startDate": null, "targetDate": null, "priority": 0}),
            ),
            (
                serde_json::json!({"name": "New", "description": "Summary", "content": "    code\n", "status": " custom ", "lead": "me", "startDate": "2028-02-29", "targetDate": "2028-03-01", "priority": 4}),
                serde_json::json!({"name": "New", "description": "Summary", "content": "    code\n", "statusId": status_id, "leadId": lead_id, "startDate": "2028-02-29", "targetDate": "2028-03-01", "priority": 4}),
            ),
            (
                serde_json::json!({"status": status_id, "lead": lead_id}),
                serde_json::json!({"statusId": status_id, "leadId": lead_id}),
            ),
        ] {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .and(body_json(
                    serde_json::json!({"query": PROJECT_GET_QUERY, "variables": {"id": id}}),
                ))
                .respond_with(ResponseTemplate::new(200).set_body_json(
                    serde_json::json!({"data": {"project": {"id": id, "name": "Existing"}}}),
                ))
                .expect(1)
                .mount(&server)
                .await;
            if fields.get("status").is_some() {
                for (after, nodes, next) in [
                    (None, serde_json::json!([]), Some("next")),
                    (
                        Some("next"),
                        serde_json::json!([{"id": status_id, "name": "Custom", "type": "planned", "color": "#fff", "position": 1.0}]),
                        None,
                    ),
                ] {
                    Mock::given(method("POST")).and(body_json(serde_json::json!({"query": PROJECT_STATUSES_QUERY, "variables": {"first": 250, "after": after}})))
                        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"data": {"projectStatuses": {"nodes": nodes, "pageInfo": {"hasNextPage": next.is_some(), "endCursor": next}}}}))).expect(1).mount(&server).await;
                }
            }
            if fields["lead"] == "me" {
                Mock::given(method("POST")).and(body_json(serde_json::json!({"query": super::super::auth::VIEWER_QUERY, "variables": {}})))
                    .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"data": {"viewer": {"id": lead_id, "name": "Ada", "email": null}}}))).expect(1).mount(&server).await;
            }
            let output: serde_json::Value = serde_json::from_str(include_str!(
                "../../tests/contract_samples/linear_project_update.json"
            ))
            .unwrap();
            Mock::given(method("POST")).and(body_json(serde_json::json!({"query": PROJECT_UPDATE_MUTATION, "variables": {"id": id, "input": input}})))
                .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"data": {"projectUpdate": {"success": true, "project": output}}}))).expect(1).mount(&server).await;
            let mut args = serde_json::json!({"id": id});
            args.as_object_mut()
                .unwrap()
                .extend(fields.as_object().unwrap().clone());
            let args = serde_json::from_value(args).unwrap();
            let updated =
                projects_update_data_with_url(&reqwest::Client::new(), &server.uri(), &args)
                    .await
                    .unwrap();
            let result = crate::mcp::tools::to_dual_result(updated).unwrap();
            assert_eq!(result["structuredContent"], output);
            assert_eq!(
                serde_json::from_str::<serde_json::Value>(
                    result["content"][0]["text"].as_str().unwrap()
                )
                .unwrap(),
                output
            );
            assert!(server
                .received_requests()
                .await
                .unwrap()
                .iter()
                .all(
                    |request| !serde_json::from_slice::<serde_json::Value>(&request.body).unwrap()
                        ["query"]
                        .as_str()
                        .unwrap()
                        .contains("states(")
                ));
        }
    }

    #[tokio::test]
    async fn project_update_name_resolution_and_status_ambiguity_scan_all_pages() {
        use wiremock::matchers::{body_json, method};
        use wiremock::{Mock, MockServer, ResponseTemplate};
        let id = "12345678-1234-1234-1234-123456789abc";
        for (project_count, status_count, succeeds) in [
            (0, 0, false),
            (1, 0, false),
            (2, 1, false),
            (1, 2, false),
            (1, 1, true),
        ] {
            let server = MockServer::start().await;
            Mock::given(method("POST")).and(body_json(serde_json::json!({"query": TEAMS_QUERY, "variables": {"first": 50, "after": null}})))
                .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"data": {"teams": {"nodes": [{"id": "t1", "key": "GUZ", "name": "Example"}], "pageInfo": {"hasNextPage": false, "endCursor": null}}}}))).expect(1).mount(&server).await;
            for (index, after) in [None, Some("next")].into_iter().enumerate() {
                let nodes = if index < project_count {
                    serde_json::json!([{"id": if index == 0 { id } else { "p2" }, "name": "Existing"}])
                } else {
                    serde_json::json!([])
                };
                Mock::given(method("POST")).and(body_json(serde_json::json!({"query": TEAM_PROJECTS_QUERY, "variables": {"id": "t1", "first": 50, "after": after}})))
                    .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"data": {"team": {"projects": {"nodes": nodes, "pageInfo": {"hasNextPage": index == 0, "endCursor": if index == 0 { Some("next") } else { None }}}}}}))).expect(1).mount(&server).await;
            }
            if project_count == 1 {
                for (index, after) in [None, Some("next")].into_iter().enumerate() {
                    let nodes = if index < status_count {
                        serde_json::json!([{"id": format!("s{index}"), "name": "Custom", "type": "planned", "color": "#fff", "position": 1.0}])
                    } else {
                        serde_json::json!([])
                    };
                    Mock::given(method("POST")).and(body_json(serde_json::json!({"query": PROJECT_STATUSES_QUERY, "variables": {"first": 250, "after": after}})))
                        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"data": {"projectStatuses": {"nodes": nodes, "pageInfo": {"hasNextPage": index == 0, "endCursor": if index == 0 { Some("next") } else { None }}}}}))).expect(1).mount(&server).await;
                }
            }
            if succeeds {
                let output: serde_json::Value = serde_json::from_str(include_str!(
                    "../../tests/contract_samples/linear_project_update.json"
                ))
                .unwrap();
                Mock::given(method("POST")).and(body_json(serde_json::json!({"query": PROJECT_UPDATE_MUTATION, "variables": {"id": id, "input": {"statusId": "s0"}}})))
                    .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"data": {"projectUpdate": {"success": true, "project": output}}}))).expect(1).mount(&server).await;
            }
            let args = serde_json::from_value(
                serde_json::json!({"id": " existing ", "team": "GUZ", "status": "CUSTOM"}),
            )
            .unwrap();
            let result =
                projects_update_data_with_url(&reqwest::Client::new(), &server.uri(), &args).await;
            assert_eq!(result.is_ok(), succeeds, "{result:?}");
            let writes = server
                .received_requests()
                .await
                .unwrap()
                .iter()
                .filter(|request| {
                    serde_json::from_slice::<serde_json::Value>(&request.body).unwrap()["query"]
                        == PROJECT_UPDATE_MUTATION
                })
                .count();
            assert_eq!(writes, usize::from(succeeds));
        }
    }

    #[tokio::test]
    async fn project_update_rejects_invalid_inputs_and_failed_reads_or_mutations() {
        use wiremock::matchers::{body_partial_json, method};
        use wiremock::{Mock, MockServer, ResponseTemplate};
        let id = "12345678-1234-1234-1234-123456789abc";
        let args: crate::linear::args::ProjectUpdateArgs =
            serde_json::from_value(serde_json::json!({"id": id, "name": "New"})).unwrap();
        let server = MockServer::start().await;
        for fields in [
            serde_json::json!({"id": id}),
            serde_json::json!({"id": id, "name": " "}),
            serde_json::json!({"id": id, "startDate": "2026-02-29"}),
            serde_json::json!({"id": id, "lead": "Ada"}),
            serde_json::json!({"id": id, "priority": 5}),
        ] {
            let invalid = serde_json::from_value(fields).unwrap();
            assert!(projects_update_data_with_url(
                &reqwest::Client::new(),
                &server.uri(),
                &invalid
            )
            .await
            .is_err());
        }
        assert!(server.received_requests().await.unwrap().is_empty());
        for (query, status, response) in [
            (
                PROJECT_GET_QUERY,
                200,
                serde_json::json!({"data": {"project": null}}),
            ),
            (
                PROJECT_GET_QUERY,
                200,
                serde_json::json!({"data": {"project": {"id": id}}}),
            ),
            (
                PROJECT_GET_QUERY,
                200,
                serde_json::json!({"data": {"project": {"id": "22345678-1234-1234-1234-123456789abc", "name": "Other"}}}),
            ),
            (
                PROJECT_UPDATE_MUTATION,
                401,
                serde_json::json!({"errors": [{"message": "Denied"}]}),
            ),
            (
                PROJECT_UPDATE_MUTATION,
                200,
                serde_json::json!({"errors": [{"message": "Partial failure"}], "data": {"projectUpdate": {"success": true, "project": {"id": id, "name": "New"}}}}),
            ),
            (
                PROJECT_UPDATE_MUTATION,
                200,
                serde_json::json!({"data": {"projectUpdate": {"success": false}}}),
            ),
            (
                PROJECT_UPDATE_MUTATION,
                200,
                serde_json::json!({"data": {"projectUpdate": {"success": true, "project": {"id": id, "name": "New"}}}}),
            ),
            (
                PROJECT_UPDATE_MUTATION,
                200,
                serde_json::json!({"data": {}}),
            ),
        ] {
            let server = MockServer::start().await;
            if query == PROJECT_UPDATE_MUTATION {
                Mock::given(method("POST"))
                    .and(body_partial_json(
                        serde_json::json!({"query": PROJECT_GET_QUERY}),
                    ))
                    .respond_with(ResponseTemplate::new(200).set_body_json(
                        serde_json::json!({"data": {"project": {"id": id, "name": "Existing"}}}),
                    ))
                    .expect(1)
                    .mount(&server)
                    .await;
            }
            Mock::given(method("POST"))
                .and(body_partial_json(serde_json::json!({"query": query})))
                .respond_with(ResponseTemplate::new(status).set_body_json(response))
                .expect(1)
                .mount(&server)
                .await;
            assert!(
                projects_update_data_with_url(&reqwest::Client::new(), &server.uri(), &args)
                    .await
                    .is_err()
            );
            if query == PROJECT_GET_QUERY {
                assert!(server
                    .received_requests()
                    .await
                    .unwrap()
                    .iter()
                    .all(|request| {
                        serde_json::from_slice::<serde_json::Value>(&request.body).unwrap()["query"]
                            == PROJECT_GET_QUERY
                    }));
            }
        }
    }

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
