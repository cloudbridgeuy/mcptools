# Lanes

Only MCP `lane_list` and `lane_create` exist. There is no lane deletion, cleanup, merge, push, or CLI interface.

## Operator authorization

Set `MCPTOOLS_LANE_REPOS` in the operator-controlled MCP server environment. By default it is an exact platform path list parsed with `std::env::split_paths`, for example `/absolute/repo-one:/absolute/repo-two` on Unix. Every entry must be absolute and canonicalizable; requested repositories must match a canonical entry. Unset, empty, invalid entries, and repositories outside that list fail closed. Aliases are compared after canonicalization. Request arguments and repository configuration never authorize access. No current-directory trust is implied.

Only the exact whole value `MCPTOOLS_LANE_REPOS='*'` explicitly authorizes all otherwise-supported repository roots accessible to the server. This broadens the operator's trust scope beyond listed repositories; it does not discover repositories or enable path-component globbing. Mixed wildcard/path lists and whitespace around `*` are invalid. Requests must still be nonempty, absolute, canonicalizable Git repository roots. Descendants, non-repositories, and roots with Git common directories outside the root remain rejected, including linked worktree roots. All storage, symlink, identity, executable-filter, and write-permission guards remain unchanged.

This feature currently requires Unix filesystem identity. Git must be on the operator-controlled PATH. Only creation needs Worktrunk, pinned to verified 0.77.0 behavior.

## Contracts

`lane_list({repo})` returns `{lanes:[{id,path,branch,head,managed,main,current,locked,state}]}`. `id`, `branch`, `head`, and `locked` are nullable. Unborn or missing HEAD is null, never a fabricated hash. `state` is `clean`, `dirty`, or `unknown`. Unavailable status and unresolved submodule state are unknown. Missing registered worktree directories remain listed as unknown and unmanaged. Ownership records without registered Git worktrees do not add lanes. `main` is Git's first registered worktree, including a bare repository record; `current` matches the canonical invocation repository root, not a branch name or caller working directory.

Listing uses `git worktree list --porcelain -z`, then bounded native Git status calls. Worktrunk is not invoked: its JSON listing writes and prunes repository caches. Git optional index writes, hooks, fsmonitor, untracked-cache, automatic maintenance, and external diff/signature commands are suppressed. Submodule status is not recursively executed; otherwise-clean worktrees with submodules report unknown.

`lane_create({repo,branch,base})` returns `{id,path,branch,baseHead}`. Branch and base are explicit local branch names, at most 100 bytes; base must already exist locally. Flags, shortcuts, PR selectors, network references, and existing branches fail before creation. Git validates full local references. Each target is `<canonical repo>/.worktrees/<branch>`; slashes retain nested branch components, for example `.worktrees/feature/topic`. The random stable ID is separate from the path. All existing target ancestors must be real directories within the permitted root, not symlinks; missing parents are allowed, existing targets are rejected before mutation. Returned path, Git repository identity, branch, and HEAD are checked after creation.

Worktrunk receives a private temporary user configuration with a literal target path, `list.full=false`, `list.summary=false`, and JSON schema 2. Inherited environment is cleared except the operator PATH; project and system Worktrunk configuration are disabled. Git system/global configuration is disabled. Trusted Git overrides are inherited by child commands. Repository-configured executable filters, diff commands, merge drivers, and includes are rejected without execution. Includes are rejected because conditional includes could activate different commands in a new branch. Symlinks in Git configuration, refs, logs, administrative worktree records, and Worktrunk storage are rejected before creation.

Each subprocess has a 20-second deadline and a combined stdout/stderr cap of 1 MiB. Process groups are killed and the child reaped on timeout or output failure. There is no generic command or flag forwarding.

Git lazy object fetching is disabled. Configured partial clones and promisor remotes are rejected before creation resolves objects; their list status is unknown. Missing local objects never authorize network recovery.

## Ownership and failure

Atomic, no-overwrite, fsynced per-branch JSON records live at `<git-common-dir>/mcptools-lanes`. They bind canonical repository/common-directory, worktree-directory, Git administrative-directory, and branch-reflog filesystem identities, plus branch, random ID, and creation HEAD. Normal descendant commits preserve management. Detachment, changed identity, branch reflog replacement, and history not descending from creation HEAD do not. Existing ownership records block branch reuse; malformed or unsupported records fail closed. Filesystem identity is not a cryptographic proof and is not portable across repository copies or restored filesystems.

After creation starts, failures report `PARTIAL_MUTATION_POSSIBLE`, requested path/branch, path existence, and actual identity when Worktrunk reports a different result. Creation is never retried or rolled back. Registration failure can leave a real unmanaged lane. The operator must inspect Git state before any manual retry.

## Security boundary

These are application guards, not an OS sandbox. The operator must control the server environment, PATH binaries, authorized repository files, and filesystem while commands run. Concurrent hostile filesystem/configuration changes are outside this boundary. Creation may write Git and Worktrunk state inside the permitted repository. Listing does not create ownership records or Worktrunk caches. Direct `tools/call` retains the server's existing write dispatch policy; `call_tool` and `execute` additionally require `allowWrites:true` for creation. Spend permission does not authorize writes.
