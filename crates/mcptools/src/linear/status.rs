use crate::prelude::*;
use mcptools_core::linear::{
    issue_list_continuation, status_page, status_request, StatusListOutput,
};

pub async fn status_list(
    client: &reqwest::Client,
    assignee: &str,
    team: Option<&str>,
    project: Option<&str>,
    open: bool,
) -> Result<StatusListOutput> {
    status_list_with_url(
        client,
        super::client::LINEAR_API_URL,
        assignee,
        team,
        project,
        open,
    )
    .await
}

async fn status_list_with_url(
    client: &reqwest::Client,
    url: &str,
    assignee: &str,
    team: Option<&str>,
    project: Option<&str>,
    open: bool,
) -> Result<StatusListOutput> {
    let mut cursor = None;
    let mut seen = Vec::new();
    let mut nodes = Vec::new();
    loop {
        let (query, variables) = status_request(assignee, team, project, open, cursor.as_deref())
            .map_err(|e| eyre!(e))?;
        let data = super::client::execute_with_url(client, url, query, variables).await?;
        let (page_nodes, page) = status_page(data).map_err(|e| eyre!(e))?;
        nodes.extend(page_nodes);
        match issue_list_continuation(&page, &seen).map_err(|e| eyre!(e))? {
            Some(next) => {
                seen.push(next.clone());
                cursor = Some(next);
            }
            None => break,
        }
    }
    nodes.sort_by(|a, b| a.identifier.cmp(&b.identifier));
    Ok(StatusListOutput { nodes })
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::{matchers::body_partial_json, Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn follows_pages_once_and_rejects_repeated_cursor() {
        let server = MockServer::start().await;
        let fixture: serde_json::Value = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../core/tests/fixtures/linear_status_response.json"
        )))
        .unwrap();
        let issues = fixture["issues"]["nodes"].as_array().unwrap();
        let page = |has_next, cursor, nodes: serde_json::Value| serde_json::json!({"data":{"issues":{"nodes":nodes,"pageInfo":{"hasNextPage":has_next,"endCursor":cursor}}}});
        Mock::given(body_partial_json(serde_json::json!({"query": mcptools_core::linear::STATUS_QUERY,"variables":{"first":5,"after":null,"filter":{"assignee":{"isMe":{"eq":true}}}}})))
            .respond_with(ResponseTemplate::new(200).set_body_json(page(true, "next", serde_json::json!(&issues[..2]))))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(body_partial_json(
            serde_json::json!({"variables":{"after":"next"}}),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(page(
            false,
            "end",
            serde_json::json!(&issues[2..]),
        )))
        .expect(1)
        .mount(&server)
        .await;
        let output = status_list_with_url(
            &reqwest::Client::new(),
            &server.uri(),
            "me",
            None,
            None,
            true,
        )
        .await
        .unwrap();
        assert_eq!(
            output
                .nodes
                .iter()
                .map(|issue| issue.identifier.as_str())
                .collect::<Vec<_>>(),
            ["GUZ-1", "GUZ-2", "GUZ-3"]
        );
        assert_eq!(server.received_requests().await.unwrap().len(), 2);
        server.verify().await;
        let server = MockServer::start().await;
        Mock::given(wiremock::matchers::method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(page(
                true,
                "same",
                serde_json::json!([]),
            )))
            .expect(2)
            .mount(&server)
            .await;
        assert!(status_list_with_url(
            &reqwest::Client::new(),
            &server.uri(),
            "me",
            None,
            None,
            false
        )
        .await
        .is_err());
    }
}
