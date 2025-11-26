use crate::prelude::{println, *};
use serde::{Deserialize, Serialize};

pub mod bitbucket;
pub mod confluence;
pub mod jira;

/// Atlassian module app - root command
#[derive(Debug, clap::Parser)]
#[command(name = "atlassian")]
#[command(about = "Atlassian (Jira, Confluence) operations")]
pub struct App {
    #[command(subcommand)]
    pub command: Commands,
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
    #[clap(subcommand)]
    Bitbucket(bitbucket::Commands),
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

/// Create an authenticated HTTP client with Basic Auth headers
pub fn create_authenticated_client(config: &AtlassianConfig) -> Result<reqwest::Client> {
    use base64::Engine;
    use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};

    let auth_string = format!("{}:{}", config.email, config.api_token);
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
    pub email: String,
    pub api_token: String,
}

impl BitbucketConfig {
    /// Default Bitbucket Cloud API base URL
    pub const DEFAULT_BASE_URL: &'static str = "https://api.bitbucket.org/2.0";

    /// Load configuration from environment variables
    /// Uses ATLASSIAN_EMAIL for auth
    /// Uses BITBUCKET_API_TOKEN if set, otherwise falls back to ATLASSIAN_API_TOKEN
    /// Uses BITBUCKET_BASE_URL with default fallback
    pub fn from_env() -> Result<Self> {
        // Try BITBUCKET_API_TOKEN first, fall back to ATLASSIAN_API_TOKEN
        let api_token = std::env::var("BITBUCKET_API_TOKEN")
            .or_else(|_| std::env::var("ATLASSIAN_API_TOKEN"))
            .map_err(|_| {
                eyre!("Neither BITBUCKET_API_TOKEN nor ATLASSIAN_API_TOKEN environment variable is set")
            })?;

        Ok(Self {
            base_url: std::env::var("BITBUCKET_BASE_URL")
                .unwrap_or_else(|_| Self::DEFAULT_BASE_URL.to_string()),
            email: std::env::var("ATLASSIAN_EMAIL")
                .map_err(|_| eyre!("ATLASSIAN_EMAIL environment variable not set"))?,
            api_token,
        })
    }

    /// Apply CLI overrides to the configuration
    pub fn with_overrides(mut self, base_url: Option<String>, api_token: Option<String>) -> Self {
        if let Some(url) = base_url {
            self.base_url = url;
        }
        if let Some(token) = api_token {
            self.api_token = token;
        }
        self
    }
}

/// Create an authenticated HTTP client for Bitbucket API
pub fn create_bitbucket_client(config: &BitbucketConfig) -> Result<reqwest::Client> {
    use base64::Engine;
    use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};

    let auth_string = format!("{}:{}", config.email, config.api_token);
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
pub async fn run(app: App, global: crate::Global) -> Result<()> {
    if global.verbose {
        println!("Running Atlassian module...");
    }

    match app.command {
        Commands::Jira(cmd) => jira::run(cmd, global).await,
        Commands::Confluence(cmd) => confluence::run(cmd, global).await,
        Commands::Bitbucket(cmd) => bitbucket::run(cmd, global).await,
    }
}
