# Work status

`work_status` answers "where are we at?" by joining Linear and Jira tickets to GitHub pull requests. It calls the existing `linear_status_list`, `jira_status_list` and `github_pr_view` code and fetches nothing else. Matching, stages, flags, Markdown and the snapshot diff are pure code in `crates/core/src/work_status.rs`.

## Input

```json
{"person":"me","sources":{"linear":{"teams":["GUZ"]},"jira":{"projects":["VIT"]},"github":{"repos":["acme/vitals"],"author":"ada"}},"snapshot":"daily","format":"json","staleDays":7,"asOf":"2026-11-10","includeClosed":false}
```

- An empty or missing source list skips that source.
- `person` is the Linear and Jira assignee (default `me`). `github.author` is a GitHub login. It searches that author's open PRs across all of GitHub with the upstream `search_pull_requests` tool (`is:pr is:open author:<login>`), independent of `repos`, then fetches each PR's details like `github_pr_view`. `repos` only scope the PRs that tickets reference (plus stack ancestors that `github_pr_view` already walks). Without `author`, only ticket-referenced PRs are fetched.
- `includeClosed` also lists done tickets, which `TICKET_DONE_PR_OPEN` needs.
- `asOf` (`YYYY-MM-DD`) pins the clock for `STALE`; default is today (UTC).
- `format: "markdown"` puts the table in the text content; `structuredContent` is always the full report.

## Output

`items[]` (one per ticket, sorted by source then key), `orphanPrs[]` (open PRs with no ticket), `flags[]`, `sourceErrors[]`, `markdown`, and `changes[]` when `snapshot` is given.

A ticket matches a PR when the PR's `ticketKeys` (title and branch) hold the ticket key, or the ticket's PR references name the PR. Matching is many to many.

A PR is merged only when it has `mergedAt`. A PR closed without `mergedAt` is `closed`: it has no stage, raises no flags, and does not count as a PR for the ticket stage.

## Stage precedence

Per open PR: `draft`, `ci_failing`, `changes_requested`, `ci_pending`, `unresolved_threads`, `approved`, then `awaiting_review`. `ci_pending` means pending CI checks; `unresolved_threads` means unresolved review threads. A ticket takes the stage of its most blocked open PR. With no open PR it is `merged` when any PR is merged, else `no_pr`.

## Flags

`TICKET_NO_PR`, `PR_NO_TICKET`, `MERGED_TICKET_OPEN`, `TICKET_DONE_PR_OPEN`, `TICKET_REVIEW_PR_DRAFT` (status name contains "review" and an open PR is draft), `CI_FAIL`, `CI_FAIL_INHERITED_FROM_STACK_BASE`, `UNRESOLVED_THREADS`, `STALE` (open ticket or orphan PR with no activity for more than `staleDays`; ticket activity is the newest of its update time and its open PRs' update times). `flags[]` sorts by ticket, PR, then code.

## Partial failure

A failing source (auth, scope, network) adds `{source, scope, message}` to `sourceErrors[]` and the report still renders. A failed author search or author PR fetch uses scope `author:<login>`, so the author is never dropped silently.

## Snapshots

`snapshot` on `work_status` reads a saved snapshot and needs no permission. `work_status_snapshot_save` (Write tool, needs `allowWrites`) builds the report and saves item stages and flags. Snapshots are named, never paths: 1-64 characters of `A-Z a-z 0-9 _ -`. Anything else (`/`, `..`, absolute paths, dots) is rejected. Files live in `work-status/<name>.json` under the mcptools data directory (`$XDG_DATA_HOME/mcptools`, else `~/Library/Application Support/mcptools` on macOS, `~/.local/share/mcptools` on Linux). `changes[]` lists `added`, `changed` (stage or flags) and `removed` items.
