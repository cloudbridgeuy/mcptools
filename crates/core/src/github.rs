use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone)]
pub struct Pull {
    pub pr: Value,
    pub reviews: Vec<Value>,
    pub threads: Vec<Value>,
    pub checks: Vec<Value>,
    pub statuses: Vec<Value>,
    pub combined: Value,
    pub parent: Option<u64>,
}

/// Origin of GitHub's hosted MCP server; only its token may be sent to GitHub's REST API.
pub const GITHUB_UPSTREAM_ORIGIN: &str = "https://api.githubcopilot.com";

/// Whether an upstream `url` has the same scheme, host, and port as `origin`.
pub fn is_github_upstream(url: Option<&str>, origin: &str) -> bool {
    let (Some(url), Ok(origin)) = (
        url.and_then(|url| reqwest::Url::parse(url).ok()),
        reqwest::Url::parse(origin),
    ) else {
        return false;
    };
    url.scheme() == origin.scheme()
        && url.host_str().is_some()
        && url.host_str() == origin.host_str()
        && url.port_or_known_default() == origin.port_or_known_default()
        && url.username().is_empty()
        && url.password().is_none()
}

fn string<'a>(value: &'a Value, keys: &[&str]) -> &'a str {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(Value::as_str))
        .unwrap_or("")
}

fn id_key(value: &Value) -> (u64, String) {
    let id = &value["id"];
    (
        id.as_u64()
            .or_else(|| id.as_str().and_then(|id| id.parse().ok()))
            .unwrap_or(0),
        id.to_string(),
    )
}

fn latest_time<'a>(value: &'a Value, keys: &[&str]) -> &'a str {
    keys.iter()
        .filter_map(|key| value.get(*key).and_then(Value::as_str))
        .max()
        .unwrap_or("")
}

pub fn stage(
    draft: bool,
    failed: bool,
    changes: bool,
    approved: bool,
    ready: bool,
) -> &'static str {
    [
        (draft, "draft"),
        (failed, "ci_failing"),
        (changes, "changes_requested"),
        (approved && ready, "mergeable"),
        (approved, "approved"),
        (true, "awaiting_review"),
    ]
    .into_iter()
    .find_map(|(applies, stage)| applies.then_some(stage))
    .unwrap_or("awaiting_review")
}

fn ci(pull: &Pull) -> (Value, Vec<Value>) {
    let sha = string(&pull.pr["head"], &["sha"]);
    let mut latest: BTreeMap<String, (String, String, (u64, String), Value)> = BTreeMap::new();
    for (source, values) in [("check_run", &pull.checks), ("status", &pull.statuses)] {
        for value in values {
            if value
                .get("head_sha")
                .and_then(Value::as_str)
                .is_some_and(|head| head != sha)
            {
                continue;
            }
            let name = string(value, &["name", "context"]);
            if name.is_empty() {
                continue;
            }
            let time = latest_time(
                value,
                &["completed_at", "updated_at", "started_at", "created_at"],
            );
            let id = id_key(value);
            let key = (time.to_owned(), source.to_owned(), id, value.clone());
            if latest.get(name).is_none_or(|old| {
                (&key.0, &key.1, &key.2, key.3.to_string())
                    > (&old.0, &old.1, &old.2, old.3.to_string())
            }) {
                latest.insert(name.to_owned(), key);
            }
        }
    }
    let mut summary = [0u64; 4];
    let mut failing = Vec::new();
    for (name, (_, source, _, value)) in latest {
        let status = string(&value, &["conclusion", "state", "status"]).to_ascii_lowercase();
        let index = match status.as_str() {
            "success" | "neutral" => 0,
            "failure" | "error" | "timed_out" | "cancelled" | "action_required"
            | "startup_failure" | "stale" => 2,
            "skipped" => 3,
            _ => 1,
        };
        summary[index] += 1;
        if index == 2 {
            let url = ["details_url", "html_url", "target_url"]
                .into_iter()
                .filter_map(|key| value[key].as_str())
                .find(|url| !url.is_empty());
            failing.push(json!({"name":name,"source":source,"url":url}));
        }
    }
    (
        json!({"pass":summary[0],"pending":summary[1],"fail":summary[2],"skipped":summary[3]}),
        failing,
    )
}

pub fn merge(input: &[Pull], selected: &[u64]) -> Result<Vec<Value>, String> {
    let by_number: BTreeMap<u64, &Pull> = input
        .iter()
        .filter_map(|pull| pull.pr["number"].as_u64().map(|number| (number, pull)))
        .collect();
    let mut output = Vec::new();
    let pattern =
        regex::Regex::new(r"(?i)\b[A-Z][A-Z0-9]+-[0-9]+\b").expect("valid ticket key pattern");
    for number in selected.iter().copied().collect::<BTreeSet<_>>() {
        let pull = by_number
            .get(&number)
            .ok_or_else(|| format!("Missing pull request {number}"))?;
        let mut reviews: BTreeMap<String, (String, (u64, String), Value)> = BTreeMap::new();
        for review in &pull.reviews {
            let login = string(&review["user"], &["login"]);
            let timestamp = string(review, &["submitted_at", "submittedAt"]);
            if login.is_empty() || timestamp.is_empty() {
                continue;
            }
            let id = id_key(review);
            let record = json!({"login":login,"state":review["state"],"submittedAt":timestamp,"id":review["id"]});
            let candidate = (timestamp.to_owned(), id, record);
            if reviews.get(login).is_none_or(|old| {
                (&candidate.0, &candidate.1, candidate.2.to_string())
                    > (&old.0, &old.1, old.2.to_string())
            }) {
                reviews.insert(login.to_owned(), candidate);
            }
        }
        let review_values: Vec<Value> = reviews.into_values().map(|(_, _, value)| value).collect();
        let approved = review_values
            .iter()
            .any(|r| string(r, &["state"]).eq_ignore_ascii_case("approved"));
        let changes = review_values
            .iter()
            .any(|r| string(r, &["state"]).eq_ignore_ascii_case("changes_requested"));
        let mut threads = BTreeMap::new();
        for thread in &pull.threads {
            if let Some(id) = thread["id"].as_str() {
                threads.insert(id.to_owned(), thread);
            }
        }
        let unresolved: Vec<String> = threads
            .iter()
            .filter(|(_, v)| {
                !v.get("is_resolved")
                    .or_else(|| v.get("isResolved"))
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
            })
            .map(|(id, _)| id.clone())
            .collect();
        let (summary, failing) = ci(pull);
        let inherited: Vec<String> = pull
            .parent
            .and_then(|id| by_number.get(&id))
            .map(|parent| {
                let names: BTreeSet<String> = ci(parent)
                    .1
                    .iter()
                    .filter_map(|failure| failure["name"].as_str().map(str::to_owned))
                    .collect();
                failing
                    .iter()
                    .filter_map(|failure| failure["name"].as_str())
                    .filter(|name| names.contains(*name))
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default();
        let title = string(&pull.pr, &["title"]);
        let branch = string(&pull.pr["head"], &["ref"]);
        let ticket_keys: BTreeSet<String> = pattern
            .find_iter(&format!("{title} {branch}"))
            .map(|m| m.as_str().to_ascii_uppercase())
            .collect();
        let draft = pull
            .pr
            .get("draft")
            .or_else(|| pull.pr.get("isDraft"))
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let mergeable = pull.pr.get("mergeable").and_then(Value::as_bool);
        let ready = mergeable == Some(true)
            && unresolved.is_empty()
            && string(&pull.combined, &["state"]) == "success"
            && summary["pending"] == 0
            && summary["fail"] == 0
            && (summary["pass"].as_u64().unwrap_or(0) + summary["skipped"].as_u64().unwrap_or(0)
                > 0);
        let mut base = json!({"branch":pull.pr["base"]["ref"]});
        if let Some(parent) = pull.parent {
            base["stackParent"] = json!(parent);
        }
        output.push(json!({"number":number,"title":title,"url":pull.pr.get("html_url").or_else(|| pull.pr.get("url")),"state":pull.pr["state"],"isDraft":draft,"mergeable":mergeable,"head":{"branch":branch,"sha":pull.pr["head"]["sha"]},"base":base,"ticketKeys":ticket_keys,"reviews":{"latestByReviewer":review_values},"threads":{"total":threads.len(),"unresolved":unresolved.len(),"unresolvedIds":unresolved},"ci":{"summary":summary,"failing":failing,"inheritedFromStackParent":inherited},"stage":stage(draft,summary["fail"] != 0 || matches!(string(&pull.combined, &["state"]), "failure" | "error"),changes,approved,ready)}));
    }
    Ok(output)
}

pub fn validate_input<'a>(
    repo: &'a str,
    numbers: Option<&[u64]>,
    author: Option<&str>,
) -> Result<(&'a str, &'a str), String> {
    let (owner, name) = repo.split_once('/').ok_or("repo must be owner/name")?;
    if owner.is_empty()
        || name.is_empty()
        || name.contains('/')
        || !owner
            .bytes()
            .chain(name.bytes())
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
        || matches!(owner, "." | "..")
        || matches!(name, "." | "..")
    {
        return Err("repo must be owner/name".into());
    }
    match (numbers, author) {
        (Some(numbers), None) if !numbers.is_empty() && numbers.iter().all(|n| *n > 0) => {
            Ok((owner, name))
        }
        (None, Some(author))
            if !author.is_empty()
                && author
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-') =>
        {
            Ok((owner, name))
        }
        _ => Err("provide either nonempty positive numbers or an author".into()),
    }
}

pub fn stack_parent(
    candidates: &[Value],
    current: u64,
    branch: &str,
    owner: &str,
    repo: &str,
) -> Option<u64> {
    candidates
        .iter()
        .filter(|pr| {
            pr["head"]["ref"] == branch
                && pr["head"]["repo"]["full_name"]
                    .as_str()
                    .is_none_or(|full| full.eq_ignore_ascii_case(&format!("{owner}/{repo}")))
        })
        .filter_map(|pr| {
            pr["number"]
                .as_u64()
                .filter(|number| *number != current)
                .map(|number| (pr["state"].as_str() == Some("open"), number))
        })
        .max()
        .map(|(_, number)| number)
}

pub fn page_items<'a>(method: &str, page: &'a Value) -> Result<&'a Vec<Value>, String> {
    let items = match method {
        "get_reviews" => page.as_array(),
        "get_check_runs" => page["check_runs"].as_array(),
        "get_review_comments" => page["review_threads"].as_array(),
        "list_pull_requests" | "search_pull_requests" => {
            page.as_array().or_else(|| page["items"].as_array())
        }
        _ => None,
    };
    items.ok_or_else(|| format!("Invalid GitHub {method} response"))
}

pub fn page_complete(page: &Value, count: usize, received: usize) -> bool {
    page["total_count"]
        .as_u64()
        .map_or(count < 100, |total| received as u64 >= total)
}

pub fn pull_numbers(pulls: &[Value]) -> Result<Vec<u64>, String> {
    pulls
        .iter()
        .map(|pull| {
            pull["number"]
                .as_u64()
                .filter(|number| *number > 0)
                .ok_or_else(|| "Invalid GitHub pull request number".into())
        })
        .collect()
}

pub fn base_branch(pr: &Value) -> Result<&str, String> {
    pr["base"]["ref"]
        .as_str()
        .filter(|branch| !branch.is_empty())
        .ok_or_else(|| "Missing GitHub base branch".into())
}

pub fn thread_cursor(page: &Value) -> Result<Option<&str>, String> {
    match page["pageInfo"]["hasNextPage"].as_bool() {
        Some(false) => Ok(None),
        Some(true) => page["pageInfo"]["endCursor"]
            .as_str()
            .filter(|cursor| !cursor.is_empty())
            .map(Some)
            .ok_or_else(|| "Missing GitHub review thread cursor".into()),
        None => Err("Missing GitHub review thread page info".into()),
    }
}

pub fn head_sha(pr: &Value, number: u64) -> Result<&str, String> {
    if pr["number"].as_u64() != Some(number) {
        return Err(format!("Unexpected pull request number for {number}"));
    }
    let sha = pr["head"]["sha"]
        .as_str()
        .ok_or("Missing GitHub head SHA")?;
    if !matches!(sha.len(), 40 | 64) || !sha.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("Invalid GitHub head SHA".into());
    }
    Ok(sha)
}

pub fn status_detail(sha: &str, combined: &Value, status: &Value) -> Result<Vec<Value>, String> {
    if combined["sha"].as_str().is_some_and(|actual| actual != sha)
        || status["sha"].as_str().is_some_and(|actual| actual != sha)
    {
        return Err("GitHub status SHA mismatch".into());
    }
    let values = status["statuses"]
        .as_array()
        .ok_or("Invalid GitHub statuses")?;
    if status["total_count"]
        .as_u64()
        .is_none_or(|count| count > values.len() as u64)
    {
        return Err("GitHub statuses are incomplete or exceed 100 entries".into());
    }
    Ok(values.clone())
}

pub fn unwrap_response(raw: Value) -> Result<Value, String> {
    if raw["isError"] == true {
        let message = raw["content"]
            .as_array()
            .and_then(|content| content.iter().find_map(|part| part["text"].as_str()))
            .unwrap_or("GitHub upstream error");
        return Err(format!("GitHub upstream error: {message}"));
    }
    if let Some(value) = raw.get("structuredContent") {
        return Ok(value.clone());
    }
    let text = raw["content"]
        .as_array()
        .and_then(|content| content.iter().find_map(|part| part["text"].as_str()))
        .ok_or("Missing GitHub upstream content")?;
    serde_json::from_str(text).map_err(|_| "Invalid GitHub upstream JSON".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn github_upstream_requires_https_copilot_host() {
        let origin = GITHUB_UPSTREAM_ORIGIN;
        assert!(is_github_upstream(
            Some("https://api.githubcopilot.com/mcp/"),
            origin
        ));
        assert!(is_github_upstream(
            Some("https://API.githubcopilot.com:443/mcp/readonly"),
            origin
        ));
        for url in [
            None,
            Some("http://api.githubcopilot.com/mcp/"),
            Some("https://api.githubcopilot.com:8443/mcp/"),
            Some("https://api.githubcopilot.com.evil.test/mcp/"),
            Some("https://evil.test/api.githubcopilot.com/mcp/"),
            Some("https://user@api.githubcopilot.com/mcp/"),
            Some("https://copilot.api.githubcopilot.com/mcp/"),
            Some("not a url"),
        ] {
            assert!(!is_github_upstream(url, origin), "{url:?}");
        }
    }

    #[test]
    fn stage_precedence() {
        assert_eq!(stage(true, true, true, true, true), "draft");
        assert_eq!(stage(false, true, true, true, true), "ci_failing");
        assert_eq!(stage(false, false, true, true, true), "changes_requested");
        assert_eq!(stage(false, false, false, true, true), "mergeable");
        assert_eq!(stage(false, false, false, true, false), "approved");
        assert_eq!(stage(false, false, false, false, true), "awaiting_review");
    }

    #[test]
    fn golden_synthetic_six_pull_requests() {
        let mut input: Vec<Pull> = (1..=6).map(|number| {
            let mut statuses: Vec<Value> = if number == 1 { (0..30).map(|i| json!({"context":format!("check-{i:02}"),"state":"success","created_at":"2026-01-01"})).collect() } else { vec![] };
            if number == 1 { statuses.push(json!({"context":"Buildkite","state":"failure","target_url":"https://ci.invalid/build","created_at":"2026-01-02"})); }
            let mut checks = if number <= 3 { vec![json!({"id":1,"name":"migrations-run","status":"completed","conclusion":"failure","details_url":match number {1=>Value::Null,2=>json!(""),_=>json!(false)},"html_url":"https://ci.invalid/migrations","completed_at":"2026-01-02"})] } else {vec![]};
            if number == 4 { checks.push(json!({"name":"skip","conclusion":"skipped"})); }
            if number == 5 { checks.push(json!({"name":"pending","status":"in_progress"})); }
            Pull { pr: json!({"number":number,"title":format!("FAKE-{number} synthetic"),"html_url":format!("https://example.invalid/pr/{number}"),"state":"open","draft":number==4,"mergeable":if number==6 {None} else {Some(true)},"head":{"ref":format!("fake-{number}"),"sha":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"},"base":{"ref":match number {2=>"fake-1",3=>"fake-2",_=>"main"}}}), reviews: if number >= 5 { vec![json!({"id":1,"state":"COMMENTED","submitted_at":"2026-01-01","user":{"login":"fake-reviewer"}}), json!({"id":2,"state":if number==5 {"CHANGES_REQUESTED"} else {"APPROVED"},"submitted_at":"2026-01-02","user":{"login":"fake-reviewer"}})] } else { vec![] }, threads: if number == 6 { vec![json!({"id":"thread-fake","is_resolved":false})] } else { vec![] }, checks, statuses, combined: json!({"state":"success"}), parent: match number {2=>Some(1),3=>Some(2),_=>None} }
        }).collect();
        let expected: Vec<Value> =
            serde_json::from_str(include_str!("github-golden.json")).unwrap();
        assert_eq!(merge(&input, &[1, 2, 3, 4, 5, 6]).unwrap(), expected);
        input.reverse();
        for pull in &mut input {
            pull.reviews.reverse();
            pull.checks.reverse();
            pull.statuses.reverse();
        }
        assert_eq!(merge(&input, &[6, 5, 4, 3, 2, 1]).unwrap(), expected);
    }

    #[test]
    fn stale_checks_and_combined_failure() {
        let pull = Pull {
            pr: json!({"number":1,"title":"FAKE-1","head":{"ref":"branch","sha":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"},"base":{"ref":"main"},"mergeable":true}),
            reviews: vec![
                json!({"id":1,"state":"APPROVED","submitted_at":"2026-01-01","user":{"login":"fake"}}),
            ],
            threads: vec![],
            checks: vec![
                json!({"name":"stale","head_sha":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","conclusion":"failure"}),
                json!({"name":"fresh","head_sha":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","conclusion":"success"}),
            ],
            statuses: vec![],
            combined: json!({"state":"failure"}),
            parent: None,
        };
        let output = merge(&[pull], &[1]).unwrap();
        assert_eq!(
            output[0]["ci"]["summary"],
            json!({"pass":1,"pending":0,"fail":0,"skipped":0})
        );
        assert_eq!(output[0]["stage"], "ci_failing");
        assert!(merge(&[], &[1]).is_err());
    }

    #[test]
    fn parent_prefers_open_and_latest_number() {
        let prs = vec![
            json!({"number":1,"state":"open","head":{"ref":"branch"}}),
            json!({"number":9,"state":"closed","head":{"ref":"branch"}}),
            json!({"number":2,"state":"open","head":{"ref":"branch"}}),
        ];
        assert_eq!(stack_parent(&prs, 3, "branch", "fake", "repo"), Some(2));
    }

    #[test]
    fn status_detail_rejects_incomplete_or_stale_data() {
        assert!(status_detail(
            "abc",
            &json!({"sha":"abc"}),
            &json!({"sha":"abc","total_count":101,"statuses":[]})
        )
        .is_err());
        assert!(status_detail(
            "abc",
            &json!({"sha":"def"}),
            &json!({"sha":"abc","total_count":0,"statuses":[]})
        )
        .is_err());
    }

    #[test]
    fn latest_numeric_id_and_rerun_timestamp_win() {
        let pull = Pull {
            pr: json!({"number":1,"head":{"ref":"branch","sha":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"},"base":{"ref":"main"}}),
            reviews: vec![
                json!({"id":9,"state":"COMMENTED","submitted_at":"2026-01-01","user":{"login":"fake"}}),
                json!({"id":10,"state":"APPROVED","submitted_at":"2026-01-01","user":{"login":"fake"}}),
            ],
            checks: vec![
                json!({"id":9,"name":"build","conclusion":"failure","completed_at":"2026-01-02"}),
                json!({"id":10,"name":"build","status":"in_progress","started_at":"2026-01-03"}),
                json!({"id":9,"name":"tie","conclusion":"failure","completed_at":"2026-01-01"}),
                json!({"id":10,"name":"tie","conclusion":"success","completed_at":"2026-01-01"}),
            ],
            statuses: vec![],
            threads: vec![],
            combined: json!({"state":"success"}),
            parent: None,
        };
        let output = merge(&[pull], &[1]).unwrap();
        assert_eq!(output[0]["reviews"]["latestByReviewer"][0]["id"], 10);
        assert_eq!(
            output[0]["ci"]["summary"],
            json!({"pass":1,"pending":1,"fail":0,"skipped":0})
        );
    }

    #[test]
    fn author_result_rejects_invalid_numbers() {
        assert_eq!(pull_numbers(&[json!({"number":1})]).unwrap(), vec![1]);
        assert!(pull_numbers(&[json!({"number":0})]).is_err());
        assert!(pull_numbers(&[json!({})]).is_err());
    }

    #[test]
    fn upstream_error_preserves_missing_scope() {
        let error = unwrap_response(json!({"isError":true,"content":[{"type":"text","text":"missing pull requests read scope"}]})).unwrap_err();
        assert!(error.contains("pull requests read scope"));
    }
}
