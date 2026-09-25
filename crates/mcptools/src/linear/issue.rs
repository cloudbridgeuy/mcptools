use crate::linear::client::execute;
use crate::prelude::*;
use mcptools_core::linear::{
    is_uuid, issue_create_input, issue_filter_value, issue_update_input, match_state,
    parse_state_selector, team_key_from_identifier, transform_issue_create, transform_issue_update,
    Activity, IssueGetOutput, IssueListFilter, IssueMini, StateResolution,
};

pub const ISSUE_QUERY: &str =
    "query ($id: String!) { issue(id: $id) { id identifier title description priorityLabel url state { name } parent { identifier } inverseRelations(first: 25) { nodes { type issue { identifier } } } createdAt creator { name displayName } history(first: 50) { nodes { createdAt actor { name displayName } botActor { name } fromState { name } toState { name } fromTitle toTitle updatedDescription addedLabels { name } removedLabels { name } fromParent { identifier } toParent { identifier } fromAssignee { name displayName } toAssignee { name displayName } fromCycle { number name } toCycle { number name } fromProject { name } toProject { name } attachment { title } relationChanges { identifier type } } pageInfo { hasNextPage } } } }";

pub const ISSUES_QUERY: &str = "query ($first: Int!, $after: String, $filter: IssueFilter) { issues(first: $first, after: $after, filter: $filter, orderBy: updatedAt) { nodes { id identifier title priorityLabel url state { name } parent { identifier } inverseRelations(first: 25) { nodes { type issue { identifier } } } } pageInfo { hasNextPage endCursor } } }";

pub const ISSUE_CREATE_MUTATION: &str = "mutation ($input: IssueCreateInput!) { issueCreate(input: $input) { success issue { id identifier title priorityLabel url state { name } parent { identifier } } } }";

pub const ISSUE_UPDATE_MUTATION: &str = "mutation IssueUpdate($id: String!, $input: IssueUpdateInput!) { issueUpdate(id: $id, input: $input) { success issue { id identifier title priorityLabel url state { name } parent { identifier } inverseRelations(first: 25) { nodes { type issue { identifier } } } } } }";

pub async fn issue_get_data(
    client: &reqwest::Client,
    id_or_identifier: &str,
) -> Result<(IssueMini, Vec<Activity>)> {
    let selector = id_or_identifier.trim();
    if selector.is_empty() {
        return Err(eyre!("Linear issue id must not be empty"));
    }
    let data = execute(client, ISSUE_QUERY, serde_json::json!({"id": selector})).await?;
    let snapshot = mcptools_core::linear::transform_issue(data.clone()).map_err(|e| match e {
        mcptools_core::linear::LinearError::MissingIssue => {
            eyre!("Linear issue not found: {}", selector)
        }
        other => eyre!("{}", other),
    })?;
    let activity = mcptools_core::linear::transform_activity(data).map_err(|e| match e {
        mcptools_core::linear::LinearError::MissingIssue => {
            eyre!("Linear issue not found: {}", selector)
        }
        other => eyre!("{}", other),
    })?;
    Ok((snapshot, activity))
}

pub async fn issue_get_output(client: &reqwest::Client, id: &str) -> Result<IssueGetOutput> {
    let (snapshot, activity) = issue_get_data(client, id).await?;
    let mut comments = Vec::new();
    let mut cursor = None;
    loop {
        let page = super::comments::comments_list_data(client, id, 50, cursor).await?;
        let has_next = page.page_info.has_next;
        cursor = page.page_info.end_cursor;
        comments.extend(page.nodes);
        if !has_next {
            break;
        }
    }
    Ok(IssueGetOutput {
        id: snapshot.id,
        identifier: snapshot.identifier,
        title: snapshot.title,
        url: snapshot.url,
        state: snapshot.state,
        description: snapshot.description,
        priority: snapshot.priority,
        parent: snapshot.parent,
        blocked_by: snapshot.blocked_by,
        comments,
        activity,
    })
}

#[allow(clippy::too_many_arguments)]
pub async fn resolve_issue_filter(
    client: &reqwest::Client,
    team: Option<&str>,
    project: Option<&str>,
    assignee: Option<&str>,
    state: Option<&str>,
    label: Option<&str>,
    cycle: Option<&str>,
    query: Option<&str>,
    updated_after: Option<&str>,
) -> Result<IssueListFilter> {
    let team_id = match team.map(str::trim).filter(|text| !text.is_empty()) {
        Some(selector) => Some(super::discover::teams_get_data(client, selector).await?.id),
        None => None,
    };
    let project_id = match project.map(str::trim).filter(|text| !text.is_empty()) {
        Some(selector) if is_uuid(selector) => Some(selector.to_string()),
        Some(selector) => match team {
            Some(team_selector) => Some(
                super::discover::projects_get_data(client, selector, team_selector.trim())
                    .await?
                    .id,
            ),
            None => {
                return Err(eyre!(
                    "Linear issue list --project '{}' needs --team to resolve the project name. Pass a project UUID to skip team resolution",
                    selector
                ));
            }
        },
        None => None,
    };
    let assignee_id = match assignee.map(str::trim).filter(|text| !text.is_empty()) {
        Some(selector) if selector.eq_ignore_ascii_case("me") => {
            Some(super::auth::auth_status_data(client).await?.id)
        }
        Some(selector) if is_uuid(selector) => Some(selector.to_string()),
        Some(selector) => {
            return Err(eyre!(
                "Linear issue list --assignee '{}' must be a user UUID or 'me'. Find the UUID with `mcptools linear users list --query NAME`",
                selector
            ));
        }
        None => None,
    };
    for (flag, value) in [
        ("--state", state),
        ("--label", label),
        ("--cycle", cycle),
        ("--query", query),
        ("--updated-after", updated_after),
    ] {
        if value.is_some_and(|text| text.trim().is_empty()) {
            return Err(eyre!("Linear issue list {} must not be empty", flag));
        }
    }
    Ok(IssueListFilter {
        team_id,
        project_id,
        assignee_id,
        state: state.map(str::trim).map(str::to_string),
        label: label.map(str::trim).map(str::to_string),
        cycle: cycle.map(str::trim).map(str::to_string),
        query: query.map(str::trim).map(str::to_string),
        updated_after: updated_after.map(str::trim).map(str::to_string),
    })
}

pub async fn issues_list_data(
    client: &reqwest::Client,
    filter: &IssueListFilter,
    limit: u32,
    cursor: Option<String>,
) -> Result<mcptools_core::linear::Paginated<mcptools_core::linear::IssueMini>> {
    let filter_value =
        issue_filter_value(filter).map_err(|e| eyre!("Invalid Linear issue filter: {}", e))?;
    let data = execute(
        client,
        ISSUES_QUERY,
        serde_json::json!({"first": limit, "after": cursor, "filter": filter_value}),
    )
    .await?;
    mcptools_core::linear::transform_issues(data).map_err(|e| eyre!("{}", e))
}

pub async fn issue_create_data(
    client: &reqwest::Client,
    team: &str,
    title: &str,
    description: Option<&str>,
    state: Option<&str>,
    assignee: Option<&str>,
    project: Option<&str>,
) -> Result<IssueMini> {
    if team.trim().is_empty() {
        return Err(eyre!("Linear issue create --team must not be empty"));
    }
    if title.trim().is_empty() {
        return Err(eyre!("Linear issue create --title must not be empty"));
    }
    for (flag, value) in [
        ("--description", description),
        ("--state", state),
        ("--assignee", assignee),
        ("--project", project),
    ] {
        if value.is_some_and(|text| text.trim().is_empty()) {
            return Err(eyre!("Linear issue create {} must not be empty", flag));
        }
    }
    if let Some(selector) = assignee.map(str::trim).filter(|text| !text.is_empty()) {
        if !selector.eq_ignore_ascii_case("me") && !is_uuid(selector) {
            return Err(eyre!(
                "Linear issue create --assignee '{}' must be a user UUID or 'me'. Find the UUID with `mcptools linear users list --query NAME`",
                selector
            ));
        }
    }
    let team_id = super::discover::teams_get_data(client, team.trim())
        .await?
        .id;
    let project_id = match project.map(str::trim).filter(|text| !text.is_empty()) {
        None => None,
        Some(selector) if is_uuid(selector) => Some(selector.to_string()),
        Some(selector) => Some(
            super::discover::projects_get_data(client, selector, team.trim())
                .await?
                .id,
        ),
    };
    let state_id = resolve_state_id(client, state, Some(team.trim()), "create").await?;
    let assignee_id = resolve_assignee_id(client, assignee, "create").await?;
    let input = issue_create_input(
        &team_id,
        title.trim(),
        description.map(str::trim),
        state_id.as_deref(),
        assignee_id.as_deref(),
        project_id.as_deref(),
    );
    let data = execute(
        client,
        ISSUE_CREATE_MUTATION,
        serde_json::json!({"input": input}),
    )
    .await?;
    transform_issue_create(data).map_err(|e| match e {
        mcptools_core::linear::LinearError::MissingIssue => {
            eyre!("Linear issue create returned no issue")
        }
        other => eyre!("{}", other),
    })
}

#[allow(clippy::too_many_arguments)]
pub async fn issue_update_data(
    client: &reqwest::Client,
    id: &str,
    title: Option<&str>,
    description: Option<&str>,
    state: Option<&str>,
    team: Option<&str>,
    assignee: Option<&str>,
    parent: Option<Option<String>>,
) -> Result<IssueMini> {
    let selector = id.trim();
    if selector.is_empty() {
        return Err(eyre!("Linear issue id must not be empty"));
    }
    for (flag, value) in [
        ("--title", title),
        ("--description", description),
        ("--state", state),
        ("--team", team),
        ("--assignee", assignee),
    ] {
        if value.is_some_and(|text| text.trim().is_empty()) {
            return Err(eyre!("Linear issue update {} must not be empty", flag));
        }
    }
    if parent
        .as_ref()
        .is_some_and(|slot| slot.as_ref().is_some_and(|pid| pid.trim().is_empty()))
    {
        return Err(eyre!("Linear parent issue id must not be empty"));
    }
    let has_field = [title, description, state, assignee]
        .iter()
        .any(|value| value.map(str::trim).is_some_and(|text| !text.is_empty()))
        || parent.is_some();
    if !has_field {
        return Err(eyre!(
            "Linear issue update needs at least one of --title, --description, --state, --assignee, --parent, --clear-parent"
        ));
    }
    if let Some(value) = assignee.map(str::trim).filter(|text| !text.is_empty()) {
        if !value.eq_ignore_ascii_case("me") && !is_uuid(value) {
            return Err(eyre!(
                "Linear issue update --assignee '{}' must be a user UUID or 'me'. Find the UUID with `mcptools linear users list --query NAME`",
                value
            ));
        }
    }
    let team_for_state = team
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .or_else(|| team_key_from_identifier(selector));
    let state_id = resolve_state_id(client, state, team_for_state, "update").await?;
    let assignee_id = resolve_assignee_id(client, assignee, "update").await?;
    let input = issue_update_input(
        title.map(str::trim),
        description.map(str::trim),
        state_id.as_deref(),
        assignee_id.as_deref(),
        parent,
    );
    let data = execute(
        client,
        ISSUE_UPDATE_MUTATION,
        serde_json::json!({"id": selector, "input": input}),
    )
    .await?;
    transform_issue_update(data).map_err(|e| match e {
        mcptools_core::linear::LinearError::MissingIssue => {
            eyre!("Linear issue not found: {}", selector)
        }
        other => eyre!("{}", other),
    })
}

async fn resolve_assignee_id(
    client: &reqwest::Client,
    assignee: Option<&str>,
    op: &str,
) -> Result<Option<String>> {
    match assignee.map(str::trim).filter(|text| !text.is_empty()) {
        None => Ok(None),
        Some(selector) if selector.eq_ignore_ascii_case("me") => {
            Ok(Some(super::auth::auth_status_data(client).await?.id))
        }
        Some(selector) if is_uuid(selector) => Ok(Some(selector.to_string())),
        Some(selector) => Err(eyre!(
            "Linear issue {} --assignee '{}' must be a user UUID or 'me'. Find the UUID with `mcptools linear users list --query NAME`",
            op,
            selector
        )),
    }
}

async fn resolve_state_id(
    client: &reqwest::Client,
    state: Option<&str>,
    team: Option<&str>,
    op: &str,
) -> Result<Option<String>> {
    let selector = match state.map(str::trim).filter(|text| !text.is_empty()) {
        None => return Ok(None),
        Some(value) => value,
    };
    if is_uuid(selector) {
        return Ok(Some(selector.to_string()));
    }
    let team_selector = match team.map(str::trim).filter(|text| !text.is_empty()) {
        None => {
            return Err(eyre!(
                "Linear issue {} --state '{}' needs --team to resolve the state name. Pass a state UUID to skip team resolution",
                op,
                selector
            ));
        }
        Some(value) => value,
    };
    let parsed = parse_state_selector(selector)
        .ok_or_else(|| eyre!("Linear issue {} --state must not be empty", op))?;
    let listed = super::discover::states_list_data(client, team_selector).await?;
    match match_state(&parsed, &listed.nodes) {
        StateResolution::Resolved(item) => Ok(Some(item.id)),
        StateResolution::NotFound(input) => Err(eyre!("Linear state not found: {}", input)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::linear::config::LinearConfig;

    #[tokio::test]
    async fn rejects_empty_selector_before_io() {
        let cfg = LinearConfig {
            api_key: "test-key".to_string(),
        };
        let client = crate::linear::client::build_client(&cfg).unwrap();
        for selector in ["", "   "] {
            let err = issue_get_data(&client, selector).await.unwrap_err();
            assert!(err.to_string().contains("must not be empty"));
        }
    }

    #[tokio::test]
    async fn update_rejects_empty_ids_before_io() {
        let cfg = LinearConfig {
            api_key: "test-key".to_string(),
        };
        let client = crate::linear::client::build_client(&cfg).unwrap();
        for selector in ["", "   "] {
            let err =
                issue_update_data(&client, selector, None, None, None, None, None, Some(None))
                    .await
                    .unwrap_err();
            assert!(err.to_string().contains("must not be empty"));
        }
        let err = issue_update_data(
            &client,
            "GUZ-81",
            None,
            None,
            None,
            None,
            None,
            Some(Some("   ".to_string())),
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("must not be empty"));
    }

    #[tokio::test]
    async fn rejects_project_name_without_team_before_io() {
        let cfg = LinearConfig {
            api_key: "test-key".to_string(),
        };
        let client = crate::linear::client::build_client(&cfg).unwrap();
        let err = resolve_issue_filter(
            &client,
            None,
            Some("MCPTools"),
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("--team"));
    }

    #[tokio::test]
    async fn rejects_unknown_assignee_before_io() {
        let cfg = LinearConfig {
            api_key: "test-key".to_string(),
        };
        let client = crate::linear::client::build_client(&cfg).unwrap();
        let err = resolve_issue_filter(
            &client,
            None,
            None,
            Some("ada"),
            None,
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("--assignee"));
    }

    #[tokio::test]
    async fn rejects_empty_filter_text_before_io() {
        let cfg = LinearConfig {
            api_key: "test-key".to_string(),
        };
        let client = crate::linear::client::build_client(&cfg).unwrap();
        let err = resolve_issue_filter(
            &client,
            None,
            None,
            None,
            Some("   "),
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("--state"));
    }

    #[tokio::test]
    async fn rejects_empty_create_title_before_io() {
        let cfg = LinearConfig {
            api_key: "test-key".to_string(),
        };
        let client = crate::linear::client::build_client(&cfg).unwrap();
        let err = issue_create_data(&client, "GUZ", "   ", None, None, None, None)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("--title"));
    }

    #[tokio::test]
    async fn rejects_empty_update_id_before_io() {
        let cfg = LinearConfig {
            api_key: "test-key".to_string(),
        };
        let client = crate::linear::client::build_client(&cfg).unwrap();
        let err = issue_update_data(&client, "   ", Some("T"), None, None, None, None, None)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("must not be empty"));
    }

    #[tokio::test]
    async fn rejects_update_without_fields_before_io() {
        let cfg = LinearConfig {
            api_key: "test-key".to_string(),
        };
        let client = crate::linear::client::build_client(&cfg).unwrap();
        let err = issue_update_data(&client, "i1", None, None, None, None, None, None)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("at least one"));
    }

    #[tokio::test]
    async fn rejects_state_name_without_team_before_io() {
        let cfg = LinearConfig {
            api_key: "test-key".to_string(),
        };
        let client = crate::linear::client::build_client(&cfg).unwrap();
        let err = issue_update_data(&client, "i1", None, None, Some("Todo"), None, None, None)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("--team"));
    }
}
