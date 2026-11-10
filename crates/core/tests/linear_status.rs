use mcptools_core::linear::{status_page, status_records, status_request};
use serde_json::json;

#[test]
fn status_records_match_golden_and_are_deterministic() {
    let input: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/linear_status_response.json")).unwrap();
    let (records, _) = status_page(input.clone()).unwrap();
    let expected = include_str!("fixtures/linear_status_expected.json").trim_end_matches('\n');
    assert_eq!(serde_json::to_string(&records).unwrap(), expected);
    let mut nodes = input["issues"]["nodes"].as_array().unwrap().clone();
    nodes.reverse();
    assert_eq!(
        serde_json::to_string(&status_records(json!(nodes)).unwrap()).unwrap(),
        expected
    );
    assert_eq!(
        records[0]
            .last_comment
            .as_ref()
            .unwrap()
            .excerpt
            .chars()
            .count(),
        200
    );
}

#[test]
fn incomplete_connections_and_bad_filters_fail() {
    let input: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/linear_status_response.json")).unwrap();
    for connection in [
        "children",
        "relations",
        "inverseRelations",
        "attachments",
        "comments",
    ] {
        let mut incomplete = input.clone();
        incomplete["issues"]["nodes"][0][connection]["pageInfo"]["hasNextPage"] = json!(true);
        assert!(status_page(incomplete).is_err(), "{connection}");
    }
    assert!(status_request("", None, None, true, None).is_err());
    let (query, variables) = status_request("me", Some("GUZ"), None, true, Some("next")).unwrap();
    assert_eq!(variables["first"], 5);
    assert_eq!(query.matches("first: 100").count(), 5);
    for connection in [
        "children",
        "relations",
        "inverseRelations",
        "attachments",
        "comments",
    ] {
        assert!(query.contains(&format!("{connection}(first: 100)")));
    }
    assert_eq!(variables["filter"]["assignee"]["isMe"]["eq"], true);
    assert_eq!(variables["after"], "next");
    assert_eq!(
        variables["filter"]["state"]["type"]["nin"],
        json!(["completed", "canceled"])
    );
    let id = "12345678-1234-4234-8234-123456789abc";
    let (_, variables) = status_request(id, Some(id), Some(id), false, None).unwrap();
    assert_eq!(variables["filter"]["assignee"]["id"]["eq"], id);
    assert_eq!(variables["filter"]["team"]["id"]["eq"], id);
    assert_eq!(variables["filter"]["project"]["id"]["eq"], id);
    assert!(variables["filter"].get("state").is_none());
}

#[test]
fn workflow_type_alone_controls_normalized_status() {
    let input: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/linear_status_response.json")).unwrap();
    for (kind, expected) in [
        ("backlog", "open"),
        ("unstarted", "open"),
        ("started", "in_progress"),
        ("completed", "done"),
        ("canceled", "canceled"),
        ("triage", "unknown"),
    ] {
        let mut issue = input["issues"]["nodes"][1].clone();
        issue["state"] = json!({"name":"Custom name", "type":kind});
        let records = status_records(json!([issue])).unwrap();
        assert!(!records[0].blocked_by.is_empty());
        assert_eq!(records[0].normalized_status, expected, "{kind}");
    }
}

#[test]
fn nested_metadata_is_complete_sorted_and_deduplicated() {
    let input: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/linear_status_response.json")).unwrap();
    let mut issue = input["issues"]["nodes"][0].clone();
    let related =
        |identifier| json!({"identifier":identifier,"state":{"name":"Done","type":"completed"}});
    let earlier = related("GUZ-4");
    let later = related("GUZ-9");
    issue["project"] = json!({"id":"project-id","name":"Project"});
    issue["assignee"] = json!({"id":"user-id","name":"User"});
    issue["parent"] = related("GUZ-1");
    issue["children"]["nodes"] = json!([later, earlier, later]);
    issue["relations"]["nodes"] = json!([
        {"type":"blocks","relatedIssue":later},
        {"type":"related","relatedIssue":related("GUZ-5")},
        {"type":"blocks","relatedIssue":earlier},
        {"type":"blocks","relatedIssue":later}
    ]);
    issue["inverseRelations"]["nodes"] = json!([
        {"type":"blocks","issue":later},
        {"type":"blocks","issue":earlier},
        {"type":"blocks","issue":later}
    ]);
    issue["attachments"]["nodes"] = json!([
        {"title":"GUZ-9 GUZ-4 GUZ-9", "url":"https://github.com/org/repo/pull/42?view=GUZ-8"},
        {"title":"", "url":"https://github.com/org/repo/pull/42#discussion"},
        {"title":"", "url":"https://example.com/org/repo/pull/43"},
        {"title":"", "url":"https://github.com.evil.test/org/repo/pull/44"},
        {"title":"", "url":"https://github.com/org/repo/pull/not-a-number"},
        {"title":"", "url":"https://github.com/org/repo/pull/0"},
        {"title":"", "url":"https://user@github.com/org/repo/pull/45"},
        {"title":"", "url":"http://github.com/org/repo/pull/46"},
        {"title":"", "url":"not a URL"}
    ]);
    issue["comments"]["nodes"] = json!([
        {"body":"older", "createdAt":"2026-01-01T00:00:00Z", "user":{"name":"Old"}},
        {"body":"é".repeat(201), "createdAt":"2026-01-03T00:00:00Z", "user":{"name":"Latest"}},
        {"body":"middle", "createdAt":"2026-01-02T00:00:00Z", "user":null}
    ]);
    let records = status_records(json!([issue.clone()])).unwrap();
    let record = serde_json::to_value(&records[0]).unwrap();
    assert_eq!(record["project"], issue["project"]);
    assert_eq!(record["assignee"], issue["assignee"]);
    assert_eq!(record["parent"], issue["parent"]);
    for field in ["children", "blockedBy", "blocks"] {
        assert_eq!(record[field], json!([earlier, later]), "{field}");
    }
    assert_eq!(
        record["ticketKeys"],
        json!(["GUZ-2", "GUZ-4", "GUZ-8", "GUZ-9"])
    );
    assert_eq!(
        record["prRefs"],
        json!([{"owner":"org","repo":"repo","number":42,"url":"https://github.com/org/repo/pull/42"}])
    );
    assert_eq!(
        record["lastComment"],
        json!({"author":"Latest","at":"2026-01-03T00:00:00Z","excerpt":"é".repeat(200)})
    );
    for connection in [
        "children",
        "relations",
        "inverseRelations",
        "attachments",
        "comments",
    ] {
        issue[connection]["nodes"].as_array_mut().unwrap().reverse();
    }
    assert_eq!(
        serde_json::to_value(status_records(json!([issue])).unwrap()).unwrap(),
        serde_json::to_value(records).unwrap()
    );
}
