use crate::prelude::*;
use futures::{stream, StreamExt, TryStreamExt};
use mcptools_core::atlassian::jira::status::{
    normalize_status, status_jql, StatusListOutput, StatusMap, STATUS_FIELDS,
};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Deserialize, JsonSchema, clap::Args)]
pub struct StatusArgs {
    #[arg(long, default_value = "me")]
    pub assignee: Option<String>,
    #[arg(long)]
    pub project: Option<String>,
    #[arg(long, default_value = "true", action = clap::ArgAction::Set)]
    pub open: Option<bool>,
    #[arg(long)]
    pub jql: Option<String>,
}

#[derive(Debug, clap::Subcommand)]
pub enum StatusCommands {
    List(StatusOptions),
}

#[derive(Debug, clap::Args)]
pub struct StatusOptions {
    #[command(flatten)]
    pub args: StatusArgs,
    #[arg(long)]
    pub json: bool,
}

pub async fn handler(command: StatusCommands, config: &super::super::JiraConfig) -> Result<()> {
    let StatusCommands::List(options) = command;
    let output = list(&options.args, config).await?;
    std::println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}

pub async fn list(
    args: &StatusArgs,
    config: &super::super::JiraConfig,
) -> Result<StatusListOutput> {
    let client = super::super::create_jira_client(config)?;
    let map = crate::config::atlassian()?.jira.status_map;
    list_with_client(args, &client, &config.base_url, &map).await
}

async fn list_with_client(
    args: &StatusArgs,
    client: &reqwest::Client,
    base: &str,
    map: &StatusMap,
) -> Result<StatusListOutput> {
    let jql = status_jql(
        args.assignee.as_deref().unwrap_or("me"),
        args.project.as_deref(),
        args.open.unwrap_or(true),
        args.jql.as_deref(),
    )
    .map_err(|e| eyre!(e))?;
    let mut cursor: Option<String> = None;
    let mut seen = BTreeSet::new();
    let mut keys = BTreeSet::new();
    let mut issues = Vec::new();
    let mut sprint_field = None;
    loop {
        let mut request = client
            .get(format!(
                "{}/rest/api/3/search/jql",
                base.trim_end_matches('/')
            ))
            .query(&[
                ("jql", jql.as_str()),
                ("maxResults", "100"),
                ("fields", STATUS_FIELDS),
                ("expand", "names"),
            ]);
        if let Some(token) = &cursor {
            request = request.query(&[("nextPageToken", token)]);
        }
        let page: Value = super::check_response(request.send().await?, "Jira search failed")
            .await?
            .json()
            .await?;
        if let Some(names) = page.get("names").and_then(Value::as_object) {
            sprint_field = names
                .iter()
                .find(|(_, name)| {
                    name.as_str()
                        .is_some_and(|n| n.eq_ignore_ascii_case("sprint"))
                })
                .map(|(id, _)| id.clone())
                .or(sprint_field);
        }
        let items = page
            .get("issues")
            .and_then(Value::as_array)
            .ok_or_else(|| eyre!("Jira search missing issues"))?;
        for item in items {
            let key = item
                .get("key")
                .and_then(Value::as_str)
                .ok_or_else(|| eyre!("Jira issue missing key"))?;
            if !keys.insert(key.to_string()) {
                return Err(eyre!("Jira search returned a repeated issue"));
            }
            issues.push(item.clone());
        }
        let next = page.get("nextPageToken").and_then(Value::as_str);
        if page.get("isLast").and_then(Value::as_bool) == Some(true)
            || next.is_none() && page.get("isLast").and_then(Value::as_bool) != Some(false)
        {
            break;
        }
        let token = next
            .filter(|v| !v.is_empty())
            .ok_or_else(|| eyre!("Jira search missing continuation"))?;
        if items.is_empty() || !seen.insert(token.to_string()) {
            return Err(eyre!("Jira search pagination did not advance"));
        }
        cursor = Some(token.to_string());
    }
    let results: Vec<(String, Value, Value)> =
        stream::iter(keys.into_iter().map(|key| async move {
            let mut url =
                reqwest::Url::parse(&format!("{}/rest/api/3/issue/", base.trim_end_matches('/')))?;
            url.path_segments_mut()
                .map_err(|_| eyre!("Invalid Jira base URL"))?
                .pop_if_empty()
                .push(&key)
                .push("transitions");
            let transitions: Value = super::check_response(
                client.get(url.clone()).send().await?,
                "Jira transitions failed",
            )
            .await?
            .json()
            .await?;
            url.path_segments_mut()
                .map_err(|_| eyre!("Invalid Jira base URL"))?
                .pop()
                .push("comment");
            let comments: Value = super::check_response(
                client
                    .get(url)
                    .query(&[("maxResults", "1"), ("orderBy", "-created")])
                    .send()
                    .await?,
                "Jira comments failed",
            )
            .await?
            .json()
            .await?;
            Ok::<_, color_eyre::Report>((key, transitions, comments))
        }))
        .buffer_unordered(4)
        .try_collect()
        .await?;
    let mut transitions = BTreeMap::new();
    let mut comments = BTreeMap::new();
    for (key, moves, comment) in results {
        transitions.insert(key.clone(), moves);
        comments.insert(key, comment);
    }
    normalize_status(
        base,
        issues,
        &transitions,
        &comments,
        map,
        sprint_field.as_deref(),
    )
    .map_err(|e| eyre!(e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::{
        matchers::{method, path, query_param},
        Mock, MockServer, ResponseTemplate,
    };

    #[tokio::test]
    async fn pages_and_fetches_details_without_live_jira() {
        let server = MockServer::start().await;
        let fixture: Value = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../core/tests/fixtures/jira_status_input.json"
        )))
        .unwrap();
        let issues = fixture["issues"].as_array().unwrap();
        Mock::given(method("GET")).and(path("/rest/api/3/search/jql")).and(query_param("jql", "assignee = currentUser() AND statusCategory != Done"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"issues": &issues[..2], "isLast":false, "nextPageToken":"next", "names":{"customfield_10020":"Sprint"}}))).up_to_n_times(1).expect(1).mount(&server).await;
        Mock::given(method("GET"))
            .and(path("/rest/api/3/search/jql"))
            .and(query_param("nextPageToken", "next"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({"issues": &issues[2..], "isLast":true})),
            )
            .expect(1)
            .mount(&server)
            .await;
        for n in 1..=4 {
            let key = format!("ENG-{n}");
            Mock::given(method("GET"))
                .and(path(format!("/rest/api/3/issue/{key}/transitions")))
                .respond_with(
                    ResponseTemplate::new(200).set_body_json(&fixture["transitions"][&key]),
                )
                .expect(1)
                .mount(&server)
                .await;
            Mock::given(method("GET"))
                .and(path(format!("/rest/api/3/issue/{key}/comment")))
                .and(query_param("orderBy", "-created"))
                .respond_with(ResponseTemplate::new(200).set_body_json(&fixture["comments"][&key]))
                .expect(1)
                .mount(&server)
                .await;
        }
        let args = StatusArgs {
            assignee: None,
            project: None,
            open: None,
            jql: None,
        };
        let map = serde_json::from_value(fixture["map"].clone()).unwrap();
        let result = list_with_client(&args, &reqwest::Client::new(), &server.uri(), &map)
            .await
            .unwrap();
        assert_eq!(result.nodes.len(), 4);
        assert_eq!(result.nodes[0].key, "ENG-1");
        assert!(result.nodes[0].transitions[0].has_screen);
        server.verify().await;
    }

    #[tokio::test]
    async fn rejects_repeated_pagination_tokens() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/rest/api/3/search/jql"))
            .respond_with(ResponseTemplate::new(200).set_body_json(
                serde_json::json!({"issues":[],"isLast":false,"nextPageToken":"stuck"}),
            ))
            .expect(1)
            .mount(&server)
            .await;
        let args = StatusArgs {
            assignee: None,
            project: None,
            open: None,
            jql: None,
        };
        assert!(list_with_client(
            &args,
            &reqwest::Client::new(),
            &server.uri(),
            &StatusMap::new()
        )
        .await
        .is_err());
        server.verify().await;
    }
    #[tokio::test]
    async fn rejects_repeated_nonempty_page_token() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/rest/api/3/search/jql"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"issues":[{"key":"ENG-1"}],"isLast":false,"nextPageToken":"same"})))
            .up_to_n_times(1).expect(1).mount(&server).await;
        Mock::given(method("GET"))
            .and(path("/rest/api/3/search/jql"))
            .and(query_param("nextPageToken", "same"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"issues":[{"key":"ENG-2"}],"isLast":false,"nextPageToken":"same"})))
            .expect(1).mount(&server).await;
        let args = StatusArgs {
            assignee: None,
            project: None,
            open: None,
            jql: None,
        };
        let error = list_with_client(
            &args,
            &reqwest::Client::new(),
            &server.uri(),
            &StatusMap::new(),
        )
        .await
        .unwrap_err();
        assert!(error.to_string().contains("pagination did not advance"));
        server.verify().await;
    }
}
