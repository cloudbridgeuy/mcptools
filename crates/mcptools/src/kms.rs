use crate::prelude::*;
use futures::future::join_all;

#[derive(Debug, clap::Parser)]
#[command(name = "kms")]
#[command(about = "AWS KMS (AWS Key Management Serivice)")]
pub struct App {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, clap::Subcommand)]
pub enum Commands {
    /// Get the policy attached to the key.
    #[clap(name = "get-policy")]
    GetPolicy(GetPolicyOptions),

    /// Gets the list of existing keys.
    #[clap(name = "list-keys")]
    ListKeys,
}

#[derive(Debug, clap::Args, serde::Serialize, serde::Deserialize, Clone)]
pub struct GetPolicyOptions {
    /// AWS KMS Key name.
    #[clap(env = "YAWNS_KMS_ALIAS")]
    alias: String,
}

pub async fn run(app: App, global: crate::Global) -> Result<()> {
    if global.verbose {
        aprintln!("KMS Client Version: {}", aws_sdk_kms::meta::PKG_VERSION);
        aprintln!(
            "AWS Region        : {}",
            global
                .region
                .as_ref()
                .ok_or_else(|| eyre!("AWS_REGION not defined"))?
        );
        aprintln!();
    }

    let config = get_sdk_config_from_global(global).await?;

    let client = aws_sdk_kms::Client::new(&config);

    match app.command {
        Commands::ListKeys => list_keys(client).await,
        Commands::GetPolicy(options) => get_policy(client, options).await,
    }
}

pub async fn list_keys(client: aws_sdk_kms::Client) -> Result<()> {
    let resp = client.list_keys().send().await?;

    log::info!("Getting the list of KMS keys");
    let keys = resp.keys.unwrap_or_default();

    let mut table = new_table();
    table.set_titles(prettytable::row!["Arn", "Id"]);

    let alias_futures = keys.into_iter().map(|key| {
        let client = client.clone();

        async move {
            let key_id = key.key_id.unwrap_or_default();
            log::info!("Getting aliases of KMS key {}", key_id);

            let resp = client.list_aliases().key_id(key_id).send().await?;
            let aliases = resp.aliases.unwrap_or_default();
            let alias_names = aliases
                .iter()
                .map(|alias| alias.alias_name.as_deref().unwrap_or_default())
                .collect::<Vec<&str>>()
                .join(", ");
            Ok((key.key_arn.unwrap_or_default(), alias_names)) as Result<(String, String)>
        }
    });

    let results = join_all(alias_futures).await;

    for (arn, alias_names) in results.into_iter().flatten() {
        table.add_row(prettytable::row![arn, alias_names]);
    }

    aprintln!("{}", table.to_string());

    Ok(())
}

pub async fn get_policy(client: aws_sdk_kms::Client, options: GetPolicyOptions) -> Result<()> {
    let resp = client.describe_key().key_id(options.alias).send().await?;

    if let Some(metadata) = resp.key_metadata {
        let resp = client
            .get_key_policy()
            .key_id(metadata.key_id)
            .policy_name("default")
            .send()
            .await?;
        if let Some(policy) = resp.policy {
            aprintln!("{}", policy)
        }
    }

    Ok(())
}

pub async fn get_sdk_config_from_global(global: crate::Global) -> Result<aws_config::SdkConfig> {
    let config_loader = aws_config::from_env();

    let config_loader = if let Some(region) = global.region.clone() {
        config_loader.region(aws_types::region::Region::new(region.clone()))
    } else {
        config_loader
    };

    let config_loader = if let Some(profile_name) = global.profile.clone() {
        config_loader.profile_name(profile_name)
    } else {
        config_loader
    };

    Ok(config_loader.load().await)
}

