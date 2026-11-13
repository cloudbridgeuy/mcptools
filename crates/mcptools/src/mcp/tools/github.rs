use super::JsonRpcError;
use mcptools_core::github::{self, Pull};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

#[derive(Deserialize, Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Args {
    repo: String,
    numbers: Option<Vec<u64>>,
    author: Option<String>,
}

pub fn output_schema() -> Value {
    let mut schema = json!({"type":"object","required":["pullRequests"],"properties":{"pullRequests":{"type":"array","items":{"type":"object","additionalProperties":false,"required":["number","title","url","state","isDraft","mergeable","head","base","ticketKeys","reviews","threads","ci","stage"],"properties":{
        "number":{"type":"integer"},"title":{"type":"string"},"url":{"type":["string","null"]},"state":{"type":"string"},"isDraft":{"type":"boolean"},"mergeable":{"type":["boolean","null"]},
        "head":{"type":"object","required":["branch","sha"],"properties":{"branch":{"type":"string"},"sha":{"type":"string"}}},
        "base":{"type":"object","required":["branch"],"properties":{"branch":{"type":"string"},"stackParent":{"type":"integer"}}},
        "ticketKeys":{"type":"array","items":{"type":"string"}},
        "reviews":{"type":"object","required":["latestByReviewer"],"properties":{"latestByReviewer":{"type":"array","items":{"type":"object","required":["login","state","submittedAt","id"],"properties":{"login":{"type":"string"},"state":{"type":"string"},"submittedAt":{"type":"string"},"id":{"type":["integer","string"]}}}}}},
        "threads":{"type":"object","required":["total","unresolved","unresolvedIds"],"properties":{"total":{"type":"integer"},"unresolved":{"type":"integer"},"unresolvedIds":{"type":"array","items":{"type":"string"}}}},
        "ci":{"type":"object","required":["summary","failing","inheritedFromStackParent"],"properties":{"summary":{"type":"object","required":["pass","pending","fail","skipped"],"properties":{"pass":{"type":"integer"},"pending":{"type":"integer"},"fail":{"type":"integer"},"skipped":{"type":"integer"}}},"failing":{"type":"array","items":{"type":"object","required":["name","source","url"],"properties":{"name":{"type":"string"},"source":{"enum":["check_run","status"]},"url":{"type":["string","null"]}}}},"inheritedFromStackParent":{"type":"array","items":{"type":"string"}}}},
        "stage":{"enum":["draft","awaiting_review","changes_requested","approved","ci_failing","mergeable"]}
    }}}}});
    let item = &mut schema["properties"]["pullRequests"]["items"];
    for field in ["mergedAt", "updatedAt"] {
        item["required"]
            .as_array_mut()
            .expect("required list")
            .push(json!(field));
        item["properties"][field] = json!({"type":["string","null"]});
    }
    schema
}

fn error(message: impl ToString) -> JsonRpcError {
    JsonRpcError {
        code: -32603,
        message: message.to_string(),
        data: None,
    }
}

async fn call(name: &str, args: Value) -> Result<Value, JsonRpcError> {
    let raw = crate::gateway::github_call(name, args)
        .await
        .map_err(error)?;
    github::unwrap_response(raw).map_err(error)
}

pub async fn search(args: Value) -> Result<Value, JsonRpcError> {
    call("search_pull_requests", args).await
}

async fn page(
    method: &str,
    owner: &str,
    repo: &str,
    number: u64,
) -> Result<Vec<Value>, JsonRpcError> {
    let mut items = Vec::new();
    let mut cursor: Option<String> = None;
    let mut cursors = BTreeSet::new();
    for page in 1..=100 {
        let mut args = json!({"owner":owner,"repo":repo,"pullNumber":number,"method":method,"perPage":100,"page":page});
        if let Some(after) = &cursor {
            args["after"] = json!(after);
        }
        let result = call("pull_request_read", args).await?;
        let values = github::page_items(method, &result).map_err(error)?;
        let count = values.len();
        items.extend(values.iter().cloned());
        if method == "get_review_comments" {
            let Some(next) = github::thread_cursor(&result).map_err(error)? else {
                return Ok(items);
            };
            if !cursors.insert(next.to_owned()) {
                return Err(error("Repeated GitHub review thread cursor"));
            }
            cursor = Some(next.to_owned());
        } else if github::page_complete(&result, count, items.len()) {
            return Ok(items);
        }
    }
    Err(error("GitHub pagination exceeded 100 pages"))
}

async fn list(
    owner: &str,
    repo: &str,
    method: &str,
    args: Value,
) -> Result<Vec<Value>, JsonRpcError> {
    let mut values = Vec::new();
    for page in 1..=100 {
        let mut query = args.clone();
        query["owner"] = json!(owner);
        query["repo"] = json!(repo);
        query["perPage"] = json!(100);
        query["page"] = json!(page);
        let response = call(method, query).await?;
        let array = github::page_items(method, &response).map_err(error)?;
        let count = array.len();
        values.extend(array.iter().cloned());
        if github::page_complete(&response, count, values.len()) {
            return Ok(values);
        }
    }
    Err(error("GitHub pagination exceeded 100 pages"))
}

async fn get(owner: &str, repo: &str, number: u64) -> Result<Pull, JsonRpcError> {
    let pr = call(
        "pull_request_read",
        json!({"owner":owner,"repo":repo,"pullNumber":number,"method":"get"}),
    )
    .await?;
    let sha = github::head_sha(&pr, number).map_err(error)?;
    let combined = call(
        "pull_request_read",
        json!({"owner":owner,"repo":repo,"pullNumber":number,"method":"get_status"}),
    )
    .await?;
    let status = crate::gateway::github_status(owner, repo, sha)
        .await
        .map_err(error)?;
    let statuses = github::status_detail(sha, &combined, &status).map_err(error)?;
    Ok(Pull {
        pr,
        reviews: page("get_reviews", owner, repo, number).await?,
        threads: page("get_review_comments", owner, repo, number).await?,
        checks: page("get_check_runs", owner, repo, number).await?,
        statuses,
        combined,
        parent: None,
    })
}

pub async fn view(arguments: Option<Value>) -> Result<Value, JsonRpcError> {
    let args: Args =
        serde_json::from_value(arguments.unwrap_or(Value::Null)).map_err(|e| JsonRpcError {
            code: -32602,
            message: format!("Invalid arguments: {e}"),
            data: None,
        })?;
    let (owner, repo) =
        github::validate_input(&args.repo, args.numbers.as_deref(), args.author.as_deref())
            .map_err(|message| JsonRpcError {
                code: -32602,
                message,
                data: None,
            })?;
    let numbers = if let Some(numbers) = args.numbers {
        numbers
    } else {
        let author = args.author.unwrap_or_default();
        github::pull_numbers(
            &list(
                owner,
                repo,
                "search_pull_requests",
                json!({"query":format!("repo:{owner}/{repo} is:pr is:open author:{author}")}),
            )
            .await?,
        )
        .map_err(error)?
    };
    let selected = numbers.iter().copied().collect::<BTreeSet<_>>();
    let mut pending: VecDeque<u64> = selected.iter().copied().collect();
    let mut pulls: BTreeMap<u64, Pull> = BTreeMap::new();
    let mut branches: BTreeMap<String, Option<u64>> = BTreeMap::new();
    while let Some(number) = pending.pop_front() {
        if pulls.contains_key(&number) {
            continue;
        }
        if pulls.len() >= 100 {
            return Err(error("GitHub stack exceeds 100 pull requests"));
        }
        let mut pull = get(owner, repo, number).await?;
        let base = github::base_branch(&pull.pr).map_err(error)?;
        let parent = if let Some(parent) = branches.get(base) {
            *parent
        } else {
            let candidates = list(
                owner,
                repo,
                "list_pull_requests",
                json!({"state":"all","head":format!("{owner}:{base}")}),
            )
            .await?;
            let parent = github::stack_parent(&candidates, number, base, owner, repo);
            branches.insert(base.to_owned(), parent);
            parent
        };
        pull.parent = parent;
        if let Some(parent) = parent {
            pending.push_back(parent);
        }
        pulls.insert(number, pull);
    }
    let records = github::merge(
        &pulls.into_values().collect::<Vec<_>>(),
        &selected.into_iter().collect::<Vec<_>>(),
    )
    .map_err(error)?;
    let text = serde_json::to_string_pretty(&records).map_err(error)?;
    Ok(
        json!({"content":[{"type":"text","text":text}],"structuredContent":{"pullRequests":records}}),
    )
}
