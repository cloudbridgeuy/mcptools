---
shaping: true
---

# Spike: Root cause of `!` becoming `\!` in JQL queries

## Context

JQL queries containing `!=` fail with a 400 error. Jira reports `\!` as an illegal JQL escape sequence. The `!` character is being transformed into `\!` somewhere between our CLI and Jira's JQL parser. Our code does not manually escape the query string.

## Goal

Identify exactly where and why `!` becomes `\!` in the request pipeline.

## Questions

| #        | Question                                                                                      |
| -------- | --------------------------------------------------------------------------------------------- |
| **S1-Q1** | What URL does reqwest produce when `.query()` encodes a JQL string containing `!=`?          |
| **S1-Q2** | Does the Jira V3 GET `/rest/api/3/search/jql` endpoint handle `%21` (URL-encoded `!`) correctly, or does it misinterpret it? |
| **S1-Q3** | Is the `\!` coming from reqwest's encoding, the shell, or Jira's own processing?             |

## Acceptance

Spike is complete when we can describe the exact transformation point where `!` becomes `\!` and whether the issue is in our code, in reqwest, or in Jira's API.

---

## Findings

### S1-Q1: What URL does reqwest produce?

Reqwest (via `form_urlencoded` / WHATWG URL spec) encodes `!` as `%21`. The full URL is:
```
.../search/jql?jql=...creator+%21%3D+currentUser%28%29&maxResults=10
```
URL-decoding roundtrips perfectly: `%21` -> `!`. **Reqwest is NOT the problem.**

### S1-Q2: Does Jira handle `%21` correctly?

We never got to test this because the issue occurs before the HTTP request. The program receives `\!` in its CLI argument, so the URL contains `%5C%21` (backslash + exclamation), which Jira correctly URL-decodes to `\!` and then rejects as an invalid JQL escape.

### S1-Q3: Where does the `\!` come from?

**Claude Code's Bash tool escapes `!` to `\!` in the command string before passing it to the shell.** This happens regardless of quoting:

```
$ printf '%s' 'hello != world' | xxd
00000000: 6865 6c6c 6f20 5c21 3d20 776f 726c 64    hello \!= world
```

The program receives the literal string `\!=` instead of `!=`. Only `!` is affected -- `$`, `"`, and other special characters pass through unchanged in single quotes.

### Root cause

Claude Code escapes `!` -> `\!` (likely to prevent bash history expansion). Our code passes the JQL string through to reqwest unchanged. Reqwest URL-encodes `\!` as `%5C%21`. Jira URL-decodes it back to `\!`. Jira's JQL parser rejects `\!` as an invalid escape sequence.

### Implication for fix

The fix belongs in our code, not in reqwest or Jira. Before sending the JQL to the API, we should strip the backslash from `\!` (since `\!` is not a valid JQL escape). A simple `.replace("\\!", "!")` on the JQL string would suffice. This is safe for both CLI and MCP paths (MCP won't have `\!`, so the replace is a no-op).
