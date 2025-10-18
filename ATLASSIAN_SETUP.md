# Atlassian Module Configuration Guide

This guide explains how to set up and configure credentials to use the `atlassian` commands for Jira and Confluence.

## Prerequisites

- Access to an Atlassian Cloud instance (Jira and/or Confluence)
- Your Atlassian email address
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

The `atlassian` commands require three environment variables:

### Option A: Set in Your Shell Profile (Recommended)

1. **Edit your shell configuration file:**
   - For bash: `~/.bashrc` or `~/.bash_profile`
   - For zsh: `~/.zshrc`
   - For fish: `~/.config/fish/config.fish`

2. **Add the following lines:**

   ```bash
   export ATLASSIAN_BASE_URL="https://your-domain.atlassian.net"
   export ATLASSIAN_EMAIL="your-email@company.com"
   export ATLASSIAN_API_TOKEN="your-api-token-here"
   ```

3. **Reload your shell:**
   ```bash
   source ~/.bashrc  # or appropriate file for your shell
   ```

### Option B: Set for Current Session Only

```bash
export ATLASSIAN_BASE_URL="https://your-domain.atlassian.net"
export ATLASSIAN_EMAIL="your-email@company.com"
export ATLASSIAN_API_TOKEN="your-api-token-here"
```

### Option C: Use a .env File (Development Only)

Create a `.env` file in your project root:

```bash
ATLASSIAN_BASE_URL=https://your-domain.atlassian.net
ATLASSIAN_EMAIL=your-email@company.com
ATLASSIAN_API_TOKEN=your-api-token-here
```

Then load it before running commands:

```bash
source .env
mcptools atlassian jira list "project = PROJ"
```

### Option D: Pass as Command-Line Arguments

```bash
mcptools \
  --atlassian-url "https://your-domain.atlassian.net" \
  --atlassian-email "your-email@company.com" \
  --atlassian-token "your-api-token-here" \
  atlassian jira list "project = PROJ"
```

## Step 4: Verify Configuration

Test your configuration with a simple Jira query:

```bash
mcptools atlassian jira list "project IS NOT EMPTY" --limit 5
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

## Usage Examples

### Jira Commands

**List open issues in a project:**
```bash
mcptools atlassian jira list "project = PROJ AND status = Open"
```

**List issues assigned to you:**
```bash
mcptools atlassian jira list "assignee = currentUser()"
```

**Search with JQL and limit results:**
```bash
mcptools atlassian jira list "text ~ 'database' AND status = 'In Progress'" --limit 20
```

**Output as JSON:**
```bash
mcptools atlassian jira list "project = PROJ" --json | jq '.issues[] | {key, summary, status}'
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

## Using with MCP (Claude)

The Atlassian module is also available as MCP tools that Claude can use:

- **`jira_list`** - Search Jira issues using JQL
- **`confluence_search`** - Search Confluence pages using CQL

These tools are automatically available when using mcptools as an MCP server.

## Troubleshooting

### Error: "ATLASSIAN_BASE_URL environment variable not set"

**Solution:** Make sure all three environment variables are set:
```bash
echo $ATLASSIAN_BASE_URL
echo $ATLASSIAN_EMAIL
echo $ATLASSIAN_API_TOKEN
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
