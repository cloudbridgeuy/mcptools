---
shaping: true
---

# JQL Exclamation Mark Escape Bug -- Shaping

## Source

> ```
> mcptools atlassian jira search 'project = VP20 AND summary ~ "Pentest" AND creator != currentUser()' --limit 100
> ```
>
> Error:
> ```
> Jira API error [400 Bad Request]: {"errorMessages":["Error in the JQL Query: '\\!' is an illegal JQL escape sequence. The valid escape sequences are \\', \\\", \\t, \\n, \\r, \\\\, '\\ ' and \\uXXXX. (line 1, character 52)"],"errors":{}}
> ```
>
> Character 52 in the JQL string corresponds to the `!` in `!=`.

---

## Problem

JQL queries containing `!=` fail with a 400 error because somewhere between the CLI argument and Jira's API, the `!` character is transformed into `\!`, which Jira's JQL parser rejects as an illegal escape sequence.

## Outcome

JQL queries with `!=` (and any other use of `!`) work correctly.

---

## Requirements (R)

| ID  | Requirement                                                  | Status    |
| --- | ------------------------------------------------------------ | --------- |
| R0  | JQL queries with `!=` operator work correctly                | Core goal |
| R1  | No regression for other JQL operators and special characters | Must-have |
| R2  | Fix applies to both CLI and MCP tool invocations             | Must-have |

---

## Investigation

### Code path analysis

The JQL query flows through:

1. Shell arg -> clap -> `options.jql_query: Option<String>` (search.rs:64)
2. `search_query = options.jql_query.clone()` (search.rs:304-308)
3. `search_issues_data(search_query, ...)` (search.rs:324)
4. `("jql", query.as_str())` added to `query_params` vec (search.rs:127)
5. `client.get(&url).query(&query_params)` -- reqwest URL-encodes the params (search.rs:157-159)
6. HTTP GET to `/rest/api/3/search/jql`

**There is no manual escaping of the JQL string anywhere in the code.** The query passes through as-is from CLI args to reqwest's `.query()` method.

### Where the `\!` comes from (Spike resolved)

See [spike-jql-exclamation.md](spike-jql-exclamation.md) for full investigation.

**Root cause:** Claude Code's Bash tool escapes `!` -> `\!` in the command string before passing it to the shell (likely to prevent bash history expansion). Our code passes the JQL string through unchanged. Reqwest URL-encodes `\!` as `%5C%21`. Jira decodes it to `\!` and rejects it as an invalid JQL escape.

Reqwest is clean. Jira is behaving correctly. The fix belongs in our input handling.

---

## A: Strip invalid JQL backslash escapes in `search_issues_data`

| Part   | Mechanism                                                                                             | Flag |
| ------ | ----------------------------------------------------------------------------------------------------- | :--: |
| **A1** | Add `.replace("\\!", "!")` on the JQL query string before passing to reqwest in `search_issues_data`  |      |

**Notes:**
- `\!` is not a valid JQL escape (valid: `\'`, `\"`, `\t`, `\n`, `\r`, `\\`, `\ `, `\uXXXX`)
- The replace is a no-op for MCP invocations (which don't go through the Bash tool)
- Single-line change in `search_issues_data` at search.rs:127

---

## Fit Check

| Req | Requirement                                            | Status    | A   |
| --- | ------------------------------------------------------ | --------- | --- |
| R0  | JQL queries with `!=` operator work correctly          | Core goal | ✅  |
| R1  | No regression for other JQL operators/special chars    | Must-have | ✅  |
| R2  | Fix applies to both CLI and MCP tool invocations       | Must-have | ✅  |

**Notes:**
- A passes R0: `\!` is stripped to `!` before API call, so `!=` works
- A passes R1: only strips `\!`, which is never valid JQL — no legitimate use cases affected
- A passes R2: `search_issues_data` is the shared function for both CLI and MCP
