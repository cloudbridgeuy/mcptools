use crate::prelude::*;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

// Disambiguate println! from prelude
use crate::prelude::println;

// Import domain models and pure functions from core
use mcptools_core::upgrade::{
    find_matching_asset, get_github_arch, get_github_os, is_version_up_to_date, parse_version_tag,
};
pub use mcptools_core::upgrade::{GitHubAsset, GitHubRelease};

#[derive(Debug, clap::Parser)]
#[command(name = "upgrade")]
#[command(about = "Upgrade mcptools to the latest version")]
pub struct App {
    /// Force upgrade even if already on latest version
    #[clap(long)]
    force: bool,
}

/// Module entry point
pub async fn run(app: App, _global: crate::Global) -> Result<()> {
    let current_version = env!("CARGO_PKG_VERSION");
    let current_exe = env::current_exe().context("Failed to get current executable path")?;

    println!("Current version: {}", current_version);

    // Fetch latest release from GitHub
    let latest_release = fetch_latest_release().await?;
    let latest_version = parse_version_tag(&latest_release.tag_name);

    println!("Latest version: {}", latest_version);

    // Check if upgrade is needed (using pure function from core)
    if !app.force
        && is_version_up_to_date(current_version, latest_version)
            .map_err(|e| eyre!("Version comparison failed: {}", e))?
    {
        println!("You are already running the latest version!");
        return Ok(());
    }

    // Get the appropriate binary for current OS/architecture (using pure functions from core)
    let os = get_github_os(env::consts::OS).map_err(|e| eyre!("{}", e))?;
    let arch = get_github_arch(env::consts::ARCH).map_err(|e| eyre!("{}", e))?;
    let asset = find_matching_asset(&latest_release, os, arch).map_err(|e| eyre!("{}", e))?;

    println!("Downloading latest version...");

    // Download the new binary
    let download_path = download_binary(&asset.browser_download_url, &current_exe).await?;

    println!("Download complete");

    // Perform the upgrade (backup old, replace with new)
    perform_upgrade(&current_exe, &download_path)?;

    println!("Successfully upgraded to v{}!", latest_version);

    Ok(())
}

/// Fetch the latest release information from GitHub API
async fn fetch_latest_release() -> Result<GitHubRelease> {
    let client = reqwest::Client::new();
    let response = client
        .get("https://api.github.com/repos/cloudbridgeuy/mcptools/releases/latest")
        .header("Accept", "application/vnd.github.v3+json")
        .header("User-Agent", "mcptools-upgrade-checker")
        .send()
        .await
        .context("Failed to fetch latest release from GitHub")?;

    if !response.status().is_success() {
        return Err(eyre!("GitHub API returned status: {} - You may have hit GitHub's rate limit. Try again later or check https://github.com/cloudbridgeuy/mcptools/releases for the latest version", response.status()));
    }

    response
        .json::<GitHubRelease>()
        .await
        .context("Failed to parse GitHub release response")
}

/// Download the binary from the given URL to a temporary file
async fn download_binary(url: &str, binary_path: &std::path::Path) -> Result<PathBuf> {
    let client = reqwest::Client::new();
    let response = client
        .get(url)
        .send()
        .await
        .context("Failed to download binary")?;

    if !response.status().is_success() {
        return Err(eyre!("Download failed with status: {}", response.status()));
    }

    let download_path = PathBuf::from(format!("{}.download", binary_path.display()));
    let bytes = response
        .bytes()
        .await
        .context("Failed to read downloaded binary")?;

    fs::write(&download_path, bytes).context("Failed to write downloaded binary to disk")?;

    Ok(download_path)
}

/// Perform the upgrade: backup current binary and replace with new one
fn perform_upgrade(current_binary: &PathBuf, new_binary: &PathBuf) -> Result<()> {
    // Check if we have write permissions
    if !has_write_permission(current_binary)? {
        return Err(eyre!(
            "Permission denied: cannot write to {}",
            current_binary.display()
        ));
    }

    let backup_path = PathBuf::from(format!("{}.backup", current_binary.display()));

    // Backup the current binary
    println!("Backing up current binary...");
    fs::copy(current_binary, &backup_path).context("Failed to create backup")?;

    // Replace with new binary
    println!("Installing new version...");
    if let Err(e) = fs::rename(new_binary, current_binary) {
        // Restore from backup on failure
        let _ = fs::rename(&backup_path, current_binary);
        return Err(e).context("Failed to replace binary, restored from backup");
    }

    // Set executable permissions
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let permissions = fs::Permissions::from_mode(0o755);
        fs::set_permissions(current_binary, permissions)
            .context("Failed to set executable permissions")?;
    }

    Ok(())
}

/// Check if we have write permissions to the binary file
fn has_write_permission(binary_path: &PathBuf) -> Result<bool> {
    let metadata = fs::metadata(binary_path).context("Failed to get binary metadata")?;
    let permissions = metadata.permissions();

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = permissions.mode();
        // Check if owner can write
        Ok((mode & 0o200) != 0)
    }

    #[cfg(not(unix))]
    {
        Ok(!permissions.read_only())
    }
}
