use crate::linear::client::{execute, execute_with_url, LINEAR_API_URL};
use crate::prelude::*;
use mcptools_core::linear::{
    match_projects, match_teams, parse_project_selector, parse_team_selector, project_create_input,
    transform_project_create, transform_team_cycles, transform_team_labels,
    transform_team_projects, transform_team_states, transform_teams, transform_users, Cycle, Label,
    Paginated, Project, ProjectResolution, Team, TeamResolution, User, WorkflowState,
    DOCUMENT_GET_QUERY,
};
use mcptools_core::linear::{transform_project_statuses, ProjectStatusListOutput};

pub const TEAMS_QUERY: &str = "query ($first: Int!, $after: String) { teams(first: $first, after: $after) { nodes { id key name } pageInfo { hasNextPage endCursor } } }";
pub const INITIATIVES_QUERY: &str = "query ($first: Int!, $after: String, $filter: InitiativeFilter) { initiatives(first: $first, after: $after, filter: $filter, orderBy: createdAt) { nodes { id name description status targetDate owner { id name email } url } pageInfo { hasNextPage endCursor } } }";

pub const INITIATIVE_GET_QUERY: &str = "query ($id: String!, $first: Int!, $after: String) { initiative(id: $id) { id name description content status targetDate owner { id name email } url projects(first: $first, after: $after, includeArchived: true, includeSubInitiatives: false, orderBy: createdAt) { nodes { id name } pageInfo { hasNextPage endCursor } } } }";
pub const DOCUMENT_CREATE_MUTATION: &str = "mutation ($input: DocumentCreateInput!) { documentCreate(input: $input) { success document { id title content url createdAt updatedAt project { id name } issue { id identifier } } } }";

pub const DOCUMENT_UPDATE_MUTATION: &str = "mutation ($id: String!, $input: DocumentUpdateInput!) { documentUpdate(id: $id, input: $input) { success document { id title content url createdAt updatedAt project { id name } } } }";

pub async fn resolve_document_filter(
    client: &reqwest::Client,
    project: Option<&str>,
    team: Option<&str>,
    issue: Option<&str>,
    query: Option<&str>,
) -> Result<mcptools_core::linear::DocumentListFilter> {
    resolve_document_filter_with_url(client, LINEAR_API_URL, project, team, issue, query).await
}

async fn resolve_document_filter_with_url(
    client: &reqwest::Client,
    url: &str,
    project: Option<&str>,
    team: Option<&str>,
    issue: Option<&str>,
    query: Option<&str>,
) -> Result<mcptools_core::linear::DocumentListFilter> {
    for (name, value) in [
        ("project", project),
        ("team", team),
        ("issue", issue),
        ("query", query),
    ] {
        if value.is_some_and(|text| text.trim().is_empty()) {
            return Err(eyre!("Linear document list {} must not be empty", name));
        }
    }
    let project_id = match project.map(str::trim) {
        Some(selector) if mcptools_core::linear::is_uuid(selector) => Some(selector.to_string()),
        Some(selector) => {
            let team = team.ok_or_else(|| {
                eyre!("Linear document list project names need --team; pass a project UUID to skip team resolution")
            })?;
            Some(
                resolve_project_with_url(client, url, selector, Some(team))
                    .await?
                    .id,
            )
        }
        None => None,
    };
    let issue_id = match issue.map(str::trim) {
        Some(selector) if mcptools_core::linear::is_uuid(selector) => Some(selector.to_string()),
        Some(selector) => {
            let (request, variables) = mcptools_core::linear::issue_get_request(selector, 0)
                .map_err(|error| eyre!(error))?;
            let data = execute_with_url(client, url, request, variables).await?;
            let (issue, _) =
                mcptools_core::linear::parse_issue_get(data).map_err(|error| eyre!(error))?;
            Some(issue.id)
        }
        None => None,
    };
    Ok(mcptools_core::linear::DocumentListFilter {
        project_id,
        issue_id,
        query: query.map(str::trim).map(str::to_string),
    })
}

pub async fn documents_list_data(
    client: &reqwest::Client,
    filter: &mcptools_core::linear::DocumentListFilter,
    limit: u32,
    cursor: Option<String>,
) -> Result<mcptools_core::linear::DocumentListOutput> {
    let (query, variables) =
        mcptools_core::linear::documents_list_request(filter, limit, cursor.as_deref())
            .map_err(|error| eyre!(error))?;
    let data = execute(client, query, variables).await?;
    mcptools_core::linear::parse_documents_list(data).map_err(|error| eyre!(error))
}

pub async fn document_update_data(
    client: &reqwest::Client,
    args: &crate::linear::args::DocumentUpdateArgs,
) -> Result<mcptools_core::linear::DocumentUpdateOutput> {
    document_update_data_with_url(client, LINEAR_API_URL, args).await
}

async fn document_update_data_with_url(
    client: &reqwest::Client,
    url: &str,
    args: &crate::linear::args::DocumentUpdateArgs,
) -> Result<mcptools_core::linear::DocumentUpdateOutput> {
    let input = mcptools_core::linear::document_update_input(args).map_err(|error| eyre!(error))?;
    let data = execute_with_url(
        client,
        url,
        DOCUMENT_UPDATE_MUTATION,
        serde_json::json!({"id": args.id, "input": input}),
    )
    .await?;
    mcptools_core::linear::transform_document_update(data, &args.id).map_err(|error| eyre!(error))
}

pub async fn document_create_data(
    client: &reqwest::Client,
    args: &crate::linear::args::DocumentCreateArgs,
) -> Result<mcptools_core::linear::DocumentCreateOutput> {
    document_create_data_with_url(client, LINEAR_API_URL, args).await
}

async fn document_create_data_with_url(
    client: &reqwest::Client,
    url: &str,
    args: &crate::linear::args::DocumentCreateArgs,
) -> Result<mcptools_core::linear::DocumentCreateOutput> {
    use mcptools_core::linear::{DocumentIssue, DocumentParent};
    mcptools_core::linear::document_create_input(args).map_err(|error| eyre!(error))?;
    let parent = match (args.project.as_deref(), args.issue.as_deref()) {
        (Some(project), _) => DocumentParent::Project(
            resolve_project_with_url(client, url, project, args.team.as_deref()).await?,
        ),
        (None, Some(selector)) => {
            let (query, variables) = mcptools_core::linear::issue_get_request(selector, 0)
                .map_err(|error| eyre!(error))?;
            let data = execute_with_url(client, url, query, variables).await?;
            let (issue, _) =
                mcptools_core::linear::parse_issue_get(data).map_err(|error| eyre!(error))?;
            DocumentParent::Issue(DocumentIssue {
                id: issue.id,
                identifier: issue.identifier,
            })
        }
        (None, None) => return Err(eyre!("One of project or issue is required")),
    };
    let (project_id, issue_id) = match &parent {
        DocumentParent::Project(project) => (Some(project.id.as_str()), None),
        DocumentParent::Issue(issue) => (None, Some(issue.id.as_str())),
    };
    let input = mcptools_core::linear::document_create_resolved_input(args, project_id, issue_id)
        .map_err(|error| eyre!(error))?;
    let data = execute_with_url(
        client,
        url,
        DOCUMENT_CREATE_MUTATION,
        serde_json::json!({"input": input}),
    )
    .await?;
    mcptools_core::linear::transform_document_create(data, &parent).map_err(|error| eyre!(error))
}

pub async fn document_get_data(
    client: &reqwest::Client,
    args: crate::linear::args::DocumentGetArgs,
) -> Result<mcptools_core::linear::DocumentGetOutput> {
    document_get_data_with_url(client, LINEAR_API_URL, args).await
}

async fn document_get_data_with_url(
    client: &reqwest::Client,
    url: &str,
    args: crate::linear::args::DocumentGetArgs,
) -> Result<mcptools_core::linear::DocumentGetOutput> {
    let (query, variables) =
        mcptools_core::linear::document_get_request(&args.id).map_err(|error| eyre!(error))?;
    let data = execute_with_url(client, url, query, variables).await?;
    mcptools_core::linear::transform_document(data, &args.id).map_err(|error| eyre!(error))
}

pub async fn initiatives_get_data(
    client: &reqwest::Client,
    args: crate::linear::args::InitiativeGetArgs,
) -> Result<mcptools_core::linear::InitiativeGetOutput> {
    initiatives_get_data_with_url(client, LINEAR_API_URL, args).await
}

pub const INITIATIVE_IDENTITY_QUERY: &str =
    "query ($id: String!) { initiative(id: $id) { id name } }";
pub const INITIATIVE_PROJECT_ADD_MUTATION: &str = "mutation ($input: InitiativeToProjectCreateInput!) { initiativeToProjectCreate(input: $input) { success initiativeToProject { id initiative { id name } project { id name } } } }";

pub async fn initiative_project_add_data(
    client: &reqwest::Client,
    args: &crate::linear::args::InitiativeProjectAddArgs,
) -> Result<mcptools_core::linear::InitiativeProjectAddOutput> {
    initiative_project_add_data_with_url(client, LINEAR_API_URL, args).await
}

async fn initiative_project_add_data_with_url(
    client: &reqwest::Client,
    url: &str,
    args: &crate::linear::args::InitiativeProjectAddArgs,
) -> Result<mcptools_core::linear::InitiativeProjectAddOutput> {
    args.validate().map_err(|error| eyre!(error))?;
    let project =
        resolve_project_with_url(client, url, &args.project, args.team.as_deref()).await?;
    if !mcptools_core::linear::is_uuid(&project.id) {
        return Err(eyre!("Malformed project identity"));
    }
    let data = execute_with_url(
        client,
        url,
        INITIATIVE_IDENTITY_QUERY,
        serde_json::json!({"id": args.initiative}),
    )
    .await?;
    let initiative = mcptools_core::linear::transform_initiative_identity(data, &args.initiative)
        .map_err(|error| eyre!(error))?;
    let data = execute_with_url(
        client,
        url,
        INITIATIVE_PROJECT_ADD_MUTATION,
        serde_json::json!({"input": {"initiativeId": initiative.id, "projectId": project.id}}),
    )
    .await?;
    mcptools_core::linear::transform_initiative_project_add(data, &initiative, &project)
        .map_err(|error| eyre!(error))
}

async fn initiatives_get_data_with_url(
    client: &reqwest::Client,
    url: &str,
    args: crate::linear::args::InitiativeGetArgs,
) -> Result<mcptools_core::linear::InitiativeGetOutput> {
    args.validate().map_err(|error| eyre!(error))?;
    let mut cursor = None::<String>;
    let mut seen = std::collections::HashSet::new();
    let mut projects = Vec::new();
    for _ in 0..1000 {
        let data = execute_with_url(
            client,
            url,
            INITIATIVE_GET_QUERY,
            serde_json::json!({"id": args.id, "first": 250, "after": cursor}),
        )
        .await?;
        let (mut output, page_info) = mcptools_core::linear::transform_initiative(data, &args.id)
            .map_err(|error| eyre!(error))?;
        projects.append(&mut output.projects);
        if !page_info.has_next {
            output.projects = projects;
            return Ok(output);
        }
        let next = page_info
            .end_cursor
            .ok_or_else(|| eyre!("Initiative project pagination cursor missing"))?;
        if !seen.insert(next.clone()) {
            return Err(eyre!(
                "Initiative project pagination cursor did not progress"
            ));
        }
        cursor = Some(next);
    }
    Err(eyre!("Initiative project pagination exceeded 1000 pages"))
}

pub async fn initiatives_list_data(
    client: &reqwest::Client,
    args: crate::linear::args::InitiativeListArgs,
) -> Result<mcptools_core::linear::InitiativeListOutput> {
    initiatives_list_data_with_url(client, LINEAR_API_URL, args).await
}

async fn initiatives_list_data_with_url(
    client: &reqwest::Client,
    url: &str,
    args: crate::linear::args::InitiativeListArgs,
) -> Result<mcptools_core::linear::InitiativeListOutput> {
    args.validate().map_err(|error| eyre!(error))?;
    let filter = args
        .query
        .map(|query| serde_json::json!({"name": {"contains": query}}));
    let mut cursor = args.cursor;
    let mut seen = std::collections::HashSet::new();
    if let Some(cursor) = &cursor {
        seen.insert(cursor.clone());
    }
    let mut nodes = Vec::new();
    for _ in 0..1000 {
        let data = execute_with_url(
            client,
            url,
            INITIATIVES_QUERY,
            serde_json::json!({"first": args.limit, "after": cursor, "filter": filter}),
        )
        .await?;
        let page =
            mcptools_core::linear::transform_initiatives(data).map_err(|error| eyre!(error))?;
        if page.page_info.has_next && page.page_info.end_cursor.is_none() {
            return Err(eyre!("Initiative pagination cursor missing"));
        }
        if let Some(next) = page
            .page_info
            .end_cursor
            .as_ref()
            .filter(|_| page.page_info.has_next)
        {
            if !seen.insert(next.clone()) {
                return Err(eyre!("Initiative pagination cursor did not progress"));
            }
        }
        nodes.extend(page.nodes);
        if !args.all || !page.page_info.has_next {
            return Ok(mcptools_core::linear::InitiativeListOutput {
                nodes,
                page_info: page.page_info,
            });
        }
        cursor = page.page_info.end_cursor;
    }
    Err(eyre!("Initiative pagination exceeded 1000 pages"))
}

pub const TEAM_PROJECTS_QUERY: &str = "query ($id: String!, $first: Int!, $after: String) { team(id: $id) { id key name projects(first: $first, after: $after) { nodes { id name } pageInfo { hasNextPage endCursor } } } }";
pub const PROJECT_CREATE_MUTATION: &str = "mutation ($input: ProjectCreateInput!) { projectCreate(input: $input) { success project { id name } } }";
pub const PROJECT_GET_QUERY: &str = "query ($id: String!) { project(id: $id) { id name } }";
pub const PROJECT_UPDATE_MUTATION: &str = "mutation ($id: String!, $input: ProjectUpdateInput!) { projectUpdate(id: $id, input: $input) { success project { id name url description content status { id name type } lead { id name email } startDate targetDate priority } } }";
pub const PROJECT_UPDATE_CREATE_MUTATION: &str = "mutation ($input: ProjectUpdateCreateInput!) { projectUpdateCreate(input: $input) { success projectUpdate { id body health createdAt url project { id name } } } }";
pub const PROJECT_UPDATES_QUERY: &str = "query ($id: String!, $first: Int!, $after: String) { project(id: $id) { id name projectUpdates(first: $first, after: $after, orderBy: createdAt) { nodes { id body health createdAt updatedAt url project { id name } } pageInfo { hasNextPage endCursor } } } }";

pub async fn project_updates_list_data(
    client: &reqwest::Client,
    args: crate::linear::args::ProjectUpdateListArgs,
) -> Result<mcptools_core::linear::ProjectUpdateListOutput> {
    project_updates_list_data_with_url(client, LINEAR_API_URL, args).await
}

async fn project_updates_list_data_with_url(
    client: &reqwest::Client,
    url: &str,
    args: crate::linear::args::ProjectUpdateListArgs,
) -> Result<mcptools_core::linear::ProjectUpdateListOutput> {
    args.validate().map_err(|error| eyre!(error))?;
    let project =
        resolve_project_with_url(client, url, &args.project, args.team.as_deref()).await?;
    if !mcptools_core::linear::is_uuid(&project.id) {
        return Err(eyre!("Malformed project identity"));
    }
    let mut cursor = args.cursor;
    let mut seen = std::collections::HashSet::new();
    if let Some(cursor) = &cursor {
        seen.insert(cursor.clone());
    }
    let mut nodes = Vec::new();
    for _ in 0..1000 {
        let data = execute_with_url(
            client,
            url,
            PROJECT_UPDATES_QUERY,
            serde_json::json!({"id": project.id, "first": args.limit, "after": cursor}),
        )
        .await?;
        let page = mcptools_core::linear::transform_project_updates(data, &project)
            .map_err(|error| eyre!(error))?;
        if page.page_info.has_next && page.page_info.end_cursor.is_none() {
            return Err(eyre!("Progress report pagination cursor missing"));
        }
        if let Some(next) = &page.page_info.end_cursor {
            if !seen.insert(next.clone()) {
                return Err(eyre!("Progress report pagination cursor did not progress"));
            }
        }
        nodes.extend(page.nodes);
        if !args.all || !page.page_info.has_next {
            return Ok(mcptools_core::linear::ProjectUpdateListOutput {
                project: page.project,
                nodes,
                page_info: page.page_info,
            });
        }
        cursor = page.page_info.end_cursor;
    }
    Err(eyre!("Progress report pagination exceeded 1000 pages"))
}

pub async fn project_updates_create_data(
    client: &reqwest::Client,
    args: &crate::linear::args::ProjectUpdateCreateArgs,
) -> Result<mcptools_core::linear::ProjectUpdateCreateOutput> {
    project_updates_create_data_with_url(client, LINEAR_API_URL, args).await
}

async fn project_updates_create_data_with_url(
    client: &reqwest::Client,
    url: &str,
    args: &crate::linear::args::ProjectUpdateCreateArgs,
) -> Result<mcptools_core::linear::ProjectUpdateCreateOutput> {
    let mut input =
        mcptools_core::linear::project_update_create_input(args).map_err(|error| eyre!(error))?;
    let project =
        resolve_project_with_url(client, url, &args.project, args.team.as_deref()).await?;
    if !mcptools_core::linear::is_uuid(&project.id) {
        return Err(eyre!("Malformed project identity"));
    }
    input["projectId"] = serde_json::json!(project.id);
    let data = execute_with_url(
        client,
        url,
        PROJECT_UPDATE_CREATE_MUTATION,
        serde_json::json!({"input": input}),
    )
    .await?;
    mcptools_core::linear::transform_project_update_create(data, &project)
        .map_err(|error| eyre!(error))
}
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

pub const PROJECT_MILESTONE_UPDATE_MUTATION: &str = "mutation ($id: String!, $input: ProjectMilestoneUpdateInput!) { projectMilestoneUpdate(id: $id, input: $input) { success projectMilestone { id name description targetDate status sortOrder project { id name } } } }";

pub async fn project_milestones_update_data(
    client: &reqwest::Client,
    args: &crate::linear::args::ProjectMilestoneUpdateArgs,
) -> Result<mcptools_core::linear::ProjectMilestone> {
    project_milestones_update_data_with_url(client, LINEAR_API_URL, args).await
}

async fn project_milestones_update_data_with_url(
    client: &reqwest::Client,
    url: &str,
    args: &crate::linear::args::ProjectMilestoneUpdateArgs,
) -> Result<mcptools_core::linear::ProjectMilestone> {
    let input = mcptools_core::linear::project_milestone_update_input(args)
        .map_err(|error| eyre!(error))?;
    let data = execute_with_url(
        client,
        url,
        PROJECT_MILESTONE_UPDATE_MUTATION,
        serde_json::json!({"id": args.id.trim(), "input": input}),
    )
    .await?;
    mcptools_core::linear::transform_project_milestone_update(data, &args.id)
        .map_err(|error| eyre!(error))
}

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
    async fn document_update_requests_preserve_omitted_fields_and_markdown() {
        use wiremock::matchers::{body_json, method};
        use wiremock::{Mock, MockServer, ResponseTemplate};
        let sample: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/contract_samples/linear_document_update.json"
        ))
        .unwrap();
        let id = sample["id"].as_str().unwrap();
        for (fields, input) in [
            (
                serde_json::json!({"title": " Revised "}),
                serde_json::json!({"title": "Revised"}),
            ),
            (
                serde_json::json!({"content": "    code\n\n# Heading\r\n"}),
                serde_json::json!({"content": "    code\n\n# Heading\r\n"}),
            ),
            (
                serde_json::json!({"title": "Revised", "content": "Both\n"}),
                serde_json::json!({"title": "Revised", "content": "Both\n"}),
            ),
            (
                serde_json::json!({"content": ""}),
                serde_json::json!({"content": ""}),
            ),
            (
                serde_json::json!({"title": "Revised", "content": null}),
                serde_json::json!({"title": "Revised"}),
            ),
            (
                serde_json::json!({"title": null, "content": "Body"}),
                serde_json::json!({"content": "Body"}),
            ),
        ] {
            let mut raw = fields;
            raw["id"] = serde_json::json!(id);
            let args = serde_json::from_value(raw).unwrap();
            let mut expected = sample.clone();
            expected
                .as_object_mut()
                .unwrap()
                .extend(input.as_object().unwrap().clone());
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .and(body_json(serde_json::json!({"query": DOCUMENT_UPDATE_MUTATION, "variables": {"id": id, "input": input}})))
                .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"data": {"documentUpdate": {"success": true, "document": expected}}})))
                .expect(1).mount(&server).await;
            let output =
                document_update_data_with_url(&reqwest::Client::new(), &server.uri(), &args)
                    .await
                    .unwrap();
            assert_eq!(serde_json::to_value(output).unwrap(), expected);
        }
    }

    #[tokio::test]
    async fn document_update_validation_and_failures_never_report_success() {
        use mcptools_core::linear::transform_document_update;
        use wiremock::matchers::method;
        use wiremock::{Mock, MockServer, ResponseTemplate};
        let sample: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/contract_samples/linear_document_update.json"
        ))
        .unwrap();
        let id = sample["id"].as_str().unwrap();
        let args = serde_json::from_value(serde_json::json!({"id": id, "content": ""})).unwrap();
        for (status, body) in [
            (
                403,
                serde_json::json!({"errors": [{"message": "Forbidden"}]}),
            ),
            (
                404,
                serde_json::json!({"errors": [{"message": "Document not found"}]}),
            ),
            (
                200,
                serde_json::json!({"errors": [{"message": "Document not found"}]}),
            ),
            (
                200,
                serde_json::json!({"data": {"documentUpdate": {"success": false, "document": sample}}}),
            ),
            (
                200,
                serde_json::json!({"data": {"documentUpdate": {"document": sample}}}),
            ),
            (
                200,
                serde_json::json!({"data": {"documentUpdate": {"success": true, "document": null}}}),
            ),
            (
                200,
                serde_json::json!({"data": {"documentUpdate": {"success": true, "document": sample}}, "errors": [{"message": "partial failure"}]}),
            ),
            (
                200,
                serde_json::json!({"data": {"documentUpdate": {"success": true, "document": sample}}, "errors": "Forbidden"}),
            ),
            (
                200,
                serde_json::json!({"data": {"documentUpdate": {"success": true, "document": sample}}, "errors": true}),
            ),
            (
                200,
                serde_json::json!({"data": {"documentUpdate": {"success": true, "document": sample}}, "errors": 1}),
            ),
            (200, serde_json::json!({"data": {}})),
        ] {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .respond_with(ResponseTemplate::new(status).set_body_json(body))
                .expect(1)
                .mount(&server)
                .await;
            assert!(
                document_update_data_with_url(&reqwest::Client::new(), &server.uri(), &args)
                    .await
                    .is_err()
            );
        }
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_string("not JSON"))
            .expect(1)
            .mount(&server)
            .await;
        assert!(
            document_update_data_with_url(&reqwest::Client::new(), &server.uri(), &args)
                .await
                .is_err()
        );
        let server = MockServer::start().await;
        for raw in [
            serde_json::json!({"id": id}),
            serde_json::json!({"id": id, "title": null, "content": null}),
            serde_json::json!({"id": " ", "title": "Title"}),
            serde_json::json!({"id": "slug", "content": "Body"}),
            serde_json::json!({"id": id, "title": "\t\n", "content": ""}),
        ] {
            let args = serde_json::from_value(raw).unwrap();
            assert!(
                document_update_data_with_url(&reqwest::Client::new(), &server.uri(), &args)
                    .await
                    .is_err()
            );
        }
        assert!(server.received_requests().await.unwrap().is_empty());
        for field in [
            "id",
            "title",
            "content",
            "url",
            "createdAt",
            "updatedAt",
            "project",
        ] {
            let mut malformed = sample.clone();
            malformed.as_object_mut().unwrap().remove(field);
            assert!(
                transform_document_update(
                    serde_json::json!({"documentUpdate": {"success": true, "document": malformed}}),
                    id
                )
                .is_err(),
                "{field}"
            );
        }
        for (field, value) in [
            (
                "id",
                serde_json::json!("12345678-1234-1234-1234-123456789abc"),
            ),
            ("title", serde_json::json!(3)),
            ("content", serde_json::json!({})),
            ("url", serde_json::json!("file:///document")),
            ("createdAt", serde_json::json!("bad")),
            ("updatedAt", serde_json::json!("bad")),
            (
                "project",
                serde_json::json!({"id": "bad", "name": "Delivery"}),
            ),
            ("project", serde_json::json!({"id": id, "name": " "})),
        ] {
            let mut malformed = sample.clone();
            malformed[field] = value;
            assert!(
                transform_document_update(
                    serde_json::json!({"documentUpdate": {"success": true, "document": malformed}}),
                    id
                )
                .is_err(),
                "{field}"
            );
        }
        let mut nullable = sample;
        nullable["project"] = serde_json::Value::Null;
        nullable["content"] = serde_json::Value::Null;
        nullable["title"] = serde_json::json!("");
        assert!(transform_document_update(
            serde_json::json!({"documentUpdate": {"success": true, "document": nullable}}),
            &args.id
        )
        .is_ok());
    }

    #[tokio::test]
    async fn document_create_inputs_resolution_and_mutation_output() {
        use mcptools_core::linear::{
            document_create_input, transform_document_create, DocumentCreateArgs,
        };
        use wiremock::matchers::{body_json, method};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let sample: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/contract_samples/linear_document_create.json"
        ))
        .unwrap();
        let id = sample["project"]["id"].as_str().unwrap();
        let project: Project = serde_json::from_value(sample["project"].clone()).unwrap();
        let parent = mcptools_core::linear::DocumentParent::Project(project.clone());
        for content in [
            None,
            Some(serde_json::Value::Null),
            Some(serde_json::json!("    code\n\n# Heading\n")),
            // Sent verbatim: no tilde escaping, link rewriting, or H1-to-title promotion.
            Some(serde_json::json!(
                "# Heading\n\nRoughly ~5 items; see crates/core/src/foo.rs:12\n"
            )),
            Some(serde_json::json!("")),
        ] {
            let mut raw = serde_json::json!({"project": id, "title": " Delivery plan "});
            if let Some(content) = &content {
                raw["content"] = content.clone();
            }
            let args: DocumentCreateArgs = serde_json::from_value(raw).unwrap();
            let mut input = document_create_input(&args).unwrap();
            assert_eq!(input["title"], "Delivery plan");
            assert_eq!(
                input.get("content"),
                content.as_ref().filter(|value| !value.is_null())
            );
            input["projectId"] = serde_json::json!(id);
            let mut expected = sample.clone();
            expected["content"] = content.unwrap_or(serde_json::Value::Null);
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .and(body_json(
                    serde_json::json!({"query": PROJECT_GET_QUERY, "variables": {"id": id}}),
                ))
                .respond_with(
                    ResponseTemplate::new(200)
                        .set_body_json(serde_json::json!({"data": {"project": project}})),
                )
                .expect(1)
                .mount(&server)
                .await;
            Mock::given(method("POST"))
                .and(body_json(serde_json::json!({"query": DOCUMENT_CREATE_MUTATION, "variables": {"input": input}})))
                .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"data": {"documentCreate": {"success": true, "document": expected}}})))
                .expect(1).mount(&server).await;
            let output =
                document_create_data_with_url(&reqwest::Client::new(), &server.uri(), &args)
                    .await
                    .unwrap();
            assert_eq!(serde_json::to_value(output).unwrap(), expected);
        }
        for fields in [
            serde_json::json!({"project": " "}),
            serde_json::json!({"project": "Delivery"}),
            serde_json::json!({"project": "Delivery", "team": " "}),
            serde_json::json!({"project": "bad\nselector"}),
            serde_json::json!({"project": "zzzzzzzz-1234-1234-1234-123456789abc"}),
            serde_json::json!({"title": "\t\n"}),
        ] {
            let mut raw = serde_json::json!({"project": id, "title": "Delivery plan"});
            raw.as_object_mut()
                .unwrap()
                .extend(fields.as_object().unwrap().clone());
            let args = serde_json::from_value(raw).unwrap();
            let server = MockServer::start().await;
            assert!(
                document_create_data_with_url(&reqwest::Client::new(), &server.uri(), &args)
                    .await
                    .is_err()
            );
            assert!(server.received_requests().await.unwrap().is_empty());
        }
        let args =
            serde_json::from_value(serde_json::json!({"project": id, "title": "Delivery plan"}))
                .unwrap();
        for response in [
            serde_json::Value::Null,
            serde_json::json!({"id": id}),
            serde_json::json!({"id": id, "name": " "}),
        ] {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .and(body_json(
                    serde_json::json!({"query": PROJECT_GET_QUERY, "variables": {"id": id}}),
                ))
                .respond_with(
                    ResponseTemplate::new(200)
                        .set_body_json(serde_json::json!({"data": {"project": response}})),
                )
                .expect(1)
                .mount(&server)
                .await;
            assert!(
                document_create_data_with_url(&reqwest::Client::new(), &server.uri(), &args)
                    .await
                    .is_err()
            );
            assert_eq!(server.received_requests().await.unwrap().len(), 1);
        }
        for field in [
            "id",
            "title",
            "content",
            "url",
            "createdAt",
            "updatedAt",
            "project",
        ] {
            let mut incomplete = sample.clone();
            incomplete.as_object_mut().unwrap().remove(field);
            assert!(transform_document_create(
                serde_json::json!({"documentCreate": {"success": true, "document": incomplete}}),
                &parent
            )
            .is_err());
            incomplete[field] = serde_json::json!(false);
            assert!(transform_document_create(
                serde_json::json!({"documentCreate": {"success": true, "document": incomplete}}),
                &parent
            )
            .is_err());
        }
        for (field, value) in [
            ("id", serde_json::json!("bad")),
            ("title", serde_json::json!(" ")),
            ("url", serde_json::json!("file:///bad")),
            ("createdAt", serde_json::json!("bad")),
            ("updatedAt", serde_json::json!("bad")),
            ("project", serde_json::Value::Null),
            (
                "project",
                serde_json::json!({"id": sample["id"], "name": "Delivery"}),
            ),
        ] {
            let mut malformed = sample.clone();
            malformed[field] = value;
            assert!(transform_document_create(
                serde_json::json!({"documentCreate": {"success": true, "document": malformed}}),
                &parent
            )
            .is_err());
        }
        for (status, body) in [
            (
                403,
                serde_json::json!({"errors": [{"message": "Forbidden"}]}),
            ),
            (
                200,
                serde_json::json!({"data": {"documentCreate": {"success": false, "document": sample}}}),
            ),
            (
                200,
                serde_json::json!({"data": {"documentCreate": {"document": sample}}}),
            ),
            (
                200,
                serde_json::json!({"data": {"documentCreate": {"success": true, "document": null}}}),
            ),
            (
                200,
                serde_json::json!({"data": {"documentCreate": {"success": true, "document": sample}}, "errors": [{"message": "partial failure"}]}),
            ),
        ] {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .and(body_json(
                    serde_json::json!({"query": PROJECT_GET_QUERY, "variables": {"id": id}}),
                ))
                .respond_with(
                    ResponseTemplate::new(200)
                        .set_body_json(serde_json::json!({"data": {"project": project}})),
                )
                .expect(1)
                .mount(&server)
                .await;
            Mock::given(method("POST"))
                .and(body_json(serde_json::json!({"query": DOCUMENT_CREATE_MUTATION, "variables": {"input": {"projectId": id, "title": "Delivery plan"}}})))
                .respond_with(ResponseTemplate::new(status).set_body_json(body))
                .expect(1).mount(&server).await;
            assert!(
                document_create_data_with_url(&reqwest::Client::new(), &server.uri(), &args)
                    .await
                    .is_err()
            );
        }
    }

    #[tokio::test]
    async fn document_create_resolves_project_names_and_rejects_ambiguity() {
        use wiremock::matchers::{body_json, method};
        use wiremock::{Mock, MockServer, ResponseTemplate};
        let sample: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/contract_samples/linear_document_create.json"
        ))
        .unwrap();
        let team = "22345678-1234-1234-1234-123456789abc";
        for nodes in [
            serde_json::json!([sample["project"]]),
            serde_json::json!([]),
            serde_json::json!([sample["project"], {"id": sample["id"], "name": "Delivery"}]),
        ] {
            let server = MockServer::start().await;
            for (query, variables, data) in [
                (
                    TEAMS_QUERY,
                    serde_json::json!({"first": 50, "after": null}),
                    serde_json::json!({"teams": {"nodes": [{"id": team, "key": "GUZ", "name": "Team"}], "pageInfo": {"hasNextPage": false, "endCursor": null}}}),
                ),
                (
                    TEAM_PROJECTS_QUERY,
                    serde_json::json!({"id": team, "first": 50, "after": null}),
                    serde_json::json!({"team": {"id": team, "projects": {"nodes": nodes, "pageInfo": {"hasNextPage": false, "endCursor": null}}}}),
                ),
            ] {
                Mock::given(method("POST"))
                    .and(body_json(
                        serde_json::json!({"query": query, "variables": variables}),
                    ))
                    .respond_with(
                        ResponseTemplate::new(200).set_body_json(serde_json::json!({"data": data})),
                    )
                    .expect(1)
                    .mount(&server)
                    .await;
            }
            let resolved = nodes.as_array().unwrap().len() == 1;
            Mock::given(method("POST"))
                .and(body_json(serde_json::json!({"query": DOCUMENT_CREATE_MUTATION, "variables": {"input": {"projectId": sample["project"]["id"], "title": "Delivery plan"}}})))
                .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"data": {"documentCreate": {"success": true, "document": sample}}})))
                .expect(u64::from(resolved)).mount(&server).await;
            let args = serde_json::from_value(serde_json::json!({"project": " delivery ", "team": "GUZ", "title": "Delivery plan"})).unwrap();
            let output =
                document_create_data_with_url(&reqwest::Client::new(), &server.uri(), &args).await;
            if resolved {
                assert_eq!(serde_json::to_value(output.unwrap()).unwrap(), sample);
            } else {
                assert!(output.is_err());
            }
        }
    }

    #[tokio::test]
    async fn document_get_preserves_markdown_and_nullable_metadata() {
        use wiremock::matchers::{body_json, method};
        use wiremock::{Mock, MockServer, ResponseTemplate};
        let sample: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/contract_samples/linear_document_get.json"
        ))
        .unwrap();
        for nullable in [false, true] {
            let server = MockServer::start().await;
            let mut document = sample.clone();
            if nullable {
                document["title"] = serde_json::json!("");
                for field in ["content", "owner", "project", "issue"] {
                    document[field] = serde_json::Value::Null;
                }
            }
            Mock::given(method("POST"))
                .and(body_json(serde_json::json!({
                    "query": DOCUMENT_GET_QUERY,
                    "variables": {"id": sample["id"]}
                })))
                .respond_with(
                    ResponseTemplate::new(200)
                        .set_body_json(serde_json::json!({"data": {"document": document}})),
                )
                .expect(1)
                .mount(&server)
                .await;
            let args = serde_json::from_value(serde_json::json!({"id": sample["id"]})).unwrap();
            let output = document_get_data_with_url(&reqwest::Client::new(), &server.uri(), args)
                .await
                .unwrap();
            let value = serde_json::to_value(&output).unwrap();
            assert_eq!(value["id"], sample["id"]);
            assert_eq!(value["createdAt"], sample["createdAt"]);
            assert_eq!(value["updatedAt"], sample["updatedAt"]);
            if nullable {
                assert_eq!(value["title"], "");
                for field in ["content", "owner", "project", "issue"] {
                    assert!(value[field].is_null(), "{field}");
                }
            } else {
                assert_eq!(value["content"], sample["content"]);
                assert_eq!(value["owner"], sample["owner"]);
                assert_eq!(value["project"], sample["project"]);
                assert_eq!(value["issue"], sample["issue"]);
            }
            let result = crate::mcp::tools::to_dual_result(output).unwrap();
            let text: serde_json::Value =
                serde_json::from_str(result["content"][0]["text"].as_str().unwrap()).unwrap();
            assert_eq!(text, result["structuredContent"]);
            assert_eq!(text, value);
        }
    }

    #[test]
    fn document_get_transform_rejects_missing_and_malformed_fields() {
        let sample: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/contract_samples/linear_document_get.json"
        ))
        .unwrap();
        let id = sample["id"].as_str().unwrap().to_string();
        let valid = serde_json::json!({"document": sample});
        for (path, value) in [
            ("/document/id", serde_json::json!("bad")),
            (
                "/document/id",
                serde_json::json!("52345678-1234-1234-1234-123456789abc"),
            ),
            ("/document/title", serde_json::json!(1)),
            ("/document/content", serde_json::json!(1)),
            ("/document/url", serde_json::json!("file:///etc/passwd")),
            ("/document/createdAt", serde_json::json!("not-a-date")),
            ("/document/updatedAt", serde_json::json!("2026-02-30")),
            ("/document/owner/id", serde_json::json!("bad")),
            ("/document/owner/name", serde_json::json!(" ")),
            ("/document/owner/email", serde_json::Value::Null),
            (
                "/document/project",
                serde_json::json!({"id": "bad", "name": "Delivery"}),
            ),
            (
                "/document/project",
                serde_json::json!({"id": "32345678-1234-1234-1234-123456789abc", "name": " "}),
            ),
            ("/document/issue/id", serde_json::json!("bad")),
            ("/document/issue/identifier", serde_json::json!(" ")),
            ("/document/issue/title", serde_json::json!(" ")),
            ("/document/issue/url", serde_json::json!("file:///issue")),
        ] {
            let mut data = valid.clone();
            *data.pointer_mut(path).unwrap() = value;
            assert!(
                mcptools_core::linear::transform_document(data, &id).is_err(),
                "{path}"
            );
        }
        for field in [
            "id",
            "title",
            "content",
            "url",
            "createdAt",
            "updatedAt",
            "owner",
            "project",
            "issue",
        ] {
            let mut data = valid.clone();
            data["document"].as_object_mut().unwrap().remove(field);
            assert!(
                mcptools_core::linear::transform_document(data, &id).is_err(),
                "{field}"
            );
        }
        let mut data = valid.clone();
        data["document"]["owner"]
            .as_object_mut()
            .unwrap()
            .remove("email");
        assert!(mcptools_core::linear::transform_document(data, &id).is_err());
        assert!(
            mcptools_core::linear::transform_document(serde_json::json!({}), &id)
                .unwrap_err()
                .contains("Malformed")
        );
        assert!(mcptools_core::linear::transform_document(
            serde_json::json!({"document": null}),
            &id
        )
        .unwrap_err()
        .contains("not found"));
    }

    #[tokio::test]
    async fn document_get_errors_and_bad_inputs_fail_before_output() {
        use wiremock::matchers::method;
        use wiremock::{Mock, MockServer, ResponseTemplate};
        let sample: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/contract_samples/linear_document_get.json"
        ))
        .unwrap();
        let id = sample["id"].clone();
        for (status, body, message) in [
            (
                403,
                serde_json::json!({"errors": [{"message": "Forbidden"}]}),
                "403",
            ),
            (
                200,
                serde_json::json!({"data": {"document": sample}, "errors": [{"message": "Permission denied"}]}),
                "Permission denied",
            ),
            (
                200,
                serde_json::json!({"data": {"document": null}}),
                "not found",
            ),
            (200, serde_json::json!({"data": {}}), "Malformed"),
        ] {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .respond_with(ResponseTemplate::new(status).set_body_json(body))
                .expect(1)
                .mount(&server)
                .await;
            let args = serde_json::from_value(serde_json::json!({"id": id})).unwrap();
            let error = document_get_data_with_url(&reqwest::Client::new(), &server.uri(), args)
                .await
                .unwrap_err();
            assert!(error.to_string().contains(message), "{error}");
        }
        let server = MockServer::start().await;
        for id in [
            "",
            " ",
            "document-slug",
            "12345678-1234-1234-1234-123456789abz",
        ] {
            let args = serde_json::from_value(serde_json::json!({"id": id})).unwrap();
            assert!(
                document_get_data_with_url(&reqwest::Client::new(), &server.uri(), args)
                    .await
                    .unwrap_err()
                    .to_string()
                    .contains("UUID or Linear document URL")
            );
        }
        assert!(server.received_requests().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn initiative_get_pages_preserve_markdown_nulls_and_all_projects() {
        use wiremock::matchers::{body_json, method};
        use wiremock::{Mock, MockServer, ResponseTemplate};
        let sample: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/contract_samples/linear_initiative_get.json"
        ))
        .unwrap();
        for empty in [false, true] {
            let server = MockServer::start().await;
            let pages = if empty { 1 } else { 2 };
            for page in 0..pages {
                let mut initiative = sample.clone();
                for field in ["description", "targetDate", "owner"] {
                    initiative[field] = serde_json::Value::Null;
                }
                let mut nodes = if empty {
                    serde_json::json!([])
                } else {
                    sample["projects"].clone()
                };
                if page == 1 {
                    nodes[0]["id"] = serde_json::json!("42345678-1234-1234-1234-123456789abc");
                    nodes[0]["name"] = serde_json::json!("Second project");
                }
                initiative["projects"] = serde_json::json!({"nodes": nodes, "pageInfo": {"hasNextPage": page + 1 < pages, "endCursor": "next"}});
                Mock::given(method("POST"))
                    .and(body_json(serde_json::json!({"query": INITIATIVE_GET_QUERY, "variables": {"id": sample["id"], "first": 250, "after": if page == 0 { serde_json::Value::Null } else { serde_json::json!("next") }}})))
                    .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"data": {"initiative": initiative}})))
                    .expect(1).mount(&server).await;
            }
            let args = serde_json::from_value(serde_json::json!({"id": sample["id"]})).unwrap();
            let output =
                initiatives_get_data_with_url(&reqwest::Client::new(), &server.uri(), args)
                    .await
                    .unwrap();
            assert_eq!(output.content.as_deref(), sample["content"].as_str());
            assert_eq!(output.projects.len(), if empty { 0 } else { 2 });
            if !empty {
                assert_eq!(output.projects[0].name, "Delivery");
                assert_eq!(output.projects[1].name, "Second project");
            }
            assert_eq!(output.initiative.name, sample["name"]);
            let value = serde_json::to_value(output).unwrap();
            for field in ["description", "targetDate", "owner"] {
                assert!(value[field].is_null());
            }
            assert!(INITIATIVE_GET_QUERY
                .contains("includeArchived: true, includeSubInitiatives: false"));
        }
    }

    #[test]
    fn initiative_get_transform_rejects_missing_and_malformed_fields() {
        let mut initiative: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/contract_samples/linear_initiative_get.json"
        ))
        .unwrap();
        let id = initiative["id"].as_str().unwrap().to_string();
        initiative["projects"] = serde_json::json!({"nodes": initiative["projects"], "pageInfo": {"hasNextPage": false, "endCursor": null}});
        let valid = serde_json::json!({"initiative": initiative});
        for (path, value) in [
            ("/initiative/id", serde_json::json!("bad")),
            (
                "/initiative/id",
                serde_json::json!("22345678-1234-1234-1234-123456789abc"),
            ),
            ("/initiative/name", serde_json::json!(" ")),
            ("/initiative/status", serde_json::json!("unknown")),
            ("/initiative/description", serde_json::json!(1)),
            ("/initiative/content", serde_json::json!(1)),
            ("/initiative/targetDate", serde_json::json!("2026-02-30")),
            (
                "/initiative/owner",
                serde_json::json!({"id": "bad", "name": "Ada", "email": "ada@example.com"}),
            ),
            ("/initiative/url", serde_json::json!("bad")),
            ("/initiative/projects", serde_json::Value::Null),
            ("/initiative/projects/nodes", serde_json::Value::Null),
            ("/initiative/projects/nodes/0/id", serde_json::json!("bad")),
            ("/initiative/projects/nodes/0/name", serde_json::json!(" ")),
            ("/initiative/projects/nodes/0/name", serde_json::json!(1)),
            ("/initiative/owner/email", serde_json::Value::Null),
            (
                "/initiative/projects/pageInfo/hasNextPage",
                serde_json::Value::Null,
            ),
            (
                "/initiative/projects/pageInfo/endCursor",
                serde_json::json!("bad\n"),
            ),
            (
                "/initiative/projects/pageInfo/endCursor",
                serde_json::json!(" "),
            ),
            (
                "/initiative/projects/pageInfo/endCursor",
                serde_json::json!(1),
            ),
        ] {
            let mut data = valid.clone();
            *data.pointer_mut(path).unwrap() = value;
            assert!(
                mcptools_core::linear::transform_initiative(data, &id).is_err(),
                "{path}"
            );
        }
        for field in [
            "id",
            "name",
            "status",
            "description",
            "content",
            "targetDate",
            "owner",
            "url",
            "projects",
        ] {
            let mut data = valid.clone();
            data["initiative"].as_object_mut().unwrap().remove(field);
            assert!(
                mcptools_core::linear::transform_initiative(data, &id).is_err(),
                "{field}"
            );
        }
        let mut data = valid.clone();
        data["initiative"]["projects"]["pageInfo"]
            .as_object_mut()
            .unwrap()
            .remove("endCursor");
        assert!(mcptools_core::linear::transform_initiative(data, &id).is_err());
        assert!(
            mcptools_core::linear::transform_initiative(serde_json::json!({}), &id)
                .unwrap_err()
                .contains("Malformed")
        );
        assert!(mcptools_core::linear::transform_initiative(
            serde_json::json!({"initiative": null}),
            &id
        )
        .unwrap_err()
        .contains("not found"));
        let mut data = valid;
        data["initiative"]["content"] = serde_json::Value::Null;
        assert!(mcptools_core::linear::transform_initiative(data, &id)
            .unwrap()
            .0
            .content
            .is_none());
    }

    #[tokio::test]
    async fn initiative_get_errors_and_bad_inputs_fail_without_partial_output() {
        use wiremock::matchers::method;
        use wiremock::{Mock, MockServer, ResponseTemplate};
        let mut initiative: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/contract_samples/linear_initiative_get.json"
        ))
        .unwrap();
        let id = initiative["id"].clone();
        initiative["projects"] = serde_json::json!({"nodes": initiative["projects"], "pageInfo": {"hasNextPage": true, "endCursor": null}});
        for (status, body, message) in [
            (
                403,
                serde_json::json!({"errors": [{"message": "Forbidden"}]}),
                "403",
            ),
            (
                200,
                serde_json::json!({"data": {"initiative": initiative}, "errors": [{"message": "Permission denied"}]}),
                "Permission denied",
            ),
            (
                200,
                serde_json::json!({"data": {"initiative": null}}),
                "not found",
            ),
            (200, serde_json::json!({"data": {}}), "Malformed"),
            (
                200,
                serde_json::json!({"data": {"initiative": initiative}}),
                "cursor missing",
            ),
        ] {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .respond_with(ResponseTemplate::new(status).set_body_json(body))
                .expect(1)
                .mount(&server)
                .await;
            let args = serde_json::from_value(serde_json::json!({"id": id})).unwrap();
            let error = initiatives_get_data_with_url(&reqwest::Client::new(), &server.uri(), args)
                .await
                .unwrap_err();
            assert!(error.to_string().contains(message), "{error}");
        }
        let server = MockServer::start().await;
        for id in ["", " ", "bad", "12345678-1234-1234-1234-123456789abz"] {
            let args = serde_json::from_value(serde_json::json!({"id": id})).unwrap();
            assert!(
                initiatives_get_data_with_url(&reqwest::Client::new(), &server.uri(), args)
                    .await
                    .unwrap_err()
                    .to_string()
                    .contains("UUID")
            );
        }
        assert!(server.received_requests().await.unwrap().is_empty());
        initiative["projects"]["pageInfo"]["endCursor"] = serde_json::json!("same");
        Mock::given(method("POST"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({"data": {"initiative": initiative}})),
            )
            .expect(2)
            .mount(&server)
            .await;
        let args = serde_json::from_value(serde_json::json!({"id": id})).unwrap();
        assert!(
            initiatives_get_data_with_url(&reqwest::Client::new(), &server.uri(), args)
                .await
                .unwrap_err()
                .to_string()
                .contains("did not progress")
        );
    }

    #[tokio::test]
    async fn initiative_get_later_page_errors_do_not_return_partial_projects() {
        use wiremock::matchers::{body_json, method};
        use wiremock::{Mock, MockServer, ResponseTemplate};
        let mut initiative: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/contract_samples/linear_initiative_get.json"
        ))
        .unwrap();
        let id = initiative["id"].clone();
        initiative["projects"] = serde_json::json!({"nodes": initiative["projects"], "pageInfo": {"hasNextPage": true, "endCursor": "next"}});
        for (status, body, message) in [
            (403, serde_json::json!({}), "403"),
            (
                200,
                serde_json::json!({"data": {"initiative": initiative}, "errors": [{"message": "Permission denied"}]}),
                "Permission denied",
            ),
            (200, serde_json::json!({"data": {}}), "Malformed"),
            (
                200,
                serde_json::json!({"data": {"initiative": null}}),
                "not found",
            ),
        ] {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .and(body_json(serde_json::json!({"query": INITIATIVE_GET_QUERY, "variables": {"id": id, "first": 250, "after": null}})))
                .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"data": {"initiative": initiative}})))
                .expect(1).mount(&server).await;
            Mock::given(method("POST"))
                .and(body_json(serde_json::json!({"query": INITIATIVE_GET_QUERY, "variables": {"id": id, "first": 250, "after": "next"}})))
                .respond_with(ResponseTemplate::new(status).set_body_json(body))
                .expect(1).mount(&server).await;
            let args = serde_json::from_value(serde_json::json!({"id": id})).unwrap();
            let error = initiatives_get_data_with_url(&reqwest::Client::new(), &server.uri(), args)
                .await
                .unwrap_err();
            assert!(error.to_string().contains(message), "{error}");
        }
    }

    #[tokio::test]
    async fn initiative_get_nested_projects_have_a_hard_page_cap() {
        use wiremock::matchers::method;
        use wiremock::{Mock, MockServer, Request, ResponseTemplate};
        let mut initiative: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/contract_samples/linear_initiative_get.json"
        ))
        .unwrap();
        let id = initiative["id"].clone();
        initiative["projects"] =
            serde_json::json!({"nodes": [], "pageInfo": {"hasNextPage": true, "endCursor": null}});
        for terminal in [false, true] {
            let server = MockServer::start().await;
            let initiative = initiative.clone();
            Mock::given(method("POST")).respond_with(move |request: &Request| {
                let body: serde_json::Value = serde_json::from_slice(&request.body).unwrap();
                let page = body["variables"]["after"].as_str().map(|value| value.parse::<usize>().unwrap()).unwrap_or(0) + 1;
                let mut initiative = initiative.clone();
                initiative["projects"]["pageInfo"] = serde_json::json!({"hasNextPage": !terminal || page < 1000, "endCursor": page.to_string()});
                ResponseTemplate::new(200).set_body_json(serde_json::json!({"data": {"initiative": initiative}}))
            }).expect(1000).mount(&server).await;
            let args = serde_json::from_value(serde_json::json!({"id": id})).unwrap();
            let result =
                initiatives_get_data_with_url(&reqwest::Client::new(), &server.uri(), args).await;
            if terminal {
                assert!(result.unwrap().projects.is_empty());
            } else {
                assert!(result
                    .unwrap_err()
                    .to_string()
                    .contains("exceeded 1000 pages"));
            }
        }
    }

    #[tokio::test]
    async fn initiative_pages_preserve_fields_filters_and_final_page_info() {
        use wiremock::matchers::{body_json, method};
        use wiremock::{Mock, MockServer, ResponseTemplate};
        let sample: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/contract_samples/linear_initiative_list.json"
        ))
        .unwrap();
        for (all, count, query) in [
            (false, 0, None),
            (false, 1, Some("Roadmap")),
            (true, 2, Some("Roadmap")),
        ] {
            let server = MockServer::start().await;
            let mut expected = Vec::new();
            for index in 0..count.max(1) {
                let mut node = sample["nodes"][0].clone();
                if index == 1 {
                    node["owner"] = serde_json::Value::Null;
                    node["targetDate"] = serde_json::Value::Null;
                    node["description"] = serde_json::Value::Null;
                }
                let nodes = if count == 0 { vec![] } else { vec![node] };
                expected.extend(nodes.clone());
                let after = if index == 0 { "start" } else { "next" };
                let has_next = index == 0 && count != 0;
                let cursor = if has_next { Some("next") } else { None };
                Mock::given(method("POST"))
                    .and(body_json(serde_json::json!({"query": INITIATIVES_QUERY, "variables": {
                        "first": 1, "after": after, "filter": query.map(|query| serde_json::json!({"name": {"contains": query}}))
                    }})))
                    .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"data": {"initiatives": {
                        "nodes": nodes, "pageInfo": {"hasNextPage": has_next, "endCursor": cursor}
                    }}})))
                    .expect(1).mount(&server).await;
            }
            let mut input = serde_json::json!({"all": all, "limit": 1, "cursor": "start"});
            if let Some(query) = query {
                input["query"] = serde_json::json!(query);
            }
            let args = serde_json::from_value(input).unwrap();
            let output =
                initiatives_list_data_with_url(&reqwest::Client::new(), &server.uri(), args)
                    .await
                    .unwrap();
            let json = serde_json::to_value(&output).unwrap();
            assert_eq!(json["nodes"], serde_json::json!(expected));
            assert_eq!(json["pageInfo"]["hasNextPage"], count == 1);
            let result = crate::mcp::tools::to_dual_result(output).unwrap();
            let text: serde_json::Value =
                serde_json::from_str(result["content"][0]["text"].as_str().unwrap()).unwrap();
            assert_eq!(text, result["structuredContent"]);
            assert_eq!(text, json);
        }
        assert!(!INITIATIVES_QUERY.contains("content"));
        assert!(!INITIATIVES_QUERY.contains("projects"));
        assert!(!INITIATIVES_QUERY.contains("team"));
    }

    #[test]
    fn initiative_transform_rejects_malformed_selected_fields() {
        let sample: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/contract_samples/linear_initiative_list.json"
        ))
        .unwrap();
        let valid = serde_json::json!({"initiatives": sample});
        for status in ["Proposed", "Planned", "Active", "Completed", "Canceled"] {
            let mut data = valid.clone();
            data["initiatives"]["nodes"][0]["status"] = serde_json::json!(status);
            assert!(mcptools_core::linear::transform_initiatives(data).is_ok());
        }
        for (path, value) in [
            ("/initiatives/nodes/0/id", serde_json::json!("not-uuid")),
            ("/initiatives/nodes/0/name", serde_json::json!(" ")),
            ("/initiatives/nodes/0/status", serde_json::json!("Unknown")),
            (
                "/initiatives/nodes/0/targetDate",
                serde_json::json!("2026-02-30"),
            ),
            ("/initiatives/nodes/0/owner/id", serde_json::json!("bad")),
            ("/initiatives/nodes/0/owner/name", serde_json::json!("")),
            ("/initiatives/nodes/0/owner/email", serde_json::json!(null)),
            (
                "/initiatives/nodes/0/url",
                serde_json::json!("file:///etc/passwd"),
            ),
            ("/initiatives/pageInfo/hasNextPage", serde_json::json!(null)),
            ("/initiatives/pageInfo/endCursor", serde_json::json!("\n")),
        ] {
            let mut data = valid.clone();
            *data.pointer_mut(path).unwrap() = value;
            assert!(
                mcptools_core::linear::transform_initiatives(data).is_err(),
                "{path}"
            );
        }
        for field in [
            "id",
            "name",
            "description",
            "status",
            "targetDate",
            "owner",
            "url",
        ] {
            let mut data = valid.clone();
            data["initiatives"]["nodes"][0]
                .as_object_mut()
                .unwrap()
                .remove(field);
            assert!(
                mcptools_core::linear::transform_initiatives(data).is_err(),
                "{field}"
            );
        }
        let mut data = valid;
        data["initiatives"]["nodes"][0]["owner"]
            .as_object_mut()
            .unwrap()
            .remove("email");
        assert!(mcptools_core::linear::transform_initiatives(data.clone()).is_err());
        data["initiatives"]["pageInfo"]
            .as_object_mut()
            .unwrap()
            .remove("endCursor");
        assert!(mcptools_core::linear::transform_initiatives(data).is_err());
    }

    #[tokio::test]
    async fn initiative_errors_and_cursor_progress_fail_without_partial_output() {
        use wiremock::matchers::{body_json, method};
        use wiremock::{Mock, MockServer, ResponseTemplate};
        for (status, response, expected) in [
            (
                403,
                serde_json::json!({"errors": [{"message": "Permission denied"}]}),
                "403",
            ),
            (
                400,
                serde_json::json!({"errors": [{"message": "Invalid query"}]}),
                "400",
            ),
            (
                200,
                serde_json::json!({"data": {"initiatives": {"nodes": []}}, "errors": [{"message": "Permission denied"}]}),
                "Permission denied",
            ),
            (
                200,
                serde_json::json!({"data": {"initiatives": {"nodes": [], "pageInfo": {"hasNextPage": true, "endCursor": null}}}}),
                "cursor missing",
            ),
            (
                200,
                serde_json::json!({"data": {"initiatives": {"nodes": [], "pageInfo": {"hasNextPage": true, "endCursor": "start"}}}}),
                "did not progress",
            ),
        ] {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .respond_with(ResponseTemplate::new(status).set_body_json(response))
                .expect(1)
                .mount(&server)
                .await;
            let args = serde_json::from_value(serde_json::json!({"cursor": "start"})).unwrap();
            let error =
                initiatives_list_data_with_url(&reqwest::Client::new(), &server.uri(), args)
                    .await
                    .unwrap_err();
            assert!(error.to_string().contains(expected), "{error}");
        }
        let server = MockServer::start().await;
        for (after, next) in [(None, "one"), (Some("one"), "two"), (Some("two"), "one")] {
            Mock::given(method("POST"))
                .and(body_json(serde_json::json!({"query": INITIATIVES_QUERY, "variables": {"first": 25, "after": after, "filter": null}})))
                .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"data": {"initiatives": {"nodes": [], "pageInfo": {"hasNextPage": true, "endCursor": next}}}})))
                .expect(1).mount(&server).await;
        }
        let args = serde_json::from_value(serde_json::json!({"all": true})).unwrap();
        let error = initiatives_list_data_with_url(&reqwest::Client::new(), &server.uri(), args)
            .await
            .unwrap_err();
        assert!(error.to_string().contains("did not progress"));
        let server = MockServer::start().await;
        let sample: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/contract_samples/linear_initiative_list.json"
        ))
        .unwrap();
        for (after, response) in [
            (
                None,
                serde_json::json!({"data": {"initiatives": {"nodes": sample["nodes"], "pageInfo": {"hasNextPage": true, "endCursor": "next"}}}}),
            ),
            (
                Some("next"),
                serde_json::json!({"errors": [{"message": "Permission denied on later page"}]}),
            ),
        ] {
            Mock::given(method("POST"))
                .and(body_json(serde_json::json!({"query": INITIATIVES_QUERY, "variables": {"first": 25, "after": after, "filter": null}})))
                .respond_with(ResponseTemplate::new(200).set_body_json(response))
                .expect(1)
                .mount(&server)
                .await;
        }
        let args = serde_json::from_value(serde_json::json!({"all": true})).unwrap();
        let error = initiatives_list_data_with_url(&reqwest::Client::new(), &server.uri(), args)
            .await
            .unwrap_err();
        assert!(error
            .to_string()
            .contains("Permission denied on later page"));
    }

    #[tokio::test]
    async fn initiative_terminal_empty_page_accepts_the_input_cursor() {
        use wiremock::matchers::method;
        use wiremock::{Mock, MockServer, ResponseTemplate};
        for all in [false, true] {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"data": {"initiatives": {"nodes": [], "pageInfo": {"hasNextPage": false, "endCursor": "start"}}}})))
                .expect(1)
                .mount(&server)
                .await;
            let args =
                serde_json::from_value(serde_json::json!({"cursor": "start", "all": all})).unwrap();
            let output =
                initiatives_list_data_with_url(&reqwest::Client::new(), &server.uri(), args)
                    .await
                    .unwrap();
            assert!(output.nodes.is_empty());
            assert!(!output.page_info.has_next);
            assert_eq!(output.page_info.end_cursor.as_deref(), Some("start"));
        }
    }

    #[tokio::test]
    async fn initiative_all_pages_has_a_hard_page_cap() {
        use wiremock::matchers::method;
        use wiremock::{Mock, MockServer, Request, ResponseTemplate};
        for terminal in [false, true] {
            let server = MockServer::start().await;
            Mock::given(method("POST")).respond_with(move |request: &Request| {
                let body: serde_json::Value = serde_json::from_slice(&request.body).unwrap();
                let page = body["variables"]["after"].as_str().map(|value| value.parse::<usize>().unwrap()).unwrap_or(0) + 1;
                ResponseTemplate::new(200).set_body_json(serde_json::json!({"data": {"initiatives": {"nodes": [], "pageInfo": {"hasNextPage": !terminal || page < 1000, "endCursor": page.to_string()}}}}))
            }).expect(1000).mount(&server).await;
            let args = serde_json::from_value(serde_json::json!({"all": true})).unwrap();
            let result =
                initiatives_list_data_with_url(&reqwest::Client::new(), &server.uri(), args).await;
            if terminal {
                assert_eq!(
                    result.unwrap().page_info.end_cursor.as_deref(),
                    Some("1000")
                );
            } else {
                assert!(result
                    .unwrap_err()
                    .to_string()
                    .contains("exceeded 1000 pages"));
            }
        }
    }

    #[tokio::test]
    async fn progress_report_rename_after_lookup_preserves_response_names() {
        use wiremock::matchers::{body_json, method};
        use wiremock::{Mock, MockServer, ResponseTemplate};
        let sample: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/contract_samples/linear_project_update_list.json"
        ))
        .unwrap();
        let id = sample["project"]["id"].as_str().unwrap();
        let mut report = sample["nodes"][0].clone();
        report["project"]["name"] = serde_json::json!("  Renamed again  ");
        report["project"]["id"] = serde_json::json!(id.to_uppercase());
        let server = MockServer::start().await;
        for (query, variables, response) in [
            (
                PROJECT_GET_QUERY,
                serde_json::json!({"id": id}),
                serde_json::json!({"data": {"project": sample["project"]}}),
            ),
            (
                PROJECT_UPDATES_QUERY,
                serde_json::json!({"id": id, "first": 25, "after": null}),
                serde_json::json!({"data": {"project": {"id": id, "name": "  Renamed  ", "projectUpdates": {"nodes": [report], "pageInfo": sample["pageInfo"]}}}}),
            ),
        ] {
            Mock::given(method("POST"))
                .and(body_json(
                    serde_json::json!({"query": query, "variables": variables}),
                ))
                .respond_with(ResponseTemplate::new(200).set_body_json(response))
                .expect(1)
                .mount(&server)
                .await;
        }
        let args = serde_json::from_value(serde_json::json!({"project": id})).unwrap();
        let output =
            project_updates_list_data_with_url(&reqwest::Client::new(), &server.uri(), args)
                .await
                .unwrap();
        assert_eq!(output.project.name, "  Renamed  ");
        assert_eq!(serde_json::to_value(&output.nodes[0]).unwrap(), report);
    }

    #[tokio::test]
    async fn progress_report_thousand_page_boundary() {
        use wiremock::matchers::{body_json, body_partial_json, method};
        use wiremock::{Mock, MockServer, Request, ResponseTemplate};
        let id = "12345678-1234-1234-1234-123456789abc";
        for terminal in [true, false] {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .and(body_json(
                    serde_json::json!({"query": PROJECT_GET_QUERY, "variables": {"id": id}}),
                ))
                .respond_with(ResponseTemplate::new(200).set_body_json(
                    serde_json::json!({"data": {"project": {"id": id, "name": "Example"}}}),
                ))
                .expect(1)
                .mount(&server)
                .await;
            Mock::given(method("POST"))
                .and(body_partial_json(serde_json::json!({"query": PROJECT_UPDATES_QUERY, "variables": {"id": id, "first": 25}})))
                .respond_with(move |request: &Request| {
                    let body: serde_json::Value = request.body_json().unwrap();
                    let page = body["variables"]["after"].as_str().map_or(1, |cursor| cursor.parse::<usize>().unwrap() + 1);
                    ResponseTemplate::new(200).set_body_json(serde_json::json!({"data": {"project": {"id": id, "name": "Renamed", "projectUpdates": {"nodes": [], "pageInfo": {"hasNextPage": !terminal || page < 1000, "endCursor": page.to_string()}}}}}))
                })
                .expect(1000)
                .mount(&server)
                .await;
            let args =
                serde_json::from_value(serde_json::json!({"project": id, "all": true})).unwrap();
            let result =
                project_updates_list_data_with_url(&reqwest::Client::new(), &server.uri(), args)
                    .await;
            if terminal {
                let output = result.unwrap();
                assert!(output.nodes.is_empty());
                assert_eq!(output.project.name, "Renamed");
                assert!(!output.page_info.has_next);
                assert_eq!(output.page_info.end_cursor.as_deref(), Some("1000"));
            } else {
                assert!(result
                    .unwrap_err()
                    .to_string()
                    .contains("exceeded 1000 pages"));
            }
        }
    }

    #[tokio::test]
    async fn progress_report_pages_preserve_order_body_and_empty_results() {
        use wiremock::matchers::{body_json, method};
        use wiremock::{Mock, MockServer, ResponseTemplate};
        let sample: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/contract_samples/linear_project_update_list.json"
        ))
        .unwrap();
        let project = &sample["project"];
        for (all, empty) in [(false, false), (true, false), (true, true)] {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .and(body_json(serde_json::json!({"query": PROJECT_GET_QUERY, "variables": {"id": project["id"].as_str().unwrap().to_uppercase()}})))
                .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"data": {"project": project}})))
                .expect(1).mount(&server).await;
            let pages = if all && !empty { 2 } else { 1 };
            for index in 0..pages {
                let mut report = sample["nodes"][0].clone();
                report["health"] = serde_json::json!(if index == 0 { "onTrack" } else { "atRisk" });
                let nodes = if empty {
                    serde_json::json!([])
                } else {
                    serde_json::json!([report])
                };
                Mock::given(method("POST"))
                    .and(body_json(serde_json::json!({"query": PROJECT_UPDATES_QUERY, "variables": {"id": project["id"], "first": 1, "after": if index == 0 { "start" } else { "next" }}})))
                    .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"data": {"project": {"id": project["id"], "name": project["name"], "projectUpdates": {"nodes": nodes, "pageInfo": {"hasNextPage": index == 0 && !empty, "endCursor": if index == 0 && !empty { "next" } else { "final" }}}}}})))
                    .expect(1).mount(&server).await;
            }
            let args = serde_json::from_value(serde_json::json!({"project": project["id"].as_str().unwrap().to_uppercase(), "limit": 1, "cursor": "start", "all": all})).unwrap();
            let output =
                project_updates_list_data_with_url(&reqwest::Client::new(), &server.uri(), args)
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
            if empty {
                assert_eq!(text["nodes"], serde_json::json!([]));
            } else {
                assert_eq!(text["nodes"][0], sample["nodes"][0]);
                if all {
                    assert_eq!(text["nodes"][1]["health"], "atRisk");
                }
            }
        }
    }

    #[tokio::test]
    async fn progress_report_strict_fields_scope_and_transport_failures() {
        use wiremock::matchers::{body_json, method};
        use wiremock::{Mock, MockServer, ResponseTemplate};
        let sample: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/contract_samples/linear_project_update_list.json"
        ))
        .unwrap();
        let project = &sample["project"];
        let valid = serde_json::json!({"data": {"project": {"id": project["id"], "name": project["name"], "projectUpdates": {"nodes": sample["nodes"], "pageInfo": sample["pageInfo"]}}}});
        let mut cases = vec![
            (401, serde_json::json!({"errors": [{"message": "Denied"}]})),
            (500, serde_json::json!({"error": "Unavailable"})),
            (
                200,
                serde_json::json!({"errors": [{"message": "Partial failure"}], "data": valid["data"]}),
            ),
            (200, serde_json::json!({"data": {"project": null}})),
            (200, serde_json::json!({"data": {}})),
        ];
        for pointer in [
            "/data/project",
            "/data/project/projectUpdates/nodes/0",
            "/data/project/projectUpdates/nodes/0/project",
            "/data/project/projectUpdates/pageInfo",
            "/data/project/projectUpdates",
        ] {
            let fields: Vec<String> = valid
                .pointer(pointer)
                .unwrap()
                .as_object()
                .unwrap()
                .keys()
                .cloned()
                .collect();
            for field in fields {
                for value in [
                    None,
                    Some(serde_json::Value::Null),
                    Some(serde_json::json!(17)),
                ] {
                    if field == "endCursor" && value == Some(serde_json::Value::Null) {
                        continue;
                    }
                    let mut body = valid.clone();
                    let object = body.pointer_mut(pointer).unwrap().as_object_mut().unwrap();
                    if let Some(value) = value {
                        object.insert(field.clone(), value);
                    } else {
                        object.remove(&field);
                    }
                    cases.push((200, body));
                }
            }
        }
        for (pointer, value) in [
            ("/data/project/id", "22345678-1234-1234-1234-123456789abc"),
            ("/data/project/name", " "),
            ("/data/project/projectUpdates/nodes/0/id", "bad"),
            ("/data/project/projectUpdates/nodes/0/health", "unknown"),
            (
                "/data/project/projectUpdates/nodes/0/createdAt",
                "yesterday",
            ),
            (
                "/data/project/projectUpdates/nodes/0/updatedAt",
                "2026-99-01T00:00:00Z",
            ),
            ("/data/project/projectUpdates/nodes/0/url", "file:///report"),
            (
                "/data/project/projectUpdates/nodes/0/project/id",
                "22345678-1234-1234-1234-123456789abc",
            ),
            ("/data/project/projectUpdates/nodes/0/project/name", " "),
        ] {
            let mut body = valid.clone();
            *body.pointer_mut(pointer).unwrap() = serde_json::json!(value);
            cases.push((200, body));
        }
        for (status, body) in cases {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .and(body_json(serde_json::json!({"query": PROJECT_GET_QUERY, "variables": {"id": project["id"]}})))
                .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"data": {"project": project}})))
                .expect(1).mount(&server).await;
            Mock::given(method("POST"))
                .and(body_json(serde_json::json!({"query": PROJECT_UPDATES_QUERY, "variables": {"id": project["id"], "first": 25, "after": null}})))
                .respond_with(ResponseTemplate::new(status).set_body_json(body.clone()))
                .expect(1).mount(&server).await;
            let args =
                serde_json::from_value(serde_json::json!({"project": project["id"]})).unwrap();
            assert!(
                project_updates_list_data_with_url(&reqwest::Client::new(), &server.uri(), args)
                    .await
                    .is_err(),
                "{body}"
            );
        }
    }

    #[tokio::test]
    async fn progress_report_cursor_hazards_and_later_failures_discard_output() {
        use wiremock::matchers::{body_json, method};
        use wiremock::{Mock, MockServer, ResponseTemplate};
        let id = "12345678-1234-1234-1234-123456789abc";
        for (all, has_next, cursors) in [
            (false, true, vec![None]),
            (true, true, vec![None]),
            (false, true, vec![Some(" ")]),
            (false, true, vec![Some("start")]),
            (false, false, vec![Some("start")]),
            (false, false, vec![Some("bad\ncursor")]),
            (true, true, vec![Some("a"), Some("b"), Some("a")]),
        ] {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .and(body_json(
                    serde_json::json!({"query": PROJECT_GET_QUERY, "variables": {"id": id}}),
                ))
                .respond_with(ResponseTemplate::new(200).set_body_json(
                    serde_json::json!({"data": {"project": {"id": id, "name": "Example"}}}),
                ))
                .expect(1)
                .mount(&server)
                .await;
            let mut after = Some("start");
            for next in cursors {
                Mock::given(method("POST"))
                    .and(body_json(serde_json::json!({"query": PROJECT_UPDATES_QUERY, "variables": {"id": id, "first": 25, "after": after}})))
                    .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"data": {"project": {"id": id, "name": "Example", "projectUpdates": {"nodes": [], "pageInfo": {"hasNextPage": has_next, "endCursor": next}}}}})))
                    .expect(1).mount(&server).await;
                after = next;
            }
            let args = serde_json::from_value(
                serde_json::json!({"project": id, "cursor": "start", "all": all}),
            )
            .unwrap();
            assert!(project_updates_list_data_with_url(
                &reqwest::Client::new(),
                &server.uri(),
                args
            )
            .await
            .unwrap_err()
            .to_string()
            .contains("pagination cursor"));
        }
        for body in [
            serde_json::json!({"data": {"project": null}}),
            serde_json::json!({"errors": [{"message": "Later page failure"}]}),
            serde_json::json!({"data": {"project": {"id": "22345678-1234-1234-1234-123456789abc", "name": "Example", "projectUpdates": {"nodes": [], "pageInfo": {"hasNextPage": false, "endCursor": null}}}}}),
        ] {
            let server = MockServer::start().await;
            for (query, variables, response) in [
                (
                    PROJECT_GET_QUERY,
                    serde_json::json!({"id": id}),
                    serde_json::json!({"data": {"project": {"id": id, "name": "Example"}}}),
                ),
                (
                    PROJECT_UPDATES_QUERY,
                    serde_json::json!({"id": id, "first": 25, "after": null}),
                    serde_json::json!({"data": {"project": {"id": id, "name": "Example", "projectUpdates": {"nodes": [], "pageInfo": {"hasNextPage": true, "endCursor": "next"}}}}}),
                ),
                (
                    PROJECT_UPDATES_QUERY,
                    serde_json::json!({"id": id, "first": 25, "after": "next"}),
                    body,
                ),
            ] {
                Mock::given(method("POST"))
                    .and(body_json(
                        serde_json::json!({"query": query, "variables": variables}),
                    ))
                    .respond_with(ResponseTemplate::new(200).set_body_json(response))
                    .expect(1)
                    .mount(&server)
                    .await;
            }
            let args =
                serde_json::from_value(serde_json::json!({"project": id, "all": true})).unwrap();
            assert!(project_updates_list_data_with_url(
                &reqwest::Client::new(),
                &server.uri(),
                args
            )
            .await
            .is_err());
        }
    }

    #[tokio::test]
    async fn progress_report_resolves_team_selectors_and_rejects_missing_projects() {
        use wiremock::matchers::{body_json, method};
        use wiremock::{Mock, MockServer, ResponseTemplate};
        let id = "12345678-1234-1234-1234-123456789abc";
        let team = "22345678-1234-1234-1234-123456789abc";
        for selector in [team.to_uppercase(), "GUZ".to_string(), "Team".to_string()] {
            for count in 0..=2 {
                let server = MockServer::start().await;
                Mock::given(method("POST"))
                    .and(body_json(serde_json::json!({"query": TEAMS_QUERY, "variables": {"first": 50, "after": null}})))
                    .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"data": {"teams": {"nodes": [{"id": team, "key": "GUZ", "name": "Team"}], "pageInfo": {"hasNextPage": false, "endCursor": null}}}})))
                    .expect(1).mount(&server).await;
                for (index, after) in [None, Some("next")].into_iter().enumerate() {
                    let nodes = if index >= 2 - count {
                        serde_json::json!([{"id": if index == 1 { id } else { team }, "name": "Example"}])
                    } else {
                        serde_json::json!([])
                    };
                    Mock::given(method("POST"))
                        .and(body_json(serde_json::json!({"query": TEAM_PROJECTS_QUERY, "variables": {"id": team, "first": 50, "after": after}})))
                        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"data": {"team": {"id": team, "projects": {"nodes": nodes, "pageInfo": {"hasNextPage": index == 0, "endCursor": if index == 0 { Some("next") } else { None }}}}}})))
                        .expect(1).mount(&server).await;
                }
                if count == 1 {
                    Mock::given(method("POST"))
                        .and(body_json(serde_json::json!({"query": PROJECT_UPDATES_QUERY, "variables": {"id": id, "first": 25, "after": null}})))
                        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"data": {"project": {"id": id, "name": "Example", "projectUpdates": {"nodes": [], "pageInfo": {"hasNextPage": false, "endCursor": null}}}}})))
                        .expect(1).mount(&server).await;
                }
                let args = serde_json::from_value(
                    serde_json::json!({"project": " example ", "team": selector}),
                )
                .unwrap();
                let result = project_updates_list_data_with_url(
                    &reqwest::Client::new(),
                    &server.uri(),
                    args,
                )
                .await;
                assert_eq!(result.is_ok(), count == 1, "{result:?}");
                assert_eq!(
                    server.received_requests().await.unwrap().len(),
                    if count == 1 { 4 } else { 3 }
                );
            }
        }
        for project in [
            serde_json::Value::Null,
            serde_json::json!({"id": team, "name": "Example"}),
        ] {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .and(body_json(
                    serde_json::json!({"query": PROJECT_GET_QUERY, "variables": {"id": id}}),
                ))
                .respond_with(
                    ResponseTemplate::new(200)
                        .set_body_json(serde_json::json!({"data": {"project": project}})),
                )
                .expect(1)
                .mount(&server)
                .await;
            let args = serde_json::from_value(serde_json::json!({"project": id})).unwrap();
            assert!(project_updates_list_data_with_url(
                &reqwest::Client::new(),
                &server.uri(),
                args
            )
            .await
            .is_err());
            assert_eq!(server.received_requests().await.unwrap().len(), 1);
        }
    }

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
    async fn milestone_update_partial_clears_and_failed_responses() {
        use mcptools_core::linear::{project_milestone_update_input, ProjectMilestoneUpdateArgs};
        use wiremock::{
            matchers::{body_json, method},
            Mock, MockServer, ResponseTemplate,
        };
        let sample: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/contract_samples/linear_project_milestone_update.json"
        ))
        .unwrap();
        let id = sample["id"].as_str().unwrap();
        let mut renamed = sample.clone();
        renamed["name"] = serde_json::json!("Concurrent milestone rename");
        renamed["project"]["name"] = serde_json::json!("Concurrent project rename");
        renamed["description"] = serde_json::Value::Null;
        renamed["targetDate"] = serde_json::Value::Null;
        assert_eq!(serde_json::to_value(mcptools_core::linear::transform_project_milestone_update(
            serde_json::json!({"projectMilestoneUpdate": {"success": true, "projectMilestone": renamed}}), &id.to_uppercase()
        ).unwrap()).unwrap(), renamed);
        for (fields, expected) in [
            (
                serde_json::json!({"name": " New "}),
                serde_json::json!({"name": "New"}),
            ),
            (
                serde_json::json!({"description": "    code\n"}),
                serde_json::json!({"description": "    code\n"}),
            ),
            (
                serde_json::json!({"description": ""}),
                serde_json::json!({"description": ""}),
            ),
            (
                serde_json::json!({"description": " \n"}),
                serde_json::json!({"description": " \n"}),
            ),
            (
                serde_json::json!({"targetDate": "2028-02-29"}),
                serde_json::json!({"targetDate": "2028-02-29"}),
            ),
            (
                serde_json::json!({"sortOrder": 0}),
                serde_json::json!({"sortOrder": 0.0}),
            ),
            (
                serde_json::json!({"clearDescription": true}),
                serde_json::json!({"description": null}),
            ),
            (
                serde_json::json!({"clearTargetDate": true}),
                serde_json::json!({"targetDate": null}),
            ),
            (
                serde_json::json!({"name": "New", "description": " x ", "targetDate": "2028-02-29", "sortOrder": -1.5}),
                serde_json::json!({"name": "New", "description": " x ", "targetDate": "2028-02-29", "sortOrder": -1.5}),
            ),
            (
                serde_json::json!({"clearDescription": true, "clearTargetDate": true}),
                serde_json::json!({"description": null, "targetDate": null}),
            ),
        ] {
            let server = MockServer::start().await;
            let mut raw = fields;
            raw["id"] = serde_json::json!(id.to_uppercase());
            let args: ProjectMilestoneUpdateArgs = serde_json::from_value(raw).unwrap();
            assert_eq!(project_milestone_update_input(&args).unwrap(), expected);
            Mock::given(method("POST")).and(body_json(serde_json::json!({"query": PROJECT_MILESTONE_UPDATE_MUTATION, "variables": {"id": id.to_uppercase(), "input": expected}})))
                .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"data": {"projectMilestoneUpdate": {"success": true, "projectMilestone": sample}}}))).expect(1).mount(&server).await;
            let output = project_milestones_update_data_with_url(
                &reqwest::Client::new(),
                &server.uri(),
                &args,
            )
            .await
            .unwrap();
            assert_eq!(serde_json::to_value(output).unwrap(), sample);
        }
        let server = MockServer::start().await;
        for fields in [
            serde_json::json!({}),
            serde_json::json!({"name": " "}),
            serde_json::json!({"id": "", "name": "New"}),
            serde_json::json!({"id": "bad", "name": "New"}),
            serde_json::json!({"description": "", "clearDescription": true}),
            serde_json::json!({"targetDate": "2028-02-29", "clearTargetDate": true}),
            serde_json::json!({"targetDate": "2026-04-31"}),
            serde_json::json!({"clearDescription": false}),
        ] {
            let mut raw = serde_json::json!({"id": id});
            raw.as_object_mut()
                .unwrap()
                .extend(fields.as_object().unwrap().clone());
            let args = serde_json::from_value(raw).unwrap();
            assert!(project_milestones_update_data_with_url(
                &reqwest::Client::new(),
                &server.uri(),
                &args
            )
            .await
            .is_err());
        }
        let mut args: ProjectMilestoneUpdateArgs =
            serde_json::from_value(serde_json::json!({"id": id, "name": "New"})).unwrap();
        for number in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            args.sort_order = Some(number);
            assert!(project_milestones_update_data_with_url(
                &reqwest::Client::new(),
                &server.uri(),
                &args
            )
            .await
            .is_err());
        }
        assert!(server.received_requests().await.unwrap().is_empty());
        args.sort_order = None;
        let valid = serde_json::json!({"data": {"projectMilestoneUpdate": {"success": true, "projectMilestone": sample}}});
        let mut failures = vec![
            serde_json::json!({"errors": [{"message": "failed"}], "data": valid["data"]}),
            serde_json::json!({}),
            serde_json::Value::Null,
            serde_json::json!({"data": null}),
            serde_json::json!({"data": []}),
            serde_json::json!({"data": {}}),
        ];
        for payload in [
            serde_json::Value::Null,
            serde_json::json!({"success": false, "projectMilestone": sample}),
            serde_json::json!({"success": "true", "projectMilestone": sample}),
            serde_json::json!({"projectMilestone": sample}),
            serde_json::json!({"success": true}),
            serde_json::json!({"success": true, "projectMilestone": null}),
        ] {
            failures.push(serde_json::json!({"data": {"projectMilestoneUpdate": payload}}));
        }
        for field in sample.as_object().unwrap().keys() {
            let mut missing = valid.clone();
            missing["data"]["projectMilestoneUpdate"]["projectMilestone"]
                .as_object_mut()
                .unwrap()
                .remove(field);
            failures.push(missing);
        }
        for (field, value) in [
            (
                "id",
                serde_json::json!("12345678-1234-1234-1234-123456789abc"),
            ),
            ("name", serde_json::json!(" ")),
            (
                "project",
                serde_json::json!({"id": "bad", "name": "Project"}),
            ),
            ("project", serde_json::json!({"id": id, "name": " "})),
            ("project", serde_json::json!({"id": id})),
            ("status", serde_json::json!("bad")),
            ("description", serde_json::json!(1)),
            ("targetDate", serde_json::json!("2026-02-29")),
            ("sortOrder", serde_json::Value::Null),
        ] {
            let mut invalid = valid.clone();
            invalid["data"]["projectMilestoneUpdate"]["projectMilestone"][field] = value;
            failures.push(invalid);
        }
        for response in failures {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .respond_with(ResponseTemplate::new(200).set_body_json(response))
                .expect(1)
                .mount(&server)
                .await;
            assert!(project_milestones_update_data_with_url(
                &reqwest::Client::new(),
                &server.uri(),
                &args
            )
            .await
            .is_err());
        }
        for response in [
            ResponseTemplate::new(500),
            ResponseTemplate::new(429),
            ResponseTemplate::new(200).set_body_string("{invalid-json"),
        ] {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .respond_with(response)
                .expect(1)
                .mount(&server)
                .await;
            assert!(project_milestones_update_data_with_url(
                &reqwest::Client::new(),
                &server.uri(),
                &args
            )
            .await
            .is_err());
        }
    }

    #[tokio::test]
    async fn initiative_project_add_checks_entities_mutation_and_failures() {
        use wiremock::matchers::{body_json, method};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let sample: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/contract_samples/linear_initiative_project_add.json"
        ))
        .unwrap();
        let args: crate::linear::args::InitiativeProjectAddArgs = serde_json::from_value(serde_json::json!({"initiative": sample["initiative"]["id"], "project": sample["project"]["id"]})).unwrap();
        let success = serde_json::json!({"data": {"initiativeToProjectCreate": {"success": true, "initiativeToProject": sample}}});
        let mut cases = vec![(200, success.clone())];
        for message in [
            "Project already associated with initiative",
            "Forbidden",
            "GraphQL failure",
        ] {
            cases.push((200, serde_json::json!({"errors": [{"message": message}]})));
        }
        cases.push((
            500,
            serde_json::json!({"errors": [{"message": "HTTP failure"}]}),
        ));
        cases.push((
            429,
            serde_json::json!({"errors": [{"message": "Rate limited"}]}),
        ));
        for pointer in [
            "/data/initiativeToProjectCreate/success",
            "/data/initiativeToProjectCreate/initiativeToProject/id",
            "/data/initiativeToProjectCreate/initiativeToProject/initiative",
            "/data/initiativeToProjectCreate/initiativeToProject/project",
        ] {
            let mut malformed = success.clone();
            *malformed.pointer_mut(pointer).unwrap() = serde_json::Value::Null;
            cases.push((200, malformed));
        }
        let mut failed = success.clone();
        failed["data"]["initiativeToProjectCreate"]["success"] = serde_json::json!(false);
        cases.push((200, failed));
        let mut mismatched = success.clone();
        mismatched["data"]["initiativeToProjectCreate"]["initiativeToProject"]["project"]["id"] =
            sample["initiative"]["id"].clone();
        cases.push((200, mismatched));
        for (index, (status, response)) in cases.into_iter().enumerate() {
            let server = MockServer::start().await;
            for (query, variables, data) in [
                (
                    PROJECT_GET_QUERY,
                    serde_json::json!({"id": args.project}),
                    serde_json::json!({"project": sample["project"]}),
                ),
                (
                    INITIATIVE_IDENTITY_QUERY,
                    serde_json::json!({"id": args.initiative}),
                    serde_json::json!({"initiative": sample["initiative"]}),
                ),
            ] {
                Mock::given(method("POST"))
                    .and(body_json(
                        serde_json::json!({"query": query, "variables": variables}),
                    ))
                    .respond_with(
                        ResponseTemplate::new(200).set_body_json(serde_json::json!({"data": data})),
                    )
                    .expect(1)
                    .mount(&server)
                    .await;
            }
            Mock::given(method("POST")).and(body_json(serde_json::json!({"query": INITIATIVE_PROJECT_ADD_MUTATION, "variables": {"input": {"initiativeId": args.initiative, "projectId": args.project}}}))).respond_with(ResponseTemplate::new(status).set_body_json(response)).expect(1).mount(&server).await;
            let result =
                initiative_project_add_data_with_url(&reqwest::Client::new(), &server.uri(), &args)
                    .await;
            if index == 0 {
                let dual = crate::mcp::tools::to_dual_result(result.unwrap()).unwrap();
                assert_eq!(dual["structuredContent"], sample);
                assert_eq!(
                    serde_json::from_str::<serde_json::Value>(
                        dual["content"][0]["text"].as_str().unwrap()
                    )
                    .unwrap(),
                    sample
                );
            } else {
                assert!(result.is_err(), "case {index}");
                if index == 1 {
                    assert!(result
                        .unwrap_err()
                        .to_string()
                        .contains("already associated"));
                }
            }
        }
        for entity in ["project", "initiative"] {
            for response in [
                serde_json::json!({"data": {entity: null}}),
                serde_json::json!({"errors": [{"message": "Forbidden"}]}),
            ] {
                let server = MockServer::start().await;
                if entity == "initiative" {
                    Mock::given(body_json(serde_json::json!({"query": PROJECT_GET_QUERY, "variables": {"id": args.project}}))).respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"data": {"project": sample["project"]}}))).expect(1).mount(&server).await;
                }
                let (query, id) = if entity == "project" {
                    (PROJECT_GET_QUERY, &args.project)
                } else {
                    (INITIATIVE_IDENTITY_QUERY, &args.initiative)
                };
                Mock::given(body_json(
                    serde_json::json!({"query": query, "variables": {"id": id}}),
                ))
                .respond_with(ResponseTemplate::new(200).set_body_json(response))
                .expect(1)
                .mount(&server)
                .await;
                assert!(initiative_project_add_data_with_url(
                    &reqwest::Client::new(),
                    &server.uri(),
                    &args
                )
                .await
                .is_err());
                assert_eq!(
                    server.received_requests().await.unwrap().len(),
                    if entity == "project" { 1 } else { 2 }
                );
            }
        }
    }

    #[tokio::test]
    async fn initiative_project_add_resolves_project_names_before_mutation() {
        use wiremock::matchers::body_json;
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let sample: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/contract_samples/linear_initiative_project_add.json"
        ))
        .unwrap();
        let team_id = "12345678-1234-1234-1234-123456789bbb";
        for count in [0, 1, 2] {
            let server = MockServer::start().await;
            Mock::given(body_json(serde_json::json!({"query": TEAMS_QUERY, "variables": {"first": 50, "after": null}}))).respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"data": {"teams": {"nodes": [{"id": team_id, "key": "GUZ", "name": "Team"}], "pageInfo": {"hasNextPage": false, "endCursor": null}}}}))).expect(1).mount(&server).await;
            let mut projects = Vec::new();
            if count > 0 {
                projects.push(sample["project"].clone());
            }
            if count > 1 {
                projects.push(serde_json::json!({"id": team_id, "name": "Example"}));
            }
            Mock::given(body_json(serde_json::json!({"query": TEAM_PROJECTS_QUERY, "variables": {"id": team_id, "first": 50, "after": null}}))).respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"data": {"team": {"id": team_id, "projects": {"nodes": projects, "pageInfo": {"hasNextPage": false, "endCursor": null}}}}}))).expect(1).mount(&server).await;
            if count == 1 {
                Mock::given(body_json(serde_json::json!({"query": INITIATIVE_IDENTITY_QUERY, "variables": {"id": sample["initiative"]["id"]}}))).respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"data": {"initiative": sample["initiative"]}}))).expect(1).mount(&server).await;
                Mock::given(body_json(serde_json::json!({"query": INITIATIVE_PROJECT_ADD_MUTATION, "variables": {"input": {"initiativeId": sample["initiative"]["id"], "projectId": sample["project"]["id"]}}}))).respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"data": {"initiativeToProjectCreate": {"success": true, "initiativeToProject": sample}}}))).expect(1).mount(&server).await;
            }
            let args = serde_json::from_value(serde_json::json!({"initiative": sample["initiative"]["id"], "project": " Example ", "team": "GUZ"})).unwrap();
            let result =
                initiative_project_add_data_with_url(&reqwest::Client::new(), &server.uri(), &args)
                    .await;
            if count == 1 {
                assert_eq!(serde_json::to_value(result.unwrap()).unwrap(), sample);
            } else {
                assert!(result.is_err());
                assert_eq!(server.received_requests().await.unwrap().len(), 2);
            }
        }
    }

    #[tokio::test]
    async fn project_update_create_preserves_markdown_and_rejects_failed_responses() {
        use wiremock::matchers::{body_json, method};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let sample: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/contract_samples/linear_project_update_create.json"
        ))
        .unwrap();
        let id = sample["project"]["id"].as_str().unwrap();
        for health in [None, Some("onTrack"), Some("atRisk"), Some("offTrack")] {
            let server = MockServer::start().await;
            let mut raw = serde_json::json!({"project": id, "body": sample["body"]});
            let mut input = serde_json::json!({"projectId": id, "body": sample["body"]});
            let mut output = sample.clone();
            if let Some(health) = health {
                raw["health"] = serde_json::json!(health);
                input["health"] = serde_json::json!(health);
                output["health"] = serde_json::json!(health);
            } else {
                output["health"] = serde_json::json!("atRisk");
                output["project"]["name"] = serde_json::json!("Renamed Example");
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
                .and(body_json(serde_json::json!({"query": PROJECT_UPDATE_CREATE_MUTATION, "variables": {"input": input}})))
                .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"data": {"projectUpdateCreate": {"success": true, "projectUpdate": output}}})))
                .expect(1).mount(&server).await;
            let args = serde_json::from_value(raw).unwrap();
            let created =
                project_updates_create_data_with_url(&reqwest::Client::new(), &server.uri(), &args)
                    .await
                    .unwrap();
            assert_eq!(serde_json::to_value(&created).unwrap(), output);
            let dual = crate::mcp::tools::to_dual_result(created).unwrap();
            let text: serde_json::Value =
                serde_json::from_str(dual["content"][0]["text"].as_str().unwrap()).unwrap();
            assert_eq!(text, dual["structuredContent"]);
        }
        let mut failures = vec![
            (
                500,
                serde_json::json!({"errors": [{"message": "HTTP failure"}]}),
            ),
            (
                200,
                serde_json::json!({"errors": [{"message": "GraphQL failure"}]}),
            ),
            (
                200,
                serde_json::json!({"data": {"projectUpdateCreate": {"success": true, "projectUpdate": sample}}, "errors": [{"message": "Partial failure"}]}),
            ),
            (200, serde_json::json!({"data": {}})),
            (
                200,
                serde_json::json!({"data": {"projectUpdateCreate": null}}),
            ),
            (
                200,
                serde_json::json!({"data": {"projectUpdateCreate": {"success": false, "projectUpdate": sample}}}),
            ),
            (
                200,
                serde_json::json!({"data": {"projectUpdateCreate": {"success": true, "projectUpdate": null}}}),
            ),
            (
                200,
                serde_json::json!({"data": {"projectUpdateCreate": {"success": true}}}),
            ),
            (
                200,
                serde_json::json!({"data": {"projectUpdateCreate": {"projectUpdate": sample}}}),
            ),
            (
                200,
                serde_json::json!({"data": {"projectUpdateCreate": {"success": "true", "projectUpdate": sample}}}),
            ),
        ];
        for field in ["id", "body", "health", "createdAt", "url", "project"] {
            for value in [
                None,
                Some(serde_json::Value::Null),
                Some(serde_json::json!(7)),
            ] {
                let mut malformed = sample.clone();
                match value {
                    Some(value) => {
                        malformed[field] = value;
                    }
                    None => {
                        malformed.as_object_mut().unwrap().remove(field);
                    }
                }
                failures.push((200, serde_json::json!({"data": {"projectUpdateCreate": {"success": true, "projectUpdate": malformed}}})));
            }
        }
        for (field, value) in [
            ("id", serde_json::json!(" ")),
            ("body", serde_json::json!(" \n")),
            ("health", serde_json::json!("unknown")),
            ("createdAt", serde_json::json!("yesterday")),
            ("url", serde_json::json!("")),
            ("project", serde_json::json!({"id": id, "name": ""})),
            (
                "project",
                serde_json::json!({"id": sample["id"], "name": "Example"}),
            ),
            ("project", serde_json::json!({"id": id})),
            ("project", serde_json::json!({"name": "Example"})),
        ] {
            let mut malformed = sample.clone();
            malformed[field] = value;
            failures.push((200, serde_json::json!({"data": {"projectUpdateCreate": {"success": true, "projectUpdate": malformed}}})));
        }
        let args =
            serde_json::from_value(serde_json::json!({"project": id, "body": sample["body"]}))
                .unwrap();
        for (status, response) in failures {
            let server = MockServer::start().await;
            Mock::given(body_json(
                serde_json::json!({"query": PROJECT_GET_QUERY, "variables": {"id": id}}),
            ))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({"data": {"project": sample["project"]}})),
            )
            .expect(1)
            .mount(&server)
            .await;
            Mock::given(body_json(serde_json::json!({"query": PROJECT_UPDATE_CREATE_MUTATION, "variables": {"input": {"projectId": id, "body": sample["body"]}}})))
                .respond_with(ResponseTemplate::new(status).set_body_json(response))
                .expect(1).mount(&server).await;
            assert!(project_updates_create_data_with_url(
                &reqwest::Client::new(),
                &server.uri(),
                &args
            )
            .await
            .is_err());
        }
    }

    #[tokio::test]
    async fn project_update_create_resolves_names_and_never_writes_invalid_projects() {
        use wiremock::matchers::body_json;
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let sample: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/contract_samples/linear_project_update_create.json"
        ))
        .unwrap();
        for nodes in [
            serde_json::json!([sample["project"]]),
            serde_json::json!([]),
            serde_json::json!([sample["project"], sample["project"]]),
        ] {
            let server = MockServer::start().await;
            let team_id = "12345678-1234-1234-1234-123456789aaa";
            Mock::given(body_json(serde_json::json!({"query": TEAMS_QUERY, "variables": {"first": 50, "after": null}})))
                .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"data": {"teams": {"nodes": [{"id": team_id, "key": "GUZ", "name": "Team"}], "pageInfo": {"hasNextPage": false, "endCursor": null}}}})))
                .expect(1).mount(&server).await;
            Mock::given(body_json(serde_json::json!({"query": TEAM_PROJECTS_QUERY, "variables": {"id": team_id, "first": 50, "after": null}})))
                .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"data": {"team": {"id": team_id, "projects": {"nodes": nodes, "pageInfo": {"hasNextPage": false, "endCursor": null}}}}})))
                .expect(1).mount(&server).await;
            let success = nodes.as_array().unwrap().len() == 1;
            Mock::given(body_json(serde_json::json!({"query": PROJECT_UPDATE_CREATE_MUTATION, "variables": {"input": {"projectId": sample["project"]["id"], "body": sample["body"]}}})))
                .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"data": {"projectUpdateCreate": {"success": true, "projectUpdate": sample}}})))
                .expect(if success { 1 } else { 0 }).mount(&server).await;
            let args = serde_json::from_value(
                serde_json::json!({"project": " example ", "team": "GUZ", "body": sample["body"]}),
            )
            .unwrap();
            assert_eq!(
                project_updates_create_data_with_url(&reqwest::Client::new(), &server.uri(), &args)
                    .await
                    .is_ok(),
                success
            );
        }
        for fields in [
            serde_json::json!({"project": " "}),
            serde_json::json!({"project": "Example"}),
            serde_json::json!({"project": "12345678-1234-1234-1234-123456789xyz"}),
            serde_json::json!({"project": "Exam\nple", "team": "GUZ"}),
            serde_json::json!({"team": " "}),
            serde_json::json!({"body": " \n\t"}),
        ] {
            let server = MockServer::start().await;
            let mut input =
                serde_json::json!({"project": sample["project"]["id"], "body": sample["body"]});
            input
                .as_object_mut()
                .unwrap()
                .extend(fields.as_object().unwrap().clone());
            let args = serde_json::from_value(input).unwrap();
            assert!(project_updates_create_data_with_url(
                &reqwest::Client::new(),
                &server.uri(),
                &args
            )
            .await
            .is_err());
            assert!(server.received_requests().await.unwrap().is_empty());
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
