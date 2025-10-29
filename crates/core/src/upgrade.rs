use serde::{Deserialize, Serialize};

/// GitHub release API response
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GitHubRelease {
    pub tag_name: String,
    pub assets: Vec<GitHubAsset>,
}

/// GitHub release asset
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GitHubAsset {
    pub name: String,
    pub browser_download_url: String,
}

/// Parse version tag by removing 'v' prefix
///
/// Converts "v1.2.3" to "1.2.3", leaves "1.2.3" unchanged
pub fn parse_version_tag(tag: &str) -> &str {
    tag.trim_start_matches('v')
}

/// Compare versions - returns true if current >= latest
///
/// Compares semantic versions by splitting on '.' and comparing each part.
/// Pads with zeros if lengths differ (e.g., "1.2" is treated as "1.2.0").
pub fn is_version_up_to_date(current: &str, latest: &str) -> Result<bool, String> {
    let current_parts: Vec<&str> = current.split('.').collect();
    let latest_parts: Vec<&str> = latest.split('.').collect();

    // Pad with zeros if lengths differ
    let max_len = current_parts.len().max(latest_parts.len());
    let mut current_parts: Vec<u32> = current_parts
        .iter()
        .map(|p| p.parse::<u32>().unwrap_or(0))
        .collect();
    let mut latest_parts: Vec<u32> = latest_parts
        .iter()
        .map(|p| p.parse::<u32>().unwrap_or(0))
        .collect();

    current_parts.resize(max_len, 0);
    latest_parts.resize(max_len, 0);

    Ok(current_parts >= latest_parts)
}

/// Find the asset matching current OS and architecture
///
/// Searches the release assets for a binary matching the target name format:
/// "mcptools-{os}-{arch}" (e.g., "mcptools-Darwin-arm64")
pub fn find_matching_asset<'a>(
    release: &'a GitHubRelease,
    os: &str,
    arch: &str,
) -> Result<&'a GitHubAsset, String> {
    let target_name = format!("mcptools-{os}-{arch}");

    release
        .assets
        .iter()
        .find(|asset| asset.name == target_name)
        .ok_or_else(|| format!("No binary found for {}-{}", os, arch))
}

/// Map Rust's OS constant to GitHub release naming
pub fn get_github_os(os: &str) -> Result<&'static str, String> {
    match os {
        "macos" => Ok("Darwin"),
        "linux" => Ok("Linux"),
        os => Err(format!("Unsupported OS: {}", os)),
    }
}

/// Map Rust's ARCH constant to GitHub release naming
pub fn get_github_arch(arch: &str) -> Result<&'static str, String> {
    match arch {
        "aarch64" => Ok("arm64"),
        "x86_64" => Ok("x86_64"),
        arch => Err(format!("Unsupported architecture: {}", arch)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ============================================================================
    // parse_version_tag tests
    // ============================================================================

    #[test]
    fn test_parse_version_tag_with_v_prefix() {
        assert_eq!(parse_version_tag("v1.2.3"), "1.2.3");
    }

    #[test]
    fn test_parse_version_tag_without_v_prefix() {
        assert_eq!(parse_version_tag("1.2.3"), "1.2.3");
    }

    #[test]
    fn test_parse_version_tag_empty() {
        assert_eq!(parse_version_tag(""), "");
    }

    #[test]
    fn test_parse_version_tag_multiple_v() {
        assert_eq!(parse_version_tag("vv1.2.3"), "1.2.3"); // Removes all leading 'v' chars
    }

    // ============================================================================
    // is_version_up_to_date tests
    // ============================================================================

    #[test]
    fn test_is_version_up_to_date_same_version() {
        assert!(is_version_up_to_date("1.2.3", "1.2.3").unwrap());
    }

    #[test]
    fn test_is_version_up_to_date_current_newer() {
        assert!(is_version_up_to_date("1.2.4", "1.2.3").unwrap());
    }

    #[test]
    fn test_is_version_up_to_date_current_older() {
        assert!(!is_version_up_to_date("1.2.3", "1.2.4").unwrap());
    }

    #[test]
    fn test_is_version_up_to_date_major_version_newer() {
        assert!(is_version_up_to_date("2.0.0", "1.9.9").unwrap());
    }

    #[test]
    fn test_is_version_up_to_date_major_version_older() {
        assert!(!is_version_up_to_date("1.9.9", "2.0.0").unwrap());
    }

    #[test]
    fn test_is_version_up_to_date_minor_version_newer() {
        assert!(is_version_up_to_date("1.3.0", "1.2.9").unwrap());
    }

    #[test]
    fn test_is_version_up_to_date_minor_version_older() {
        assert!(!is_version_up_to_date("1.2.9", "1.3.0").unwrap());
    }

    #[test]
    fn test_is_version_up_to_date_padding_current_shorter() {
        // "1.2" should be treated as "1.2.0"
        assert!(is_version_up_to_date("1.2", "1.2.0").unwrap());
    }

    #[test]
    fn test_is_version_up_to_date_padding_latest_shorter() {
        // "1.2.0" vs "1.2" (treated as "1.2.0")
        assert!(is_version_up_to_date("1.2.0", "1.2").unwrap());
    }

    #[test]
    fn test_is_version_up_to_date_padding_current_older() {
        // "1.2" (treated as "1.2.0") vs "1.2.1"
        assert!(!is_version_up_to_date("1.2", "1.2.1").unwrap());
    }

    #[test]
    fn test_is_version_up_to_date_four_parts() {
        assert!(is_version_up_to_date("1.2.3.4", "1.2.3.4").unwrap());
        assert!(is_version_up_to_date("1.2.3.5", "1.2.3.4").unwrap());
        assert!(!is_version_up_to_date("1.2.3.3", "1.2.3.4").unwrap());
    }

    #[test]
    fn test_is_version_up_to_date_invalid_parts_treated_as_zero() {
        // Non-numeric parts are treated as 0
        assert!(is_version_up_to_date("1.2.x", "1.2.0").unwrap());
        assert!(is_version_up_to_date("1.2.0", "1.2.x").unwrap());
    }

    // ============================================================================
    // find_matching_asset tests
    // ============================================================================

    #[test]
    fn test_find_matching_asset_darwin_arm64() {
        let release = GitHubRelease {
            tag_name: "v1.0.0".to_string(),
            assets: vec![
                GitHubAsset {
                    name: "mcptools-Darwin-arm64".to_string(),
                    browser_download_url: "https://example.com/darwin-arm64".to_string(),
                },
                GitHubAsset {
                    name: "mcptools-Linux-x86_64".to_string(),
                    browser_download_url: "https://example.com/linux-x86_64".to_string(),
                },
            ],
        };

        let asset = find_matching_asset(&release, "Darwin", "arm64").unwrap();
        assert_eq!(asset.name, "mcptools-Darwin-arm64");
        assert_eq!(
            asset.browser_download_url,
            "https://example.com/darwin-arm64"
        );
    }

    #[test]
    fn test_find_matching_asset_linux_x86_64() {
        let release = GitHubRelease {
            tag_name: "v1.0.0".to_string(),
            assets: vec![
                GitHubAsset {
                    name: "mcptools-Darwin-arm64".to_string(),
                    browser_download_url: "https://example.com/darwin-arm64".to_string(),
                },
                GitHubAsset {
                    name: "mcptools-Linux-x86_64".to_string(),
                    browser_download_url: "https://example.com/linux-x86_64".to_string(),
                },
            ],
        };

        let asset = find_matching_asset(&release, "Linux", "x86_64").unwrap();
        assert_eq!(asset.name, "mcptools-Linux-x86_64");
        assert_eq!(
            asset.browser_download_url,
            "https://example.com/linux-x86_64"
        );
    }

    #[test]
    fn test_find_matching_asset_not_found() {
        let release = GitHubRelease {
            tag_name: "v1.0.0".to_string(),
            assets: vec![GitHubAsset {
                name: "mcptools-Darwin-arm64".to_string(),
                browser_download_url: "https://example.com/darwin-arm64".to_string(),
            }],
        };

        let result = find_matching_asset(&release, "Windows", "x86_64");
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "No binary found for Windows-x86_64");
    }

    #[test]
    fn test_find_matching_asset_empty_assets() {
        let release = GitHubRelease {
            tag_name: "v1.0.0".to_string(),
            assets: vec![],
        };

        let result = find_matching_asset(&release, "Darwin", "arm64");
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "No binary found for Darwin-arm64");
    }

    #[test]
    fn test_find_matching_asset_multiple_assets() {
        let release = GitHubRelease {
            tag_name: "v1.0.0".to_string(),
            assets: vec![
                GitHubAsset {
                    name: "mcptools-Darwin-arm64".to_string(),
                    browser_download_url: "https://example.com/darwin-arm64".to_string(),
                },
                GitHubAsset {
                    name: "mcptools-Darwin-x86_64".to_string(),
                    browser_download_url: "https://example.com/darwin-x86_64".to_string(),
                },
                GitHubAsset {
                    name: "mcptools-Linux-x86_64".to_string(),
                    browser_download_url: "https://example.com/linux-x86_64".to_string(),
                },
            ],
        };

        let asset = find_matching_asset(&release, "Darwin", "x86_64").unwrap();
        assert_eq!(asset.name, "mcptools-Darwin-x86_64");
    }

    // ============================================================================
    // get_github_os tests
    // ============================================================================

    #[test]
    fn test_get_github_os_macos() {
        assert_eq!(get_github_os("macos").unwrap(), "Darwin");
    }

    #[test]
    fn test_get_github_os_linux() {
        assert_eq!(get_github_os("linux").unwrap(), "Linux");
    }

    #[test]
    fn test_get_github_os_unsupported() {
        let result = get_github_os("windows");
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Unsupported OS: windows");
    }

    #[test]
    fn test_get_github_os_empty() {
        let result = get_github_os("");
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Unsupported OS: ");
    }

    // ============================================================================
    // get_github_arch tests
    // ============================================================================

    #[test]
    fn test_get_github_arch_aarch64() {
        assert_eq!(get_github_arch("aarch64").unwrap(), "arm64");
    }

    #[test]
    fn test_get_github_arch_x86_64() {
        assert_eq!(get_github_arch("x86_64").unwrap(), "x86_64");
    }

    #[test]
    fn test_get_github_arch_unsupported() {
        let result = get_github_arch("arm");
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Unsupported architecture: arm");
    }

    #[test]
    fn test_get_github_arch_empty() {
        let result = get_github_arch("");
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Unsupported architecture: ");
    }
}
