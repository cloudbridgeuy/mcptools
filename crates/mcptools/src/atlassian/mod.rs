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
    Jira(jira::App),

    /// Confluence operations
    Confluence(confluence::App),

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
    pub fn new(service: &jira::Global, shared: &Global) -> Result<Self> {
        let file = crate::config::atlassian()?;
        let resolve = mcptools_core::config::resolve;
        Ok(Self {
            base_url: resolve(
                service.jira_url.clone(),
                shared.atlassian_url.clone(),
                file.jira.base_url,
                file.base_url,
            )
            .ok_or_else(|| eyre!("Jira URL not configured"))?,
            email: resolve(
                service.jira_email.clone(),
                shared.atlassian_email.clone(),
                file.jira.email,
                file.email,
            )
            .ok_or_else(|| eyre!("Jira email not configured"))?,
            api_token: resolve(
                service.jira_token.clone(),
                shared.atlassian_token.clone(),
                file.jira.api_token,
                file.api_token,
            )
            .ok_or_else(|| eyre!("Jira API token not configured"))?,
        })
    }

    pub fn from_env() -> Result<Self> {
        let shared = Global {
            atlassian_url: std::env::var("ATLASSIAN_BASE_URL").ok(),
            atlassian_email: std::env::var("ATLASSIAN_EMAIL").ok(),
            atlassian_token: std::env::var("ATLASSIAN_API_TOKEN").ok(),
        };
        let service = jira::Global {
            jira_url: std::env::var("JIRA_BASE_URL").ok(),
            jira_email: std::env::var("JIRA_EMAIL").ok(),
            jira_token: std::env::var("JIRA_API_TOKEN").ok(),
            jira_board_id: None,
        };
        Self::new(&service, &shared)
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
    pub fn new(service: &confluence::Global, shared: &Global) -> Result<Self> {
        let file = crate::config::atlassian()?;
        let resolve = mcptools_core::config::resolve;
        Ok(Self {
            base_url: resolve(
                service.confluence_url.clone(),
                shared.atlassian_url.clone(),
                file.confluence.base_url,
                file.base_url,
            )
            .ok_or_else(|| eyre!("Confluence URL not configured"))?,
            email: resolve(
                service.confluence_email.clone(),
                shared.atlassian_email.clone(),
                file.confluence.email,
                file.email,
            )
            .ok_or_else(|| eyre!("Confluence email not configured"))?,
            api_token: resolve(
                service.confluence_token.clone(),
                shared.atlassian_token.clone(),
                file.confluence.api_token,
                file.api_token,
            )
            .ok_or_else(|| eyre!("Confluence API token not configured"))?,
        })
    }

    pub fn from_env() -> Result<Self> {
        let shared = Global {
            atlassian_url: std::env::var("ATLASSIAN_BASE_URL").ok(),
            atlassian_email: std::env::var("ATLASSIAN_EMAIL").ok(),
            atlassian_token: std::env::var("ATLASSIAN_API_TOKEN").ok(),
        };
        let service = confluence::Global {
            confluence_url: std::env::var("CONFLUENCE_BASE_URL").ok(),
            confluence_email: std::env::var("CONFLUENCE_EMAIL").ok(),
            confluence_token: std::env::var("CONFLUENCE_API_TOKEN").ok(),
        };
        Self::new(&service, &shared)
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
    pub const DEFAULT_BASE_URL: &'static str = "https://api.bitbucket.org/2.0";

    pub fn new(service: &bitbucket::Global, _shared: &Global) -> Result<Self> {
        let file = crate::config::atlassian()?.bitbucket;
        let resolve = mcptools_core::config::resolve;
        Ok(Self {
            base_url: resolve(
                service.bitbucket_url.clone(),
                None,
                file.base_url,
                Some(Self::DEFAULT_BASE_URL.to_string()),
            )
            .expect("Bitbucket base URL default is set"),
            username: resolve(
                service.bitbucket_username.clone(),
                None,
                file.username,
                None,
            )
            .ok_or_else(|| eyre!("Bitbucket username not configured"))?,
            app_password: resolve(
                service.bitbucket_app_password.clone(),
                None,
                file.app_password,
                None,
            )
            .ok_or_else(|| eyre!("Bitbucket app password not configured"))?,
        })
    }

    pub fn from_env() -> Result<Self> {
        Self::new(
            &bitbucket::Global {
                bitbucket_url: std::env::var("BITBUCKET_BASE_URL").ok(),
                bitbucket_username: std::env::var("BITBUCKET_USERNAME").ok(),
                bitbucket_app_password: std::env::var("BITBUCKET_APP_PASSWORD").ok(),
            },
            &Global {
                atlassian_url: None,
                atlassian_email: None,
                atlassian_token: None,
            },
        )
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
        Commands::Jira(jira_app) => jira::run(jira_app, app.global, main_global).await,
        Commands::Confluence(confluence_app) => {
            confluence::run(confluence_app, app.global, main_global).await
        }
        Commands::Bitbucket(bitbucket_app) => {
            bitbucket::run(bitbucket_app, app.global, main_global).await
        }
    }
}
