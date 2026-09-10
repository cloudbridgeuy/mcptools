use crate::prelude::*;

#[derive(Clone)]
pub struct LinearConfig {
    pub api_key: String,
}

impl std::fmt::Debug for LinearConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LinearConfig")
            .field("api_key", &"[redacted]")
            .finish()
    }
}

impl LinearConfig {
    pub fn from_env() -> Result<Self> {
        match std::env::var("LINEAR_API_KEY") {
            Ok(key) if !key.trim().is_empty() => Ok(Self { api_key: key }),
            _ => Err(eyre!(
                "LINEAR_API_KEY environment variable not set. Set LINEAR_API_KEY to a Linear API key."
            )),
        }
    }
}
