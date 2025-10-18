#[derive(thiserror::Error, Debug, serde::Deserialize, serde::Serialize)]
#[allow(clippy::enum_variant_names)]
pub enum Error {
    #[error("Generic {0}")]
    Generic(String),

    #[error("Upgrade failed: {0}")]
    UpgradeFailed(String),

    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    #[error("Network error: {0}")]
    Network(String),

    #[error("Already running the latest version")]
    AlreadyLatest,
}
