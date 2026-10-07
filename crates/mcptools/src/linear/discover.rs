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

pub const PROJECT_MILESTONES_QUERY: &str = "query ($id: String!, $first: Int!, $after: String) { project(id: $id) { id name projectMilestones(first: $first, after: $after) { nodes { id name description targetDate status sortOrder project { id name } } pageInfo { hasNextPage endCursor } } } }";

pub const PROJECT_MILESTONE_CREATE_MUTATION: &str = "mutation ($input: ProjectMilestoneCreateInput!) { projectMilestoneCreate(input: $input) { success projectMilestone { id name description targetDate status sortOrder project { id name } } } }";

pub async fn project_milestones_create_data(
    client: &reqwest::Client,
    args: &crate::linear::args::ProjectMilestoneCreateArgs,
) -> Result<mcptools_core::linear::ProjectMilestone> {
    project_milestones_create_data_with_url(client, LINEAR_API_URL, args).await
}

async fn project_milestones_create_data_with_url(
    client: &reqwest::Client,
    url: &str,
    args: &crate::linear::args::ProjectMilestoneCreateArgs,
) -> Result<mcptools_core::linear::ProjectMilestone> {
    let mut input = mcptools_core::linear::project_milestone_create_input(args)
        .map_err(|error| eyre!(error))?;
    let project =
        resolve_project_with_url(client, url, &args.project, args.team.as_deref()).await?;
    if !mcptools_core::linear::is_uuid(&project.id) {
        return Err(eyre!("Malformed project identity"));
    }
    input["projectId"] = serde_json::json!(project.id);
    let data = execute_with_url(
        client,
        url,
        PROJECT_MILESTONE_CREATE_MUTATION,
        serde_json::json!({"input": input}),
    )
    .await?;
    mcptools_core::linear::transform_project_milestone_create(data, &project)
        .map_err(|error| eyre!(error))
}

pub async fn project_milestones_list_data(
    client: &reqwest::Client,
    args: crate::linear::args::ProjectMilestoneListArgs,
) -> Result<mcptools_core::linear::ProjectMilestoneListOutput> {
    project_milestones_list_data_with_url(client, LINEAR_API_URL, args).await
}

async fn project_milestones_list_data_with_url(
    client: &reqwest::Client,
    url: &str,
    args: crate::linear::args::ProjectMilestoneListArgs,
) -> Result<mcptools_core::linear::ProjectMilestoneListOutput> {
    args.validate().map_err(|error| eyre!(error))?;
    let selector =
        parse_project_selector(&args.project).ok_or_else(|| eyre!("Missing project selector"))?;
    let id = match &selector {
        mcptools_core::linear::ProjectSelector::Id(id) => id.clone(),
        mcptools_core::linear::ProjectSelector::Name(_) => {
            resolve_project_with_url(client, url, &args.project, args.team.as_deref())
                .await?
                .id
        }
    };
    if !mcptools_core::linear::is_uuid(&id) {
        return Err(eyre!("Malformed project identity"));
    }
    let mut cursor = args.cursor;
    let mut seen = std::collections::HashSet::new();
    if let Some(cursor) = &cursor {
        seen.insert(cursor.clone());
    }
    let mut nodes = Vec::new();
    let mut project_name = None;
    loop {
        let data = execute_with_url(
            client,
            url,
            PROJECT_MILESTONES_QUERY,
            serde_json::json!({"id": id, "first": args.limit, "after": cursor}),
        )
        .await?;
        let page = mcptools_core::linear::transform_project_milestones(data, &id)
            .map_err(|error| eyre!(error))?;
        if project_name.get_or_insert_with(|| page.project.name.clone()) != &page.project.name {
            return Err(eyre!("Project identity changed across milestone pages"));
        }
        nodes.extend(page.nodes);
        if page.page_info.has_next && page.page_info.end_cursor.is_none() {
            return Err(eyre!("Milestone pagination cursor missing or blank"));
        }
        if let Some(next) = &page.page_info.end_cursor {
            if !seen.insert(next.clone()) {
                return Err(eyre!("Milestone pagination cursor did not progress"));
            }
        }
        if !args.all || !page.page_info.has_next {
            return Ok(mcptools_core::linear::ProjectMilestoneListOutput {
                project: page.project,
                nodes,
                page_info: page.page_info,
            });
        }
        cursor = page.page_info.end_cursor;
    }
}

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
        if data["teams"]["pageInfo"].get("endCursor").is_none() {
            return Err(eyre!("Team pageInfo missing endCursor"));
        }
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
        project_update_input, resolve_project_status, transform_project_update,
    };
    let mut input = project_update_input(args).map_err(|error| eyre!(error))?;
    let project = resolve_project_with_url(client, url, &args.id, args.team.as_deref()).await?;
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

async fn resolve_project_with_url(
    client: &reqwest::Client,
    url: &str,
    project_selector: &str,
    team_selector: Option<&str>,
) -> Result<Project> {
    use mcptools_core::linear::ProjectSelector;
    let selector = parse_project_selector(project_selector)
        .ok_or_else(|| eyre!("Missing project selector"))?;
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
                team_selector.ok_or_else(|| eyre!("Project names require team"))?,
            )
            .await?;
            if team.id.trim().is_empty()
                || team.key.trim().is_empty()
                || team.name.trim().is_empty()
            {
                return Err(eyre!("Malformed team identity"));
            }
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
                if !data["team"]["id"]
                    .as_str()
                    .is_some_and(|id| id.eq_ignore_ascii_case(&team.id))
                {
                    return Err(eyre!("Project lookup returned a different team identity"));
                }
                if data["team"]["projects"]["pageInfo"]
                    .get("endCursor")
                    .is_none()
                {
                    return Err(eyre!("Project pageInfo missing endCursor"));
                }
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
    Ok(project)
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
    async fn milestone_pages_preserve_scope_nulls_whitespace_and_final_cursor() {
        use wiremock::matchers::{body_json, method};
        use wiremock::{Mock, MockServer, ResponseTemplate};
        let sample: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/contract_samples/linear_project_milestone_list.json"
        ))
        .unwrap();
        let id = sample["project"]["id"].as_str().unwrap();
        for (all, empty) in [(false, false), (true, false), (true, true)] {
            let server = MockServer::start().await;
            let pages = if all && !empty { 2 } else { 1 };
            for index in 0..pages {
                let nodes = if empty {
                    serde_json::json!([])
                } else {
                    serde_json::json!([sample["nodes"][index]])
                };
                let next = if index == 0 && !empty {
                    "next"
                } else {
                    "final"
                };
                Mock::given(method("POST"))
                    .and(body_json(serde_json::json!({"query": PROJECT_MILESTONES_QUERY, "variables": {"id": id.to_uppercase(), "first": 1, "after": if index == 0 { "start" } else { "next" }}})))
                    .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"data": {"project": {
                        "id": id, "name": "Example", "projectMilestones": {"nodes": nodes, "pageInfo": {"hasNextPage": index == 0 && !empty, "endCursor": next}}
                    }}}))).expect(1).mount(&server).await;
            }
            let args = serde_json::from_value(serde_json::json!({"project": id.to_uppercase(), "limit": 1, "cursor": "start", "all": all})).unwrap();
            let output =
                project_milestones_list_data_with_url(&reqwest::Client::new(), &server.uri(), args)
                    .await
                    .unwrap();
            assert_eq!(output.nodes.len(), if empty { 0 } else { pages });
            assert_eq!(output.page_info.has_next, !all && !empty);
            assert_eq!(
                output.page_info.end_cursor.as_deref(),
                Some(if all || empty { "final" } else { "next" })
            );
            let result = crate::mcp::tools::to_dual_result(output).unwrap();
            let text: serde_json::Value =
                serde_json::from_str(result["content"][0]["text"].as_str().unwrap()).unwrap();
            assert_eq!(text, result["structuredContent"]);
            assert_eq!(text["project"], sample["project"]);
            if !empty {
                assert_eq!(text["nodes"][0], sample["nodes"][0]);
                assert_eq!(text["nodes"][0]["targetDate"], "2026-12-01");
                if all {
                    assert_eq!(text["nodes"][1], sample["nodes"][1]);
                    assert!(text["nodes"][1].get("description").unwrap().is_null());
                    assert!(text["nodes"][1].get("targetDate").unwrap().is_null());
                }
            }
        }
    }

    #[tokio::test]
    async fn milestone_name_resolution_scans_all_pages_and_rejects_ambiguity() {
        use wiremock::matchers::{body_json, method};
        use wiremock::{Mock, MockServer, ResponseTemplate};
        let id = "12345678-1234-1234-1234-123456789abc";
        for count in 0..=2 {
            let server = MockServer::start().await;
            for (index, after) in [None, Some("teams-next")].into_iter().enumerate() {
                let nodes = if index == 0 {
                    serde_json::json!([])
                } else {
                    serde_json::json!([{"id": "t1", "key": "GUZ", "name": "Team"}])
                };
                Mock::given(method("POST"))
                    .and(body_json(serde_json::json!({"query": TEAMS_QUERY, "variables": {"first": 50, "after": after}})))
                    .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"data": {"teams": {"nodes": nodes, "pageInfo": {"hasNextPage": index == 0, "endCursor": if index == 0 { Some("teams-next") } else { None }}}}}))).expect(1).mount(&server).await;
            }
            for (index, after) in [None, Some("projects-next")].into_iter().enumerate() {
                let nodes = if index >= 2 - count {
                    serde_json::json!([{"id": if index == 1 { id } else { "22345678-1234-1234-1234-123456789abc" }, "name": "Example"}])
                } else {
                    serde_json::json!([])
                };
                Mock::given(method("POST"))
                    .and(body_json(serde_json::json!({"query": TEAM_PROJECTS_QUERY, "variables": {"id": "t1", "first": 50, "after": after}})))
                    .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"data": {"team": {"id": "t1", "projects": {"nodes": nodes, "pageInfo": {"hasNextPage": index == 0, "endCursor": if index == 0 { Some("projects-next") } else { None }}}}}}))).expect(1).mount(&server).await;
            }
            if count == 1 {
                Mock::given(method("POST"))
                    .and(body_json(serde_json::json!({"query": PROJECT_MILESTONES_QUERY, "variables": {"id": id, "first": 25, "after": null}})))
                    .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"data": {"project": {"id": id, "name": "Example", "projectMilestones": {"nodes": [], "pageInfo": {"hasNextPage": false, "endCursor": null}}}}}))).expect(1).mount(&server).await;
            }
            let args = serde_json::from_value(
                serde_json::json!({"project": " example ", "team": " GUZ "}),
            )
            .unwrap();
            let result =
                project_milestones_list_data_with_url(&reqwest::Client::new(), &server.uri(), args)
                    .await;
            assert_eq!(result.is_ok(), count == 1, "{result:?}");
            assert_eq!(
                server.received_requests().await.unwrap().len(),
                if count == 1 { 5 } else { 4 }
            );
        }
    }

    #[tokio::test]
    async fn milestone_errors_malformed_payloads_and_wrong_scope_never_succeed() {
        use wiremock::matchers::{body_json, method};
        use wiremock::{Mock, MockServer, ResponseTemplate};
        let sample: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/contract_samples/linear_project_milestone_list.json"
        ))
        .unwrap();
        let id = sample["project"]["id"].as_str().unwrap();
        let valid = serde_json::json!({"data": {"project": {"id": id, "name": "Example", "projectMilestones": {"nodes": sample["nodes"], "pageInfo": sample["pageInfo"]}}}});
        let mut cases = vec![
            (401, serde_json::json!({"errors": [{"message": "Denied"}]})),
            (
                200,
                serde_json::json!({"errors": [{"message": "Partial error"}], "data": valid["data"]}),
            ),
            (200, serde_json::json!({"data": {"project": null}})),
            (200, serde_json::json!({"data": {}})),
        ];
        for field in [
            "id",
            "name",
            "description",
            "targetDate",
            "status",
            "sortOrder",
            "project",
        ] {
            let mut malformed = valid.clone();
            malformed["data"]["project"]["projectMilestones"]["nodes"][0]
                .as_object_mut()
                .unwrap()
                .remove(field);
            cases.push((200, malformed));
        }
        for pointer in [
            "/data/project/id",
            "/data/project/projectMilestones/nodes/0/project/id",
        ] {
            let mut malformed = valid.clone();
            *malformed.pointer_mut(pointer).unwrap() =
                serde_json::json!("42345678-1234-1234-1234-123456789abc");
            cases.push((200, malformed));
        }
        for field in ["hasNextPage", "endCursor"] {
            let mut malformed = valid.clone();
            malformed["data"]["project"]["projectMilestones"]["pageInfo"]
                .as_object_mut()
                .unwrap()
                .remove(field);
            cases.push((200, malformed));
        }
        for (pointer, value) in [
            (
                "/data/project/projectMilestones/nodes/0/status",
                serde_json::json!("unknown"),
            ),
            (
                "/data/project/projectMilestones/nodes/0/sortOrder",
                serde_json::json!("first"),
            ),
            (
                "/data/project/projectMilestones/pageInfo/hasNextPage",
                serde_json::json!("false"),
            ),
            (
                "/data/project/projectMilestones/pageInfo/endCursor",
                serde_json::json!(7),
            ),
        ] {
            let mut malformed = valid.clone();
            *malformed.pointer_mut(pointer).unwrap() = value;
            cases.push((200, malformed));
        }
        for (status, body) in cases {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .and(body_json(serde_json::json!({"query": PROJECT_MILESTONES_QUERY, "variables": {"id": id, "first": 25, "after": null}})))
                .respond_with(ResponseTemplate::new(status).set_body_json(body)).expect(1).mount(&server).await;
            let args = serde_json::from_value(serde_json::json!({"project": id})).unwrap();
            assert!(project_milestones_list_data_with_url(
                &reqwest::Client::new(),
                &server.uri(),
                args
            )
            .await
            .is_err());
        }
    }

    #[tokio::test]
    async fn milestone_and_update_shared_resolution_accept_uppercase_team_uuid() {
        use wiremock::matchers::{body_json, method};
        use wiremock::{Mock, MockServer, ResponseTemplate};
        let server = MockServer::start().await;
        let team = "22345678-1234-1234-1234-123456789abc";
        let project = "12345678-1234-1234-1234-123456789abc";
        for (query, variables, data) in [
            (
                TEAMS_QUERY,
                serde_json::json!({"first": 50, "after": null}),
                serde_json::json!({"teams": {"nodes": [{"id": team, "key": "GUZ", "name": "Team"}], "pageInfo": {"hasNextPage": false, "endCursor": null}}}),
            ),
            (
                TEAM_PROJECTS_QUERY,
                serde_json::json!({"id": team, "first": 50, "after": null}),
                serde_json::json!({"team": {"id": team, "projects": {"nodes": [{"id": project, "name": "Example"}], "pageInfo": {"hasNextPage": false, "endCursor": null}}}}),
            ),
        ] {
            Mock::given(method("POST"))
                .and(body_json(
                    serde_json::json!({"query": query, "variables": variables}),
                ))
                .respond_with(
                    ResponseTemplate::new(200).set_body_json(serde_json::json!({"data": data})),
                )
                .expect(2)
                .mount(&server)
                .await;
        }
        Mock::given(method("POST"))
            .and(body_json(serde_json::json!({"query": PROJECT_MILESTONES_QUERY, "variables": {"id": project, "first": 25, "after": null}})))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"data": {"project": {"id": project, "name": "Example", "projectMilestones": {"nodes": [], "pageInfo": {"hasNextPage": false, "endCursor": null}}}}})))
            .expect(1).mount(&server).await;
        let client = reqwest::Client::new();
        let selector = team.to_uppercase();
        let resolved =
            resolve_project_with_url(&client, &server.uri(), " example ", Some(&selector))
                .await
                .unwrap();
        assert_eq!(resolved.id, project);
        let args =
            serde_json::from_value(serde_json::json!({"project": " example ", "team": selector}))
                .unwrap();
        let output = project_milestones_list_data_with_url(&client, &server.uri(), args)
            .await
            .unwrap();
        assert_eq!(output.project, resolved);
        assert!(output.nodes.is_empty());
    }

    #[tokio::test]
    async fn milestone_missing_stalled_repeated_and_later_page_errors_discard_output() {
        use wiremock::matchers::{body_json, method};
        use wiremock::{Mock, MockServer, ResponseTemplate};
        let id = "12345678-1234-1234-1234-123456789abc";
        for (all, has_next, cursors) in [
            (false, true, vec![None]),
            (false, true, vec![Some(" ")]),
            (false, true, vec![Some("start")]),
            (true, true, vec![None]),
            (true, true, vec![Some(" ")]),
            (true, true, vec![Some("start")]),
            (true, true, vec![Some("a"), Some("b"), Some("a")]),
            (false, false, vec![Some(" ")]),
            (false, false, vec![Some("start")]),
            (true, false, vec![Some(" ")]),
            (true, false, vec![Some("start")]),
            (false, false, vec![Some("bad\ncursor")]),
        ] {
            let server = MockServer::start().await;
            let mut after = Some("start");
            for next in cursors {
                Mock::given(method("POST"))
                    .and(body_json(serde_json::json!({"query": PROJECT_MILESTONES_QUERY, "variables": {"id": id, "first": 25, "after": after}})))
                    .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"data": {"project": {"id": id, "name": "Example", "projectMilestones": {"nodes": [], "pageInfo": {"hasNextPage": has_next, "endCursor": next}}}}}))).expect(1).mount(&server).await;
                after = next;
            }
            let args = serde_json::from_value(
                serde_json::json!({"project": id, "cursor": "start", "all": all}),
            )
            .unwrap();
            assert!(project_milestones_list_data_with_url(
                &reqwest::Client::new(),
                &server.uri(),
                args
            )
            .await
            .unwrap_err()
            .to_string()
            .contains("pagination cursor"));
        }
        let sample: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/contract_samples/linear_project_milestone_list.json"
        ))
        .unwrap();
        for body in [
            serde_json::json!({"data": {"project": null}}),
            serde_json::json!({"data": {"project": {"id": id, "name": "Example", "projectMilestones": {"nodes": [], "pageInfo": {"hasNextPage": false}}}}}),
            serde_json::json!({"data": {"project": {"id": id, "name": "Renamed", "projectMilestones": {"nodes": [], "pageInfo": {"hasNextPage": false, "endCursor": null}}}}}),
        ] {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .and(body_json(serde_json::json!({"query": PROJECT_MILESTONES_QUERY, "variables": {"id": id, "first": 25, "after": null}})))
                .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"data": {"project": {"id": id, "name": "Example", "projectMilestones": {"nodes": sample["nodes"], "pageInfo": {"hasNextPage": true, "endCursor": "next"}}}}}))).expect(1).mount(&server).await;
            Mock::given(method("POST"))
                .and(body_json(serde_json::json!({"query": PROJECT_MILESTONES_QUERY, "variables": {"id": id, "first": 25, "after": "next"}})))
                .respond_with(ResponseTemplate::new(200).set_body_json(body)).expect(1).mount(&server).await;
            let args =
                serde_json::from_value(serde_json::json!({"project": id, "all": true})).unwrap();
            assert!(project_milestones_list_data_with_url(
                &reqwest::Client::new(),
                &server.uri(),
                args
            )
            .await
            .is_err());
        }
    }

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
                    .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"data": {"team": {"id": "t1", "projects": {"nodes": nodes, "pageInfo": {"hasNextPage": index == 0, "endCursor": if index == 0 { Some("next") } else { None }}}}}}))).expect(1).mount(&server).await;
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
    async fn milestone_create_validates_inputs_and_mutation_responses() {
        use mcptools_core::linear::{project_milestone_create_input, ProjectMilestoneCreateArgs};
        use wiremock::matchers::{body_json, method};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let id = "12345678-1234-1234-1234-123456789abc";
        let sample: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/contract_samples/linear_project_milestone_create.json"
        ))
        .unwrap();
        for (fields, expected) in [
            (
                serde_json::json!({}),
                serde_json::json!({"name": "Release"}),
            ),
            (
                serde_json::json!({"team": null, "description": null, "targetDate": null, "sortOrder": null}),
                serde_json::json!({"name": "Release"}),
            ),
            (
                serde_json::json!({"description": "    code\n\n# Heading\n", "targetDate": "2028-02-29", "sortOrder": -1.5}),
                serde_json::json!({"name": "Release", "description": "    code\n\n# Heading\n", "targetDate": "2028-02-29", "sortOrder": -1.5}),
            ),
            (
                serde_json::json!({"description": ""}),
                serde_json::json!({"name": "Release", "description": ""}),
            ),
            (
                serde_json::json!({"description": " \t\n "}),
                serde_json::json!({"name": "Release", "description": " \t\n "}),
            ),
        ] {
            let server = MockServer::start().await;
            let mut raw = serde_json::json!({"project": id, "name": " Release "});
            raw.as_object_mut()
                .unwrap()
                .extend(fields.as_object().unwrap().clone());
            let args: ProjectMilestoneCreateArgs = serde_json::from_value(raw).unwrap();
            let mut input = project_milestone_create_input(&args).unwrap();
            assert_eq!(input, expected);
            input["projectId"] = serde_json::json!(id);
            let mut sample = sample.clone();
            if fields
                .get("description")
                .is_some_and(serde_json::Value::is_null)
            {
                sample["description"] = serde_json::Value::Null;
                sample["targetDate"] = serde_json::Value::Null;
            }
            Mock::given(method("POST"))
                .and(body_json(
                    serde_json::json!({"query": PROJECT_GET_QUERY, "variables": {"id": id}}),
                ))
                .respond_with(
                    ResponseTemplate::new(200)
                        .set_body_json(serde_json::json!({"data": {"project": sample["project"]}})),
                )
                .expect(1)
                .mount(&server)
                .await;
            Mock::given(method("POST"))
                .and(body_json(serde_json::json!({"query": PROJECT_MILESTONE_CREATE_MUTATION, "variables": {"input": input}})))
                .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"data": {"projectMilestoneCreate": {"success": true, "projectMilestone": sample}}})))
                .expect(1).mount(&server).await;
            let created = project_milestones_create_data_with_url(
                &reqwest::Client::new(),
                &server.uri(),
                &args,
            )
            .await
            .unwrap();
            assert_eq!(serde_json::to_value(&created).unwrap(), sample);
            let dual = crate::mcp::tools::to_dual_result(created).unwrap();
            let text: serde_json::Value =
                serde_json::from_str(dual["content"][0]["text"].as_str().unwrap()).unwrap();
            assert_eq!(text, dual["structuredContent"]);
        }
        for fields in [
            serde_json::json!({"project": " "}),
            serde_json::json!({"project": "Example"}),
            serde_json::json!({"name": " "}),
            serde_json::json!({"team": " "}),
            serde_json::json!({"targetDate": "2026-02-29"}),
            serde_json::json!({"targetDate": "2026-2-01"}),
            serde_json::json!({"targetDate": "0000-01-01"}),
            serde_json::json!({"project": "12345678-1234-1234-1234-123456789xyz"}),
        ] {
            let server = MockServer::start().await;
            let mut raw = serde_json::json!({"project": id, "name": "Release"});
            raw.as_object_mut()
                .unwrap()
                .extend(fields.as_object().unwrap().clone());
            let args = serde_json::from_value(raw).unwrap();
            assert!(project_milestones_create_data_with_url(
                &reqwest::Client::new(),
                &server.uri(),
                &args
            )
            .await
            .is_err());
            assert!(server.received_requests().await.unwrap().is_empty());
        }
        let mut args: ProjectMilestoneCreateArgs =
            serde_json::from_value(serde_json::json!({"project": id, "name": "Release"})).unwrap();
        for order in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            args.sort_order = Some(order);
            assert!(project_milestone_create_input(&args).is_err());
        }
        args.sort_order = None;
        let valid = serde_json::json!({"data": {"projectMilestoneCreate": {"success": true, "projectMilestone": sample}}});
        let mut responses = vec![
            serde_json::json!({"data": {}}),
            serde_json::json!({"data": {"projectMilestoneCreate": {"success": true}}}),
            serde_json::json!({"data": {"projectMilestoneCreate": {"projectMilestone": sample}}}),
            serde_json::json!({"data": {"projectMilestoneCreate": null}}),
        ];
        for (pointer, value) in [
            (
                "/data/projectMilestoneCreate/success",
                serde_json::json!(false),
            ),
            (
                "/data/projectMilestoneCreate/success",
                serde_json::Value::Null,
            ),
            (
                "/data/projectMilestoneCreate/success",
                serde_json::json!("true"),
            ),
            (
                "/data/projectMilestoneCreate/projectMilestone",
                serde_json::Value::Null,
            ),
            (
                "/data/projectMilestoneCreate/projectMilestone/id",
                serde_json::json!("bad"),
            ),
            (
                "/data/projectMilestoneCreate/projectMilestone/name",
                serde_json::json!(" "),
            ),
            (
                "/data/projectMilestoneCreate/projectMilestone/project/id",
                serde_json::json!("22345678-1234-1234-1234-123456789abc"),
            ),
            (
                "/data/projectMilestoneCreate/projectMilestone/status",
                serde_json::json!("bad"),
            ),
            (
                "/data/projectMilestoneCreate/projectMilestone/sortOrder",
                serde_json::json!("1"),
            ),
            (
                "/data/projectMilestoneCreate/projectMilestone/description",
                serde_json::json!(3),
            ),
            (
                "/data/projectMilestoneCreate/projectMilestone/targetDate",
                serde_json::json!("2026-02-29"),
            ),
        ] {
            let mut response = valid.clone();
            *response.pointer_mut(pointer).unwrap() = value;
            responses.push(response);
        }
        for field in [
            "id",
            "name",
            "description",
            "targetDate",
            "status",
            "sortOrder",
            "project",
        ] {
            let mut response = valid.clone();
            response["data"]["projectMilestoneCreate"]["projectMilestone"]
                .as_object_mut()
                .unwrap()
                .remove(field);
            responses.push(response);
        }
        let mut partial = valid.clone();
        partial["errors"] = serde_json::json!([{"message": "partial failure"}]);
        responses.push(partial);
        for response in responses {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .and(body_json(
                    serde_json::json!({"query": PROJECT_GET_QUERY, "variables": {"id": id}}),
                ))
                .respond_with(
                    ResponseTemplate::new(200)
                        .set_body_json(serde_json::json!({"data": {"project": sample["project"]}})),
                )
                .expect(1)
                .mount(&server)
                .await;
            Mock::given(method("POST")).and(body_json(serde_json::json!({"query": PROJECT_MILESTONE_CREATE_MUTATION, "variables": {"input": {"projectId": id, "name": "Release"}}})))
                .respond_with(ResponseTemplate::new(200).set_body_json(response))
                .expect(1).mount(&server).await;
            assert!(project_milestones_create_data_with_url(
                &reqwest::Client::new(),
                &server.uri(),
                &args
            )
            .await
            .is_err());
        }
    }

    #[tokio::test]
    async fn milestone_create_resolves_all_pages_and_never_mutates_failed_selectors() {
        use wiremock::matchers::{body_json, method};
        use wiremock::{Mock, MockServer, ResponseTemplate};
        let project = "12345678-1234-1234-1234-123456789abc";
        let team = "32345678-1234-1234-1234-123456789abc";
        let sample: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/contract_samples/linear_project_milestone_create.json"
        ))
        .unwrap();
        for mode in [
            "resolved",
            "missing",
            "ambiguous",
            "team-missing",
            "team-ambiguous",
        ] {
            let server = MockServer::start().await;
            let teams = match mode {
                "team-missing" => serde_json::json!([]),
                "team-ambiguous" => {
                    serde_json::json!([{"id": team, "key": "GUZ", "name": "Team"}, {"id": project, "key": "GUZ", "name": "Other"}])
                }
                _ => serde_json::json!([{"id": team, "key": "GUZ", "name": "Team"}]),
            };
            for (after, nodes, has_next, next) in [
                (None, serde_json::json!([]), true, Some("teams-next")),
                (Some("teams-next"), teams, false, None),
            ] {
                Mock::given(method("POST")).and(body_json(serde_json::json!({"query": TEAMS_QUERY, "variables": {"first": 50, "after": after}})))
                    .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"data": {"teams": {"nodes": nodes, "pageInfo": {"hasNextPage": has_next, "endCursor": next}}}})))
                    .expect(1).mount(&server).await;
            }
            if !mode.starts_with("team-") {
                let nodes = match mode {
                    "missing" => serde_json::json!([]),
                    "ambiguous" => {
                        serde_json::json!([{"id": project, "name": "Example"}, {"id": team, "name": "Example"}])
                    }
                    _ => serde_json::json!([{"id": project, "name": "Example"}]),
                };
                for (after, nodes, has_next, next) in [
                    (None, serde_json::json!([]), true, Some("projects-next")),
                    (Some("projects-next"), nodes, false, None),
                ] {
                    Mock::given(method("POST")).and(body_json(serde_json::json!({"query": TEAM_PROJECTS_QUERY, "variables": {"id": team, "first": 50, "after": after}})))
                        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"data": {"team": {"id": team, "projects": {"nodes": nodes, "pageInfo": {"hasNextPage": has_next, "endCursor": next}}}}})))
                        .expect(1).mount(&server).await;
                }
            }
            if mode == "resolved" {
                Mock::given(method("POST")).and(body_json(serde_json::json!({"query": PROJECT_MILESTONE_CREATE_MUTATION, "variables": {"input": {"projectId": project, "name": "Release"}}})))
                    .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"data": {"projectMilestoneCreate": {"success": true, "projectMilestone": sample}}})))
                    .expect(1).mount(&server).await;
            }
            let args = serde_json::from_value(
                serde_json::json!({"project": " example ", "team": " GUZ ", "name": "Release"}),
            )
            .unwrap();
            let result = project_milestones_create_data_with_url(
                &reqwest::Client::new(),
                &server.uri(),
                &args,
            )
            .await;
            assert_eq!(result.is_ok(), mode == "resolved", "{mode}: {result:?}");
            let mutations = server
                .received_requests()
                .await
                .unwrap()
                .iter()
                .filter(|request| {
                    request.body_json::<serde_json::Value>().unwrap()["query"]
                        == PROJECT_MILESTONE_CREATE_MUTATION
                })
                .count();
            assert_eq!(mutations, usize::from(mode == "resolved"));
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
