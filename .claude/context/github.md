# GitHub pull request status

`github_pr_view` takes `repo` as `owner/name` and either `numbers` or `author`. Author selection returns open pull requests; numbers may select any state. Text output is an array sorted by number. MCP structured output wraps the same array in `pullRequests`. Each result includes reviews, review threads, CI checks, commit statuses, and stack ancestry.

Configure a read-only GitHub MCP gateway with `pull_request_read`, `list_pull_requests`, and `search_pull_requests`. The gateway must use an expanded `Authorization: Bearer ...` header. The tool uses the same header for GitHub's REST commit status endpoint. The gateway's `url` must be HTTPS on host `api.githubcopilot.com` (default port); any other upstream exposing these tool names is rejected, and its header is never sent. Grant pull request, checks, and commit status read access.

Stage precedence is `draft`, `ci_failing`, `changes_requested`, `mergeable`, `approved`, then `awaiting_review`. Mergeable requires approval, a known mergeable state, no unresolved threads, and complete CI. Unknown mergeability is `null`.

The tool fetches reviews, threads, checks, combined commit status, and up to 100 detailed commit statuses per pull request, including ancestors outside the requested set. It fails if more than 100 statuses exist. Stack parents require one cached pull request search per base branch. No configuration is changed.
