# Upgrade Command

Self-update mcptools to the latest version from GitHub releases.

## CLI Usage

```bash
# Check and upgrade to latest version
mcptools upgrade

# Force upgrade even if already on latest
mcptools upgrade --force
```

## How It Works

1. **Version Check**: Fetches latest release from `https://api.github.com/repos/cloudbridgeuy/mcptools/releases/latest`
2. **Comparison**: Compares current version with latest (semantic versioning)
3. **Download**: Downloads the appropriate binary for your OS/architecture
4. **Backup**: Creates a backup of the current binary (`mcptools.backup`)
5. **Replace**: Replaces the current binary with the new one
6. **Permissions**: Sets executable permissions (Unix: 0o755)

## Supported Platforms

The upgrade command automatically detects your platform:

| OS | Architecture | Asset Pattern |
|----|--------------|---------------|
| macOS | arm64 (M1/M2) | `*-darwin-arm64` |
| macOS | x86_64 | `*-darwin-x86_64` |
| Linux | x86_64 | `*-linux-x86_64` |
| Linux | aarch64 | `*-linux-aarch64` |

## Error Handling

### Rate Limiting
GitHub API has rate limits. If you hit them:
```
GitHub API returned status: 403 - You may have hit GitHub's rate limit.
Try again later or check https://github.com/cloudbridgeuy/mcptools/releases
```

### Permission Denied
If the binary is in a protected location:
```
Permission denied: cannot write to /usr/local/bin/mcptools
```
Solution: Use `sudo mcptools upgrade` or move the binary to a user-writable location.

### Rollback
If the upgrade fails during replacement, the command automatically restores from the backup.

## Manual Upgrade

If the automatic upgrade fails, you can manually:

1. Visit https://github.com/cloudbridgeuy/mcptools/releases
2. Download the appropriate binary for your platform
3. Replace your existing `mcptools` binary
4. Make it executable: `chmod +x mcptools`

## Version Information

Check your current version:
```bash
mcptools --version
```
