mod common;

use serde_json::json;
use wiremock::matchers::{body_json, header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn comments_map_payload_response_and_dispatch_in_all_modes() {
    let stub = MockServer::start().await;
    let uri = format!("{}/2.0/", stub.uri());
    let mut server = common::spawn_server(&[
        ("BITBUCKET_BASE_URL", &uri),
        ("BITBUCKET_USERNAME", "test-user"),
        ("BITBUCKET_APP_PASSWORD", "test-password"),
        ("MCPTOOLS_CODE_MODE", "true"),
    ])
    .unwrap();
    let discovery = server
        .tools_call("find_tools", json!({"domain": "bitbucket"}))
        .await;
    let found = discovery["structuredContent"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|tool| tool["name"] == "bitbucket_pr_comment_add")
        .unwrap();
    let declaration = found["declaration"].as_str().unwrap();
    for field in [
        "prNumber: number",
        "comment: string",
        "inline",
        "from",
        "to",
        "updated_on",
    ] {
        assert!(declaration.contains(field), "{declaration}");
    }

    let input =
        json!({"repo": "workspace/repo", "prNumber": 123, "comment": "**Review**\nUnicode: é 🦀"});
    let denied = server
        .tools_call(
            "call_tool",
            json!({"name": "bitbucket_pr_comment_add", "input": input}),
        )
        .await;
    assert_eq!(denied["isError"], true);
    assert!(denied["content"][0]["text"]
        .as_str()
        .unwrap()
        .contains("allowWrites"));
    let denied_spend = server
        .tools_call(
            "call_tool",
            json!({"name": "bitbucket_pr_comment_add", "input": input, "allowSpend": true}),
        )
        .await;
    assert_eq!(denied_spend["isError"], true);
    let code = format!("return await bitbucket_pr_comment_add({input});");
    for permission in [json!({}), json!({"allowSpend": true})] {
        let mut args = permission;
        args["code"] = json!(code);
        let denied = server.tools_call("execute", args).await;
        assert_eq!(denied["isError"], true);
        assert!(denied["structuredContent"]["error"]["message"]
            .as_str()
            .unwrap()
            .contains("allowWrites"));
    }
    assert!(stub.received_requests().await.unwrap().is_empty());

    for (index, inline) in [
        None,
        Some(json!({"path": "old name.rs", "from": 7})),
        Some(json!({"path": "src/main.rs", "to": 9})),
        Some(json!({"path": "src/main.rs", "from": 7, "to": 9})),
    ]
    .into_iter()
    .enumerate()
    {
        let mut args = input.clone();
        let mut payload = json!({"content": {"raw": input["comment"]}});
        if let Some(inline) = &inline {
            args["inline"] = inline.clone();
            payload["inline"] = inline.clone();
        }
        let response = json!({
            "id": 42, "user": {"display_name": "Test User"},
            "content": {"raw": input["comment"], "markup": "markdown"},
            "created_on": "2026-10-06T10:00:00+00:00",
            "updated_on": "2026-10-06T10:01:00+00:00", "inline": inline
        });
        Mock::given(method("POST"))
            .and(path(
                "/2.0/repositories/workspace/repo/pullrequests/123/comments",
            ))
            .and(header(
                "Authorization",
                "Basic dGVzdC11c2VyOnRlc3QtcGFzc3dvcmQ=",
            ))
            .and(header("Content-Type", "application/json"))
            .and(body_json(payload))
            .respond_with(ResponseTemplate::new(201).set_body_json(response))
            .expect(1)
            .mount(&stub)
            .await;
        let result = match index {
            0 => {
                args["allowWrites"] = json!(true);
                server.tools_call("bitbucket_pr_comment_add", args).await
            }
            1 => server
                .tools_call(
                    "call_tool",
                    json!({"name": "bitbucket_pr_comment_add", "input": args, "allowWrites": true}),
                )
                .await,
            _ => {
                let output = server.tools_call("execute", json!({"code": format!("return await bitbucket_pr_comment_add({args});"), "allowWrites": true})).await;
                assert!(output["structuredContent"]["error"].is_null(), "{output}");
                json!({"structuredContent": output["structuredContent"]["result"]})
            }
        };
        let output = &result["structuredContent"];
        assert_eq!(output["id"], 42, "{result}");
        assert_eq!(output["content"], input["comment"]);
        assert_eq!(output["author"], "Test User");
        assert_eq!(output["updated_on"], "2026-10-06T10:01:00+00:00");
        assert_eq!(
            output["inline"]["path"],
            inline.as_ref().map_or(json!(null), |v| v["path"].clone())
        );
        assert_eq!(
            output["inline"]["from"],
            inline.as_ref().map_or(json!(null), |v| v["from"].clone())
        );
        assert_eq!(
            output["inline"]["to"],
            inline.as_ref().map_or(json!(null), |v| v["to"].clone())
        );
        if let Some(text) = result["content"][0]["text"].as_str() {
            assert_eq!(
                serde_json::from_str::<serde_json::Value>(text).unwrap(),
                *output
            );
        }
        stub.verify().await;
        stub.reset().await;
    }
}

#[tokio::test]
async fn invalid_comments_fail_before_http() {
    let stub = MockServer::start().await;
    let uri = stub.uri();
    let mut server = common::spawn_server(&[
        ("BITBUCKET_BASE_URL", &uri),
        ("BITBUCKET_USERNAME", "test-user"),
        ("BITBUCKET_APP_PASSWORD", "test-password"),
        ("MCPTOOLS_CODE_MODE", "false"),
    ])
    .unwrap();
    for patch in [
        json!({"repo": ""}),
        json!({"repo": "workspace"}),
        json!({"repo": "/repo"}),
        json!({"repo": "workspace/repo/extra"}),
        json!({"repo": "workspace/.."}),
        json!({"repo": "workspace/repo?foo=bar"}),
        json!({"repo": "workspace/repo#fragment"}),
        json!({"repo": "workspace/%2e%2e"}),
        json!({"repo": "workspace/ repo"}),
        json!({"prNumber": 0}),
        json!({"prNumber": -1}),
        json!({"prNumber": 1.5}),
        json!({"comment": " \n\t"}),
        json!({"comment": null}),
        json!({"extra": true}),
        json!({"inline": {"path": "src/main.rs"}}),
        json!({"inline": {"path": " ", "to": 1}}),
        json!({"inline": {"path": "src/\u{0000}main.rs", "to": 1}}),
        json!({"inline": {"path": "src/main.rs", "from": 0}}),
        json!({"inline": {"path": "src/main.rs", "to": -1}}),
        json!({"inline": {"path": "src/main.rs", "to": 1.5}}),
        json!({"inline": {"path": "src/main.rs", "to": 1, "start_to": 1}}),
    ] {
        let mut input = json!({"repo": "workspace/repo", "prNumber": 123, "comment": "Review", "allowWrites": true});
        input
            .as_object_mut()
            .unwrap()
            .extend(patch.as_object().unwrap().clone());
        let response = server.request(json!({"jsonrpc": "2.0", "id": 99, "method": "tools/call", "params": {"name": "bitbucket_pr_comment_add", "arguments": input}})).await;
        assert_eq!(response["error"]["code"], -32602, "{response}");
    }
    assert!(stub.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn comment_http_and_response_errors_are_reported_without_retry() {
    let stub = MockServer::start().await;
    let uri = stub.uri();
    let mut server = common::spawn_server(&[
        ("BITBUCKET_BASE_URL", &uri),
        ("BITBUCKET_USERNAME", "test-user"),
        ("BITBUCKET_APP_PASSWORD", "test-password"),
        ("MCPTOOLS_CODE_MODE", "false"),
    ])
    .unwrap();
    for (status, body, expected) in [
        (400, "invalid anchor", "400"),
        (401, "unauthorized", "401"),
        (403, "forbidden", "403"),
        (404, "not found", "404"),
        (429, "rate limited", "429"),
        (500, "server error", "500"),
        (201, "not JSON", "Failed to parse"),
        (201, "{}", "Failed to parse"),
    ] {
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(status).set_body_string(body))
            .expect(1)
            .mount(&stub)
            .await;
        let response = server.request(json!({"jsonrpc": "2.0", "id": 99, "method": "tools/call", "params": {"name": "bitbucket_pr_comment_add", "arguments": {"repo": "workspace/repo", "prNumber": 123, "comment": "Review", "allowWrites": true}}})).await;
        assert_eq!(response["error"]["code"], -32603, "{response}");
        assert!(
            response["error"]["message"]
                .as_str()
                .unwrap()
                .contains(expected),
            "{response}"
        );
        if status != 201 {
            assert!(response["error"]["message"]
                .as_str()
                .unwrap()
                .contains(body));
        }
        stub.verify().await;
        stub.reset().await;
    }
}

#[tokio::test]
async fn comment_and_branch_management_map_confirmed_rest_endpoints() {
    let stub = MockServer::start().await;
    let uri = format!("{}/2.0", stub.uri());
    let mut server = common::spawn_server(&[
        ("BITBUCKET_BASE_URL", &uri),
        ("BITBUCKET_USERNAME", "test-user"),
        ("BITBUCKET_APP_PASSWORD", "test-password"),
        ("MCPTOOLS_CODE_MODE", "false"),
    ])
    .unwrap();
    let comment = json!({
        "id": 42,
        "user": {"display_name": "Test User"},
        "content": {"raw": "Review"},
        "created_on": "2026-10-13T10:00:00Z",
        "updated_on": "2026-10-13T10:00:00Z",
        "deleted": false
    });

    Mock::given(method("GET"))
        .and(path(
            "/2.0/repositories/workspace/repo/pullrequests/7/comments",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "values": [comment.clone()],
            "size": 1
        })))
        .expect(1)
        .mount(&stub)
        .await;
    let listed = server
        .tools_call(
            "bitbucket_pr_comment_list",
            json!({"repo":"workspace/repo","prNumber":7}),
        )
        .await;
    assert_eq!(listed["structuredContent"]["comments"][0]["id"], 42);
    stub.verify().await;
    stub.reset().await;

    Mock::given(method("PUT"))
        .and(path(
            "/2.0/repositories/workspace/repo/pullrequests/7/comments/42",
        ))
        .and(body_json(json!({"content":{"raw":"Updated"}})))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": 42,
            "user": {"display_name": "Test User"},
            "content": {"raw": "Updated"},
            "created_on": "2026-10-13T10:00:00Z",
            "updated_on": "2026-10-13T11:00:00Z",
            "deleted": false
        })))
        .expect(1)
        .mount(&stub)
        .await;
    let updated = server
        .tools_call(
            "bitbucket_pr_comment_update",
            json!({"repo":"workspace/repo","prNumber":7,"commentId":42,"comment":"Updated","allowWrites":true}),
        )
        .await;
    assert_eq!(updated["structuredContent"]["content"], "Updated");
    stub.verify().await;
    stub.reset().await;

    Mock::given(method("DELETE"))
        .and(path(
            "/2.0/repositories/workspace/repo/pullrequests/7/comments/42",
        ))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&stub)
        .await;
    let deleted = server
        .tools_call(
            "bitbucket_pr_comment_delete",
            json!({"repo":"workspace/repo","prNumber":7,"commentId":42,"allowWrites":true}),
        )
        .await;
    assert_eq!(deleted["structuredContent"]["deleted"], true);
    stub.verify().await;
    stub.reset().await;

    Mock::given(method("POST"))
        .and(path(
            "/2.0/repositories/workspace/repo/pullrequests/7/comments/42/resolve",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "user":{"display_name":"Resolver"},
            "created_on":"2026-10-13T12:00:00Z"
        })))
        .expect(1)
        .mount(&stub)
        .await;
    let resolved = server
        .tools_call(
            "bitbucket_pr_comment_resolve",
            json!({"repo":"workspace/repo","prNumber":7,"commentId":42,"allowWrites":true}),
        )
        .await;
    assert_eq!(resolved["structuredContent"]["resolved"], true);
    stub.verify().await;
    stub.reset().await;

    Mock::given(method("DELETE"))
        .and(path(
            "/2.0/repositories/workspace/repo/pullrequests/7/comments/42/resolve",
        ))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&stub)
        .await;
    let reopened = server
        .tools_call(
            "bitbucket_pr_comment_reopen",
            json!({"repo":"workspace/repo","prNumber":7,"commentId":42,"allowWrites":true}),
        )
        .await;
    assert_eq!(reopened["structuredContent"]["resolved"], false);
    stub.verify().await;
    stub.reset().await;

    Mock::given(method("POST"))
        .and(path("/2.0/repositories/workspace/repo/refs/branches"))
        .and(body_json(
            json!({"name":"feature-one","target":{"hash":"main"}}),
        ))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({
            "name":"feature-one",
            "target":{"hash":"abc123"}
        })))
        .expect(1)
        .mount(&stub)
        .await;
    let created = server
        .tools_call(
            "bitbucket_repo_branch_create",
            json!({"repo":"workspace/repo","name":"feature-one","target":"main","allowWrites":true}),
        )
        .await;
    assert_eq!(created["structuredContent"]["commit_hash"], "abc123");
    stub.verify().await;
    stub.reset().await;

    Mock::given(method("DELETE"))
        .and(path(
            "/2.0/repositories/workspace/repo/refs/branches/feature-one",
        ))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&stub)
        .await;
    let deleted = server
        .tools_call(
            "bitbucket_repo_branch_delete",
            json!({"repo":"workspace/repo","name":"feature-one","allowWrites":true}),
        )
        .await;
    assert_eq!(deleted["structuredContent"]["deleted"], true);
    stub.verify().await;
}
