use crate::prelude::{println, *};
use serde::{Deserialize, Serialize};

pub mod bitbucket;
pub mod confluence;
pub mod jira;

/// Shared Atlassian configuration options
#[derive(Debug, Clone, clap::Args)]
pub struct Global {
    /// Atlassian base URL (e.g., https://your-domain.atlassian.net)
    #[clap(long, env = "ATLASSIAN_BASE_URL")]
    pub atlassian_url: Option<String>,

    /// Atlassian email
    #[clap(long, env = "ATLASSIAN_EMAIL")]
    pub atlassian_email: Option<String>,

    /// Atlassian API token
    #[clap(long, env = "ATLASSIAN_API_TOKEN", hide = true)]
    pub atlassian_token: Option<String>,
}

/// Atlassian module app - root command
#[derive(Debug, clap::Parser)]
#[command(name = "atlassian")]
#[command(about = "Atlassian (Jira, Confluence) operations")]
pub struct App {
    #[command(subcommand)]
    pub command: Commands,

    #[clap(flatten)]
    pub global: Global,
}

#[derive(Debug, clap::Subcommand)]
pub enum Commands {
    /// Jira operations
    #[clap(subcommand)]
    Jira(jira::Commands),

    /// Confluence operations
    #[clap(subcommand)]
    Confluence(confluence::Commands),

    /// Bitbucket operations
    Bitbucket(bitbucket::App),
}

/// Atlassian configuration from environment variables
#[derive(Debug, Clone)]
pub struct AtlassianConfig {
    pub base_url: String,
    pub email: String,
    pub api_token: String,
}

impl AtlassianConfig {
    /// Load configuration from environment variables
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            base_url: std::env::var("ATLASSIAN_BASE_URL")
                .map_err(|_| eyre!("ATLASSIAN_BASE_URL environment variable not set"))?,
            email: std::env::var("ATLASSIAN_EMAIL")
                .map_err(|_| eyre!("ATLASSIAN_EMAIL environment variable not set"))?,
            api_token: std::env::var("ATLASSIAN_API_TOKEN")
                .map_err(|_| eyre!("ATLASSIAN_API_TOKEN environment variable not set"))?,
        })
    }
}

/// Jira-specific configuration with fallback to shared Atlassian credentials
#[derive(Debug, Clone)]
pub struct JiraConfig {
    pub base_url: String,
    pub email: String,
    pub api_token: String,
}

impl JiraConfig {
    /// Load configuration from environment variables
    /// Tries JIRA_* first, falls back to ATLASSIAN_*
    pub fn from_env() -> Result<Self> {
        let base_url = std::env::var("JIRA_BASE_URL")
            .or_else(|_| std::env::var("ATLASSIAN_BASE_URL"))
            .map_err(|_| {
                eyre!("Neither JIRA_BASE_URL nor ATLASSIAN_BASE_URL environment variable is set")
            })?;

        let email = std::env::var("JIRA_EMAIL")
            .or_else(|_| std::env::var("ATLASSIAN_EMAIL"))
            .map_err(|_| {
                eyre!("Neither JIRA_EMAIL nor ATLASSIAN_EMAIL environment variable is set")
            })?;

        let api_token = std::env::var("JIRA_API_TOKEN")
            .or_else(|_| std::env::var("ATLASSIAN_API_TOKEN"))
            .map_err(|_| {
                eyre!("Neither JIRA_API_TOKEN nor ATLASSIAN_API_TOKEN environment variable is set")
            })?;

        Ok(Self {
            base_url,
            email,
            api_token,
        })
    }
}

/// Confluence-specific configuration with fallback to shared Atlassian credentials
#[derive(Debug, Clone)]
pub struct ConfluenceConfig {
    pub base_url: String,
    pub email: String,
    pub api_token: String,
}

impl ConfluenceConfig {
    /// Load configuration from environment variables
    /// Tries CONFLUENCE_* first, falls back to ATLASSIAN_*
    pub fn from_env() -> Result<Self> {
        let base_url = std::env::var("CONFLUENCE_BASE_URL")
            .or_else(|_| std::env::var("ATLASSIAN_BASE_URL"))
            .map_err(|_| {
                eyre!(
                    "Neither CONFLUENCE_BASE_URL nor ATLASSIAN_BASE_URL environment variable is set"
                )
            })?;

        let email = std::env::var("CONFLUENCE_EMAIL")
            .or_else(|_| std::env::var("ATLASSIAN_EMAIL"))
            .map_err(|_| {
                eyre!("Neither CONFLUENCE_EMAIL nor ATLASSIAN_EMAIL environment variable is set")
            })?;

        let api_token = std::env::var("CONFLUENCE_API_TOKEN")
            .or_else(|_| std::env::var("ATLASSIAN_API_TOKEN"))
            .map_err(|_| {
                eyre!(
                    "Neither CONFLUENCE_API_TOKEN nor ATLASSIAN_API_TOKEN environment variable is set"
                )
            })?;

        Ok(Self {
            base_url,
            email,
            api_token,
        })
    }
}

/// Create an authenticated HTTP client with Basic Auth headers
pub fn create_authenticated_client(config: &AtlassianConfig) -> Result<reqwest::Client> {
    create_basic_auth_client(&config.email, &config.api_token)
}

/// Create an authenticated HTTP client for Jira API
pub fn create_jira_client(config: &JiraConfig) -> Result<reqwest::Client> {
    create_basic_auth_client(&config.email, &config.api_token)
}

/// Create an authenticated HTTP client for Confluence API
pub fn create_confluence_client(config: &ConfluenceConfig) -> Result<reqwest::Client> {
    create_basic_auth_client(&config.email, &config.api_token)
}

/// Internal helper to create Basic Auth HTTP client
fn create_basic_auth_client(email: &str, api_token: &str) -> Result<reqwest::Client> {
    use base64::Engine;
    use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};

    let auth_string = format!("{}:{}", email, api_token);
    let auth_encoded = base64::engine::general_purpose::STANDARD.encode(&auth_string);

    let mut headers = HeaderMap::new();
    headers.insert(
        AUTHORIZATION,
        HeaderValue::from_str(&format!("Basic {auth_encoded}"))
            .map_err(|e| eyre!("Invalid header value: {}", e))?,
    );
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));

    reqwest::Client::builder()
        .default_headers(headers)
        .build()
        .map_err(|e| eyre!("Failed to build HTTP client: {}", e))
}

/// Bitbucket configuration from environment variables
#[derive(Debug, Clone)]
pub struct BitbucketConfig {
    pub base_url: String,
    pub username: String,
    pub app_password: String,
}

impl BitbucketConfig {
    /// Default Bitbucket Cloud API base URL
    pub const DEFAULT_BASE_URL: &'static str = "https://api.bitbucket.org/2.0";

    /// Create configuration from CLI options with fallback to shared Atlassian options
    /// Used by CLI path
    pub fn new(bitbucket_global: &bitbucket::Global, _atlassian_global: &Global) -> Result<Self> {
        let base_url = bitbucket_global
            .bitbucket_url
            .clone()
            .unwrap_or_else(|| Self::DEFAULT_BASE_URL.to_string());

        let username = bitbucket_global
            .bitbucket_username
            .as_ref()
            .ok_or_else(|| {
                eyre!(
                "Bitbucket username not configured. Set --bitbucket-username or BITBUCKET_USERNAME"
            )
            })?;

        let app_password = bitbucket_global.bitbucket_app_password
            .as_ref()
            .ok_or_else(|| eyre!(
                "Bitbucket app password not configured. Set --bitbucket-app-password or BITBUCKET_APP_PASSWORD"
            ))?;

        Ok(Self {
            base_url,
            username: username.clone(),
            app_password: app_password.clone(),
        })
    }

    /// Load configuration from environment variables
    /// Used by MCP server path
    pub fn from_env() -> Result<Self> {
        let username = std::env::var("BITBUCKET_USERNAME")
            .map_err(|_| eyre!("BITBUCKET_USERNAME environment variable not set"))?;

        let app_password = std::env::var("BITBUCKET_APP_PASSWORD")
            .map_err(|_| eyre!("BITBUCKET_APP_PASSWORD environment variable not set"))?;

        Ok(Self {
            base_url: std::env::var("BITBUCKET_BASE_URL")
                .unwrap_or_else(|_| Self::DEFAULT_BASE_URL.to_string()),
            username,
            app_password,
        })
    }
}

/// Resolve the effective base URL from config and optional override.
/// Pure function - no side effects.
pub fn resolve_bitbucket_base_url(
    config: &BitbucketConfig,
    base_url_override: Option<&str>,
) -> String {
    match base_url_override {
        Some(override_url) => override_url.trim_end_matches('/').to_string(),
        None => config.base_url.trim_end_matches('/').to_string(),
    }
}

/// Create an authenticated HTTP client for Bitbucket API
pub fn create_bitbucket_client(config: &BitbucketConfig) -> Result<reqwest::Client> {
    use base64::Engine;
    use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};

    let auth_string = format!("{}:{}", config.username, config.app_password);
    let auth_encoded = base64::engine::general_purpose::STANDARD.encode(&auth_string);

    let mut headers = HeaderMap::new();
    headers.insert(
        AUTHORIZATION,
        HeaderValue::from_str(&format!("Basic {auth_encoded}"))
            .map_err(|e| eyre!("Invalid header value: {}", e))?,
    );
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));

    reqwest::Client::builder()
        .default_headers(headers)
        .build()
        .map_err(|e| eyre!("Failed to build HTTP client: {}", e))
}

/// Module entry point
pub async fn run(app: App, main_global: crate::Global) -> Result<()> {
    if main_global.verbose {
        println!("Running Atlassian module...");
    }

    match app.command {
        Commands::Jira(cmd) => jira::run(cmd, main_global).await,
        Commands::Confluence(cmd) => confluence::run(cmd, main_global).await,
        Commands::Bitbucket(bitbucket_app) => {
            bitbucket::run(bitbucket_app, app.global, main_global).await
        }
    }
}
