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
        let env = std::env::var("LINEAR_API_KEY");
        let file = if matches!(&env, Err(std::env::VarError::NotPresent)) {
            crate::config::linear()?.api_key
        } else {
            None
        };
        let key = mcptools_core::config::resolve(env.ok(), None, file, None);
        match key {
            Some(key) if !key.trim().is_empty() => Ok(Self { api_key: key }),
            _ => Err(eyre!(
                "LINEAR_API_KEY environment variable not set. Set LINEAR_API_KEY to a Linear API key."
            )),
        }
    }
}
