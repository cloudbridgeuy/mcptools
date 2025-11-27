# Atlassian Module Configuration Guide

This guide explains how to set up and configure credentials to use the `atlassian` commands for Jira, Confluence, and Bitbucket.

## Prerequisites

- Access to an Atlassian Cloud instance (Jira, Confluence, and/or Bitbucket)
- Your Atlassian email address (for Jira/Confluence)
- Your Bitbucket username and app password (for Bitbucket)
- Administrator or user account with appropriate permissions

## Step 1: Generate an Atlassian API Token

### For Jira Cloud and Confluence Cloud

1. **Log in to your Atlassian account:**

   - Go to https://id.atlassian.com/manage-profile/security/api-tokens

2. **Create a new API token:**
   - Click "Create API token"
   - Give it a descriptive name (e.g., "mcptools")
   - Click "Create"
   - Copy the generated token (you won't be able to see it again)

### Important Notes

- API tokens are **long-lived credentials** (never expire unless manually revoked)
- **Treat them like passwords** - never commit them to version control
- You can revoke tokens anytime if they're compromised
- For security, create a separate token for mcptools rather than using a personal token

### For Bitbucket Cloud

Bitbucket uses a separate authentication method with app passwords:

1. **Log in to your Bitbucket account:**

   - Go to https://bitbucket.org/account/settings/app-passwords/

2. **Create a new app password:**

   - Click "Create app password"
   - Give it a descriptive name (e.g., "mcptools")
   - Select the following permissions:
     - **Repositories**: Read
     - **Pull requests**: Read
   - Click "Create"
   - Copy the generated password (you won't be able to see it again)

3. **Note your Bitbucket username:**
   - Your username is shown in your profile settings
   - This is NOT your email address

## Step 2: Get Your Atlassian Base URL

Your base URL depends on where your Atlassian instance is hosted:

- **Atlassian Cloud (most common):** `https://your-domain.atlassian.net`
  - Example: `https://mycompany.atlassian.net`
- **Self-hosted:** `https://your-jira-server.com`

To find your URL:

- Open your Jira or Confluence instance in a browser
- Look at the URL bar - extract the base domain
- Example: If your Jira URL is `https://mycompany.atlassian.net/browse/PROJ-123`, your base URL is `https://mycompany.atlassian.net`

## Step 3: Configure Environment Variables

### Environment Variable Precedence

Each Atlassian service supports its own environment variables that override the shared `ATLASSIAN_*` variables:

| Service        | Service-Specific Variables                                        | Fallback Variables   |
| -------------- | ----------------------------------------------------------------- | -------------------- |
| **Jira**       | `JIRA_BASE_URL`, `JIRA_EMAIL`, `JIRA_API_TOKEN`                   | `ATLASSIAN_*`        |
| **Confluence** | `CONFLUENCE_BASE_URL`, `CONFLUENCE_EMAIL`, `CONFLUENCE_API_TOKEN` | `ATLASSIAN_*`        |
| **Bitbucket**  | `BITBUCKET_USERNAME`, `BITBUCKET_APP_PASSWORD`                    | (uses app passwords) |

**Why service-specific variables?** Atlassian may require different API tokens for different services. Use service-specific variables when your Jira and Confluence tokens differ.

### Option A: Shared Credentials (Simplest)

If you use the same credentials for Jira and Confluence:

```bash
# Add to your shell profile (~/.bashrc, ~/.zshrc, etc.)
export ATLASSIAN_BASE_URL="https://your-domain.atlassian.net"
export ATLASSIAN_EMAIL="your-email@company.com"
export ATLASSIAN_API_TOKEN="your-api-token-here"
```

### Option B: Service-Specific Credentials

If you need different credentials for each service:

```bash
# Shared fallback (optional if setting all service-specific vars)
export ATLASSIAN_BASE_URL="https://your-domain.atlassian.net"
export ATLASSIAN_EMAIL="your-email@company.com"
export ATLASSIAN_API_TOKEN="your-default-token"

# Jira-specific overrides (takes precedence)
export JIRA_BASE_URL="https://your-jira.atlassian.net"
export JIRA_EMAIL="jira-user@company.com"
export JIRA_API_TOKEN="your-jira-specific-token"

# Confluence-specific overrides (takes precedence)
export CONFLUENCE_BASE_URL="https://your-confluence.atlassian.net"
export CONFLUENCE_EMAIL="confluence-user@company.com"
export CONFLUENCE_API_TOKEN="your-confluence-specific-token"
```

### Option C: Use a .env File (Development Only)

Create a `.env` file in your project root:

```bash
# Shared credentials
ATLASSIAN_BASE_URL=https://your-domain.atlassian.net
ATLASSIAN_EMAIL=your-email@company.com
ATLASSIAN_API_TOKEN=your-api-token-here

# Service-specific overrides (optional)
JIRA_API_TOKEN=your-jira-specific-token
CONFLUENCE_API_TOKEN=your-confluence-specific-token
```

Then load it before running commands:

```bash
source .env
mcptools atlassian jira search "project = PROJ"
```

### For Bitbucket

Bitbucket uses app passwords (not API tokens):

```bash
export BITBUCKET_USERNAME="your-bitbucket-username"
export BITBUCKET_APP_PASSWORD="your-app-password-here"
```

**Important:** Your Bitbucket username is NOT your email address. Find it in your Bitbucket profile settings.

## Step 4: Verify Configuration

### Test Jira Configuration

Test your Jira configuration with a simple query:

```bash
mcptools atlassian jira search "project IS NOT EMPTY" --limit 5
```

Expected output (if successful):

```
Found N issue(s):

+----------+-------------------+--------+----------+
| Key      | Summary           | Status | Assignee |
+==========+===================+========+==========+
| PROJ-123 | Issue title       | Open   | John Doe |
+----------+-------------------+--------+----------+
...
```

If you get an error like `ATLASSIAN_BASE_URL environment variable not set`, ensure all three environment variables are correctly configured.

### Test Bitbucket Configuration

Test your Bitbucket configuration by listing PRs:

```bash
mcptools atlassian bitbucket pr list --repo "your-workspace/your-repo" --limit 5
```

Expected output (if successful):

```
Found N pull request(s):

+----+------------------------+----------+-------+----------------+------------------+
| ID | Title                  | Author   | State | Source Branch  | Dest Branch      |
+====+========================+==========+=======+================+==================+
| 42 | Add new feature        | johndoe  | OPEN  | feature/new    | main             |
+----+------------------------+----------+-------+----------------+------------------+
...
```

If you get an authentication error, verify your `BITBUCKET_USERNAME` and `BITBUCKET_APP_PASSWORD` are correct.

## Usage Examples

### Jira Commands

**Search for open issues in a project:**

```bash
mcptools atlassian jira search "project = PROJ AND status = Open"
```

**Search for issues assigned to you:**

```bash
mcptools atlassian jira search "assignee = currentUser()"
```

**Search with JQL and limit results:**

```bash
mcptools atlassian jira search "text ~ 'database' AND status = 'In Progress'" --limit 20
```

**Output as JSON:**

```bash
mcptools atlassian jira search "project = PROJ" --json | jq '.issues[] | {key, summary, status}'
```

### Confluence Commands

**Search for pages about a topic:**

```bash
mcptools atlassian confluence search "text ~ 'deployment'"
```

**Search in a specific space:**

```bash
mcptools atlassian confluence search "space = WIKI AND text ~ 'guide'"
```

**Limit results and output as JSON:**

```bash
mcptools atlassian confluence search "text ~ 'api'" --limit 5 --json
```

### Bitbucket Commands

**List open pull requests:**

```bash
mcptools atlassian bitbucket pr list --repo "myworkspace/myrepo"
```

**Filter by state:**

```bash
mcptools atlassian bitbucket pr list --repo "myworkspace/myrepo" --state OPEN
mcptools atlassian bitbucket pr list --repo "myworkspace/myrepo" --state MERGED --state DECLINED
```

**Read PR details with diff:**

```bash
mcptools atlassian bitbucket pr read --repo "myworkspace/myrepo" 123
```

**Limit diff output:**

```bash
# Truncate to 200 lines
mcptools atlassian bitbucket pr read --repo "myworkspace/myrepo" 123 --line-limit 200

# Skip diff entirely
mcptools atlassian bitbucket pr read --repo "myworkspace/myrepo" 123 --no-diff

# Only show diff (skip PR details)
mcptools atlassian bitbucket pr read --repo "myworkspace/myrepo" 123 --diff-only
```

**Output as JSON:**

```bash
mcptools atlassian bitbucket pr list --repo "myworkspace/myrepo" --json
```

## Using with MCP (Claude)

The Atlassian module is also available as MCP tools that Claude can use:

- **`jira_search`** - Search Jira issues using JQL
- **`jira_get`** - Get detailed information about a Jira ticket
- **`jira_create`** - Create a new Jira ticket
- **`jira_update`** - Update fields on a Jira ticket
- **`jira_fields`** - List available custom field values
- **`confluence_search`** - Search Confluence pages using CQL
- **`bitbucket_pr_list`** - List pull requests in a repository
- **`bitbucket_pr_read`** - Read PR details including diff and comments

These tools are automatically available when using mcptools as an MCP server.

## Troubleshooting

### Error: "Neither JIRA_BASE_URL nor ATLASSIAN_BASE_URL environment variable is set"

**Solution:** Set either `JIRA_BASE_URL` or `ATLASSIAN_BASE_URL`:

```bash
# Option 1: Service-specific
export JIRA_BASE_URL="https://your-domain.atlassian.net"

# Option 2: Shared (used as fallback)
export ATLASSIAN_BASE_URL="https://your-domain.atlassian.net"
```

### Error: "Neither CONFLUENCE_API_TOKEN nor ATLASSIAN_API_TOKEN environment variable is set"

**Solution:** Set either `CONFLUENCE_API_TOKEN` or `ATLASSIAN_API_TOKEN`:

```bash
# Option 1: Service-specific
export CONFLUENCE_API_TOKEN="your-confluence-token"

# Option 2: Shared (used as fallback)
export ATLASSIAN_API_TOKEN="your-shared-token"
```

### Error: "ATLASSIAN_BASE_URL environment variable not set" (legacy)

**Solution:** Make sure either shared or service-specific environment variables are set:

```bash
# Check shared variables
echo $ATLASSIAN_BASE_URL
echo $ATLASSIAN_EMAIL
echo $ATLASSIAN_API_TOKEN

# Or service-specific for Jira
echo $JIRA_BASE_URL
echo $JIRA_EMAIL
echo $JIRA_API_TOKEN
```

If any are missing, configure them using one of the methods above.

### Error: "Jira API error [401]: ..."

**Solution:** Your authentication credentials are incorrect. Check:

- API token is correct (copy it again from https://id.atlassian.com/manage-profile/security/api-tokens)
- Email address matches the one associated with the token
- Base URL is correct

### Error: "Jira API error [403]: ..."

**Solution:** Your user account doesn't have permission to perform this action. Check:

- Your account has permission to view the projects/issues you're querying
- Your token has appropriate scopes (should have `read:jira-work` and `search:jira`)

### Error: "Jira API error [404]: ..."

**Solution:** The resource doesn't exist or your base URL is incorrect. Verify:

- Base URL is correct (should NOT include `/browse` or `/wiki`)
- The project or issue exists

### Connection Timeouts

If commands are timing out:

- Check your internet connection
- Verify your Atlassian instance is accessible from your network
- Try with a simpler query to isolate the issue

### Error: "BITBUCKET_USERNAME environment variable not set"

**Solution:** Make sure both Bitbucket environment variables are set:

```bash
echo $BITBUCKET_USERNAME
echo $BITBUCKET_APP_PASSWORD
```

If missing, configure them as shown in Step 3.

### Error: "Bitbucket API error [401]: ..."

**Solution:** Your Bitbucket credentials are incorrect. Check:

- App password is correct (create a new one if needed)
- Username is your Bitbucket username, NOT your email address
- App password has the required permissions (Repositories: Read, Pull requests: Read)

### Error: "Repository not found" or [404]

**Solution:** Verify:

- Repository format is correct: `workspace/repo_slug` (e.g., `mycompany/my-repo`)
- You have access to the repository
- Repository name is spelled correctly (case-sensitive)

## Security Best Practices

1. **Never commit credentials to version control:**

   - Add `.env` to your `.gitignore`
   - Don't hardcode tokens in scripts

2. **Use environment variables:**

   - Keep tokens out of command history
   - Use shell profiles to auto-load on session start

3. **Rotate tokens regularly:**

   - Review and revoke old tokens
   - Create new tokens for different use cases

4. **Limit token scope:**

   - Only grant necessary permissions
   - Atlassian API tokens have broad permissions by default

5. **Monitor token usage:**
   - Check Atlassian's security log for token usage
   - Revoke tokens immediately if compromised

## Additional Resources

- [Atlassian API Token Documentation](https://support.atlassian.com/atlassian-account/docs/manage-api-tokens-for-your-atlassian-account/)
- [Jira Cloud REST API Documentation](https://developer.atlassian.com/cloud/jira/rest/v3)
- [Confluence Cloud REST API Documentation](https://developer.atlassian.com/cloud/confluence/rest/v2)
- [Jira Query Language (JQL) Documentation](https://support.atlassian.com/jira-software-cloud/docs/advanced-searching-using-jql/)
- [Confluence Query Language (CQL) Documentation](https://support.atlassian.com/confluence-cloud/docs/advanced-searching-using-cql/)
- [Bitbucket App Passwords Documentation](https://support.atlassian.com/bitbucket-cloud/docs/app-passwords/)
- [Bitbucket Cloud REST API Documentation](https://developer.atlassian.com/cloud/bitbucket/rest/)
