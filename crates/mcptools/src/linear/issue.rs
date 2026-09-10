use crate::linear::client::execute;
use crate::prelude::*;
use mcptools_core::linear::{is_uuid, issue_filter_value, IssueListFilter};

pub const ISSUE_QUERY: &str =
    "query ($id: String!) { issue(id: $id) { id identifier title url state { name } } }";

pub const ISSUES_QUERY: &str = "query ($first: Int!, $after: String, $filter: IssueFilter) { issues(first: $first, after: $after, filter: $filter, orderBy: updatedAt) { nodes { id identifier title url state { name } } pageInfo { hasNextPage endCursor } } }";

pub async fn issue_get_data(
    client: &reqwest::Client,
    id_or_identifier: &str,
) -> Result<mcptools_core::linear::IssueMini> {
    let selector = id_or_identifier.trim();
    if selector.is_empty() {
        return Err(eyre!("Linear issue id must not be empty"));
    }
    let data = execute(client, ISSUE_QUERY, serde_json::json!({"id": selector})).await?;
    mcptools_core::linear::transform_issue(data).map_err(|e| match e {
        mcptools_core::linear::LinearError::MissingIssue => {
            eyre!("Linear issue not found: {}", selector)
        }
        other => eyre!("{}", other),
    })
}

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
}
