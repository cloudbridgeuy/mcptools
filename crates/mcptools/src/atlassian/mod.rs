use crate::prelude::{println, *};
use serde::{Deserialize, Serialize};

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

/// Module entry point
pub async fn run(app: App, global: crate::Global) -> Result<()> {
    if global.verbose {
        println!("Running Atlassian module...");
    }

    match app.command {
        Commands::Jira(cmd) => jira::run(cmd, global).await,
        Commands::Confluence(cmd) => confluence::run(cmd, global).await,
    }
}
